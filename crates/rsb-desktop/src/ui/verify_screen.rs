use dioxus::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

use rsb_sdk::core::NotificationEvent;
use rsb_sdk::operation::operations_helpers::record_verify_operation;
use rsb_sdk::report::ReportData;
use rsb_sdk::{CancellationToken, VerifyOptions, perform_verify};

use crate::ui::s3_settings::S3Settings;
use crate::ui::{
    app::AppConfig,
    i18n::{Translations, get_texts, tr},
    loading_state::{LoadingOverlay, LoadingStyle},
    ops,
    profile_loader::{
        LoadedProfile, load_profile, repository_config, repository_label, resolve_key_blocking,
    },
    shared::ProgressBar,
};

/// Judges a verification like `rsb verify` (whose exit code fails on any
/// problem): `Err` with the problems listed when files had errors, so a
/// failed verification is never shown as a plain status.
pub fn verify_outcome(t: &Translations, report: &ReportData) -> Result<String, String> {
    let summary = tr(
        t.vf_summary,
        &[
            &report.status,
            &report.total_files,
            &report.files_with_errors,
        ],
    );
    if report.files_with_errors == 0 && report.errors.is_empty() {
        return Ok(format!("✅ {}", summary));
    }
    let mut msg = tr(t.vf_problems, &[&summary]);
    for e in report.errors.iter().take(10) {
        msg.push_str("\n  ❌ ");
        msg.push_str(e);
    }
    if report.errors.len() > 10 {
        msg.push_str(&format!(
            "\n  {}",
            tr(t.vf_more_errors, &[&(report.errors.len() - 10)])
        ));
    }
    Err(msg)
}

