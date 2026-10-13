use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{
        LoadedProfile, load_profile, repository_config, repository_label, resolve_key_blocking,
    },
};
use rsb_sdk::operation::operations_helpers::record_prune_operation;

use dioxus::prelude::*;
use rsb_sdk::core;
use std::path::PathBuf;

/// What prune keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Retention {
    /// The newest N snapshots (`rsb prune -l N`).
    KeepLast(usize),
    /// Snapshots of the last N days; the newest one is always kept.
    Days(u32),
}

/// Parses the retention fields (`mode` = "count" or "days"). Unlike the
/// old screen, invalid input is an error instead of silently becoming 30.
pub fn parse_retention(mode: &str, value: &str) -> Result<Retention, String> {
    let v = value.trim();
    let n: u32 = v.parse().ok().filter(|n| *n >= 1).ok_or_else(|| {
        format!(
            "Invalid retention '{}': enter a whole number of at least 1",
            v
        )
    })?;
    match mode {
        "days" => Ok(Retention::Days(n.min(36_500))),
        _ => Ok(Retention::KeepLast(n as usize)),
    }
}

/// Text of a prune plan / result.
pub fn describe_plan(doomed: &[String], dry_run: bool) -> String {
    if doomed.is_empty() {
        return "✅ Nothing to prune: no snapshot matches the retention.".to_string();
    }
    let mut msg = if dry_run {
        format!(
            "🧪 Preview: {} snapshot(s) would be deleted (nothing was deleted):",
            doomed.len()
        )
    } else {
        format!(
            "✅ Pruned {} snapshot(s) and their unreferenced data:",
            doomed.len()
        )
    };
    for s in doomed.iter().take(20) {
        msg.push_str("\n  • ");
        msg.push_str(s.trim_start_matches("snapshots/").trim_end_matches(".toml"));
    }
    if doomed.len() > 20 {
        msg.push_str(&format!("\n  … {} more", doomed.len() - 20));
    }
    msg
}

async fn run_prune(
    cfg: rsb_sdk::config::Config,
    key: Option<String>,
    retention: Retention,
    dry_run: bool,
) -> Result<Vec<String>, String> {
    let storage = core::storage_backend::get_storage(&cfg)
        .await
        .map_err(|e| e.to_string())?;
    let doomed = match retention {
        Retention::KeepLast(n) => core::plan_prune_by_count_with_storage(&*storage, n).await,
        Retention::Days(d) => core::plan_prune_by_age_with_storage(&*storage, d).await,
    }
    .map_err(|e| e.to_string())?;
    if dry_run || doomed.is_empty() {
        return Ok(doomed);
    }
    // Same refusal as `rsb prune`: an encrypted repository cannot be
    // garbage-collected without its key.
    let encrypted = rsb_sdk::crypto::keyfile::repo_key_exists(&*storage)
        .await
        .map_err(|e| e.to_string())?
        || cfg.encryption_required();
    if encrypted && key.is_none() {
        return Err(format!(
            "The repository is encrypted and no key is available: type the key, set {} or store it in the OS keyring.",
            rsb_sdk::config::ENCRYPTION_KEY_ENV
        ));
    }
    core::prune_with_storage(storage, key.as_deref(), &doomed)
        .await
        .map_err(|e| e.to_string())?;
    Ok(doomed)
}

