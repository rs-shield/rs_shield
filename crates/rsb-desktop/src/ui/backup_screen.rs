use chrono::Local;
use dioxus::prelude::*;
use rsb_sdk::backup::perform_backup_with_cancellation;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::integrations_screen::IntegrationConfig;
use rsb_sdk::core::{NotificationEvent, NotificationManager, NotificationPayload};
use rsb_sdk::operation::operations_helpers::record_backup_operation;
use rsb_sdk::{CancellationToken, config};

use crate::ui::{
    app::AppConfig,
    error_handler::format_user_error,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{ProfileData, load_profile},
    shared::ProgressBar,
};

#[component]
pub fn BackupScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile_path = use_signal(PathBuf::new);
    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut generate_report = use_signal(|| false);
    let mut dry_run = use_signal(|| false);
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
                let path = h.path().to_path_buf();
                match load_profile(&path) {
                    Ok(cfg) => {
                        let profile_data = ProfileData::from(cfg);
                        profile_path.set(path);
                        source_path.set(PathBuf::from(profile_data.source_path));
                        dest_path.set(PathBuf::from(profile_data.destination_path));
                        key.set(profile_data.encryption_key);

                        // Carregar todas as configurações do perfil
                        app_config
                            .exclude_patterns
                            .set(profile_data.exclude_patterns);
                        app_config.backup_mode.set(profile_data.backup_mode);
                        app_config.s3_bucket.set(profile_data.s3_bucket);
                        app_config.s3_region.set(profile_data.s3_region);
                        app_config.s3_endpoint.set(profile_data.s3_endpoint);
                        app_config
                            .encrypt_patterns
                            .set(profile_data.encrypt_patterns);
                        app_config
                            .pause_on_low_battery
                            .set(profile_data.pause_on_low_battery);
                        app_config
                            .pause_on_high_cpu
                            .set(profile_data.pause_on_high_cpu);
                        app_config
                            .compression_level
                            .set(profile_data.compression_level);

                        status_msg.set("✅ Profile loaded successfully!".to_string());
                    }
                    Err(e) => {
                        status_msg.set(format_user_error(e, "backup"));
                    }
                }
            }
        });
    };

    let handle_backup = move |_| {
        if is_running() {
            return;
        }

        is_running.set(true);
        // Reset do token de cancellation para nova operação
        let token = CancellationToken::new();
        cancellation_token.set(token.clone());
        last_report_path.set(None);
        progress.set(0.0);
        status_msg.set(texts.starting.to_string());

        let src = source_path();
        let dst = dest_path();
        let key_val = key();
        let key_opt = if key_val.is_empty() {
            None
        } else {
            Some(key_val)
        };
        let create_report = generate_report();
        let dry_run_mode = dry_run();

        let excludes_str = app_config.exclude_patterns();
        let excludes: Vec<String> = excludes_str
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let encrypt_patterns_str = app_config.encrypt_patterns();
        let encrypt_patterns: Vec<String> = encrypt_patterns_str
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        let pause_on_low_battery_str = app_config.pause_on_low_battery();
        let pause_on_low_battery = pause_on_low_battery_str.parse::<u8>().ok();

        let pause_on_high_cpu_str = app_config.pause_on_high_cpu();
        let pause_on_high_cpu = pause_on_high_cpu_str.parse::<u8>().ok();

        let compression_level_str = app_config.compression_level();
        let compression_level = compression_level_str.parse::<u8>().ok();

        let mode = app_config.backup_mode();
        let s3_bucket = app_config.s3_bucket();
        let s3_region = app_config.s3_region();
        let s3_endpoint = app_config.s3_endpoint();
        let s3_access_key = app_config.s3_access_key();
        let s3_secret_key = app_config.s3_secret_key();

        let s3_bucket_opt = if s3_bucket.is_empty() {
            None
        } else {
            Some(s3_bucket)
        };
        let s3_region_opt = if s3_region.is_empty() {
            None
        } else {
            Some(s3_region)
        };
        let s3_endpoint_opt = if s3_endpoint.is_empty() {
            None
        } else {
            Some(s3_endpoint)
        };
        let s3_access_key_opt = if s3_access_key.is_empty() {
            None
        } else {
            Some(s3_access_key)
        };
        let s3_secret_key_opt = if s3_secret_key.is_empty() {
            None
        } else {
            Some(s3_secret_key)
        };

        let s3_config = if s3_bucket_opt.is_some() {
            Some(config::S3Config {
                bucket: s3_bucket_opt.clone(),
                region: s3_region_opt.clone(),
                endpoint: s3_endpoint_opt.clone(),
                access_key: s3_access_key_opt,
                secret_key: s3_secret_key_opt,
            })
        } else {
            None
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

            let res = async {
                if src.as_os_str().is_empty()
                    || (dst.as_os_str().is_empty() && s3_bucket_opt.is_none())
                {
                    return Err(texts.please_define_paths.to_string());
                }

                let cfg = config::Config {
                    source_path: src.to_string_lossy().into_owned(),
                    destination_path: dst.to_string_lossy().into_owned(),
                    exclude_patterns: excludes,
                    encryption_key: None,
                    pause_on_low_battery,
                    encrypt_patterns: Some(encrypt_patterns),
                    backup_mode: mode.clone(),
                    s3_bucket: s3_bucket_opt,
                    s3_region: s3_region_opt,
                    s3_endpoint: s3_endpoint_opt,
                    s3: s3_config,
                    s3_buckets: None,
                    pause_on_high_cpu,
                    compression_level,
                    channel_buffer_size: 8192,
                    max_threads: None,
                };

                let token = cancellation_token();
                perform_backup_with_cancellation(
                    &cfg,
                    &mode,
                    key_opt.as_deref(),
                    dry_run_mode,
                    true,
                    None,
                    Some(progress_cb),
                    Some(token),
                )
                .await
                .map_err(|e| e.to_string())
            }
            .await;

            match res {
                Ok(mut report) => {
                    let duration = report.duration.as_secs();
                    let backup_size = format!("{}B", report.duration.as_secs());

                    let _ = record_backup_operation(
                        report.status.clone(),
                        report.files_processed,
                        duration,
                        backup_size,
                        src.to_string_lossy().to_string(),
                        dst.to_string_lossy().to_string(),
                    );

                    // Send notifications
                    if let Some(profile_parent) = profile_path().parent() {
                        let integrations = IntegrationConfig::load(profile_parent);
                        let mut manager = NotificationManager::new();

                        // Add email config if enabled
                        if let Some(email_cfg) = integrations.to_email_config() {
                            manager.set_email_config(email_cfg);
                        }

                        // Add chat integrations if enabled
                        for chat_integration in integrations.to_chat_integrations() {
                            manager.add_chat_integration(chat_integration);
                        }

                        // Send backup completion notification
                        let notification = NotificationPayload {
                            event: NotificationEvent::BackupCompleted,
                            title: "✅ Backup Concluído".to_string(),
                            message: format!(
                                "Arquivos: {}\nDuração: {}s\nStatus: {}",
                                report.files_processed, duration, report.status
                            ),
                            details: Some(format!(
                                "Origem: {}\nDestino: {}",
                                src.to_string_lossy(),
                                dst.to_string_lossy()
                            )),
                            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        };

                        let _ = manager.send(&notification).await;
                    }

                    if create_report {
                        report.profile_path = "Desktop UI".to_string();
                        let html = rsb_sdk::report::generate_html(&report);
                        let filename = format!(
                            "rsb-report-backup-{}.html",
                            Local::now().format("%Y%m%d-%H%M%S")
                        );
                        if fs::write(&filename, html).is_ok() {
                            status_msg.set(format!("{} Relatório: {}", report.status, filename));
                            last_report_path.set(Some(PathBuf::from(filename)));
                        } else {
                            status_msg.set(format!("{} (Error writing report)", report.status));
                        }
                    } else {
                        status_msg.set(report.status);
                    }
                    progress.set(1.0);
                }
                Err(e) => {
                    let _ = record_backup_operation(
                        "Failed".to_string(),
                        0,
                        0,
                        "0B".to_string(),
                        src.to_string_lossy().to_string(),
                        dst.to_string_lossy().to_string(),
                    );

                    // Send error notification
                    if let Some(profile_parent) = profile_path().parent() {
                        let integrations = IntegrationConfig::load(profile_parent);
                        let mut manager = NotificationManager::new();

                        if let Some(email_cfg) = integrations.to_email_config() {
                            manager.set_email_config(email_cfg);
                        }

                        for chat_integration in integrations.to_chat_integrations() {
                            manager.add_chat_integration(chat_integration);
                        }

                        let notification = NotificationPayload {
                            event: NotificationEvent::BackupFailed,
                            title: "❌ Backup Falhou".to_string(),
                            message: format!("Erro: {}", e),
                            details: Some(format!(
                                "Origem: {}\nDestino: {}",
                                src.to_string_lossy(),
                                dst.to_string_lossy()
                            )),
                            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        };

                        let _ = manager.send(&notification).await;
                    }

                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                }
            }
            is_running.set(false);
        });
    };

    let handle_cancel = move |_| {
        cancellation_token().cancel();
        status_msg.set("⏹️ Backup canceled by user".to_string());
        is_running.set(false);
    };

    rsx! {
        // Loading overlay
        LoadingOverlay {
            is_visible: is_running(),
            style: LoadingStyle::ProgressBar,
            message: status_msg().to_string(),
            progress: progress(),
        }

        div { class: "card",
            h2 { class: "page-title", "{texts.backup_title}" }

            div { class: "form-group",
                label { class: "label-text", "📋 Load Profile (Optional)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Select a profile",
                        value: "{profile_path.read().to_string_lossy()}",
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_load_profile,
                        disabled: is_running(),
                        "📂"
                    }
                }
                p { class: "hint", "Loads all profile settings automatically" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.source_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.source_label}",
                        value: "{source_path.read().to_string_lossy()}",
                        oninput: move |evt| source_path.set(PathBuf::from(evt.value()))
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
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
                        oninput: move |evt| dest_path.set(PathBuf::from(evt.value()))
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
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
                p { class: "hint", "Leave blank to disable encryption" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.s3_title}" }
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "S3 Bucket (Optional)",
                    value: "{app_config.s3_bucket}",
                    readonly: true
                }
                p { class: "hint", "Configured via profile or global settings" }
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
                        "🧪 Dry-Run Mode - Simulates backup without writing files"
                    }
                }
            }

            if is_running() {
                div { class: "flex gap-3 mb-4",
                    button {
                        class: "flex-1 px-4 py-3 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                        onclick: handle_cancel,
                         "⏹️ Cancel Backup"
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
                    "📄 Open Report"
                }
            }

            div { class: "status-box mt-6",
                p { class: "font-semibold mb-2", "Status:" }
                p { "{status_msg}" }
            }
        }
    }
}
