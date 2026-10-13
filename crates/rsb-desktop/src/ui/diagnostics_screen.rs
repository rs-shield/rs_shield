use crate::ui::profile_loader::non_empty;
use crate::ui::{
    app::AppConfig,
    i18n::{KeyTexts, fill, get_key_texts, get_texts},
    loading_state::{LoadingOverlay, LoadingStyle},
};
use dioxus::prelude::*;
use rsb_sdk::backup::diagnostic::{BackupDiagnostics, missing_directories, run_backup_diagnostics};
use rsb_sdk::credentials::Fido2Manager;
use rsb_sdk::operation::operations::{Operation, OperationsManager};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Clone, Debug)]
struct DiagnosticInfo {
    category: String,
    status: DiagnosticStatus,
    message: String,
    details: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum DiagnosticStatus {
    Healthy,
    Warning,
    Critical,
}

impl DiagnosticStatus {
    fn icon(&self) -> &'static str {
        match self {
            DiagnosticStatus::Healthy => "✅",
            DiagnosticStatus::Warning => "⚠️",
            DiagnosticStatus::Critical => "❌",
        }
    }
}

/// One line of the report.
fn info(
    status: DiagnosticStatus,
    category: &str,
    message: String,
    details: Option<String>,
) -> DiagnosticInfo {
    DiagnosticInfo {
        category: category.to_string(),
        status,
        message,
        details,
    }
}

