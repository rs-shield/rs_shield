use chrono::Local;
use dioxus::prelude::*;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use rsb_sdk::{CancellationToken, config, core};

use crate::ui::{
    app::AppConfig,
    i18n::get_texts,
    operations_helpers::record_restore_operation,
    profile_loader::{ProfileData, load_profile},
    shared::ProgressBar,
};

#[component]
pub fn RestoreScreen() -> Element {
    let mut app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());
    let mut profile_path = use_signal(PathBuf::new);
    let mut source_path = use_signal(PathBuf::new);
    let mut restore_to = use_signal(PathBuf::new);
    let mut key = use_signal(String::new);
    let mut snapshot_id = use_signal(String::new);
    let mut generate_report = use_signal(|| false);
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
                        source_path.set(PathBuf::from(profile_data.destination_path));
                        key.set(profile_data.encryption_key);

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

                        status_msg.set("✅ Profile loaded successfully!".to_string());
                    }
                    Err(e) => {
                        status_msg.set(format!("❌ Error loading profile: {}", e));
                    }
                }
            }
        });
    };

    let handle_restore = move |_| {
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

        let create_report = generate_report();

        spawn(async move {
            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<(f64, String)>();

            let bkp = source_path();
            let rst = restore_to();
            let snap = snapshot_id();
            let key_val = key();
            let excludes_str = app_config.exclude_patterns();
            let mode = app_config.backup_mode();
            let s3_bucket = app_config.s3_bucket();
            let s3_region = app_config.s3_region();
            let s3_endpoint = app_config.s3_endpoint();
            let s3_access_key = app_config.s3_access_key();
            let s3_secret_key = app_config.s3_secret_key();
            let rst_for_record = rst.clone();

            let progress_cb = Arc::new(move |current: usize, total: usize, msg: String| {
                let _ = tx.send((current as f64 / total.max(1) as f64, msg));
            });

            spawn(async move {
                while let Some((p, msg)) = rx.recv().await {
                    progress.set(p);
                    status_msg.set(msg);
                }
            });

            let res = async {
                let snap_opt = if snap.is_empty() { None } else { Some(snap) };
                let key_opt = if key_val.is_empty() {
                    None
                } else {
                    Some(key_val)
                };
                let excludes: Vec<String> = excludes_str
                    .lines()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
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

                if rst.as_os_str().is_empty() {
                    return Err("❌ Please select a restore target.".to_string());
                }

                if bkp.as_os_str().is_empty() && s3_bucket_opt.is_none() {
                    return Err(texts.define_backup_or_s3.to_string());
                }

                let cfg = config::Config {
                    source_path: "dummy".to_string(),
                    destination_path: bkp.to_string_lossy().into_owned(),
                    exclude_patterns: excludes,
                    encryption_key: None,
                    encrypt_patterns: None,
                    pause_on_low_battery: None,
                    backup_mode: mode.clone(),
                    s3_bucket: s3_bucket_opt,
                    s3_region: s3_region_opt,
                    s3_endpoint: s3_endpoint_opt,
                    s3: s3_config,
                    s3_buckets: None,
                    pause_on_high_cpu: None,
                    compression_level: Some(3),
                    max_threads: None,
                    channel_buffer_size: 8192, // ⚡ Default buffer size for manifest updates
                };

                let token = cancellation_token();
                core::restore::perform_restore_with_cancellation(
                    &cfg,
                    snap_opt.as_deref(),
                    rst,
                    key_opt.as_deref(),
                    true,  // UI default is force
                    false, // versioned
                    Some(progress_cb),
                    Some(token),
                )
                .await
                .map_err(|e| e.to_string())
            }
            .await;

            match res {
                Ok(mut report) => {
                    let _ = record_restore_operation(
                        report.status.clone(),
                        report.files_processed,
                        report.duration.as_secs(),
                        bkp.to_string_lossy().to_string(),
                        rst_for_record.to_string_lossy().to_string(),
                    );
                    if create_report {
                        report.profile_path = "Desktop UI".to_string();
                        let html = rsb_sdk::report::generate_html(&report);
                        let filename = format!(
                            "rsb-report-restore-{}.html",
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
                    let _ = record_restore_operation(
                        "Falha".to_string(),
                        0,
                        0,
                        bkp.to_string_lossy().to_string(),
                        rst_for_record.to_string_lossy().to_string(),
                    );
                    status_msg.set(format!("{} {}", texts.error_prefix, e))
                }
            }

            is_running.set(false);
        });
    };

    let handle_cancel = move |_| {
        cancellation_token().cancel();
        status_msg.set("⏹️  Restore canceled by user".to_string());
        is_running.set(false);
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "{texts.restore_title}" }

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
                p { class: "hint", "Loads the backup path from the selected profile" }
            }

            div { class: "form-group",
                label { class: "label-text", "{texts.backup_folder_label}" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "{texts.backup_folder_label}",
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
                p { class: "hint", "Snapshot ID to restore (optional)" }
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
                p { class: "hint", "If the backup was encrypted, provide the key" }
            }

            div { class: "form-group",
                div { class: "flex items-center gap-3 p-3 bg-green-50 dark:bg-green-900/20 rounded-lg border border-green-200 dark:border-green-800",
                    input {
                        r#type: "checkbox",
                        id: "gen_report_restore",
                        class: "w-4 h-4 rounded",
                        checked: "{generate_report}",
                        oninput: move |evt| generate_report.set(evt.value() == "true")
                    }
                    label {
                        class: "label-text mb-0 cursor-pointer",
                        r#for: "gen_report_restore",
                        "{texts.generate_report_label}"
                    }
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
                button {
                    class: "w-full btn-success mb-4",
                    onclick: handle_restore,
                    disabled: is_running(),
                    "{texts.start_restore}"
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