#[component]
pub fn PruneScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut repo_path = use_signal(PathBuf::new);
    let mut retention_mode = use_signal(|| "count".to_string());
    let mut retention_value = use_signal(|| "10".to_string());
    let mut is_running = use_signal(|| false);
    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut key = use_signal(String::new);
    // Explicit confirmation (`rsb prune --confirm`): without it only the
    // preview runs.
    let mut confirm_delete = use_signal(|| false);

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
                        status_msg.set(format!("✅ Profile loaded: {}", p.path.display()));
                        profile.set(Some(p));
                    }
                    Err(e) => status_msg.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let mut start = move |dry_run: bool| {
        if is_running() {
            return;
        }
        if !dry_run && !confirm_delete() {
            status_msg.set(
                "⚠️ Tick the confirmation box (or use Preview) before deleting snapshots."
                    .to_string(),
            );
            return;
        }
        let retention = match parse_retention(&retention_mode(), &retention_value()) {
            Ok(r) => r,
            Err(e) => {
                status_msg.set(format!("❌ {}", e));
                return;
            }
        };
        is_running.set(true);
        status_msg.set(texts.starting.to_string());

        let loaded = profile();
        let path = repo_path();
        let typed_key = key();
        let s3 = S3Settings::from_app(&app_config);

        spawn(async move {
            let cfg = repository_config(loaded.as_ref(), &path, &s3);
            let label = cfg.as_ref().map(repository_label).unwrap_or_default();
            let started = std::time::Instant::now();
            let result = async {
                let cfg = cfg?;
                let key = resolve_key_blocking(typed_key, loaded.clone()).await;
                run_prune(cfg, key, retention, dry_run).await
            }
            .await;

            match result {
                Ok(doomed) => {
                    status_msg.set(describe_plan(&doomed, dry_run));
                    if !dry_run {
                        let _ = record_prune_operation(
                            "Success".to_string(),
                            doomed.len(),
                            started.elapsed().as_secs(),
                            "N/A".to_string(),
                            label,
                        );
                        confirm_delete.set(false);
                    }
                }
                Err(e) => {
                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                    if !dry_run {
                        let _ = record_prune_operation(
                            "Failed".to_string(),
                            0,
                            started.elapsed().as_secs(),
                            "N/A".to_string(),
                            label,
                        );
                    }
                }
            }
            is_running.set(false);
        });
    };

    let profile_label = profile()
        .map(|p| p.path.to_string_lossy().into_owned())
        .unwrap_or_default();

    rsx! {
    LoadingOverlay {
        is_visible: is_running(),
        style: LoadingStyle::Spinner,
        message: status_msg().to_string(),
        progress: 0.0,
    }

    div { class: "card",
        h2 { class: "page-title", "{texts.prune_title}" }

        div { class: "form-group",
            label { class: "label-text", "📋 Load Profile (Optional)" }
            div { class: "flex gap-3",
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "Select a profile",
                    value: "{profile_label}",
                    readonly: true
                }
                button {
                    class: "btn-icon",
                    onclick: handle_load_profile,
                    disabled: is_running(),
                    "📂"
                }
            }
            p { class: "hint", "Prunes the profile's repository (local or S3), like `rsb prune <profile>`." }
        }

        div { class: "form-group",
            label { class: "label-text", "{texts.prune_path_label}" }
            div { class: "flex gap-3",
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "{texts.prune_path_label}",
                    value: "{repo_path.read().to_string_lossy()}",
                    oninput: move |evt| {
                        profile.set(None);
                        repo_path.set(PathBuf::from(evt.value()));
                    }
                }
                button {
                    class: "btn-icon",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                                profile.set(None);
                                repo_path.set(handle.path().to_path_buf());
                            }
                        });
                    },
                    "📂"
                }
            }
            p { class: "hint", "The repository folder (`rsb prune --backup <dir>`). Empty: the S3 bucket configured in Settings." }
        }

        div { class: "form-group",
            label { class: "label-text", "{texts.key_label_opt}" }
            input {
                class: "input-field",
                r#type: "password",
                placeholder: "{texts.key_label_opt}",
                value: "{key}",
                oninput: move |evt| key.set(evt.value())
            }
            p { class: "hint", "Needed for encrypted repositories. Blank: RSB_ENCRYPTION_KEY or the profile's key in the OS keyring is used." }
        }

        div { class: "form-group",
            label { class: "label-text", "Retention" }
            div { class: "flex items-center gap-4",
                select {
                    class: "select-field",
                    onchange: move |evt| retention_mode.set(evt.value()),
                    option { value: "count", selected: retention_mode() == "count", "Keep the last N snapshots (-l)" }
                    option { value: "days", selected: retention_mode() == "days", "Keep the snapshots of the last N days" }
                }
                input {
                    class: "input-field",
                    r#type: "number",
                    min: "1",
                    value: "{retention_value}",
                    oninput: move |evt| retention_value.set(evt.value())
                }
            }
            p { class: "hint", "The newest snapshot is never deleted. Data no longer referenced by any snapshot is removed too." }
        }

        div { class: "form-group",
            label { class: "label-text",
                input {
                    r#type: "checkbox",
                    checked: confirm_delete(),
                    onchange: move |_| confirm_delete.toggle()
                }
                " I understand that pruning permanently deletes snapshots and data (--confirm)"
            }
        }

        div { class: "flex gap-3 mb-4",
            button {
                class: "flex-1 btn-secondary",
                onclick: move |_| start(true),
                disabled: is_running(),
                "🧪 Preview (--dry-run)"
            }
            button {
                class: "flex-1 btn-warning",
                onclick: move |_| start(false),
                disabled: is_running() || !confirm_delete(),
                if is_running() { "{texts.executing}..." } else { "{texts.start_prune}" }
            }
        }

        div { class: "status-box mt-6",
            p { class: "font-semibold mb-2", "Status:" }
            p { class: "whitespace-pre-wrap", "{status_msg}" }
        }
    }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsb_sdk::storage::{LocalStorage, Storage};

    #[test]
    fn retention_is_validated() {
        assert_eq!(parse_retention("count", "3"), Ok(Retention::KeepLast(3)));
        assert_eq!(parse_retention("days", " 30 "), Ok(Retention::Days(30)));
        assert!(parse_retention("count", "0").is_err());
        assert!(parse_retention("days", "abc").is_err());
        assert!(parse_retention("days", "").is_err());
    }

    #[test]
    fn plan_text() {
        assert!(describe_plan(&[], true).contains("Nothing to prune"));
        let doomed = vec!["snapshots/2026-01-01T000000Z.toml".to_string()];
        let preview = describe_plan(&doomed, true);
        assert!(preview.contains("would be deleted") && preview.contains("2026-01-01T000000Z"));
        assert!(describe_plan(&doomed, false).contains("Pruned 1"));
    }

    #[tokio::test]
    async fn encrypted_repository_is_not_pruned_without_key() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        let storage = LocalStorage::new(repo.to_str().unwrap());
        for s in ["2026-01-01T000000Z", "2026-01-02T000000Z"] {
            storage
                .write(&format!("snapshots/{}.toml", s), b"")
                .await
                .unwrap();
        }
        storage.write("keys/repo.key", b"{}").await.unwrap();
        std::fs::create_dir_all(repo.join("data")).unwrap();
        let cfg = rsb_sdk::config_from_backup_path(repo, None).unwrap();

        // Preview works without a key...
        let plan = run_prune(cfg.clone(), None, Retention::KeepLast(1), true)
            .await
            .unwrap();
        assert_eq!(plan.len(), 1);
        // ...but deleting is refused before anything is touched.
        let err = run_prune(cfg, None, Retention::KeepLast(1), false)
            .await
            .unwrap_err();
        assert!(err.contains("encrypted"));
        assert!(repo.join("snapshots/2026-01-01T000000Z.toml").exists());
    }
}
