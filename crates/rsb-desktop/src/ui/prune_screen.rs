use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::{Translations, get_texts, tr},
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{
        LoadedProfile, load_profile, repository_config, repository_label, resolve_key_blocking,
    },
};
use rsb_sdk::operation::operations_helpers::record_prune_operation;

use dioxus::prelude::*;
use rsb_sdk::core::{self, PruneStats};
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
pub fn parse_retention(t: &Translations, mode: &str, value: &str) -> Result<Retention, String> {
    let v = value.trim();
    let n: u32 = v
        .parse()
        .ok()
        .filter(|n| *n >= 1)
        .ok_or_else(|| tr(t.pr_invalid_retention, &[&v]))?;
    match mode {
        "days" => Ok(Retention::Days(n.min(36_500))),
        _ => Ok(Retention::KeepLast(n as usize)),
    }
}

/// Text of a prune plan / result. `stats` (from the SDK's `preview_prune`
/// or `prune_with_stats`) adds the data objects; it is `None` for a
/// preview of an encrypted repository without its key, whose manifests
/// cannot be read.
pub fn describe_plan(
    t: &Translations,
    doomed: &[String],
    stats: Option<&PruneStats>,
    dry_run: bool,
) -> String {
    if doomed.is_empty() {
        return t.pr_nothing_to_prune.to_string();
    }
    let objects = match stats {
        Some(s) if s.gc_ran => tr(t.pr_unreferenced_objects, &[&s.objects_deleted]),
        Some(_) => t.pr_no_data_objects.to_string(),
        None => t.pr_objects_unknown.to_string(),
    };
    let mut msg = if dry_run {
        tr(t.pr_preview_result, &[&doomed.len(), &objects])
    } else {
        tr(
            t.pr_pruned_result,
            &[
                &stats.map_or(doomed.len(), |s| s.snapshots_deleted),
                &objects,
            ],
        )
    };
    for s in doomed.iter().take(20) {
        msg.push_str("\n  • ");
        msg.push_str(s.trim_start_matches("snapshots/").trim_end_matches(".toml"));
    }
    if doomed.len() > 20 {
        msg.push_str(&format!(
            "\n  {}",
            tr(t.vf_more_errors, &[&(doomed.len() - 20)])
        ));
    }
    msg
}

