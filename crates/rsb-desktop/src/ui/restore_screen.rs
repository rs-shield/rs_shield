use dioxus::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

use rsb_sdk::core::NotificationEvent;
use rsb_sdk::operation::operations_helpers::record_restore_operation;
use rsb_sdk::{CancellationToken, RestoreOptions, VerifyOptions, perform_verify};

use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    ops,
    profile_loader::{
        LoadedProfile, load_profile, repository_config, repository_label, resolve_key_blocking,
    },
    shared::ProgressBar,
};

/// What to restore, as typed on screen.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RestoreSelection {
    pub snapshot_id: Option<String>,
    pub at_or_before: Option<chrono::DateTime<chrono::Utc>>,
    pub include: Vec<String>,
}

/// Validates the snapshot / date / file fields exactly like `rsb restore
/// --snapshot/--date/--files` (before the repository is touched).
pub fn restore_selection(
    snapshot: &str,
    date: &str,
    files: &str,
) -> Result<RestoreSelection, String> {
    let snapshot_id = Some(snapshot.trim().to_string()).filter(|s| !s.is_empty());
    let at_or_before = if date.trim().is_empty() {
        None
    } else {
        Some(rsb_sdk::core::parse_restore_date(date).map_err(|e| e.to_string())?)
    };
    if at_or_before.is_some() && snapshot_id.is_some() {
        return Err("Give either a snapshot ID or a date, not both.".to_string());
    }
    let include = include_patterns(files);
    rsb_sdk::core::FileSelector::new(&include).map_err(|e| e.to_string())?;
    Ok(RestoreSelection {
        snapshot_id,
        at_or_before,
        include,
    })
}

