use dioxus::prelude::*;
use std::fs;
use std::path::PathBuf;

use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    shared::ProgressBar,
};

#[derive(Clone, Debug)]
struct IntegrityStatus {
    backup_path: String,
    status: String,
    is_valid: bool,
    snapshots_count: usize,
    data_files_count: usize,
    encrypted_files_count: usize,
    issues: Vec<String>,
    suggestions: Vec<String>,
}

#[component]
pub fn BackupIntegrityScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut backup_path = use_signal(|| PathBuf::new());
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
            // Simulating progress
            progress.set(0.2);
            status_msg.set("🔍 Counting snapshots...".to_string());

            let snapshots_dir = path.join("snapshots");
            let data_dir = path.join("data");

            // Verificar estrutura básica
            let structure_valid = snapshots_dir.exists() && data_dir.exists();

            let mut issues = Vec::new();
            let mut suggestions = Vec::new();

            if !snapshots_dir.exists() {
                issues.push("❌ 'snapshots/' directory not found".to_string());
                suggestions.push("Verify that the entire folder was copied".to_string());
            }
            if !data_dir.exists() {
                issues.push("❌ 'data/' directory not found".to_string());
                suggestions
                    .push("Re-copy the complete backup from the original computer".to_string());
            }

            progress.set(0.4);
            status_msg.set("📊 Analyzing content...".to_string());

            // Contar snapshots
            let snapshots_count = fs::read_dir(&snapshots_dir)
                .ok()
                .map(|e| e.count())
                .unwrap_or(0);

            // Contar arquivos de dados
            let mut data_files_count = 0;
            let mut encrypted_files_count = 0;

            if data_dir.exists() {
                if let Ok(entries) = fs::read_dir(&data_dir) {
                    for entry in entries.flatten() {
                        if let Ok(metadata) = entry.metadata() {
                            if metadata.is_file() {
                                if entry.path().to_string_lossy().contains("enc") {
                                    encrypted_files_count += 1;
                                } else {
                                    data_files_count += 1;
                                }
                            }
                        }
                    }
                }
            }

            progress.set(0.8);

            if snapshots_count == 0 {
                issues.push("⚠️ No snapshots found".to_string());
                suggestions.push("The backup may be corrupted or incomplete".to_string());
            }

            progress.set(1.0);

            let is_valid = structure_valid && snapshots_count > 0;
            let status = if is_valid {
                format!(
                    "✅ Integrity Report\n📁 {}\nSnapshots: {}\nNormal Files: {}\nEncrypted Files: {}\n✅ Your backup is ready to be restored on another computer!",
                    path.to_string_lossy(),
                    snapshots_count,
                    data_files_count,
                    encrypted_files_count
                )
            } else {
                "❌ Backup is invalid or incomplete".to_string()
            };

            integrity_result.set(Some(IntegrityStatus {
                backup_path: path.to_string_lossy().to_string(),
                status,
                is_valid,
                snapshots_count,
                data_files_count,
                encrypted_files_count,
                issues,
                suggestions,
            }));

            status_msg.set(if is_valid {
                "✅ Verification completed successfully!".to_string()
            } else {
                "❌ Verification completed with issues".to_string()
            });

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