/// Plans the prune (snapshots to delete for `retention`), then previews it
/// with the SDK's `preview_prune` (dry run) or runs `prune_with_stats`.
async fn run_prune(
    t: &Translations,
    cfg: rsb_sdk::config::Config,
    key: Option<String>,
    retention: Retention,
    dry_run: bool,
) -> Result<(Vec<String>, Option<PruneStats>), String> {
    let storage = core::storage_backend::get_storage(&cfg)
        .await
        .map_err(|e| e.to_string())?;
    let doomed = match retention {
        Retention::KeepLast(n) => core::plan_prune_by_count_with_storage(&*storage, n).await,
        Retention::Days(d) => core::plan_prune_by_age_with_storage(&*storage, d).await,
    }
    .map_err(|e| e.to_string())?;
    if doomed.is_empty() {
        return Ok((doomed, None));
    }
    // Same refusal as `rsb prune`: an encrypted repository cannot be
    // garbage-collected (nor its objects counted) without its key.
    let encrypted = rsb_sdk::crypto::keyfile::repo_key_exists(&*storage)
        .await
        .map_err(|e| e.to_string())?
        || cfg.encryption_required();
    if encrypted && key.is_none() {
        if dry_run {
            return Ok((doomed, None));
        }
        return Err(tr(
            t.pr_encrypted_no_key,
            &[&rsb_sdk::config::ENCRYPTION_KEY_ENV],
        ));
    }
    let stats = if dry_run {
        core::preview_prune(&*storage, key.as_deref(), &doomed).await
    } else {
        core::prune_with_stats(storage, key.as_deref(), &doomed).await
    }
    .map_err(|e| e.to_string())?;
    Ok((doomed, Some(stats)))
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
                match load_profile(h.path(), texts) {
                    Ok(p) => {
                        repo_path.set(PathBuf::from(repository_label(&p.config)));
                        status_msg.set(tr(texts.vf_profile_loaded, &[&p.path.display()]));
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
            status_msg.set(texts.pr_confirm_required.to_string());
            return;
        }
        let retention = match parse_retention(texts, &retention_mode(), &retention_value()) {
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
            let cfg = repository_config(loaded.as_ref(), &path, &s3, texts);
            let label = cfg.as_ref().map(repository_label).unwrap_or_default();
            let started = std::time::Instant::now();
            let result = async {
                let cfg = cfg?;
                let key = resolve_key_blocking(typed_key, loaded.clone()).await;
                run_prune(texts, cfg, key, retention, dry_run).await
            }
            .await;

            match result {
                Ok((doomed, stats)) => {
                    status_msg.set(describe_plan(texts, &doomed, stats.as_ref(), dry_run));
                    if !dry_run {
                        let _ = record_prune_operation(
                            "Success".to_string(),
                            stats.map_or(0, |s| s.snapshots_deleted),
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
            label { class: "label-text", "{texts.vf_load_profile_label}" }
            div { class: "flex gap-3",
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "{texts.vf_select_profile}",
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
            p { class: "hint", "{texts.pr_profile_hint}" }
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
            p { class: "hint", "{texts.pr_backup_hint}" }
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
            p { class: "hint", "{texts.pr_key_hint}" }
        }

        div { class: "form-group",
            label { class: "label-text", "{texts.pr_retention_label}" }
            div { class: "flex items-center gap-4",
                select {
                    class: "select-field",
                    onchange: move |evt| retention_mode.set(evt.value()),
                    option { value: "count", selected: retention_mode() == "count", "{texts.pr_keep_last}" }
                    option { value: "days", selected: retention_mode() == "days", "{texts.pr_keep_days}" }
                }
                input {
                    class: "input-field",
                    r#type: "number",
                    min: "1",
                    value: "{retention_value}",
                    oninput: move |evt| retention_value.set(evt.value())
                }
            }
            p { class: "hint", "{texts.pr_retention_hint}" }
        }

        div { class: "form-group",
            label { class: "label-text",
                input {
                    r#type: "checkbox",
                    checked: confirm_delete(),
                    onchange: move |_| confirm_delete.toggle()
                }
                " {texts.pr_confirm_label}"
            }
        }

        div { class: "flex gap-3 mb-4",
            button {
                class: "flex-1 btn-secondary",
                onclick: move |_| start(true),
                disabled: is_running(),
                "{texts.pr_preview_button}"
            }
            button {
                class: "flex-1 btn-warning",
                onclick: move |_| start(false),
                disabled: is_running() || !confirm_delete(),
                if is_running() { "{texts.executing}..." } else { "{texts.start_prune}" }
            }
        }

        div { class: "status-box mt-6",
            p { class: "font-semibold mb-2", "{texts.status_label}" }
            p { class: "whitespace-pre-wrap", "{status_msg}" }
        }
    }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::Language;
    use rsb_sdk::storage::{LocalStorage, Storage};

    #[test]
    fn retention_is_validated() {
        let t = get_texts(Language::En);
        assert_eq!(parse_retention(t, "count", "3"), Ok(Retention::KeepLast(3)));
        assert_eq!(parse_retention(t, "days", " 30 "), Ok(Retention::Days(30)));
        assert!(parse_retention(t, "count", "0").is_err());
        assert!(parse_retention(t, "days", "abc").is_err());
        assert!(parse_retention(t, "days", "").is_err());
    }

    #[test]
    fn plan_text() {
        let t = get_texts(Language::En);
        assert!(describe_plan(t, &[], None, true).contains("Nothing to prune"));
        let doomed = vec!["snapshots/2026-01-01T000000Z.toml".to_string()];
        let stats = PruneStats {
            snapshots_deleted: 1,
            objects_deleted: 3,
            gc_ran: true,
        };
        let preview = describe_plan(t, &doomed, Some(&stats), true);
        assert!(preview.contains("would be deleted") && preview.contains("2026-01-01T000000Z"));
        assert!(preview.contains("3 unreferenced"));
        assert!(
            describe_plan(t, &doomed, None, true).contains("unknown without the encryption key")
        );
        assert!(describe_plan(t, &doomed, Some(&stats), false).contains("Pruned 1"));
    }

    #[tokio::test]
    async fn encrypted_repository_is_not_pruned_without_key() {
        let t = get_texts(Language::En);
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
        let (plan, stats) = run_prune(t, cfg.clone(), None, Retention::KeepLast(1), true)
            .await
            .unwrap();
        assert_eq!(plan.len(), 1);
        assert!(stats.is_none());
        // ...but deleting is refused before anything is touched.
        let err = run_prune(t, cfg, None, Retention::KeepLast(1), false)
            .await
            .unwrap_err();
        assert!(err.contains("encrypted"));
        assert!(repo.join("snapshots/2026-01-01T000000Z.toml").exists());
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    use crate::ui::i18n::Language;

    #[tokio::test]
    async fn preview_counts_with_the_sdk_and_deletes_nothing() {
        let t = get_texts(Language::En);
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("snapshots")).unwrap();
        std::fs::create_dir_all(repo.join("data/clear")).unwrap();
        let keep = "a".repeat(64);
        std::fs::write(repo.join("snapshots/2026-01-01T000000Z.toml"), "").unwrap();
        std::fs::write(repo.join("snapshots/2026-01-02T000000Z.toml"), {
            let mut m = std::collections::HashMap::new();
            m.insert(
                std::path::PathBuf::from("f"),
                rsb_sdk::core::types::FileMetadata::new(keep.clone(), false, false),
            );
            toml::to_string(&m).unwrap()
        })
        .unwrap();
        std::fs::write(repo.join("data/clear").join(&keep), "k").unwrap();
        std::fs::write(repo.join("data/clear").join("b".repeat(64)), "d").unwrap();
        let cfg = rsb_sdk::config_from_backup_path(repo, None).unwrap();

        let (plan, stats) = run_prune(t, cfg.clone(), None, Retention::KeepLast(1), true)
            .await
            .unwrap();
        assert_eq!(plan.len(), 1);
        assert_eq!(stats.map(|s| s.objects_deleted), Some(1));
        assert!(repo.join("snapshots/2026-01-01T000000Z.toml").exists());

        let (_, stats) = run_prune(t, cfg, None, Retention::KeepLast(1), false)
            .await
            .unwrap();
        assert_eq!(stats.map(|s| s.snapshots_deleted), Some(1));
        assert!(!repo.join("snapshots/2026-01-01T000000Z.toml").exists());
        assert!(repo.join("data/clear").join(&keep).exists());
    }
}
