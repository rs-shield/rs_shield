use dioxus::prelude::*;
use rsb_sdk::backup::{BackupOptions, perform_backup};
use rsb_sdk::config::{self, Config};
use rsb_sdk::core::NotificationEvent;
use rsb_sdk::operation::operations_helpers::record_backup_operation;
use rsb_sdk::{CancellationToken, VerifyOptions, perform_verify};
use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::{Translations, get_texts, tr},
    loading_state::{LoadingOverlay, LoadingStyle},
    ops,
    profile_loader::{
        LoadedProfile, check_backup_key, load_profile, repository_label, resolve_key_blocking,
    },
    shared::ProgressBar,
};

/// Settings of a backup without a profile (paths typed on screen, options
/// from the Settings screen).
#[derive(Debug, Clone, Default)]
pub struct AdHocBackup {
    pub source: PathBuf,
    pub destination: PathBuf,
    pub exclude_patterns: String,
    pub encrypt_patterns: String,
    pub pause_on_low_battery: String,
    pub pause_on_high_cpu: String,
    pub compression_level: String,
    pub backup_mode: String,
    pub s3: S3Settings,
}

/// One pattern per non-blank line.
fn lines(text: &str) -> Vec<String> {
    text.lines()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Optional number field: blank = not set, anything else must parse.
fn optional_u8(text: &str, what: &str, max: u8, t: &Translations) -> Result<Option<u8>, String> {
    let value = text.trim();
    if value.is_empty() {
        return Ok(None);
    }
    match value.parse::<u8>() {
        Ok(v) if v <= max => Ok(Some(v)),
        _ => Err(tr(t.bk_invalid_number, &[&what, &text.trim(), &max])),
    }
}

/// Builds the backup configuration of a backup without a profile.
pub fn adhoc_backup_config(a: &AdHocBackup, t: &Translations) -> Result<Config, String> {
    if a.source.as_os_str().is_empty() {
        return Err(t.bk_select_source.to_string());
    }
    if a.destination.as_os_str().is_empty() && !a.s3.is_enabled() {
        return Err(t.bk_select_destination.to_string());
    }
    let mut cfg = Config {
        source_path: a.source.to_string_lossy().into_owned(),
        destination_path: a.destination.to_string_lossy().into_owned(),
        exclude_patterns: lines(&a.exclude_patterns),
        encrypt_patterns: Some(lines(&a.encrypt_patterns)).filter(|p| !p.is_empty()),
        pause_on_low_battery: optional_u8(&a.pause_on_low_battery, t.bk_battery_threshold, 100, t)?,
        pause_on_high_cpu: optional_u8(&a.pause_on_high_cpu, t.bk_cpu_threshold, 100, t)?,
        compression_level: optional_u8(
            &a.compression_level,
            t.bk_compression_level,
            config::MAX_COMPRESSION_LEVEL,
            t,
        )?,
        ..Default::default()
    };
    if !a.backup_mode.trim().is_empty() {
        cfg.backup_mode = a.backup_mode.trim().to_string();
    }
    a.s3.apply_to(&mut cfg);
    Ok(cfg)
}

/// Status line of a finished backup (with the errors, if any).
pub fn backup_summary(
    report: &rsb_sdk::report::ReportData,
    dry_run: bool,
    t: &Translations,
) -> String {
    let mut msg = tr(
        t.bk_summary,
        &[
            &if report.files_with_errors > 0 {
                "⚠️ "
            } else {
                "✅ "
            },
            &report.status,
            &report.files_processed,
            &report.files_skipped,
            &report.files_with_errors,
            &report.duration.as_secs(),
        ],
    );
    if dry_run {
        msg.push_str(t.bk_summary_dry_run);
    }
    for e in report.errors.iter().take(5) {
        msg.push_str("\n  ❌ ");
        msg.push_str(e);
    }
    if report.errors.len() > 5 {
        msg.push_str("\n  ");
        msg.push_str(&tr(t.bk_summary_more, &[&(report.errors.len() - 5)]));
    }
    msg
}

#[component]
pub fn BackupScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut generate_report = use_signal(|| false);
    let mut dry_run = use_signal(|| false);
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
                match load_profile(h.path(), texts) {
                    Ok(p) => {
                        // The profile is used as is (like `rsb backup
                        // profile.toml`); the app-wide settings are not
                        // overwritten by it.
                        source_path.set(PathBuf::from(&p.config.source_path));
                        dest_path.set(PathBuf::from(repository_label(&p.config)));
                        status_msg.set(tr(texts.bk_profile_loaded, &[&p.path.display()]));
                        profile.set(Some(p));
                    }
                    Err(e) => status_msg.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let handle_backup = move |_| {
        if is_running() {
            return;
        }

        is_running.set(true);
        let token = CancellationToken::new();
        cancellation_token.set(token.clone());
        last_report_path.set(None);
        progress.set(0.0);
        status_msg.set(texts.starting.to_string());

        let loaded = profile();
        let typed_key = key();
        let create_report = generate_report();
        let dry_run_mode = dry_run();
        let verify_mode = verify_after() && !dry_run_mode;
        let adhoc = AdHocBackup {
            source: source_path(),
            destination: dest_path(),
            exclude_patterns: app_config.exclude_patterns(),
            encrypt_patterns: app_config.encrypt_patterns(),
            pause_on_low_battery: app_config.pause_on_low_battery(),
            pause_on_high_cpu: app_config.pause_on_high_cpu(),
            compression_level: app_config.compression_level(),
            backup_mode: app_config.backup_mode(),
            // Session credentials typed in Settings stay in memory only.
            s3: S3Settings::from_app(&app_config),
        };

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let progress_cb = Arc::new(move |current: usize, total: usize, msg: String| {
            let _ = tx.send((current, total, msg));
        });

        spawn(async move {
            spawn(async move {
                while let Some((cur, tot, msg)) = rx.recv().await {
                    if tot > 0 {
                        progress.set(cur as f64 / tot as f64);
                    }
                    status_msg.set(msg);
                }
            });

            let cfg = match &loaded {
                Some(p) => Ok(p.config.clone()),
                None => adhoc_backup_config(&adhoc, texts),
            };
            let src_label = cfg
                .as_ref()
                .map(|c| c.source_path.clone())
                .unwrap_or_default();
            let dst_label = cfg.as_ref().map(repository_label).unwrap_or_default();

            let res: Result<(rsb_sdk::report::ReportData, Option<String>), String> = async {
                let cfg = cfg?;
                // Same key lookup as the CLI (typed → env → keyring → TOML);
                // an encrypted profile is never backed up without a key.
                let key = resolve_key_blocking(typed_key, loaded.clone()).await;
                // Without a profile the Settings' encrypt patterns only
                // choose what a typed key encrypts.
                let key = if loaded.is_some() {
                    check_backup_key(&cfg, key, texts)?
                } else {
                    key
                };
                let report = perform_backup(
                    &cfg,
                    BackupOptions {
                        // A profile keeps its own mode, like the CLI.
                        mode: loaded.is_none().then(|| cfg.backup_mode.clone()),
                        dry_run: dry_run_mode,
                        resume: true,
                        on_progress: Some(progress_cb),
                        cancellation_token: Some(token),
                        ..Default::default()
                    }
                    .with_encryption_key(key.as_deref()),
                )
                .await
                .map_err(|e| e.to_string())?;

                let verified = if verify_mode {
                    status_msg.set(texts.bk_verifying.to_string());
                    let v = perform_verify(
                        &cfg,
                        VerifyOptions::default().with_encryption_key(key.as_deref()),
                    )
                    .await
                    .map_err(|e| tr(texts.bk_verify_failed, &[&e]))?;
                    if v.files_with_errors > 0 || !v.errors.is_empty() {
                        return Err(tr(
                            texts.bk_verify_problems,
                            &[
                                &v.files_with_errors,
                                &v.errors.first().cloned().unwrap_or_default(),
                            ],
                        ));
                    }
                    Some(tr(texts.bk_verify_passed, &[&v.total_files]))
                } else {
                    None
                };
                Ok((report, verified))
            }
            .await;

            match res {
                Ok((mut report, verified)) => {
                    let duration = report.duration.as_secs();
                    if !dry_run_mode
                        && let Err(e) = record_backup_operation(
                            report.status.clone(),
                            report.files_processed,
                            duration,
                            "N/A".to_string(),
                            src_label.clone(),
                            dst_label.clone(),
                        )
                    {
                        tracing::warn!("Could not record the backup in the history: {}", e);
                    }

                    if !dry_run_mode {
                        let failed = report.files_with_errors > 0;
                        ops::notify(
                            if failed {
                                NotificationEvent::BackupFailed
                            } else {
                                NotificationEvent::BackupCompleted
                            },
                            if failed {
                                "⚠️ Backup completed with errors"
                            } else {
                                "✅ Backup completed"
                            },
                            format!(
                                "Files: {}\nErrors: {}\nDuration: {}s\nStatus: {}",
                                report.files_processed,
                                report.files_with_errors,
                                duration,
                                report.status
                            ),
                            format!("Source: {}\nDestination: {}", src_label, dst_label),
                        )
                        .await;
                    }

                    let mut msg = backup_summary(&report, dry_run_mode, texts);
                    if let Some(v) = verified {
                        msg.push('\n');
                        msg.push_str(&v);
                    }
                    if create_report {
                        report.profile_path = loaded
                            .as_ref()
                            .map(|p| p.path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| "Desktop UI".to_string());
                        match ops::write_report("backup", &report) {
                            Ok(path) => {
                                msg.push('\n');
                                msg.push_str(&tr(texts.bk_report_path, &[&path.display()]));
                                last_report_path.set(Some(path));
                            }
                            Err(e) => msg.push_str(&format!("\n❌ {}", e)),
                        }
                    }
                    status_msg.set(msg);
                    progress.set(1.0);
                }
                Err(e) => {
                    if !dry_run_mode {
                        let _ = record_backup_operation(
                            "Failed".to_string(),
                            0,
                            0,
                            "N/A".to_string(),
                            src_label.clone(),
                            dst_label.clone(),
                        );
                        ops::notify(
                            NotificationEvent::BackupFailed,
                            "❌ Backup failed",
                            format!("Error: {}", e),
                            format!("Source: {}\nDestination: {}", src_label, dst_label),
                        )
                        .await;
                    }
                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                }
            }
            is_running.set(false);
        });
    };

    // Cancelling only asks the backup to stop: the screen stays busy until
    // it really finished (it writes no snapshot once cancelled), so a second
    // backup cannot be started on top of it.
    let handle_cancel = move |_| {
        cancellation_token().cancel();
        status_msg.set(texts.bk_cancelling.to_string());
    };

    let profile_label = profile()
        .map(|p| p.path.to_string_lossy().into_owned())
        .unwrap_or_default();

    rsx! {
        LoadingOverlay {
            is_visible: is_running(),
            style: LoadingStyle::ProgressBar,
            message: status_msg().to_string(),
            progress: progress(),
        }

        div { class: "card",
            h2 { class: "page-title", "{texts.backup_title}" }

            div { class: "form-group",
                label { class: "label-text", "{texts.bk_load_profile_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.bk_select_profile}",
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
                p { class: "hint",
                    "{texts.bk_profile_hint}"
                }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.source_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.source_label}",
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
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.dest_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.dest_label}",
                        value: "{dest_path.read().to_string_lossy()}",
                        oninput: move |evt| {
                            profile.set(None);
                            dest_path.set(PathBuf::from(evt.value()));
                        }
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    profile.set(None);
                                    dest_path.set(handle.path().to_path_buf());
                                }
                            });
                        },
                        "📂"
                    }
                }
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
                p { class: "hint",
                    "{texts.bk_key_hint}"
                }
            }

            if profile().is_none() {
                div { class: "form-group",
                    label { class: "label-text", "{texts.s3_title}" }
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.bk_s3_bucket_placeholder}",
                        value: "{app_config.s3_bucket}",
                        readonly: true
                    }
                    p { class: "hint", "{texts.bk_s3_hint}" }
                }
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-indigo-50 dark:bg-indigo-900/20 rounded-lg border border-indigo-200 dark:border-indigo-800",
                    input {
                        r#type: "checkbox",
                        id: "gen_report",
                        class: "w-4 h-4 rounded",
                        checked: "{generate_report}",
                        oninput: move |evt| generate_report.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "gen_report",
                        "{texts.generate_report_label}"
                    }
                }
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-emerald-50 dark:bg-emerald-900/20 rounded-lg border border-emerald-200 dark:border-emerald-800",
                    input {
                        r#type: "checkbox",
                        id: "verify_after",
                        class: "w-4 h-4 rounded",
                        checked: "{verify_after}",
                        oninput: move |evt| verify_after.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "verify_after",
                        "{texts.bk_verify_after_label}"
                    }
                }
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-amber-50 dark:bg-amber-900/20 rounded-lg border border-amber-200 dark:border-amber-800",
                    input {
                        r#type: "checkbox",
                        id: "dry_run_mode",
                        class: "w-4 h-4 rounded",
                        checked: "{dry_run}",
                        oninput: move |evt| dry_run.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "dry_run_mode",
                        "{texts.bk_dry_run_label}"
                    }
                }
            }

            if is_running() {
                div { class: "flex gap-3 mb-4",
                    button {
                        class: "flex-1 px-4 py-3 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                        onclick: handle_cancel,
                         "{texts.bk_cancel_button}"
                    }
                }
            } else {
                button {
                    class: "w-full btn-primary mb-4",
                    onclick: handle_backup,
                    disabled: is_running(),
                    "{texts.start_backup}"
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
                    "{texts.bk_open_report}"
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

    fn t() -> &'static Translations {
        get_texts(Language::En)
    }

    fn adhoc() -> AdHocBackup {
        AdHocBackup {
            source: PathBuf::from("/src"),
            destination: PathBuf::from("/repo"),
            exclude_patterns: "*.tmp\n\n node_modules \n".into(),
            encrypt_patterns: String::new(),
            compression_level: "5".into(),
            backup_mode: "full".into(),
            ..Default::default()
        }
    }

    #[test]
    fn adhoc_config_from_fields() {
        let cfg = adhoc_backup_config(&adhoc(), t()).unwrap();
        assert_eq!(cfg.exclude_patterns, ["*.tmp", "node_modules"]);
        assert_eq!(cfg.compression_level, Some(5));
        assert_eq!(cfg.backup_mode, "full");
        assert!(cfg.encrypt_patterns.is_none() && !cfg.encryption_required());
        assert!(cfg.s3.is_none());
    }

    #[test]
    fn adhoc_config_rejects_bad_input() {
        let mut a = adhoc();
        a.compression_level = "23".into();
        assert!(adhoc_backup_config(&a, t()).is_err());
        let mut a = adhoc();
        a.pause_on_high_cpu = "abc".into();
        assert!(adhoc_backup_config(&a, t()).is_err());
        let mut a = adhoc();
        a.destination = PathBuf::new();
        assert!(adhoc_backup_config(&a, t()).is_err());
        // ...unless the backup goes to S3.
        a.s3 = S3Settings::from_values("bucket".into(), String::new(), String::new());
        let cfg = adhoc_backup_config(&a, t()).unwrap();
        assert_eq!(cfg.s3_bucket.as_deref(), Some("bucket"));
    }

    #[test]
    fn summary_reports_errors_instead_of_plain_success() {
        let report = rsb_sdk::report::ReportData {
            status: "Failure with errors".into(),
            files_processed: 3,
            files_with_errors: 1,
            errors: vec!["a.txt: permission denied".into()],
            ..Default::default()
        };
        let s = backup_summary(&report, false, t());
        assert!(s.starts_with("⚠️"));
        assert!(s.contains("a.txt: permission denied"));
        assert!(backup_summary(&report, true, t()).contains("dry run"));
    }
}
