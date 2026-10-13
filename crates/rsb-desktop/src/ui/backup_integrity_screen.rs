use dioxus::prelude::*;
use std::path::PathBuf;

use crate::ui::diagnostics_screen::diagnose_repository;
use rsb_sdk::BackupValidation;
use rsb_sdk::backup::diagnostic::BackupDiagnostics;
use std::path::Path;

use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    shared::ProgressBar,
};

#[derive(Clone, Debug)]
struct IntegrityStatus {
    backup_path: String,
    #[allow(dead_code)]
    status: String,
    is_valid: bool,
    snapshots_count: usize,
    data_files_count: usize,
    encrypted_files_count: usize,
    issues: Vec<String>,
    suggestions: Vec<String>,
}

/// Structure check of a repository folder with the same SDK validation as
/// `rsb restore --backup` (snapshot manifests only; objects counted under
/// `data/clear` and `data/enc`). The old screen only looked at files
/// directly inside `data/` and so always reported 0 data files.
fn integrity_status(
    path: &Path,
    v: &BackupValidation,
    d: Option<&BackupDiagnostics>,
) -> IntegrityStatus {
    let encrypted = d.map(|d| d.details.encrypted_files_count).unwrap_or(0);
    let mut issues: Vec<String> = v.issues.iter().map(|i| format!("❌ {}", i)).collect();
    issues.extend(v.warnings.iter().map(|w| format!("⚠️ {}", w)));
    IntegrityStatus {
        backup_path: path.to_string_lossy().to_string(),
        status: if v.is_valid { "valid" } else { "invalid" }.to_string(),
        is_valid: v.is_valid,
        snapshots_count: v.snapshots_count,
        data_files_count: v.data_files_count.saturating_sub(encrypted),
        encrypted_files_count: encrypted,
        issues,
        suggestions: v.suggestions.clone(),
    }
}

#[component]
pub fn BackupIntegrityScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut backup_path = use_signal(PathBuf::new);
    let mut status_msg = use_signal(|| "🔍 Select a backup folder".to_string());
    let mut is_checking = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    let mut integrity_result = use_signal(|| Option::<IntegrityStatus>::None);

    let handle_select_backup = move |_| {
        spawn(async move {
            if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                backup_path.set(handle.path().to_path_buf());
                status_msg.set("✅ Folder selected. Click 'Verify Integrity'".to_string());
                integrity_result.set(None);
            }
        });
    };

    let handle_check_integrity = move |_| {
        if is_checking() {
            return;
        }

        let path = backup_path();
        if path.as_os_str().is_empty() {
            status_msg.set("❌ Select a backup folder".to_string());
            return;
        }

        is_checking.set(true);
        progress.set(0.0);
        status_msg.set("🔍 Checking backup structure...".to_string());

        spawn(async move {
            progress.set(0.3);
            status_msg.set("🔍 Checking backup structure...".to_string());
            let p = path.clone();
            let validation =
                tokio::task::spawn_blocking(move || rsb_sdk::validate_backup_structure(&p))
                    .await
                    .map_err(|e| e.to_string());
            progress.set(0.6);
            let diagnostics = diagnose_repository(path.clone(), None).await;
            progress.set(1.0);
            match validation {
                Ok(v) => {
                    let result = integrity_status(&path, &v, diagnostics.ok().as_ref());
                    status_msg.set(if result.is_valid {
                        "✅ Verification completed successfully!".to_string()
                    } else {
                        "❌ Verification completed with issues".to_string()
                    });
                    integrity_result.set(Some(result));
                }
                Err(e) => status_msg.set(format!("❌ {}", e)),
            }
            is_checking.set(false);
        });
    };

    rsx! {
        div { class: "tab-content",
            // Loading overlay
            LoadingOverlay {
                is_visible: is_checking(),
                style: LoadingStyle::ProgressBar,
                message: status_msg().to_string(),
                progress: progress(),
            }

            h2 { class: "tab-title", "{texts.integrity_report_title}" }

            div { class: "form-group",
                label { class: "label-text", "Backup Folder" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Select the backup folder",
                        value: "{backup_path.read().to_string_lossy()}",
                        disabled: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_backup,
                        "📂"
                    }
                }
            }

            div { class: "status-message",
                "{status_msg()}"
            }

            if is_checking() || progress() > 0.0 && progress() < 1.0 {
                div {
                    ProgressBar { progress: progress() }
                }
            }

            button {
                class: "w-full btn-primary mb-4",
                onclick: handle_check_integrity,
                disabled: is_checking() || backup_path.read().as_os_str().is_empty(),
                "🔍 Verify Integrity"
            }

            if let Some(result) = integrity_result() {
                div { class: "integrity-report",
                    h3 { class: "report-title",
                        if result.is_valid { "{texts.integrity_report_title}" } else { "❌ Issues Detected" }
                    }

                    div { class: "report-path",
                        "📁 {result.backup_path}"
                    }

                    div { class: "report-stats",
                        div { class: "stat-item",
                            span { class: "stat-label", "{texts.integrity_snapshots_count}:" }
                            span { class: "stat-value", "{result.snapshots_count}" }
                        }
                        div { class: "stat-item",
                            span { class: "stat-label", "{texts.integrity_normal_files}:" }
                            span { class: "stat-value", "{result.data_files_count}" }
                        }
                        div { class: "stat-item",
                            span { class: "stat-label", "{texts.integrity_encrypted_files}:" }
                            span { class: "stat-value", "{result.encrypted_files_count}" }
                        }
                    }

                    if !result.issues.is_empty() {
                        div { class: "issues-section",
                            h4 { class: "issues-title", "🔴 Issues:" }
                            ul { class: "issues-list",
                                for issue in result.issues.iter() {
                                    li { "{issue}" }
                                }
                            }
                        }
                    }

                    if !result.suggestions.is_empty() {
                        div { class: "suggestions-section",
                            h4 { class: "suggestions-title", "💡 Suggestions:" }
                            ul { class: "suggestions-list",
                                for suggestion in result.suggestions.iter() {
                                    li { "• {suggestion}" }
                                }
                            }
                        }
                    }

                    if result.is_valid {
                        div { class: "success-message",
                            "{texts.integrity_ready_to_restore}"
                        }
                    } else {
                        div { class: "warning-message",
                            "⚠️ Resolve the issues above before attempting to restore"
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn counts_objects_in_the_real_layout() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        std::fs::create_dir_all(repo.join("snapshots")).unwrap();
        std::fs::create_dir_all(repo.join("data/clear")).unwrap();
        std::fs::create_dir_all(repo.join("data/enc")).unwrap();
        std::fs::write(repo.join("snapshots/2026-01-01T000000Z.toml"), "").unwrap();
        std::fs::write(repo.join("data/clear/aa"), "x").unwrap();
        std::fs::write(repo.join("data/enc/bb"), "x").unwrap();
        std::fs::write(repo.join("data/enc/cc"), "x").unwrap();
        let v = rsb_sdk::validate_backup_structure(repo);
        let d = diagnose_repository(repo.to_path_buf(), None).await.unwrap();
        let s = integrity_status(repo, &v, Some(&d));
        assert!(s.is_valid);
        assert_eq!(
            (
                s.snapshots_count,
                s.data_files_count,
                s.encrypted_files_count
            ),
            (1, 1, 2)
        );

        let empty = tempfile::tempdir().unwrap();
        let v = rsb_sdk::validate_backup_structure(empty.path());
        let s = integrity_status(empty.path(), &v, None);
        assert!(!s.is_valid && !s.issues.is_empty());
    }
}
