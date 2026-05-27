use crate::ui::{
    app::AppConfig,
    error_handler::format_user_error,
    i18n::get_texts,
    loading_state::{LoadingOverlay, LoadingStyle},
    profile_loader::{ProfileData, load_profile},
};
use rsb_sdk::operation::operations_helpers::record_prune_operation;

use dioxus::prelude::*;
use rsb_sdk::{config, core};
use std::path::PathBuf;

#[component]
pub fn PruneScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile_path = use_signal(PathBuf::new);
    let mut repo_path = use_signal(PathBuf::new);
    let mut retention_days = use_signal(|| "30".to_string());
    let mut is_running = use_signal(|| false);
    let mut status_msg = use_signal(|| texts.ready.to_string());
    let mut key = use_signal(String::new);

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
                        repo_path.set(PathBuf::from(profile_data.destination_path));
                        key.set(profile_data.encryption_key);

                        // Load all profile settings
                        app_config
                            .exclude_patterns
                            .set(profile_data.exclude_patterns);
                        app_config.backup_mode.set(profile_data.backup_mode);
                        app_config.s3_bucket.set(profile_data.s3_bucket);
                        app_config.s3_region.set(profile_data.s3_region);
                        app_config.s3_endpoint.set(profile_data.s3_endpoint);
                        app_config.s3_access_key.set(profile_data.s3_access_key);
                        app_config.s3_secret_key.set(profile_data.s3_secret_key);

                        status_msg.set("✅ Profile loaded successfully!".to_string());
                    }
                    Err(e) => {
                        status_msg.set(format_user_error(e, "prune"));
                    }
                }
            }
        });
    };

    let handle_prune = move |_| {
        if is_running() {
            return;
        }
        is_running.set(true);
        status_msg.set(texts.starting.to_string());

        let path = repo_path();
        let days_str = retention_days();
        let days = days_str.parse::<usize>().unwrap_or(30);
        let key_val = key();
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

        let cfg = config::Config {
            source_path: String::new(),
            destination_path: path.to_string_lossy().into_owned(),
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
            compression_level: None,
            max_threads: None,
            channel_buffer_size: 8192, // ⚡ Default buffer size for manifest updates
        };

        spawn(async move {
            let path_for_record = path.clone();

            let result = tokio::spawn(async move {
                if path.as_os_str().is_empty() && cfg.s3_bucket.is_none() {
                    return Err(texts.please_define_paths.to_string());
                }
                core::perform_prune(&cfg, days)
                    .await
                    .map_err(|e| e.to_string())
            })
            .await;

            match result {
                Ok(Ok(_)) => {
                    status_msg.set(texts.prune_success.to_string());
                    let _ = record_prune_operation(
                        "Sucesso".to_string(),
                        0,
                        0,
                        "N/A".to_string(),
                        path_for_record.to_string_lossy().to_string(),
                    );
                }
                Ok(Err(e)) => {
                    status_msg.set(format!("{} {}", texts.error_prefix, e));
                    let _ = record_prune_operation(
                        "Falha".to_string(),
                        0,
                        0,
                        "0B".to_string(),
                        path_for_record.to_string_lossy().to_string(),
                    );
                }
                Err(_) => {
                    status_msg.set(format!(
                        "{} {}",
                        texts.error_prefix, "A tarefa de limpeza falhou ao ser executada."
                    ));
                    let _ = record_prune_operation(
                        "Falha".to_string(),
                        0,
                        0,
                        "0B".to_string(),
                        path_for_record.to_string_lossy().to_string(),
                    );
                }
            }

            is_running.set(false);
        });
    };

    rsx! {
    // Loading overlay
    LoadingOverlay {
        is_visible: is_running(),
        style: LoadingStyle::ProgressBar,
        message: status_msg().to_string(),
        progress: 0.0,
    }

    div { class: "card",
        h2 { class: "page-title", "{texts.prune_title}" }

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
                onclick: move |evt| handle_load_profile(evt),
                    disabled: is_running(),
                    "📂"
                }
            }
            p { class: "hint", "Loads the backup path from the selected profile" }
        }

        div { class: "form-group",
            label { class: "label-text", "{texts.prune_path_label}" }
            div { class: "flex gap-3",
                input {
                    class: "input-field",
                    r#type: "text",
                    placeholder: "{texts.prune_path_label}",
                    value: "{repo_path.read().to_string_lossy()}",
                    oninput: move |evt| repo_path.set(PathBuf::from(evt.value()))
                }
                button {
                    class: "btn-icon",
                    onclick: move |_| {
                        spawn(async move {
                            if let Some(handle) = rfd::AsyncFileDialog::new().pick_folder().await {
                                repo_path.set(handle.path().to_path_buf());
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
            p { class: "hint", "If the backup was encrypted, provide the key" }
        }

        div { class: "form-group",
            label { class: "label-text", "{texts.retention_label}" }
            div { class: "flex items-center gap-4",
                input {
                    class: "input-field",
                    r#type: "number",
                    min: "1",
                    value: "{retention_days}",
                    oninput: move |evt| retention_days.set(evt.value())
                }
                span { class: "text-sm font-semibold text-slate-700 dark:text-slate-300 whitespace-nowrap", "days" }
            }
            p { class: "hint", "Backups older than this period will be removed" }}
        }

        button {
            class: "w-full btn-warning mb-4",
            onclick: handle_prune,
            disabled: is_running(),
            if is_running() { "{texts.executing}..." } else { "{texts.start_prune}" }
        }

        div { class: "status-box mt-6",
            p { class: "font-semibold mb-2", "Status:" }
            p { "{status_msg}" }
        }
    }
}