#[component]
pub fn VerifyScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut backup_path = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut snapshot_id = use_signal(String::new);
    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut is_running = use_signal(|| false);
    let mut fast_verify = use_signal(|| false);
    let mut repair = use_signal(|| false);
    let mut generate_report = use_signal(|| false);
    let mut last_report_path = use_signal(|| Option::<PathBuf>::None);
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
                        backup_path.set(PathBuf::from(repository_label(&p.config)));
                        status_msg.set(tr(texts.vf_profile_loaded, &[&p.path.display()]));
                        profile.set(Some(p));
                    }
                    Err(e) => status_msg.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let handle_verify = move |_| {
        if is_running() {
            return;
        }
        is_running.set(true);
        let token = CancellationToken::new();
        cancellation_token.set(token.clone());
        progress.set(0.0);
        status_msg.set(texts.starting.to_string());
        last_report_path.set(None);

        let loaded = profile();
        let bkp = backup_path();
        let typed_key = key();
        let snapshot = Some(snapshot_id().trim().to_string()).filter(|s| !s.is_empty());
        let is_repair = repair();
        // --repair always performs the full check (like the CLI).
        let is_fast = fast_verify() && !is_repair;
        let create_report = generate_report();
        // Session credentials typed in Settings stay in memory only.
        let s3 = S3Settings::from_app(&app_config);

        spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();
            let progress_cb = Arc::new(move |cur: usize, tot: usize, msg: String| {
                let prog = if tot > 0 {
                    cur as f64 / tot as f64
                } else {
                    0.0
                };
                let _ = tx.send((prog, msg));
            });
            spawn(async move {
                while let Some((p, msg)) = rx.recv().await {
                    progress.set(p);
                    status_msg.set(msg);
                }
            });

            let cfg = repository_config(loaded.as_ref(), &bkp, &s3, texts);
            let repo_label = cfg.as_ref().map(repository_label).unwrap_or_default();

            let res = async {
                let cfg = cfg?;
                let key = resolve_key_blocking(typed_key, loaded.clone()).await;
                perform_verify(
                    &cfg,
                    VerifyOptions {
                        snapshot_id: snapshot,
                        fast: is_fast,
                        repair: is_repair,
                        on_progress: Some(progress_cb),
                        cancellation_token: Some(token),
                        ..Default::default()
                    }
                    .with_encryption_key(key.as_deref()),
                )
                .await
                .map_err(|e| e.to_string())
            }
            .await;

            match res {
                Ok(mut report) => {
                    let _ = record_verify_operation(
                        report.status.clone(),
                        report.files_processed,
                        report.files_with_errors,
                        report.duration.as_secs(),
                        repo_label.clone(),
                    );
                    let outcome = verify_outcome(texts, &report);
                    ops::notify(
                        if outcome.is_ok() {
                            NotificationEvent::VerificationCompleted
                        } else {
                            NotificationEvent::VerificationFailed
                        },
                        if outcome.is_ok() {
                            "✅ Verification completed"
                        } else {
                            "⚠️ Verification found problems"
                        },
                        format!(
                            "Files: {}\nErrors: {}\nDuration: {}s",
                            report.total_files,
                            report.files_with_errors,
                            report.duration.as_secs()
                        ),
                        format!("Backup: {}", repo_label),
                    )
                    .await;

                    let mut msg = match outcome {
                        Ok(m) | Err(m) => m,
                    };
                    if create_report {
                        report.profile_path = loaded
                            .as_ref()
                            .map(|p| p.path.to_string_lossy().into_owned())
                            .unwrap_or_else(|| format!("Backup: {}", repo_label));
                        match ops::write_report("verify", &report) {
                            Ok(path) => {
                                msg.push_str(&format!(
                                    "\n{}",
                                    tr(texts.vf_report_path, &[&path.display()])
                                ));
                                last_report_path.set(Some(path));
                            }
                            Err(e) => msg.push_str(&format!("\n❌ {}", e)),
                        }
                    }
                    status_msg.set(msg);
                    progress.set(1.0);
                }
                Err(e) => {
                    let _ =
                        record_verify_operation("Failed".to_string(), 0, 0, 0, repo_label.clone());
                    ops::notify(
                        NotificationEvent::VerificationFailed,
                        "❌ Verification failed",
                        format!("Error: {}", e),
                        format!("Backup: {}", repo_label),
                    )
                    .await;
                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                }
            }

            is_running.set(false);
        });
    };

    let handle_cancel = move |_| {
        cancellation_token().cancel();
        status_msg.set(texts.vf_cancelling.to_string());
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
            h2 { class: "page-title", "{texts.verify_title}" }

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
                p { class: "hint", "{texts.vf_profile_hint}" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.backup_folder_simple_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.backup_folder_simple_label}",
                        value: "{backup_path.read().to_string_lossy()}",
                        oninput: move |evt| {
                            profile.set(None);
                            backup_path.set(PathBuf::from(evt.value()));
                        }
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    profile.set(None);
                                    backup_path.set(h.path().to_path_buf());
                                }
                            });
                        },
                        "📂"
                    }
                }
                p { class: "hint", "{texts.vf_backup_hint}" }
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
                p { class: "hint", "{texts.vf_snapshot_hint}" }
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
                p { class: "hint", "{texts.vf_key_hint}" }
            }

            div { class: "form-group space-y-2",
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: fast_verify(),
                        disabled: repair(),
                        onchange: move |_| fast_verify.toggle()
                    }
                    "{texts.vf_fast_label}"
                }
                label { class: "label-text flex items-center gap-2",
                    input {
                        r#type: "checkbox",
                        checked: repair(),
                        onchange: move |_| repair.toggle()
                    }
                    "{texts.vf_repair_label}"
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
                        "{texts.vf_cancel_button}"
                    }
                }
            } else {
                button {
                    class: "w-full btn-warning mb-4",
                    onclick: handle_verify,
                    disabled: is_running(),
                    "{texts.start_verify}"
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
                    "{texts.vf_open_report}"
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

    #[test]
    fn problems_are_a_failure_with_the_errors_listed() {
        let t = get_texts(Language::En);
        let ok = ReportData {
            status: "Success".into(),
            total_files: 4,
            ..Default::default()
        };
        assert!(
            verify_outcome(t, &ok)
                .unwrap()
                .contains("4 file(s) checked")
        );

        let bad = ReportData {
            status: "Failure with errors".into(),
            total_files: 4,
            files_with_errors: 1,
            errors: vec!["data/enc/ab: missing".into()],
            ..Default::default()
        };
        let msg = verify_outcome(t, &bad).unwrap_err();
        assert!(msg.contains("data/enc/ab: missing"));

        // Errors without a per-file count (e.g. an unreadable manifest)
        // are a failure too.
        let bad = ReportData {
            errors: vec!["manifest".into()],
            ..Default::default()
        };
        assert!(verify_outcome(t, &bad).is_err());
    }
}