#[component]
pub fn RestoreScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut source_path = use_signal(PathBuf::new);
    let mut restore_to = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut snapshot_id = use_signal(String::new);
    let mut include_text = use_signal(String::new);
    let mut date_text = use_signal(String::new);
    let mut generate_report = use_signal(|| false);
    let mut overwrite = use_signal(|| false);
    let mut versioned = use_signal(|| false);
    let mut verify_after = use_signal(|| false);
    let mut last_report_path = use_signal(|| Option::<PathBuf>::None);

    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut is_running = use_signal(|| false);
    let mut progress = use_signal(|| 0.0);
    let mut cancellation_token = use_signal(CancellationToken::new);

    let handle_load_profile = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new()
                .add_filter("TOML", &["toml"])
                .pick_file()
                .await
            {
                match load_profile(h.path()) {
                    Ok(p) => {
                        source_path.set(PathBuf::from(repository_label(&p.config)));
                        status_msg.set(format!("✅ Profile loaded: {}", p.path.display()));
                        profile.set(Some(p));
                    }
                    Err(e) => status_msg.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let handle_load_backup_directly = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                let backup_path = h.path().to_path_buf();
                let path = backup_path.clone();
                let validation =
                    tokio::task::spawn_blocking(move || rsb_sdk::validate_backup_structure(&path))
                        .await;
                let Ok(validation) = validation else {
                    status_msg.set("❌ Could not read the backup folder".to_string());
                    return;
                };

                if !validation.is_valid {
                    let mut error_msg = format!("{}\n", texts.backup_validation_failed);
                    for issue in &validation.issues {
                        error_msg.push_str(&format!("  ❌ {}\n", issue));
                    }
                    for suggestion in &validation.suggestions {
                        error_msg.push_str(&format!("  💡 {}\n", suggestion));
                    }
                    status_msg.set(error_msg);
                    return;
                }

                profile.set(None);
                source_path.set(backup_path);
                status_msg.set(format!(
                    "{}\n📊 {} {} | {} {}",
                    texts.portable_backup_loaded,
                    validation.snapshots_count,
                    texts.snapshots_found,
                    validation.data_files_count,
                    texts.data_files_found
                ));
            }
        });
    };

    let handle_restore = move |_| {
        if is_running() {
            return;
        }
        is_running.set(true);
        let token = CancellationToken::new();
        cancellation_token.set(token.clone());
        last_report_path.set(None);
        progress.set(0.0);
        status_msg.set(texts.starting.to_string());

        let create_report = generate_report();
        let loaded = profile();
        let bkp = source_path();
        let rst = restore_to();
        let selection = restore_selection(&snapshot_id(), &date_text(), &include_text());
        let typed_key = key();
        let force = overwrite();
        let versioned_mode = versioned();
        let verify_mode = verify_after();
        // Session credentials typed in Settings stay in memory only.
        let s3 = S3Settings::from_app(&app_config);

        spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();
            let progress_cb = Arc::new(move |current: usize, total: usize, msg: String| {
                let _ = tx.send((current as f64 / total.max(1) as f64, msg));
            });
            spawn(async move {
                while let Some((p, msg)) = rx.recv().await {
                    progress.set(p);
                    status_msg.set(msg);
                }
            });

            let cfg = repository_config(loaded.as_ref(), &bkp, &s3);
            let source_label = cfg.as_ref().map(repository_label).unwrap_or_default();
            let target_label = rst.to_string_lossy().to_string();

            let res: Result<(rsb_sdk::report::ReportData, Vec<String>), String> = async {
                let sel = selection?;
                if rst.as_os_str().is_empty() {
                    return Err("Please select a restore target.".to_string());
                }
                let cfg = cfg?;
                let key = resolve_key_blocking(typed_key, loaded.clone()).await;
                let mut notes = Vec::new();
                // A date is resolved here (as the CLI does) so the chosen
                // snapshot can be shown and verified.
                let snapshot = match sel.at_or_before {
                    Some(when) => {
                        let id = rsb_sdk::core::find_snapshot_at_or_before_in(&cfg, None, when)
                            .await
                            .map_err(|e| e.to_string())?;
                        notes.push(format!("📅 Latest snapshot at or before that date: {}", id));
                        Some(id)
                    }
                    None => sel.snapshot_id,
                };
                let report = rsb_sdk::core::perform_restore(
                    &cfg,
                    RestoreOptions {
                        snapshot_id: snapshot.clone(),
                        include: sel.include,
                        force,
                        versioned: versioned_mode,
                        on_progress: Some(progress_cb),
                        cancellation_token: Some(token),
                        ..RestoreOptions::new(rst.clone())
                    }
                    .with_encryption_key(key.as_deref()),
                )
                .await
                .map_err(|e| e.to_string())?;

                if verify_mode {
                    status_msg.set("🔍 Verifying the restored snapshot...".to_string());
                    let v = perform_verify(
                        &cfg,
                        VerifyOptions {
                            snapshot_id: snapshot,
                            ..Default::default()
                        }
                        .with_encryption_key(key.as_deref()),
                    )
                    .await
                    .map_err(|e| format!("Restore verification failed: {}", e))?;
                    if v.files_with_errors > 0 || !v.errors.is_empty() {
                        return Err(format!(
                            "Restore verification found problems ({} file(s) with errors): {}",
                            v.files_with_errors,
                            v.errors.first().cloned().unwrap_or_default()
                        ));
                    }
                    notes.push(format!("🔍 Verification passed: {} files", v.total_files));
                }
                Ok((report, notes))
            }
            .await;

            match res {
                Ok((mut report, notes)) => {
                    let _ = record_restore_operation(
                        report.status.clone(),
                        report.files_processed,
                        report.duration.as_secs(),
                        source_label.clone(),
                        target_label.clone(),
                    );
                    ops::notify(
                        NotificationEvent::RestoreCompleted,
                        "✅ Restore completed",
                        format!(
                            "Files: {}\nDuration: {}s",
                            report.files_processed,
                            report.duration.as_secs()
                        ),
                        format!("Source: {}\nTarget: {}", source_label, target_label),
                    )
                    .await;

                    let mut msg = format!(
                        "{}: {} file(s) restored, {} skipped, {} with errors",
                        report.status,
                        report.files_processed,
                        report.files_skipped,
                        report.files_with_errors
                    );
                    for e in report.errors.iter().take(5) {
                        msg.push_str(&format!("\n  ❌ {}", e));
                    }
                    for n in notes {
                        msg.push('\n');
                        msg.push_str(&n);
                    }
                    if create_report {
                        report.profile_path = loaded
                            .as_ref()
                            .map(|p| p.path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| format!("Backup: {}", source_label));
                        match ops::write_report("restore", &report) {
                            Ok(path) => {
                                msg.push_str(&format!("\n📄 Report: {}", path.display()));
                                last_report_path.set(Some(path));
                            }
                            Err(e) => msg.push_str(&format!("\n❌ {}", e)),
                        }
                    }
                    status_msg.set(msg);
                    progress.set(1.0);
                }
                Err(e) => {
                    let _ = record_restore_operation(
                        "Failed".to_string(),
                        0,
                        0,
                        source_label.clone(),
                        target_label.clone(),
                    );
                    ops::notify(
                        NotificationEvent::RestoreFailed,
                        "❌ Restore failed",
                        format!("Error: {}", e),
                        format!("Source: {}\nTarget: {}", source_label, target_label),
                    )
                    .await;
                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                }
            }
            is_running.set(false);
        });
    };

    // The restore stops after the files in progress; the screen stays busy
    // until it has.
    let handle_cancel = move |_| {
        cancellation_token().cancel();
        status_msg.set("⏹️ Cancelling — waiting for the files in progress...".to_string());
    };

    let profile_label = profile()
        .map(|p| p.path.to_string_lossy().into_owned())
        .unwrap_or_default();
    let has_source = profile().is_some()
        || !source_path.read().as_os_str().is_empty()
        || !app_config.s3_bucket().trim().is_empty();

    rsx! {
        LoadingOverlay {
            is_visible: is_running(),
            style: LoadingStyle::ProgressBar,
            message: status_msg().to_string(),
            progress: progress(),
        }

        div { class: "card",
            h2 { class: "page-title", "{texts.restore_title}" }

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
                p { class: "hint", "Restores from the profile's repository (local or S3), like `rsb restore <profile>`." }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.backup_folder_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.backup_folder_label}",
                        value: "{source_path.read().to_string_lossy()}",
                        oninput: move |evt| {
                            profile.set(None);
                            source_path.set(PathBuf::from(evt.value()));
                        }
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    profile.set(None);
                                    source_path.set(handle.path().to_path_buf());
                                }
                            });
                        },
                        "📂"
                    }
                }
                p { class: "hint", "The repository folder (`rsb restore --backup <dir>`). Empty: the S3 bucket configured in Settings." }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.restore_target_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.restore_target_label}",
                        value: "{restore_to.read().to_string_lossy()}",
                        oninput: move |evt| restore_to.set(PathBuf::from(evt.value()))
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    restore_to.set(handle.path().to_path_buf());
                                }
                            });
                        },
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.snapshot_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "{texts.snapshot_label}",
                    value: "{snapshot_id}",
                    oninput: move |evt| snapshot_id.set(evt.value())
                }
                p { class: "hint", "Snapshot ID to restore (optional, default: the latest)" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.restore_date_label}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "2026-10-01 or 2026-10-01 12:00",
                    value: "{date_text}",
                    oninput: move |evt| date_text.set(evt.value())
                }
                p { class: "hint",
                    "YYYY-MM-DD (end of that day), YYYY-MM-DD HH:MM (local time) or RFC 3339. Leave the snapshot ID empty."
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.restore_files_label}" }
                textarea {
                    class: "input-field font-mono",
                    placeholder: "docs/\nphotos/2025/*.jpg\nnotes/todo.txt",
                    value: "{include_text}",
                    oninput: move |evt: Event<FormData>| include_text.set(evt.value()),
                    rows: "3"
                }
                p { class: "hint",
                    "Exact path, folder (docs/) or glob (**/*.pdf), relative to the backup source. Empty restores everything."
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.key_label}" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "{texts.key_label}",
                    value: "{key}",
                    oninput: move |evt| key.set(evt.value())
                }
                p { class: "hint", "Blank: RSB_ENCRYPTION_KEY or the profile's key in the OS keyring is used." }
            }

            div { class: "form-group space-y-2",
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: overwrite(),
                        onchange: move |_| overwrite.toggle()
                    }
                    "Overwrite existing files in the target (--force)"
                }
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: versioned(),
                        onchange: move |_| versioned.toggle()
                    }
                    "Restore into a new restore_<timestamp> folder (--versioned)"
                }
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: verify_after(),
                        onchange: move |_| verify_after.toggle()
                    }
                    "Verify the snapshot afterwards (--verify)"
                }
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: generate_report(),
                        onchange: move |_| generate_report.toggle()
                    }
                    "{texts.generate_report_label}"
                }
            }

            if is_running() {
                div { class: "flex gap-3 mb-4",
                    button {
                        class: "flex-1 px-4 py-3 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                        onclick: handle_cancel,
                        "⏹️  Cancel Restore"
                    }
                }
            } else {
                div { class: "flex gap-3 mb-4",
                    button {
                        class: "flex-1 btn-primary",
                        onclick: handle_load_backup_directly,
                        disabled: is_running(),
                        "{texts.load_backup_directly}"
                    }
                    button {
                        class: "flex-1 btn-success",
                        onclick: handle_restore,
                        disabled: is_running() || !has_source || restore_to.read().as_os_str().is_empty(),
                        "{texts.start_restore}"
                    }
                }
            }

            if is_running() || progress() > 0.0 {
                ProgressBar { progress: progress() }
            }

            if let Some(path) = last_report_path() {
                button {
                    class: "w-full mt-4 px-4 py-2 bg-slate-500 dark:bg-slate-600 hover:bg-slate-600 dark:hover:bg-slate-700 text-white font-semibold rounded-lg transition-colors",
                    onclick: move |_| {
                        let _ = open::that(&path);
                    },
                    "📄 Open Report"
                }
            }

            div { class: "status-box mt-6",
                p { class: "font-semibold mb-2", "Status:" }
                p { class: "whitespace-pre-wrap", "{status_msg}" }
            }
        }
    }
}

/// One `--files` pattern per non-blank line (line endings stripped).
fn include_patterns(text: &str) -> Vec<String> {
    text.lines()
        .map(|l| l.trim_end_matches('\r'))
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn include_patterns_one_per_line() {
        assert!(include_patterns("").is_empty());
        assert!(include_patterns("  \n\n").is_empty());
        assert_eq!(
            include_patterns("docs/\r\n**/*.pdf\n\nmy file.txt"),
            ["docs/", "**/*.pdf", "my file.txt"]
        );
    }

    #[test]
    fn selection_is_validated_like_the_cli() {
        let all = restore_selection(" ", "", "").unwrap();
        assert_eq!(all, RestoreSelection::default());
        let s = restore_selection("2026-01-01T000000Z", "", "docs/\n").unwrap();
        assert_eq!(s.snapshot_id.as_deref(), Some("2026-01-01T000000Z"));
        assert_eq!(s.include, ["docs/"]);
        assert!(
            restore_selection("", "2026-10-01", "")
                .unwrap()
                .at_or_before
                .is_some()
        );
        assert!(restore_selection("id", "2026-10-01", "").is_err());
        assert!(restore_selection("", "not a date", "").is_err());
    }
}
