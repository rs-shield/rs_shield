//! Repository key (`keys/repo.key`, format v3): export, import and change
//! password — the same SDK calls as `rsb key export/import/change-password`.

use dioxus::prelude::*;
use rsb_sdk::crypto::keyfile::{self, REPO_KEY_PATH};
use rsb_sdk::crypto::{lifecycle, paper};
use rsb_sdk::storage::Storage;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::{Translations, get_texts, tr},
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{LoadedProfile, load_profile, repository_config, repository_label},
};

/// Largest key file accepted for import (as `rsb key import`).
pub const MAX_IMPORT_SIZE: u64 = 64 * 1024;

/// Creates `path` (it must not exist) readable only by the owner and writes
/// `data` (as `rsb key export -o`).
pub fn write_private_file(t: &Translations, path: &Path, data: &[u8]) -> Result<(), String> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| tr(t.cp_cannot_create, &[&path.display(), &e]))?;
    f.write_all(data)
        .and_then(|_| f.sync_all())
        .map_err(|e| tr(t.cp_cannot_write, &[&path.display(), &e]))
}

/// Reads an exported key file, in JSON or paper form (as `rsb key import`).
pub fn read_import_file(t: &Translations, path: &Path) -> Result<Zeroizing<Vec<u8>>, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| tr(t.rk_cannot_read, &[&path.display(), &e]))?
        .len();
    if len > MAX_IMPORT_SIZE {
        return Err(tr(t.rk_too_large, &[&path.display()]));
    }
    let content = Zeroizing::new(
        std::fs::read(path).map_err(|e| tr(t.rk_cannot_read, &[&path.display(), &e]))?,
    );
    if paper::is_paper(&content) {
        let text = std::str::from_utf8(&content).map_err(|e| e.to_string())?;
        paper::decode(text)
            .map(Zeroizing::new)
            .map_err(|e| tr(t.rk_paper_key_error, &[&e]))
    } else {
        Ok(content)
    }
}

/// Exports the key file to `out` (JSON, or the printable paper form) after
/// checking that `password` opens it.
pub async fn export_key(
    t: &Translations,
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
    write_private_file(t, out, &data)?;
    Ok(tr(t.rk_exported, &[&out.display()]))
}

/// Installs an exported key file (see `rsb key import`).
pub async fn import_key(
    t: &Translations,
    storage: Arc<dyn Storage>,
    file: &Path,
    password: &str,
    force: bool,
) -> Result<String, String> {
    let raw = read_import_file(t, file)?;
    let out = lifecycle::import_key_file(storage, &raw, password.as_bytes(), force)
        .await
        .map_err(|e| e.to_string())?;
    if out.unchanged {
        return Ok(tr(t.rk_import_unchanged, &[&REPO_KEY_PATH]));
    }
    let mut msg = tr(t.rk_imported, &[&REPO_KEY_PATH, &out.matched_by]);
    if let Some(b) = out.replaced_backup {
        msg.push_str(&tr(t.rk_previous_kept, &[&b]));
    }
    Ok(msg)
}

/// Changes the repository password (rewraps the key; no data rewritten).
pub async fn change_password(
    t: &Translations,
    storage: Arc<dyn Storage>,
    old: &str,
    new: &str,
    confirm: &str,
) -> Result<String, String> {
    if new.is_empty() {
        return Err(t.rk_new_password_empty.to_string());
    }
    if new != confirm {
        return Err(t.rk_new_passwords_mismatch.to_string());
    }
    let current = keyfile::read_key_file(&*storage)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| tr(t.rk_key_not_found, &[&REPO_KEY_PATH]))?;
    keyfile::change_password(
        storage,
        old.as_bytes(),
        new.as_bytes(),
        Some(current.params()),
    )
    .await
    .map_err(|e| e.to_string())?;
    Ok(t.rk_password_changed.to_string())
}

