//! Repository key (`keys/repo.key`, format v3): init, migrate, rotate,
//! export, import and change password — the same SDK calls as
//! `rsb key init/migrate/rotate/export/import/change-password`.

use dioxus::prelude::*;
use rsb_sdk::CancellationToken;
use rsb_sdk::crypto::keyfile::{self, KdfParams, REPO_KEY_PATH};
use rsb_sdk::crypto::lifecycle::{self, KeyStatus};
use rsb_sdk::crypto::paper;
use rsb_sdk::storage::Storage;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::{fill, get_key_texts},
    key_lifecycle::{
        check_new_password, init_key, migrate, migrate_preview, off_ui_thread, rotate,
        rotate_preview, status_text,
    },
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{LoadedProfile, load_profile, repository_config, repository_label},
    shared::{ProgressBar, progress_reporter},
};

/// Largest key file accepted for import (as `rsb key import`).
pub const MAX_IMPORT_SIZE: u64 = 64 * 1024;

/// Creates `path` (it must not exist) readable only by the owner and writes
/// `data` (as `rsb key export -o`).
pub fn write_private_file(path: &Path, data: &[u8]) -> Result<(), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| format!("Cannot create {}: {}", path.display(), e))?;
    f.write_all(data)
        .and_then(|_| f.sync_all())
        .map_err(|e| format!("Cannot write {}: {}", path.display(), e))
}

/// Reads an exported key file, in JSON or paper form (as `rsb key import`).
pub fn read_import_file(path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("Cannot read {}: {}", path.display(), e))?
        .len();
    if len > MAX_IMPORT_SIZE {
        return Err(format!("{} is too large for a key file", path.display()));
    }
    let content = Zeroizing::new(
        std::fs::read(path).map_err(|e| format!("Cannot read {}: {}", path.display(), e))?,
    );
    if paper::is_paper(&content) {
        let text = std::str::from_utf8(&content).map_err(|e| e.to_string())?;
        paper::decode(text)
            .map(Zeroizing::new)
            .map_err(|e| format!("Paper key: {}", e))
    } else {
        Ok(content)
    }
}

/// Exports the key file to `out` (JSON, or the printable paper form) after
/// checking that `password` opens it.
pub async fn export_key(
    storage: &dyn Storage,
    password: &str,
    out: &Path,
    paper_form: bool,
) -> Result<String, String> {
    let raw = lifecycle::export_key_file(storage, password.as_bytes())
        .await
        .map_err(|e| e.to_string())?;
    let data = if paper_form {
        let file = keyfile::RepoKeyFile::from_bytes(&raw).map_err(|e| e.to_string())?;
        let compact = file.to_compact_bytes().map_err(|e| e.to_string())?;
        paper::encode(&compact).into_bytes()
    } else {
        raw
    };
    write_private_file(out, &data)?;
    Ok(format!(
        "✅ Key file exported to {} (mode 0600). Together with the password it decrypts every backup: store it safely.",
        out.display()
    ))
}

/// Installs an exported key file (see `rsb key import`).
pub async fn import_key(
    storage: Arc<dyn Storage>,
    file: &Path,
    password: &str,
    force: bool,
) -> Result<String, String> {
    let raw = read_import_file(file)?;
    let out = lifecycle::import_key_file(storage, &raw, password.as_bytes(), force)
        .await
        .map_err(|e| e.to_string())?;
    if out.unchanged {
        return Ok(format!(
            "ℹ️ {} already holds this key file; nothing changed.",
            REPO_KEY_PATH
        ));
    }
    let mut msg = format!("✅ Imported {} ({}).", REPO_KEY_PATH, out.matched_by);
    if let Some(b) = out.replaced_backup {
        msg.push_str(&format!(" The previous key file was kept at {}.", b));
    }
    Ok(msg)
}