/// Runs `rsb diagnose --backup <dir>` (read-only) off the UI thread.
pub async fn diagnose_repository(
    path: PathBuf,
    key: Option<String>,
) -> Result<BackupDiagnostics, String> {
    let handle = tokio::runtime::Handle::current();
    tokio::task::spawn_blocking(move || {
        handle
            .block_on(run_backup_diagnostics(&path, key.as_deref(), false, false))
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}

fn path_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| p.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

/// What `rsb diagnose --repair` would do on `repo`: `Ok(text)` to confirm,
/// or `Err(reason)` when there is nothing to repair.
pub fn structure_repair_preview(repo: &Path, t: &KeyTexts) -> Result<String, String> {
    if !repo.is_dir() {
        return Err(fill(
            t.diag_path_not_found,
            &[("path", &repo.display().to_string())],
        ));
    }
    let missing = missing_directories(repo);
    if missing.is_empty() {
        return Err(t.diag_repair_none.to_string());
    }
    Ok(fill(
        t.diag_repair_confirm,
        &[("list", &path_list(&missing))],
    ))
}

/// Runs `rsb diagnose --repair` (creates the missing `snapshots/` and
/// `data/` directories) off the UI thread and says what was created.
pub async fn repair_structure(repo: PathBuf, t: &'static KeyTexts) -> Result<String, String> {
    if !repo.is_dir() {
        return Err(fill(
            t.diag_path_not_found,
            &[("path", &repo.display().to_string())],
        ));
    }
    let before = missing_directories(&repo);
    let handle = tokio::runtime::Handle::current();
    let path = repo.clone();
    tokio::task::spawn_blocking(move || {
        handle
            .block_on(run_backup_diagnostics(&path, None, false, true))
            .map(|_| ())
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    let still = missing_directories(&repo);
    let created: Vec<PathBuf> = before.into_iter().filter(|p| !still.contains(p)).collect();
    if !still.is_empty() {
        return Err(format!("❌ {}", path_list(&still)));
    }
    Ok(fill(t.diag_repair_done, &[("list", &path_list(&created))]))
}

/// Turns the SDK diagnostics of a repository into report lines (real
/// results; this screen used to show hard-coded "healthy" values).
fn repository_findings(d: &BackupDiagnostics) -> Vec<DiagnosticInfo> {
    let mut out = Vec::new();
    let det = &d.details;
    out.push(info(
        if det.structure_valid && det.snapshots_count > 0 {
            DiagnosticStatus::Healthy
        } else {
            DiagnosticStatus::Critical
        },
        "Repository",
        format!("{} — {}", d.backup_path, d.status),
        Some(format!(
            "Snapshots: {} | Data objects: {} clear, {} encrypted | {:.2} MB",
            det.snapshots_count, det.data_files_count, det.encrypted_files_count, det.total_size_mb
        )),
    ));
    for issue in &d.issues {
        out.push(info(
            DiagnosticStatus::Critical,
            "Issue",
            issue.trim().to_string(),
            None,
        ));
    }
    for warning in &d.warnings {
        out.push(info(
            DiagnosticStatus::Warning,
            "Warning",
            warning.trim().to_string(),
            None,
        ));
    }
    for s in &d.suggestions {
        out.push(info(
            DiagnosticStatus::Warning,
            "Suggestion",
            s.clone(),
            None,
        ));
    }
    out
}

/// FIDO2 status from the credentials actually registered.
fn fido2_finding(credentials: usize) -> DiagnosticInfo {
    if credentials == 0 {
        info(
            DiagnosticStatus::Warning,
            "FIDO2",
            "No FIDO2 security key registered".to_string(),
            Some("Register one with `rsb auth register`".to_string()),
        )
    } else {
        info(
            DiagnosticStatus::Healthy,
            "FIDO2",
            format!("{} security key(s) registered", credentials),
            None,
        )
    }
}

/// Last backup recorded in the operations history.
fn last_backup_finding(last: Option<&Operation>) -> DiagnosticInfo {
    match last {
        None => info(
            DiagnosticStatus::Warning,
            "Last Backup",
            "No backup recorded on this computer".to_string(),
            None,
        ),
        Some(op) => {
            let ok = op.status.eq_ignore_ascii_case("success");
            info(
                if ok {
                    DiagnosticStatus::Healthy
                } else {
                    DiagnosticStatus::Critical
                },
                "Last Backup",
                format!("{} — {}", op.timestamp, op.status),
                Some(format!(
                    "{} file(s) | {} → {}",
                    op.files_processed,
                    op.source_path.clone().unwrap_or_default(),
                    op.destination_path.clone().unwrap_or_default()
                )),
            )
        }
    }
}

#[component]
pub fn DiagnosticsScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let kt = get_key_texts(app_config.language());
    let fido2 = use_context::<Arc<Mutex<Fido2Manager>>>();

    let mut diagnostics = use_signal(Vec::<DiagnosticInfo>::new);
    let mut is_loading = use_signal(|| false);
    let mut last_run = use_signal(|| Option::<String>::None);
    let mut repo_path = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut repair_pending = use_signal(|| Option::<String>::None);
    let mut repair_msg = use_signal(String::new);

    let run_diagnostics = use_callback(move |_: ()| {
        is_loading.set(true);
        let fido2 = fido2.clone();
        let repo = repo_path.peek().clone();
        let key_val = non_empty(&key.peek());
        spawn(async move {
            let mut diags = Vec::new();
            if repo.as_os_str().is_empty() {
                diags.push(info(
                    DiagnosticStatus::Warning,
                    "Repository",
                    "No backup folder selected".to_string(),
                    Some(
                        "Select a backup folder to check it (rsb diagnose --backup <dir>)"
                            .to_string(),
                    ),
                ));
            } else {
                match diagnose_repository(repo, key_val).await {
                    Ok(d) => diags.extend(repository_findings(&d)),
                    Err(e) => diags.push(info(DiagnosticStatus::Critical, "Repository", e, None)),
                }
            }
            let credentials = fido2.lock().await.list_credentials().len();
            diags.push(fido2_finding(credentials));
            let last = tokio::task::spawn_blocking(|| {
                OperationsManager::new()
                    .get_history()
                    .operations
                    .iter()
                    .find(|o| o.operation_type == "Backup")
                    .cloned()
            })
            .await
            .ok()
            .flatten();
            diags.push(last_backup_finding(last.as_ref()));

            diagnostics.set(diags);
            last_run.set(Some(chrono::Local::now().format("%H:%M:%S").to_string()));
            is_loading.set(false);
        });
    });

    use_effect(move || {
        run_diagnostics.call(());
    });

    rsx! {
        div { class: "space-y-4",
            // Loading overlay
            LoadingOverlay {
                is_visible: is_loading(),
                style: LoadingStyle::Spinner,
                message: texts.diagnostics_running.to_string(),
            }

            // Header
                div { class: "flex flex-col sm:flex-row sm:justify-between sm:items-center gap-2",
                    h2 { class: "text-lg font-bold text-slate-900 dark:text-white", "🔧 {texts.diagnostics_title}" }
                    button {
                        class: "btn-primary py-1.5 px-2 flex items-center gap-1 text-sm",
                    onclick: move |_| run_diagnostics.call(()),
                    disabled: is_loading(),
                    if is_loading() { "{texts.diagnostics_running}" } else { "{texts.diagnostics_run}" }
                }
            }

            // Last run info
            if let Some(run_time) = last_run.read().as_ref() {
                div { class: "text-xs text-slate-500 dark:text-slate-400", "Last run: {run_time}" }
            }

            div { class: "form-group",
                label { class: "label-text", "Backup folder (rsb diagnose --backup)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Select the repository folder",
                        value: "{repo_path.read().to_string_lossy()}",
                        oninput: move |evt| repo_path.set(PathBuf::from(evt.value()))
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    repo_path.set(h.path().to_path_buf());
                                    run_diagnostics.call(());
                                }
                            });
                        },
                        "📂"
                    }
                }
                input {
                    class: "input-field mt-2",
                    r#type: "password",
                    placeholder: "Encryption key (optional)",
                    value: "{key}",
                    oninput: move |evt| key.set(evt.value())
                }
                button {
                    class: "btn-secondary mt-2 text-sm",
                    disabled: is_loading() || repo_path.read().as_os_str().is_empty(),
                    onclick: move |_| {
                        repair_msg.set(String::new());
                        match structure_repair_preview(&repo_path.peek(), kt) {
                            Ok(text) => repair_pending.set(Some(text)),
                            Err(e) => {
                                repair_pending.set(None);
                                repair_msg.set(e);
                            }
                        }
                    },
                    "{kt.diag_repair_button}"
                }
                p { class: "hint", "{kt.diag_repair_local_only}" }
                if let Some(text) = repair_pending() {
                    div { class: "status-box mt-2 border-l-4 border-amber-500",
                        p { class: "font-semibold mb-2", "{kt.confirm_title}" }
                        p { class: "whitespace-pre-wrap mb-3", "{text}" }
                        div { class: "flex gap-3",
                            button {
                                class: "flex-1 btn-warning",
                                disabled: is_loading(),
                                onclick: move |_| {
                                    repair_pending.set(None);
                                    is_loading.set(true);
                                    let repo = repo_path.peek().clone();
                                    spawn(async move {
                                        let res = repair_structure(repo, kt).await;
                                        is_loading.set(false);
                                        repair_msg.set(match res {
                                            Ok(m) | Err(m) => m,
                                        });
                                        run_diagnostics.call(());
                                    });
                                },
                                "{kt.confirm_button}"
                            }
                            button {
                                class: "flex-1 btn-secondary",
                                onclick: move |_| repair_pending.set(None),
                                "{kt.cancel_button}"
                            }
                        }
                    }
                }
                if !repair_msg().is_empty() {
                    p { class: "text-sm mt-2 whitespace-pre-wrap", "{repair_msg}" }
                }
            }

            // Diagnostics summary
                div { class: "grid grid-cols-1 sm:grid-cols-3 gap-2",
                {
                    let healthy = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Healthy).count();
                    rsx! {
                        div { class: "p-3 bg-emerald-50 dark:bg-emerald-900/20 border border-emerald-200 dark:border-emerald-800/50 rounded-lg",
                               p { class: "text-xs font-semibold text-emerald-900 dark:text-emerald-100", "{texts.diagnostics_healthy}" }
                               p { class: "text-lg font-bold text-emerald-700 dark:text-emerald-400 mt-2", "{healthy}" }
                        }
                    }
                }

                {
                    let warnings = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Warning).count();
                    rsx! {
                        div { class: "p-4 bg-yellow-50 dark:bg-yellow-900/20 border border-yellow-200 dark:border-yellow-800/50 rounded-lg",
                               p { class: "text-xs font-semibold text-yellow-900 dark:text-yellow-100", "{texts.diagnostics_warnings}" }
                               p { class: "text-lg font-bold text-yellow-700 dark:text-yellow-400 mt-2", "{warnings}" }
                        }
                    }
                }

                {
                    let critical = diagnostics.read().iter().filter(|d| d.status == DiagnosticStatus::Critical).count();
                    rsx! {
                        div { class: "p-4 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800/50 rounded-lg",
                               p { class: "text-xs font-semibold text-red-900 dark:text-red-100", "{texts.diagnostics_critical}" }
                               p { class: "text-lg font-bold text-red-700 dark:text-red-400 mt-2", "{critical}" }
                        }
                    }
                }
            }

            // Diagnostics details
            div { class: "space-y-4",
                for diag in diagnostics.read().iter() {
                    {
                        let icon = diag.status.icon();
                        let category = diag.category.clone();
                        let message = diag.message.clone();
                        let details = diag.details.clone();

                        rsx! {
                                div {
                                    key: "{category}",
                                    class: "p-3 border rounded-lg transition-all",
                                    class: if diag.status == DiagnosticStatus::Healthy {
                                    "bg-emerald-50 dark:bg-emerald-900/20 border-emerald-200 dark:border-emerald-800/50"
                                } else if diag.status == DiagnosticStatus::Warning {
                                    "bg-yellow-50 dark:bg-yellow-900/20 border-yellow-200 dark:border-yellow-800/50"
                                } else {
                                    "bg-red-50 dark:bg-red-900/20 border-red-200 dark:border-red-800/50"
                                },
                                div { class: "flex items-start gap-2",
                                    span { class: "text-lg flex-shrink-0 mt-1", "{icon}" }
                                    div { class: "flex-1",
                                        h3 { class: "font-semibold text-sm", class: if diag.status == DiagnosticStatus::Healthy {
                                            "text-emerald-900 dark:text-emerald-100"
                                        } else if diag.status == DiagnosticStatus::Warning {
                                            "text-yellow-900 dark:text-yellow-100"
                                        } else {
                                            "text-red-900 dark:text-red-100"
                                        }, "{category}" }
                                        p { class: "text-sm mt-1", class: if diag.status == DiagnosticStatus::Healthy {
                                            "text-emerald-700 dark:text-emerald-200"
                                        } else if diag.status == DiagnosticStatus::Warning {
                                            "text-yellow-700 dark:text-yellow-200"
                                        } else {
                                            "text-red-700 dark:text-red-200"
                                        }, "{message}" }
                                        if let Some(detail) = details {
                                            p { class: "text-xs mt-2 opacity-75", class: if diag.status == DiagnosticStatus::Healthy {
                                                "text-emerald-600 dark:text-emerald-300"
                                            } else if diag.status == DiagnosticStatus::Warning {
                                                "text-yellow-600 dark:text-yellow-300"
                                            } else {
                                                "text-red-600 dark:text-red-300"
                                            }, "{detail}" }
                                        }
                                    }
                                }
                            }
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
    async fn findings_reflect_the_real_repository() {
        let dir = tempfile::tempdir().unwrap();
        let empty = repository_findings(
            &diagnose_repository(dir.path().to_path_buf(), None)
                .await
                .unwrap(),
        );
        assert_eq!(empty[0].status, DiagnosticStatus::Critical);

        let repo = dir.path();
        std::fs::create_dir_all(repo.join("snapshots")).unwrap();
        std::fs::create_dir_all(repo.join("data/enc")).unwrap();
        std::fs::write(repo.join("snapshots/2026-01-01T000000Z.toml"), "").unwrap();
        std::fs::write(repo.join("snapshots/.x.toml.rsb-tmp-1"), "").unwrap();
        std::fs::write(repo.join("data/enc/abc"), "x").unwrap();
        let d = diagnose_repository(repo.to_path_buf(), None).await.unwrap();
        assert_eq!(d.details.snapshots_count, 1, "temp files are not snapshots");
        let f = repository_findings(&d);
        assert_eq!(f[0].status, DiagnosticStatus::Healthy);
        // Encrypted data without a key is flagged.
        assert!(f.iter().any(|i| i.status == DiagnosticStatus::Warning));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn structure_repair_previews_then_creates_the_missing_directories() {
        use crate::ui::i18n::EN_KEY_TEXTS;
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        // A path that does not exist is never created.
        assert!(structure_repair_preview(&repo, &EN_KEY_TEXTS).is_err());
        assert!(repair_structure(repo.clone(), &EN_KEY_TEXTS).await.is_err());
        assert!(!repo.exists());

        std::fs::create_dir_all(repo.join("snapshots")).unwrap();
        let preview = structure_repair_preview(&repo, &EN_KEY_TEXTS).unwrap();
        assert!(preview.contains("data"), "{preview}");
        assert!(!repo.join("data").exists(), "the preview changes nothing");
        let done = repair_structure(repo.clone(), &EN_KEY_TEXTS).await.unwrap();
        assert!(done.contains("data"), "{done}");
        assert!(repo.join("data").is_dir());
        assert_eq!(
            structure_repair_preview(&repo, &EN_KEY_TEXTS).unwrap_err(),
            EN_KEY_TEXTS.diag_repair_none
        );
    }

    #[test]
    fn fido2_and_history_findings() {
        assert_eq!(fido2_finding(0).status, DiagnosticStatus::Warning);
        assert_eq!(fido2_finding(2).status, DiagnosticStatus::Healthy);
        assert_eq!(last_backup_finding(None).status, DiagnosticStatus::Warning);
        let op = Operation {
            id: "1".into(),
            operation_type: "Backup".into(),
            timestamp: "2026-10-01 10:00:00".into(),
            duration_secs: 1,
            status: "Failed".into(),
            files_processed: 0,
            files_skipped: 0,
            files_with_errors: 0,
            backup_size: "N/A".into(),
            source_path: None,
            destination_path: None,
        };
        assert_eq!(
            last_backup_finding(Some(&op)).status,
            DiagnosticStatus::Critical
        );
    }
}