#[component]
pub fn KeyScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut repo_path = use_signal(PathBuf::new);
    let mut password = use_signal(String::new);
    let mut new_password = use_signal(String::new);
    let mut confirm_password = use_signal(String::new);
    let mut paper_form = use_signal(|| false);
    let mut force = use_signal(|| false);
    let mut busy = use_signal(|| false);
    let mut status = use_signal(String::new);

    // Opens the storage of the selected repository (profile, folder or the
    // S3 bucket of Settings).
    let open_storage = move || {
        let s3 = S3Settings::from_app(&app_config);
        let cfg = repository_config(profile().as_ref(), &repo_path(), &s3, texts);
        async move {
            let cfg = cfg?;
            rsb_sdk::core::storage_backend::get_storage(&cfg)
                .await
                .map_err(|e| e.to_string())
        }
    };

    let handle_load_profile = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new()
                .add_filter("TOML", &["toml"])
                .pick_file()
                .await
            {
                match load_profile(h.path(), texts) {
                    Ok(p) => {
                        repo_path.set(PathBuf::from(repository_label(&p.config)));
                        status.set(tr(texts.rk_profile_loaded, &[&p.path.display()]));
                        profile.set(Some(p));
                    }
                    Err(e) => status.set(format!("❌ {}", e)),
                }
            }
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
                export_key(texts, &*storage, &pw, h.path(), paper_mode).await
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
                import_key(texts, storage, h.path(), &pw, force_mode).await
            }
            .await;
            status.set(res.unwrap_or_else(|e| format!("❌ {}", e)));
            busy.set(false);
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
                change_password(texts, storage, &old, &new, &confirm).await
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

    rsx! {
        LoadingOverlay {
            is_visible: busy(),
            style: LoadingStyle::Spinner,
            message: texts.rk_working.to_string(),
        }

        div { class: "card",
            h2 { class: "page-title", "{texts.rk_title}" }
            p { class: "hint mb-4",
                "{texts.rk_intro}"
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.rk_profile_optional}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: texts.rk_select_profile,
                        value: "{profile_label}",
                        readonly: true
                    }
                    button { class: "btn-icon", onclick: handle_load_profile, disabled: busy(), "📂" }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.rk_repo_folder}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: texts.rk_repo_folder_placeholder,
                        value: "{repo_path.read().to_string_lossy()}",
                        oninput: move |evt| {
                            profile.set(None);
                            repo_path.set(PathBuf::from(evt.value()));
                        }
                    }
                    button {
                        class: "btn-icon",
                        disabled: busy(),
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    profile.set(None);
                                    repo_path.set(h.path().to_path_buf());
                                }
                            });
                        },
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.rk_current_password}" }
                input {
                    class: "input-field",
                    r#type: "password",
                    value: "{password}",
                    oninput: move |evt| password.set(evt.value())
                }
            }

            h3 { class: "section-title", "{texts.rk_export_import}" }
            div { class: "form-group space-y-2",
                label { class: "label-text flex items-center gap-2",
                    input { r#type: "checkbox", checked: paper_form(), onchange: move |_| paper_form.toggle() }
                    "{texts.rk_paper_checkbox}"
                }
                label { class: "label-text flex items-center gap-2",
                    input { r#type: "checkbox", checked: force(), onchange: move |_| force.toggle() }
                    "{texts.rk_force_checkbox}"
                }
            }
            div { class: "flex gap-3 mb-4",
                button {
                    class: "flex-1 btn-primary",
                    disabled: busy() || password().is_empty(),
                    onclick: handle_export,
                    "{texts.rk_export_button}"
                }
                button {
                    class: "flex-1 btn-secondary",
                    disabled: busy() || password().is_empty(),
                    onclick: handle_import,
                    "{texts.rk_import_button}"
                }
            }

            h3 { class: "section-title", "{texts.rk_change_password_title}" }
            div { class: "form-group",
                input {
                    class: "input-field mb-2",
                    r#type: "password",
                    placeholder: texts.rk_new_password,
                    value: "{new_password}",
                    oninput: move |evt| new_password.set(evt.value())
                }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: texts.rk_confirm_new_password,
                    value: "{confirm_password}",
                    oninput: move |evt| confirm_password.set(evt.value())
                }
                p { class: "hint", "{texts.rk_change_password_hint}" }
            }
            button {
                class: "w-full btn-warning mb-4",
                disabled: busy() || password().is_empty() || new_password().is_empty(),
                onclick: handle_change,
                "{texts.rk_change_password_button}"
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
    use crate::ui::i18n::Language;
    use rsb_sdk::crypto::keyfile::KdfParams;

    fn t() -> &'static Translations {
        get_texts(Language::En)
    }
    use rsb_sdk::storage::LocalStorage;

    #[test]
    fn export_file_is_private_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("repo.key");
        write_private_file(t(), &p, b"{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(write_private_file(t(), &p, b"x").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"{}");
    }

    #[test]
    fn oversized_import_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("big");
        std::fs::write(&p, vec![b'a'; MAX_IMPORT_SIZE as usize + 1]).unwrap();
        assert!(read_import_file(t(), &p).unwrap_err().contains("too large"));
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
        assert!(
            export_key(t(), &*storage, "nope", &out, false)
                .await
                .is_err()
        );
        assert!(!out.exists());
        export_key(t(), &*storage, "old-pw", &out, false)
            .await
            .unwrap();
        let paper_out = dir.path().join("paper.txt");
        export_key(t(), &*storage, "old-pw", &paper_out, true)
            .await
            .unwrap();

        assert!(
            change_password(t(), storage.clone(), "old-pw", "new-pw", "typo")
                .await
                .is_err()
        );
        change_password(t(), storage.clone(), "old-pw", "new-pw", "new-pw")
            .await
            .unwrap();
        assert!(
            lifecycle::export_key_file(&*storage, b"new-pw")
                .await
                .is_ok()
        );

        // The paper export (old password) restores the original key file.
        let msg = import_key(t(), storage.clone(), &paper_out, "old-pw", true)
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