/// Changes the repository password (rewraps the key; no data rewritten).
pub async fn change_password(
    storage: Arc<dyn Storage>,
    old: &str,
    new: &str,
    confirm: &str,
) -> Result<String, String> {
    if new.is_empty() {
        return Err("The new password must not be empty".to_string());
    }
    if new != confirm {
        return Err("The new passwords do not match".to_string());
    }
    let current = keyfile::read_key_file(&*storage)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| {
            format!(
                "{} not found: run an encrypted backup (or `rsb key init`) first",
                REPO_KEY_PATH
            )
        })?;
    keyfile::change_password(
        storage,
        old.as_bytes(),
        new.as_bytes(),
        Some(current.params()),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok("✅ Repository password changed. Existing backups are unchanged. Update RSB_ENCRYPTION_KEY / the OS keyring entry used by scheduled backups.".to_string())
}

/// A key operation waiting for the user's confirmation, with the text that
/// says what it will do.
#[derive(Clone, Debug, PartialEq)]
enum Pending {
    Migrate(String),
    Rotate(String),
}

#[component]
pub fn KeyScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let t = get_key_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut repo_path = use_signal(PathBuf::new);
    // Passwords live only in these signals (memory) and are cleared after
    // use; they are never logged or written anywhere.
    let mut password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut init_password = use_signal(String::new);
    let mut init_confirm = use_signal(String::new);
    let mut paper_form = use_signal(|| false);
    let mut force = use_signal(|| false);
    // Short operations (spinner) and long ones (progress bar + stop).
    let mut busy = use_signal(|| false);
    let mut running = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    let mut token = use_signal(CancellationToken::new);
    let mut pending = use_signal(|| Option::<Pending>::None);
    let mut key_status = use_signal(|| Option::<KeyStatus>::None);
    let mut status = use_signal(String::new);

    // Opens the storage of the selected repository (profile, folder or the
    // S3 bucket of Settings) through the SDK storage abstraction.
    let open_storage = move || {
        let s3 = S3Settings::from_app(&app_config);
        let cfg = repository_config(profile().as_ref(), &repo_path(), &s3);
        async move {
            let cfg = cfg?;
            rsb_sdk::core::storage_backend::get_storage(&cfg)
                .await
                .map_err(|e| e.to_string())
        }
    };

    // Reads the key status (no password) off the UI thread.
    let fetch_status = move || {
        let storage = open_storage();
        async move {
            let storage = storage.await?;
            off_ui_thread(move || async move {
                lifecycle::key_status(&*storage)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await?
        }
    };

    let mut forget_repo_state = move || {
        key_status.set(None);
        pending.set(None);
    };

    let handle_load_profile = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new()
                .add_filter("TOML", &["toml"])
                .pick_file()
                .await
            {
                match load_profile(h.path()) {
                    Ok(p) => {
                        repo_path.set(PathBuf::from(repository_label(&p.config)));
                        status.set(fill(
                            t.profile_loaded,
                            &[("path", &p.path.display().to_string())],
                        ));
                        profile.set(Some(p));
                        forget_repo_state();
                    }
                    Err(e) => status.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let handle_check = move |_| {
        busy.set(true);
        pending.set(None);
        status.set(t.checking.to_string());
        let fut = fetch_status();
        spawn(async move {
            match fut.await {
                Ok(s) => {
                    status.set(status_text(&s, t));
                    key_status.set(Some(s));
                }
                Err(e) => status.set(format!("❌ {}", e)),
            }
            busy.set(false);
        });
    };

    // First step of migrate/rotate: read what it will do and ask.
    let mut prepare = move |rotate_op: bool| {
        busy.set(true);
        pending.set(None);
        status.set(t.checking.to_string());
        let fut = fetch_status();
        spawn(async move {
            match fut.await {
                Ok(s) if !s.key_file => {
                    status.set(format!("❌ {}", t.status_no_key));
                    key_status.set(Some(s));
                }
                Ok(s) => {
                    status.set(status_text(&s, t));
                    pending.set(Some(if rotate_op {
                        Pending::Rotate(rotate_preview(&s, t))
                    } else {
                        Pending::Migrate(migrate_preview(&s, t))
                    }));
                    key_status.set(Some(s));
                }
                Err(e) => status.set(format!("❌ {}", e)),
            }
            busy.set(false);
        });
    };

    // Second step: the confirmed migrate/rotate, off the UI thread, with
    // progress and stop (resumable: running it again continues).
    let handle_confirm = move |_| {
        let Some(op) = pending() else {
            return;
        };
        let pw = Zeroizing::new(password());
        if pw.is_empty() {
            status.set(format!("❌ {}", t.need_password));
            return;
        }
        pending.set(None);
        let is_rotate = matches!(op, Pending::Rotate(_));
        let tok = CancellationToken::new();
        token.set(tok.clone());
        progress.set(0.0);
        running.set(true);
        status.set(
            if is_rotate {
                t.rotate_running
            } else {
                t.migrate_running
            }
            .to_string(),
        );
        let cb = progress_reporter(progress, status, running);
        let storage = open_storage();
        spawn(async move {
            let res = async {
                let storage = storage.await.map_err(|e| format!("❌ {}", e))?;
                off_ui_thread(move || async move {
                    if is_rotate {
                        rotate(storage, &pw, Some(cb), Some(tok), t).await
                    } else {
                        migrate(storage, &pw, Some(cb), Some(tok), t).await
                    }
                })
                .await
                .map_err(|e| format!("❌ {}", e))?
            }
            .await;
            running.set(false);
            if res.is_ok() {
                progress.set(1.0);
            }
            status.set(match res {
                Ok(m) | Err(m) => m,
            });
            if let Ok(s) = fetch_status().await {
                key_status.set(Some(s));
            }
        });
    };

    let handle_stop = move |_| {
        token().cancel();
        status.set(t.stopping.to_string());
    };

    let handle_init = move |_| {
        let new = Zeroizing::new(init_password());
        let confirm = Zeroizing::new(init_confirm());
        if let Err(e) = check_new_password(&new, &confirm, t) {
            status.set(format!("❌ {}", e));
            return;
        }
        busy.set(true);
        pending.set(None);
        status.set(t.working.to_string());
        let storage = open_storage();
        spawn(async move {
            let res = async {
                let storage = storage.await.map_err(|e| format!("❌ {}", e))?;
                off_ui_thread(move || async move {
                    init_key(storage, &new, &confirm, KdfParams::default(), t).await
                })
                .await
                .map_err(|e| format!("❌ {}", e))?
            }
            .await;
            match res {
                Ok((msg, s)) => {
                    init_password.set(String::new());
                    init_confirm.set(String::new());
                    status.set(msg);
                    key_status.set(Some(s));
                }
                Err(e) => status.set(e),
            }
            busy.set(false);
        });
    };

    let handle_export = move |_| {
        let pw = Zeroizing::new(password());
        let paper_mode = paper_form();
        let storage = open_storage();
        spawn(async move {
            let name = if paper_mode {
                "repo-key.txt"
            } else {
                "repo.key"
            };
            let Some(h) = rfd::AsyncFileDialog::new()
                .set_file_name(name)
                .save_file()
                .await
            else {
                return;
            };
            busy.set(true);
            let res = async {
                let storage = storage.await?;
                export_key(&*storage, &pw, h.path(), paper_mode).await
            }
            .await;
            status.set(res.unwrap_or_else(|e| format!("❌ {}", e)));
            busy.set(false);
        });
    };

    let handle_import = move |_| {
        let pw = Zeroizing::new(password());
        let force_mode = force();
        let storage = open_storage();
        spawn(async move {
            let Some(h) = rfd::AsyncFileDialog::new().pick_file().await else {
                return;
            };
            busy.set(true);
            let res = async {
                let storage = storage.await?;
                import_key(storage, h.path(), &pw, force_mode).await
            }
            .await;
            status.set(res.unwrap_or_else(|e| format!("❌ {}", e)));
            busy.set(false);
            forget_repo_state();
        });
    };

    let handle_change = move |_| {
        let old = Zeroizing::new(password());
        let new = Zeroizing::new(new_password());
        let confirm = Zeroizing::new(confirm_password());
        let storage = open_storage();
        busy.set(true);
        spawn(async move {
            let res = async {
                let storage = storage.await?;
                change_password(storage, &old, &new, &confirm).await
            }
            .await;
            if res.is_ok() {
                new_password.set(String::new());
                confirm_password.set(String::new());
            }
            status.set(res.unwrap_or_else(|e| format!("❌ {}", e)));
            busy.set(false);
        });
    };

    let profile_label = profile()
        .map(|p| p.path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let locked = busy() || running();
    let no_password = password().is_empty();
    let known = key_status();
    let init_disabled = locked || known.as_ref().is_some_and(|s| !s.can_init());
    let key_ops_disabled = locked || no_password || known.as_ref().is_some_and(|s| !s.key_file);

    rsx! {
        LoadingOverlay {
            is_visible: busy(),
            style: LoadingStyle::Spinner,
            message: t.working.to_string(),
        }

        div { class: "card",
            h2 { class: "page-title", "{t.title}" }
            p { class: "hint mb-4", "{t.intro}" }

            div { class: "form-group",
                label { class: "label-text", "{t.profile_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{t.profile_placeholder}",
                        value: "{profile_label}",
                        readonly: true
                    }
                    button { class: "btn-icon", onclick: handle_load_profile, disabled: locked, "📂" }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{t.repo_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{t.repo_placeholder}",
                        value: "{repo_path.read().to_string_lossy()}",
                        disabled: locked,
                        oninput: move |evt| {
                            profile.set(None);
                            repo_path.set(PathBuf::from(evt.value()));
                            forget_repo_state();
                        }
                    }
                    button {
                        class: "btn-icon",
                        disabled: locked,
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    profile.set(None);
                                    repo_path.set(h.path().to_path_buf());
                                    forget_repo_state();
                                }
                            });
                        },
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{t.password_label}" }
                input {
                    class: "input-field",
                    r#type: "password",
                    autocomplete: "off",
                    value: "{password}",
                    oninput: move |evt| password.set(evt.value())
                }
            }

            h3 { class: "section-title", "{t.status_section}" }
            button {
                class: "w-full btn-secondary mb-4",
                disabled: locked,
                onclick: handle_check,
                "{t.check_button}"
            }

            h3 { class: "section-title", "{t.init_section}" }
            div { class: "form-group",
                p { class: "hint", "{t.init_hint}" }
                input {
                    class: "input-field mb-2",
                    r#type: "password",
                    autocomplete: "off",
                    placeholder: "{t.new_password}",
                    value: "{init_password}",
                    oninput: move |evt| init_password.set(evt.value())
                }
                input {
                    class: "input-field",
                    r#type: "password",
                    autocomplete: "off",
                    placeholder: "{t.confirm_password}",
                    value: "{init_confirm}",
                    oninput: move |evt| init_confirm.set(evt.value())
                }
            }
            button {
                class: "w-full btn-primary mb-4",
                disabled: init_disabled || init_password().is_empty(),
                onclick: handle_init,
                "{t.init_button}"
            }

            h3 { class: "section-title", "{t.migrate_section}" }
            p { class: "hint", "{t.migrate_hint}" }
            button {
                class: "w-full btn-secondary mb-4",
                disabled: key_ops_disabled,
                onclick: move |_| prepare(false),
                "{t.migrate_button}"
            }

            h3 { class: "section-title", "{t.rotate_section}" }
            p { class: "hint", "{t.rotate_hint}" }
            button {
                class: "w-full btn-warning mb-4",
                disabled: key_ops_disabled,
                onclick: move |_| prepare(true),
                "{t.rotate_button}"
            }

            if let Some(op) = pending() {
                div { class: "status-box mb-4 border-l-4 border-amber-500",
                    p { class: "font-semibold mb-2", "{t.confirm_title}" }
                    p { class: "whitespace-pre-wrap mb-3",
                        match &op {
                            Pending::Migrate(text) | Pending::Rotate(text) => text.clone(),
                        }
                    }
                    if no_password {
                        p { class: "hint mb-2", "{t.need_password}" }
                    }
                    div { class: "flex gap-3",
                        button {
                            class: "flex-1 btn-warning",
                            disabled: locked || no_password,
                            onclick: handle_confirm,
                            "{t.confirm_button}"
                        }
                        button {
                            class: "flex-1 btn-secondary",
                            onclick: move |_| pending.set(None),
                            "{t.cancel_button}"
                        }
                    }
                }
            }

            if running() {
                div { class: "mb-4",
                    ProgressBar { progress: progress() }
                    button {
                        class: "w-full mt-2 px-4 py-2 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                        onclick: handle_stop,
                        "{t.stop_button}"
                    }
                }
            }

            h3 { class: "section-title", "{t.export_section}" }
            div { class: "form-group space-y-2",
                label { class: "label-text flex items-center gap-2",
                    input { r#type: "checkbox", checked: paper_form(), onchange: move |_| paper_form.toggle() }
                    "{t.paper_form}"
                }
                label { class: "label-text flex items-center gap-2",
                    input { r#type: "checkbox", checked: force(), onchange: move |_| force.toggle() }
                    "{t.import_force}"
                }
            }
            div { class: "flex gap-3 mb-4",
                button {
                    class: "flex-1 btn-primary",
                    disabled: locked || no_password,
                    onclick: handle_export,
                    "{t.export_button}"
                }
                button {
                    class: "flex-1 btn-secondary",
                    disabled: locked || no_password,
                    onclick: handle_import,
                    "{t.import_button}"
                }
            }

            h3 { class: "section-title", "{t.change_section}" }
            div { class: "form-group",
                input {
                    class: "input-field mb-2",
                    r#type: "password",
                    autocomplete: "off",
                    placeholder: "{t.new_password}",
                    value: "{new_password}",
                    oninput: move |evt| new_password.set(evt.value())
                }
                input {
                    class: "input-field",
                    r#type: "password",
                    autocomplete: "off",
                    placeholder: "{t.confirm_password}",
                    value: "{confirm_password}",
                    oninput: move |evt| confirm_password.set(evt.value())
                }
                p { class: "hint", "{t.change_hint}" }
            }
            button {
                class: "w-full btn-warning mb-4",
                disabled: locked || no_password || new_password().is_empty(),
                onclick: handle_change,
                "{t.change_button}"
            }

            if !status().is_empty() {
                div { class: "status-box mt-4",
                    p { class: "whitespace-pre-wrap", "{status}" }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsb_sdk::crypto::keyfile::KdfParams;
    use rsb_sdk::storage::LocalStorage;

    #[test]
    fn export_file_is_private_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("repo.key");
        write_private_file(&p, b"{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(write_private_file(&p, b"x").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"{}");
    }

    #[test]
    fn oversized_import_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big");
        std::fs::write(&p, vec![b'a'; MAX_IMPORT_SIZE as usize + 1]).unwrap();
        assert!(read_import_file(&p).unwrap_err().contains("too large"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn export_change_password_and_import_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(repo.to_str().unwrap()));
        keyfile::init_repo_key(storage.clone(), b"old-pw", KdfParams::MIN_CREATE)
            .await
            .unwrap();

        // Wrong password: nothing exported.
        let out = dir.path().join("export.key");
        assert!(export_key(&*storage, "nope", &out, false).await.is_err());
        assert!(!out.exists());
        export_key(&*storage, "old-pw", &out, false).await.unwrap();
        let paper_out = dir.path().join("paper.txt");
        export_key(&*storage, "old-pw", &paper_out, true)
            .await
            .unwrap();

        assert!(
            change_password(storage.clone(), "old-pw", "new-pw", "typo")
                .await
                .is_err()
        );
        change_password(storage.clone(), "old-pw", "new-pw", "new-pw")
            .await
            .unwrap();
        assert!(
            lifecycle::export_key_file(&*storage, b"new-pw")
                .await
                .is_ok()
        );

        // The paper export (old password) restores the original key file.
        let msg = import_key(storage.clone(), &paper_out, "old-pw", true)
            .await
            .unwrap();
        assert!(msg.contains("Imported"), "{msg}");
        assert!(
            lifecycle::export_key_file(&*storage, b"old-pw")
                .await
                .is_ok()
        );
    }
}
