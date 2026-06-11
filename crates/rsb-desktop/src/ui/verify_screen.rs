use chrono::Local;
use dioxus::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use crate::ui::integrations_screen::IntegrationConfig;
use rsb_sdk::core::{NotificationEvent, NotificationManager, NotificationPayload};
use rsb_sdk::operation::operations_helpers::record_verify_operation;
use rsb_sdk::{CancellationToken, config, perform_verify};

use crate::ui::{
    app::AppConfig,
    error_handler::format_user_error,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{ProfileData, load_profile},
    shared::ProgressBar,
};

#[component]
pub fn VerifyScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile_path = use_signal(PathBuf::new);
    let mut backup_path = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut is_running = use_signal(|| false);
    let mut fast_verify = use_signal(|| false);
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
                let path = h.path().to_path_buf();
                match load_profile(&path) {
                    Ok(cfg) => {
                        let profile_data = ProfileData::from(cfg);
                        profile_path.set(path);
                        backup_path.set(PathBuf::from(profile_data.destination_path));
                        key.set(profile_data.encryption_key);

                        // Load all profile settings
                        app_config
                            .exclude_patterns
                            .set(profile_data.exclude_patterns);
                        app_config.backup_mode.set(profile_data.backup_mode);
                        app_config.s3_bucket.set(profile_data.s3_bucket);
                        app_config.s3_region.set(profile_data.s3_region);
                        app_config.s3_endpoint.set(profile_data.s3_endpoint);

                        status_msg.set("✅ Perfil carregado com sucesso!".to_string());
                    }
                    Err(e) => {
                        status_msg.set(format_user_error(e, "verify"));
                    }
                }
            }
        });
    };

    let handle_verify = move |_| {
        if is_running() {
            return;
        }
        is_running.set(true);

        // Reset do token de cancellation para nova operação
        let token = CancellationToken::new();
        cancellation_token.set(token.clone());

        progress.set(0.0);
        status_msg.set(texts.starting.to_string());
        last_report_path.set(None);

        let bkp = backup_path();
        let key_val = key();
        let is_fast = fast_verify();
        let create_report = generate_report();
        let key_opt = if key_val.is_empty() {
            None
        } else {
            Some(key_val)
        };

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

        spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();

            let progress_cb = Arc::new(move |cur, tot, msg: String| {
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

            let res = async {
                if bkp.as_os_str().is_empty() && s3_bucket_opt.is_none() {
                    return Err(texts.define_backup_or_s3.to_string());
                }

                let cfg = config::Config {
                    source_path: "dummy".to_string(),
                    destination_path: bkp.to_string_lossy().into_owned(),
                    exclude_patterns: vec![],
                    encryption_key: key_opt,
                    pause_on_low_battery: None,
                    encrypt_patterns: None,
                    backup_mode: "incremental".to_string(),
                    s3_bucket: s3_bucket_opt,
                    s3_region: s3_region_opt,
                    s3_endpoint: s3_endpoint_opt,
                    s3: s3_config,
                    s3_buckets: None,
                    pause_on_high_cpu: None,
                    compression_level: Some(3),
                    max_threads: None,
                    channel_buffer_size: 8192,
                };

                let token = cancellation_token();
                perform_verify(&cfg, None, false, is_fast, Some(progress_cb), Some(token))
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
                        bkp.to_string_lossy().to_string(),
                    );

                    // Send notifications
                    if let Some(bkp_parent) = bkp.parent() {
                        let integrations = IntegrationConfig::load(bkp_parent);
                        let mut manager = NotificationManager::new();

                        if let Some(email_cfg) = integrations.to_email_config() {
                            manager.set_email_config(email_cfg);
                        }

                        for chat_integration in integrations.to_chat_integrations() {
                            manager.add_chat_integration(chat_integration);
                        }

                        let notification = NotificationPayload {
                            event: if report.files_with_errors > 0 {
                                NotificationEvent::VerificationFailed
                            } else {
                                NotificationEvent::VerificationCompleted
                            },
                            title: if report.files_with_errors > 0 {
                                "⚠️ Verificação Concluída com Erros".to_string()
                            } else {
                                "✅ Verificação Concluída".to_string()
                            },
                            message: format!(
                                "Arquivos: {}\nErros: {}\nDuração: {}s",
                                report.files_processed,
                                report.files_with_errors,
                                report.duration.as_secs()
                            ),
                            details: Some(format!("Backup: {}", bkp.to_string_lossy())),
                            timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                        };

                        let _ = manager.send(&notification).await;
                    }

                    if create_report {
                        report.profile_path = "Desktop UI".to_string();
                        let html = rsb_sdk::report::generate_html(&report);
                        let filename = format!(
                            "rsb-report-verify-{}.html",
                            Local::now().format("%Y%m%d-%H%M%S")
                        );
                        if fs::write(&filename, html).is_ok() {
                            status_msg.set(format!("{} Relatório: {}", report.status, filename));
                            last_report_path.set(Some(PathBuf::from(filename)));
                        } else {
                            status_msg.set(format!("{} (Erro ao gravar relatório)", report.status));
                        }
                    } else {
                        status_msg.set(report.status);
                    }
                    progress.set(1.0);
                }
                Err(e) => {
                    let _ = record_verify_operation(
                        "Falha".to_string(),
                        0,
                        0,
                        0,
                        bkp.to_string_lossy().to_string(),
                    );

                    // Send error notification
                    if let Some(bkp_parent) = bkp.parent() {
                        let integrations = IntegrationConfig::load(bkp_parent);
                        let mut manager = NotificationManager::new();

                        if let Some(email_cfg) = integrations.to_email_config() {
                            manager.set_email_config(email_cfg);
                        }

                        for chat_integration in integrations.to_chat_integrations() {
                            manager.add_chat_integration(chat_integration);
                        }

                        let notification = NotificationPayload {
                            event: NotificationEvent::VerificationFailed,
                            title: "❌ Verificação Falhou".to_string(),
                            message: format!("Erro: {}", e),
                            details: Some(format!("Backup: {}", bkp.to_string_lossy())),
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
        status_msg.set("⏹️ Verificação cancelada pelo usuário".to_string());
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
            h2 { class: "page-title", "{texts.verify_title}" }

            div { class: "form-group",
                label { class: "label-text", "📋 Carregar Perfil (Opcional)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Selecione um perfil",
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
                p { class: "hint", "Carrega o caminho de backup do perfil" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.backup_folder_simple_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.backup_folder_simple_label}",
                        value: "{backup_path.read().to_string_lossy()}",
                        oninput: move |evt| backup_path.set(PathBuf::from(evt.value()))
                    }
                    button {
                        class: "btn-icon",
                        onclick: move |_| {
                            spawn(async move {
                                if let Some(h) = rfd::AsyncFileDialog::new().pick_folder().await {
                                    backup_path.set(h.path().to_path_buf());
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
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-amber-50 dark:bg-amber-900/20 rounded-lg border border-amber-200 dark:border-amber-800",
                    input {
                        r#type: "checkbox",
                        id: "fast_verify_check",
                        class: "w-4 h-4 rounded",
                        checked: "{fast_verify}",
                        oninput: move |evt| fast_verify.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "fast_verify_check",
                        "⚡ Verificação Rápida (Lite)"
                    }
                }
                p { class: "hint mt-2", "Verifica apenas a integridade do ficheiro armazenado sem desencriptar." }
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-amber-50 dark:bg-amber-900/20 rounded-lg border border-amber-200 dark:border-amber-800",
                    input {
                        r#type: "checkbox",
                        id: "gen_report_verify",
                        class: "w-4 h-4 rounded",
                        checked: "{generate_report}",
                        oninput: move |evt| generate_report.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "gen_report_verify",
                        "{texts.generate_report_label}"
                    }
                }
            }

            if is_running() {
                div { class: "flex gap-3 mb-4",
                    button {
                        class: "flex-1 px-4 py-3 bg-red-500 dark:bg-red-600 hover:bg-red-600 dark:hover:bg-red-700 text-white font-semibold rounded-lg transition-colors",
                        onclick: handle_cancel,
                        "⏹️ Cancelar Verificação"
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
                    "📄 Abrir Relatório"
                }
            }

            div { class: "status-box mt-6",
                p { class: "font-semibold mb-2", "Status:" }
                p { "{status_msg}" }
            }
        }
    }
}
