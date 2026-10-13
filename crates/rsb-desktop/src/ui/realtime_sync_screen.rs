use crate::ui::profile_loader::{
    LoadedProfile, load_profile, repository_label, resolve_key_blocking,
};
use crate::ui::{app::AppConfig, i18n::get_texts};
use battery::Manager;
use dioxus::prelude::*;
use notify_rust::Notification;
use rsb_sdk::config::Config;
use rsb_sdk::realtime::{SyncStats, SyncStrategy, WatchEvent, mirror_and_backup_sync};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;

/// Event structure for webhook
#[derive(Debug, Clone, Serialize, Deserialize)]
struct NotificationEvent {
    event_type: String, // "sync_complete", "backup_created", "error", "battery_low"
    title: String,
    message: String,
    timestamp: String,
    status: String, // "success", "error", "warning"
}

/// Send system notification with fallback and webhook
fn send_system_notification(
    title: &str,
    message: &str,
    notification_type: &str,
    webhook_url: Option<&str>,
) {
    let icon = match notification_type {
        "success" => "dialog-information",
        "error" => "dialog-error",
        "warning" => "dialog-warning",
        "info" => "dialog-information",
        _ => "dialog-information",
    };

    // Criar evento para webhook
    let event = NotificationEvent {
        event_type: notification_type.to_string(),
        title: title.to_string(),
        message: message.to_string(),
        timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        status: match notification_type {
            "success" => "success".to_string(),
            "error" => "error".to_string(),
            "warning" => "warning".to_string(),
            _ => "info".to_string(),
        },
    };

    // Try sending via notify-rust (system)
    let title = title.to_string();
    let message = message.to_string();
    let icon = icon.to_string();
    let webhook_url = webhook_url.map(|s| s.to_string());

    std::thread::spawn(move || {
        // 1. Try system notification
        let notif_result = Notification::new()
            .summary(&title)
            .body(&message)
            .icon(&icon)
            .timeout(5000)
            .show();

        match notif_result {
            Ok(_) => {
                println!(
                    "✅ [notify-rust] Notification sent: {} - {}",
                    title, message
                );
            }
            Err(e) => {
                eprintln!("⚠️ [notify-rust] Failed: {:?}", e);
                #[cfg(target_os = "macos")]
                {
                    eprintln!("   macOS tip: System Preferences > Notifications > Terminal");
                    eprintln!("   Certifique-se de que 'Allow Notifications' está ativado");
                }
            }
        }

        // 2. Send to webhook if configured
        if let Some(url) = webhook_url {
            send_webhook_notification(&event, &url);
        }
    });
}

/// Send notification to webhook (blocking; called from a notification
/// thread). It used to spawn a thread returning a future that was never
/// polled, so no webhook was ever sent.
fn send_webhook_notification(event: &NotificationEvent, webhook_url: &str) {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            tracing::warn!("[Webhook] cannot start a runtime: {}", e);
            return;
        }
    };
    runtime.block_on(async {
        match reqwest::Client::new()
            .post(webhook_url)
            .json(event)
            .send()
            .await
        {
            Ok(response) if response.status().is_success() => {
                tracing::info!("[Webhook] sent");
            }
            Ok(response) => tracing::warn!("[Webhook] status {}", response.status()),
            Err(e) => tracing::warn!("[Webhook] error sending: {}", e),
        }
    });
}

/// Check battery status
fn check_battery_status() -> (f32, bool, String) {
    match Manager::new() {
        Ok(manager) => match manager.batteries() {
            Ok(mut batteries) => {
                if let Some(battery) = batteries.next() {
                    match battery {
                        Ok(batt) => {
                            let percent = batt.state_of_charge().value * 100.0;
                            let is_charging = batt.state() == battery::State::Charging;
                            let energy_full = batt.energy_full().value;
                            let design = batt.energy_full_design().value;
                            let health = if design > 0.0 {
                                format!("{:.1}%", energy_full / design * 100.0)
                            } else {
                                "N/A".to_string()
                            };

                            return (percent, is_charging, health);
                        }
                        Err(e) => {
                            eprintln!("⚠️ Error getting battery info: {:?}", e);
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("⚠️ Error listing batteries: {:?}", e);
            }
        },
        Err(e) => {
            eprintln!("⚠️ Battery manager unavailable: {:?}", e);
        }
    }

    // Fallback: simulate with default values
    (100.0, true, "N/A".to_string())
}

/// Sync event for history
#[derive(Clone, Debug)]
struct SyncEvent {
    timestamp: String,
    event_type: String, // "file_synced", "backup_created", "error"
    details: String,
}

/// Repository settings of a watch without profile: back up `source` to the
/// `backup` folder, everything encrypted (`rsb watch` always encrypts).
fn watch_config(source: &std::path::Path, backup: &std::path::Path) -> Config {
    Config {
        encrypt_patterns: Some(vec!["*".to_string()]),
        compression_level: Some(3),
        ..Config::new(
            source.to_string_lossy().into_owned(),
            backup.to_string_lossy().into_owned(),
        )
    }
}

/// Appends an event to the history shown on screen (last 20 kept).
fn push_event(history: &mut Signal<Vec<SyncEvent>>, kind: &str, details: String) {
    let mut h = history();
    h.push(SyncEvent {
        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        event_type: kind.to_string(),
        details,
    });
    if h.len() > 20 {
        h.remove(0);
    }
    history.set(h);
}

/// Message shown once monitoring stopped (pending changes are not lost
/// silently: they need a manual backup).
fn stop_summary(stats: &SyncStats) -> String {
    let mut msg = format!(
        "⏹️ Monitoring stopped: {} change(s) backed up, {} backup(s) succeeded, {} failed",
        stats.synced, stats.backups_completed, stats.backups_failed
    );
    if stats.pending_changes > 0 {
        msg.push_str(&format!(
            "\n⚠️ {} change(s) were NOT backed up yet: run a backup to capture them.",
            stats.pending_changes
        ));
    }
    if let Some(e) = &stats.last_error {
        msg.push_str(&format!("\n❌ Last backup error: {}", e));
    }
    msg
}

#[component]
pub fn RealtimeSyncScreen() -> Element {
    let app_config = use_context::<AppConfig>();
    let texts = get_texts(app_config.language());

    let mut source_path = use_signal(PathBuf::new);
    let mut dest_path = use_signal(PathBuf::new);
    let mut backup_path = use_signal(PathBuf::new);
    let mut backup_password = use_signal(String::new);
    let mut is_monitoring = use_signal(|| false);
    let mut stopping = use_signal(|| false);
    let mut profile = use_signal(|| Option::<LoadedProfile>::None);
    let mut status_msg = use_signal(|| "Ready to monitor and backup".to_string());

    let mut files_synced = use_signal(|| 0usize);
    let mut files_changed = use_signal(|| 0usize);
    let mut backups_created = use_signal(|| 0usize);
    let mut sync_errors = use_signal(|| 0usize);

    // History events for dashboard
    let mut sync_history = use_signal(Vec::<SyncEvent>::new);
    let mut last_sync_time = use_signal(|| String::from("Nunca"));
    let mut success_rate = use_signal(|| 100.0);

    // Notificações do sistema
    let mut notifications_enabled = use_signal(|| true);

    // AlertDialog (visual fallback for notifications)
    let mut show_alert = use_signal(|| false);
    let mut alert_title = use_signal(String::new);
    let mut alert_message = use_signal(String::new);
    let mut alert_type = use_signal(|| String::from("info")); // "success", "error", "warning", "info"

    // Monitoramento de bateria
    let mut battery_percent = use_signal(|| 100.0);
    let mut battery_status = use_signal(|| "AC".to_string());
    let mut show_low_battery_alert = use_signal(|| false);
    let mut battery_health = use_signal(|| String::from("N/A"));

    // Webhook notifications
    let mut webhook_enabled = use_signal(|| false);
    let mut webhook_url = use_signal(String::new);
    let _show_webhook_config = use_signal(|| false);

    // Ignore configuration (patterns)
    let mut ignore_patterns = use_signal(|| {
        vec![
            ".*\\.tmp$".to_string(),
            ".*\\.lock$".to_string(),
            ".*\\.swp$".to_string(),
            ".git".to_string(),
            ".DS_Store".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
        ]
    });
    let mut new_pattern = use_signal(String::new);
    let mut show_ignore_editor = use_signal(|| false);
    let mut pattern_test_input = use_signal(String::new);
    let mut pattern_test_result = use_signal(String::new);

    // Function to show visual AlertDialog
    let mut show_dialog = move |title: &str, message: &str, alert_type_str: &str| {
        alert_title.set(title.to_string());
        alert_message.set(message.to_string());
        alert_type.set(alert_type_str.to_string());
        show_alert.set(true);

        // Auto-fechar após 5 segundos (para "success" e "info")
        if alert_type_str == "success" || alert_type_str == "info" {
            spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
                show_alert.set(false);
            });
        }
    };

    let handle_select_source = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                profile.set(None);
                source_path.set(folder.path().to_path_buf());
                status_msg.set(texts.source_folder_selected.to_string());
            }
        });
    };

    let handle_select_dest = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                dest_path.set(folder.path().to_path_buf());
                status_msg.set(texts.dest_folder_selected.to_string());
            }
        });
    };

    let handle_select_backup = move |_| {
        spawn(async move {
            if let Some(folder) = rfd::AsyncFileDialog::new().pick_folder().await {
                profile.set(None);
                backup_path.set(folder.path().to_path_buf());
                status_msg.set(texts.backup_folder_selected.to_string());
            }
        });
    };

    let handle_load_profile = move |_| {
        spawn(async move {
            if let Some(h) = rfd::AsyncFileDialog::new()
                .add_filter("TOML", &["toml"])
                .pick_file()
                .await
            {
                match load_profile(h.path()) {
                    Ok(p) => {
                        source_path.set(PathBuf::from(&p.config.source_path));
                        backup_path.set(PathBuf::from(repository_label(&p.config)));
                        status_msg.set(format!("✅ Profile loaded: {}", p.path.display()));
                        profile.set(Some(p));
                    }
                    Err(e) => status_msg.set(format!("❌ {}", e)),
                }
            }
        });
    };

    let handle_start_monitoring = move |_| {
        if is_monitoring() || stopping() {
            return;
        }
        let loaded = profile();
        if loaded.is_none() && source_path().as_os_str().is_empty() {
            status_msg.set(texts.select_source_folder.to_string());
            return;
        }
        if dest_path().as_os_str().is_empty() {
            status_msg.set(texts.select_dest_folder.to_string());
            return;
        }
        if loaded.is_none() && backup_path().as_os_str().is_empty() {
            status_msg.set(texts.select_backup_folder.to_string());
            return;
        }

        is_monitoring.set(true);
        status_msg.set("⏳ Starting...".to_string());
        files_synced.set(0);
        files_changed.set(0);
        backups_created.set(0);
        sync_errors.set(0);

        let cfg = match &loaded {
            Some(p) => p.config.clone(),
            None => watch_config(&source_path(), &backup_path()),
        };
        let mirror = dest_path();
        let typed = backup_password();
        let strategy = SyncStrategy {
            batch_interval: 2,
            ignore_patterns: ignore_patterns(),
            ..SyncStrategy::default()
        };
        let notify_enabled = notifications_enabled();
        let webhook = webhook_enabled()
            .then(|| webhook_url.read().clone())
            .filter(|u| !u.trim().is_empty());

        spawn(async move {
            // `rsb watch` always encrypts: no key, no watch.
            let Some(key) = resolve_key_blocking(typed, loaded.clone()).await else {
                status_msg.set(format!("❌ {}", texts.enter_backup_password));
                is_monitoring.set(false);
                return;
            };
            let mut cfg = cfg;
            cfg.encryption_key = Some(key);
            let source = rsb_sdk::utils::expand_path(&cfg.source_path);

            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<WatchEvent>();
            let rt = Arc::new(mirror_and_backup_sync(
                &cfg,
                mirror,
                strategy,
                Some(Arc::new(move |e| {
                    let _ = tx.send(e);
                })),
            ));
            if let Err(e) = rt.start(&source).await {
                status_msg.set(format!("❌ Cannot watch {}: {}", source.display(), e));
                is_monitoring.set(false);
                return;
            }
            // Back up once at start-up, then only on changes (like rsb watch).
            rt.queue_full_backup(&source).await;
            let loop_handle = rt.clone().spawn_processing_loop();
            status_msg.set("🟢 Monitoring changes (initial backup queued)...".to_string());
            if notify_enabled {
                send_system_notification("RS Shield", "Monitoring started", "info", None);
            }

            let mut last_failed = 0usize;
            let mut ticks = 0usize;
            while is_monitoring() {
                tokio::select! {
                    Some(event) = rx.recv() => {
                        let (kind, details, title) = match &event {
                            WatchEvent::Mirrored { files } => {
                                files_changed.set(files_changed() + files);
                                ("file_synced", format!("{} file(s) mirrored", files), "📄 Files Synchronized")
                            }
                            WatchEvent::BackedUp { number, summary } => {
                                backups_created.set(*number);
                                status_msg.set(format!("✅ Backup #{}: {}", number, summary));
                                ("backup_created", format!("Backup #{}: {}", number, summary), "💾 Backup Created")
                            }
                        };
                        push_event(&mut sync_history, kind, details.clone());
                        last_sync_time.set(chrono::Local::now().format("%H:%M:%S").to_string());
                        if notify_enabled {
                            send_system_notification(title, &details, "success", webhook.as_deref());
                        }
                    }
                    _ = tokio::time::sleep(tokio::time::Duration::from_secs(1)) => {}
                }

                let stats = rt.get_stats().await;
                files_synced.set(stats.synced);
                sync_errors.set(stats.backups_failed);
                let total = stats.backups_completed + stats.backups_failed;
                if total > 0 {
                    success_rate.set(stats.backups_completed as f64 / total as f64 * 100.0);
                }
                if stats.backups_failed > last_failed {
                    last_failed = stats.backups_failed;
                    let err = stats
                        .last_error
                        .clone()
                        .unwrap_or_else(|| "unknown error".into());
                    status_msg.set(format!(
                        "❌ Backup failed (will retry): {} — {} change(s) pending",
                        err, stats.pending_changes
                    ));
                    push_event(
                        &mut sync_history,
                        "error",
                        format!("Backup failed: {}", err),
                    );
                    if notify_enabled {
                        send_system_notification(
                            "❌ Backup Error",
                            &err,
                            "error",
                            webhook.as_deref(),
                        );
                    }
                }

                ticks += 1;
                if ticks.is_multiple_of(10) {
                    let (percent, is_charging, health) =
                        tokio::task::spawn_blocking(check_battery_status)
                            .await
                            .unwrap_or((100.0, true, "N/A".to_string()));
                    battery_percent.set(percent);
                    battery_status.set(if is_charging {
                        "⚡ Charging".to_string()
                    } else {
                        "🔋 Battery".to_string()
                    });
                    battery_health.set(health);
                    if percent < 15.0 && !is_charging && !show_low_battery_alert() {
                        show_low_battery_alert.set(true);
                        let msg = format!(
                            "Battery at {:.0}% - Consider connecting the charger",
                            percent
                        );
                        show_dialog("⚠️ Low Battery", &msg, "warning");
                        if notify_enabled {
                            send_system_notification(
                                "⚠️ Low Battery",
                                &msg,
                                "warning",
                                webhook.as_deref(),
                            );
                        }
                    } else if percent > 20.0 && is_charging {
                        show_low_battery_alert.set(false);
                    }
                }
            }

            // Stop: a backup in progress is allowed to finish (its snapshot
            // is all-or-nothing), then report what is still pending.
            stopping.set(true);
            status_msg
                .set("⏹️ Stopping — a backup in progress is allowed to finish...".to_string());
            rt.stop().await;
            let _ = loop_handle.await;
            let stats = rt.get_stats().await;
            files_synced.set(stats.synced);
            backups_created.set(stats.backups_completed);
            sync_errors.set(stats.backups_failed);
            status_msg.set(stop_summary(&stats));
            stopping.set(false);
            if notify_enabled {
                send_system_notification(
                    "⏹️ Monitoring Stopped",
                    &stop_summary(&stats),
                    "info",
                    None,
                );
            }
        });
    };

    let handle_stop_monitoring = move |_| {
        // The monitoring task sees this, stops the watcher and waits for a
        // running backup to finish.
        is_monitoring.set(false);
    };

    // Handlers to manage ignore patterns
    let handle_add_pattern = move |_| {
        let pattern = new_pattern().trim().to_string();
        if !pattern.is_empty() {
            let mut patterns = ignore_patterns();
            if !patterns.contains(&pattern) {
                patterns.push(pattern);
                ignore_patterns.set(patterns);
                new_pattern.set(String::new());
                status_msg.set("✅ Pattern added successfully".to_string());
            } else {
                status_msg.set("❌ Pattern already exists".to_string());
            }
        } else {
            status_msg.set("❌ Enter a valid pattern".to_string());
        }
    };

    let handle_test_pattern = move |_| {
        let test_input = pattern_test_input().trim().to_string();
        if test_input.is_empty() {
            pattern_test_result.set("❌ Enter a file path to test".to_string());
            return;
        }

        let patterns = ignore_patterns();
        let mut matched_patterns = Vec::new();

        for pattern in &patterns {
            if let Ok(regex) = regex::Regex::new(pattern)
                && regex.is_match(&test_input)
            {
                matched_patterns.push(pattern.clone());
            }
        }

        if matched_patterns.is_empty() {
            pattern_test_result.set(format!("✅ '{}' would NOT be ignored", test_input));
        } else {
            pattern_test_result.set(format!(
                "🚫 '{}' would be ignored by: {}",
                test_input,
                matched_patterns.join(", ")
            ));
        }
    };

    let handle_reset_patterns = move |_| {
        ignore_patterns.set(vec![
            ".*\\.tmp$".to_string(),
            ".*\\.lock$".to_string(),
            ".*\\.swp$".to_string(),
            ".git".to_string(),
            ".DS_Store".to_string(),
            "node_modules".to_string(),
            "target".to_string(),
        ]);
        status_msg.set("✅ Patterns restored to default".to_string());
    };

    rsx! {
        div { class: "card",
            h2 { class: "page-title", "⚡ Real-Time Sync with Backup" }

            p { class: "hint mb-4",
                "Watches a folder; after each burst of changes it mirrors new/modified files to the synchronization folder and runs an encrypted incremental backup (same engine as `rsb watch`)."
            }

            div { class: "form-group",
                label { class: "label-text", "📋 Profile (Optional, like `rsb watch <profile>`)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "No profile: use the folders below",
                        value: "{profile().map(|p| p.path.to_string_lossy().into_owned()).unwrap_or_default()}",
                        readonly: true
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_load_profile,
                        disabled: is_monitoring(),
                        "📂"
                    }
                    if profile().is_some() {
                        button {
                            class: "btn-icon",
                            onclick: move |_| profile.set(None),
                            disabled: is_monitoring(),
                            "✖"
                        }
                    }
                }
                p { class: "hint", "With a profile its source, repository (local or S3) and keyring key are used; the password below is then optional." }
            }

            div { class: "form-group",
                label { class: "label-text", "Folder to Monitor (Source)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Click to select",
                        value: "{source_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_source,
                        disabled: is_monitoring(),
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Synchronization Folder (Destination)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Click to select",
                        value: "{dest_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_dest,
                        disabled: is_monitoring(),
                        "📂"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "Backup Folder (Automatic)" }
                div { class: "flex gap-3",
                    input {
                        class: "input-field",
                        r#type: "text",
                        placeholder: "Click to select",
                        value: "{backup_path.read().to_string_lossy()}",
                        readonly: true,
                        disabled: is_monitoring()
                    }
                    button {
                        class: "btn-icon",
                        onclick: handle_select_backup,
                        disabled: is_monitoring(),
                        "💾"
                    }
                }
            }

            div { class: "form-group",
                label { class: "label-text", "🔐 Encryption Password" }
                input {
                    class: "input-field",
                    r#type: "password",
                    placeholder: "Enter a strong password",
                    value: "{backup_password()}",
                    oninput: move |evt| backup_password.set(evt.value()),
                    disabled: is_monitoring()
                }
            }

            if !status_msg().is_empty() {
                div {
                    class: "alert",
                    class: if status_msg().starts_with("✅") { "alert-success" } else if status_msg().starts_with("❌") { "alert-error" } else { "alert-info" },
                    "{status_msg}"
                }
            }

            // Notification settings
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-3 mb-3 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🔔" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "System Notifications" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                if notifications_enabled() { "Enabled" } else { "Disabled" }
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors",
                        class: if notifications_enabled() {
                            "bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 hover:bg-green-200 dark:hover:bg-green-900/50"
                        } else {
                            "bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600"
                        },
                        onclick: move |_| notifications_enabled.set(!notifications_enabled()),
                        if notifications_enabled() { "✅ Active" } else { "❌ Inactive" }
                    }
                }
                div { class: "mt-3 text-xs text-slate-600 dark:text-slate-400 space-y-1",
                    p { "📨 You will receive notifications for:" }
                    ul { class: "list-disc list-inside ml-1",
                        li { "Start and stop monitoring" }
                        li { "Files successfully synchronized" }
                        li { "Backups created and encrypted" }
                        li { "Errors during the process" }
                    }
                }
            }

            // Webhook Notifications Configuration
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-3 mb-3 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between mb-3",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🔗" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "Webhook Notifications" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                if webhook_enabled() { "Enabled" } else { "Disabled" }
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors",
                        class: if webhook_enabled() {
                            "bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-900/50"
                        } else {
                            "bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600"
                        },
                        onclick: move |_| webhook_enabled.set(!webhook_enabled()),
                        if webhook_enabled() { "✅ Active" } else { "❌ Inactive" }
                    }
                }

                if webhook_enabled() {
                    div { class: "space-y-3 border-t border-slate-200 dark:border-slate-600 pt-4",
                        div { class: "form-group",
                            label { class: "label-text text-xs", "Webhook URL (e.g., https://webhook.site/abc123)" }
                            input {
                                class: "input-field",
                                r#type: "text",
                                placeholder: "https://your-webhook.com/notify",
                                value: "{webhook_url()}",
                                oninput: move |evt| webhook_url.set(evt.value()),
                                disabled: is_monitoring()
                            }
                        }

                        div { class: "bg-blue-50 dark:bg-blue-900/20 rounded p-3 text-xs text-blue-700 dark:text-blue-300 border border-blue-200 dark:border-blue-800 space-y-2",
                            p { class: "font-semibold", "📤 JSON payload structure:" }
                            pre { class: "block font-mono text-xs overflow-auto bg-white dark:bg-slate-800 p-2 rounded border border-blue-200 dark:border-blue-700 text-left",
    r#"{{"#
    r#"  "event_type": "sync_complete","#
    r#"  "title": "Files Synchronized","#
    r#"  "message": "5 file(s) synchronized successfully","#
    r#"  "timestamp": "2026-02-07 14:30:45","#
    r#"  "status": "success""#
    r#"}}"#
                            }
                        }
                    }
                }
            }

            // Ignore patterns configuration
            div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-3 mb-3 border border-slate-200 dark:border-slate-700",
                div { class: "flex items-center justify-between mb-3",
                    div { class: "flex items-center gap-3",
                        span { class: "text-lg", "🚫" }
                        div {
                            p { class: "text-sm font-medium text-slate-900 dark:text-slate-100", "Ignore Patterns" }
                            p { class: "text-xs text-slate-600 dark:text-slate-400",
                                "{ignore_patterns().len()} patterns configured"
                            }
                        }
                    }
                    button {
                        class: "px-3 py-2 rounded-lg font-medium text-sm transition-colors bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                        onclick: move |_| show_ignore_editor.set(!show_ignore_editor()),
                        if show_ignore_editor() { "🔼 Fechar" } else { "⚙️ Editar" }
                    }
                }

                // Patterns editor (collapsible)
                if show_ignore_editor() {
                    div { class: "space-y-3 border-t border-slate-200 dark:border-slate-600 pt-4",
                        // Add new pattern
                        div { class: "flex gap-2",
                            input {
                                class: "input-field flex-1",
                                placeholder: "e.g., .*\\.temp$ or node_modules",
                                value: "{new_pattern()}",
                                oninput: move |evt| new_pattern.set(evt.value()),
                                disabled: is_monitoring()
                            }
                            button {
                                class: "btn-icon",
                                onclick: handle_add_pattern,
                                disabled: is_monitoring(),
                                "➕"
                            }
                        }

                        // Test pattern
                        div { class: "bg-white dark:bg-slate-800 rounded p-3 border border-slate-200 dark:border-slate-600",
                            p { class: "text-xs font-medium text-slate-700 dark:text-slate-300 mb-2", "🧪 Test Pattern" }
                            div { class: "flex gap-2 mb-2",
                                input {
                                    class: "input-field flex-1 text-sm",
                                    placeholder: "Enter a path (e.g., /home/user/.tmp/file.txt)",
                                    value: "{pattern_test_input()}",
                                    oninput: move |evt| pattern_test_input.set(evt.value()),
                                }
                                button {
                                    class: "px-3 py-1 rounded text-sm font-medium bg-blue-100 dark:bg-blue-900/30 text-blue-700 dark:text-blue-300 hover:bg-blue-200 dark:hover:bg-blue-900/50",
                                    onclick: handle_test_pattern,
                                    "Testar"
                                }
                            }
                            if !pattern_test_result().is_empty() {
                                p { class: "text-xs text-slate-700 dark:text-slate-300", "{pattern_test_result}" }
                            }
                        }

                        // Patterns list
                        div { class: "space-y-1",
                            for pattern in ignore_patterns() {
                                div { class: "flex items-center justify-between bg-white dark:bg-slate-800 p-2 rounded border border-slate-200 dark:border-slate-600",
                                    code { class: "text-xs font-mono text-slate-700 dark:text-slate-300 flex-1", "{pattern}" }
                                    button {
                                        class: "px-2 py-1 rounded text-xs font-medium bg-red-100 dark:bg-red-900/30 text-red-700 dark:text-red-300 hover:bg-red-200 dark:hover:bg-red-900/50",
                                        onclick: move |_| {
                                            let mut patterns = ignore_patterns();
                                            patterns.retain(|p| p != &pattern);
                                            ignore_patterns.set(patterns);
                                            status_msg.set(format!("✅ Pattern '{}' removed", pattern));
                                        },
                                        disabled: is_monitoring(),
                                        "✕"
                                    }
                                }
                            }
                        }

                        // Restore patterns button
                        button {
                            class: "w-full px-3 py-2 rounded text-sm font-medium bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                            onclick: handle_reset_patterns,
                            disabled: is_monitoring(),
                            "🔄 Restore Patterns"
                        }
                    }
                } else {
                    // Patterns preview (when collapsed)
                    div { class: "flex flex-wrap gap-2",
                        for pattern in ignore_patterns().iter().take(5) {
                            div { class: "px-2 py-1 rounded text-xs bg-white dark:bg-slate-800 border border-slate-300 dark:border-slate-600 text-slate-700 dark:text-slate-300",
                                code { "{pattern}" }
                            }
                        }
                        if ignore_patterns().len() > 5 {
                            div { class: "px-2 py-1 rounded text-xs bg-slate-200 dark:bg-slate-700 text-slate-600 dark:text-slate-400",
                                "+{ignore_patterns().len() - 5}"
                            }
                        }
                    }
                }
            }

            div { class: "flex gap-3 mb-4",
                button {
                    class: "btn-primary flex-1",
                    onclick: handle_start_monitoring,
                    disabled: is_monitoring() || stopping() || source_path().as_os_str().is_empty(),
                    if is_monitoring() { "🔴 Monitoring..." } else if stopping() { "⏳ Stopping..." } else { "▶️ Start" }
                }
                button {
                    class: "btn-secondary flex-1",
                    onclick: handle_stop_monitoring,
                    disabled: !is_monitoring(),
                    "⏹️ Stop"
                }
            }

            // Statistics panel
            div { class: "grid grid-cols-4 gap-3 mb-4",
                div { class: "bg-blue-50 dark:bg-blue-900/20 border border-blue-200 dark:border-blue-800 rounded-lg p-3",
                    p { class: "text-xs text-blue-600 dark:text-blue-400 font-medium", "Files Synced" }
                    p { class: "text-xl font-bold text-blue-900 dark:text-blue-300", "{files_synced}" }
                }
                div { class: "bg-green-50 dark:bg-green-900/20 border border-green-200 dark:border-green-800 rounded-lg p-3",
                    p { class: "text-xs text-green-600 dark:text-green-400 font-medium", "Changes Detected" }
                    p { class: "text-xl font-bold text-green-900 dark:text-green-300", "{files_changed}" }
                }
                div { class: "bg-purple-50 dark:bg-purple-900/20 border border-purple-200 dark:border-purple-800 rounded-lg p-3",
                    p { class: "text-xs text-purple-600 dark:text-purple-400 font-medium", "Backups Created" }
                    p { class: "text-xl font-bold text-purple-900 dark:text-purple-300", "{backups_created}" }
                }
                div { class: "bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-800 rounded-lg p-3",
                    p { class: "text-xs text-red-600 dark:text-red-400 font-medium", "Errors" }
                    p { class: "text-xl font-bold text-red-900 dark:text-red-300", "{sync_errors}" }
                }
            }

            // Visual dashboard with graphs
            if is_monitoring() {
                div { class: "card-section mb-4",
                    h3 { class: "section-title", "📊 Real-Time Dashboard" }

                    // Visual success rate
                    div { class: "mb-6",
                        div { class: "flex justify-between items-center mb-2",
                            span { class: "text-sm font-medium text-slate-700 dark:text-slate-300", "Success Rate" }
                            span { class: "text-sm font-bold text-slate-900 dark:text-slate-100", "{(success_rate() as i32)}%" }
                        }
                        div { class: "w-full bg-slate-200 dark:bg-slate-700 rounded-full h-3 overflow-hidden",
                            div {
                                class: "h-full transition-all duration-500",
                                class: if success_rate() >= 90.0 { "bg-green-500" } else if success_rate() >= 70.0 { "bg-yellow-500" } else { "bg-red-500" },
                                style: "width: {success_rate()}%"
                            }
                        }
                    }

                    // Last sync
                    div { class: "grid grid-cols-2 gap-3 mb-5",
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-3 border border-slate-200 dark:border-slate-700",
                            p { class: "text-xs text-slate-600 dark:text-slate-400 font-medium mb-1", "⏱️ Last Sync" }
                            p { class: "text-lg font-bold text-slate-900 dark:text-slate-100", "{last_sync_time}" }
                        }
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 border border-slate-200 dark:border-slate-700",
                            p { class: "text-xs text-slate-600 dark:text-slate-400 font-medium mb-1", "📈 Ratio" }
                            {
                                let total = backups_created() + sync_errors();
                                if total > 0 {
                                    rsx! {
                                        p { class: "text-lg font-bold text-slate-900 dark:text-slate-100",
                                            "{backups_created()}/{total} backups OK"
                                        }
                                    }
                                } else {
                                    rsx! {
                                        p { class: "text-lg font-bold text-slate-900 dark:text-slate-100",
                                            "Waiting for events"
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // History events
                    if !sync_history().is_empty() {
                        div { class: "bg-slate-50 dark:bg-slate-900/50 rounded-lg p-4 border border-slate-200 dark:border-slate-700",
                            p { class: "text-sm font-semibold text-slate-900 dark:text-slate-100 mb-3", "📜 History (Last 20 events)" }
                            div { class: "max-h-48 overflow-y-auto space-y-2",
                                    {
                                        let events: Vec<_> = sync_history().iter().rev().cloned().collect();
                                        events.into_iter().map(|event| {
                                            let (bg_color, icon, text_color) = match event.event_type.as_str() {
                                                "file_synced" => ("bg-blue-50 dark:bg-blue-900/30", "📄", "text-blue-900 dark:text-blue-200"),
                                                "backup_created" => ("bg-green-50 dark:bg-green-900/30", "✅", "text-green-900 dark:text-green-200"),
                                                "error" => ("bg-red-50 dark:bg-red-900/30", "❌", "text-red-900 dark:text-red-200"),
                                                _ => ("bg-slate-100 dark:bg-slate-800", "🔔", "text-slate-900 dark:text-slate-200"),
                                            };

                                            rsx! {
                                                div { key: "{event.timestamp}-{event.event_type}", class: "flex items-start gap-3 p-2 {bg_color} rounded border border-slate-200 dark:border-slate-700",
                                                    span { class: "text-lg flex-shrink-0", "{icon}" }
                                                    div { class: "flex-1 min-w-0",
                                                        div { class: "flex justify-between items-start gap-2",
                                                            p { class: "text-xs font-medium text-slate-600 dark:text-slate-400", "{event.timestamp}" }
                                                            span { class: "text-xs px-2 py-0.5 bg-slate-200 dark:bg-slate-700 rounded whitespace-nowrap {text_color}",
                                                                "{event.event_type}"
                                                            }
                                                        }
                                                        p { class: "text-sm text-slate-700 dark:text-slate-300 mt-1", "{event.details}" }
                                                    }
                                                }
                                            }
                                        })
                                    }
                            }
                        }
                    }
                }
            }

            // Information
            div { class: "card-section",
                h3 { class: "section-title", "ℹ️ How it Works" }
                div { class: "space-y-2 text-sm text-slate-700 dark:text-slate-300",
                    p { "🔍 Monitors folder in real-time (every 2 seconds)" }
                    p { "✅ Syncs new/modified files to destination" }
                    p { "💾 Creates automatic backup (.tar.gz) upon each detected change" }
                    p { "📊 Shows statistics: files, changes, backups" }
                    p { "🔴 Keep open for continuous operation" }
                }
            }


            // AlertDialog Modal (Visual Notification)
            if show_alert() {
                div { class: "fixed inset-0 bg-black/50 flex items-center justify-center z-50",
                    div { class: "bg-white dark:bg-slate-900 rounded-lg shadow-xl max-w-md w-full mx-4 border border-slate-200 dark:border-slate-700",
                        // Header
                        div { class: "flex items-center gap-3 p-4 border-b border-slate-200 dark:border-slate-700",
                            {
                                let icon = match alert_type().as_str() {
                                    "success" => "✅",
                                    "error" => "❌",
                                    "warning" => "⚠️",
                                    _ => "ℹ️",
                                };
                                rsx! { span { class: "text-2xl", "{icon}" } }
                            }
                            h2 { class: "text-lg font-bold text-slate-900 dark:text-slate-100", "{alert_title}" }
                        }

                        // Body
                        div { class: "p-4",
                            p { class: "text-slate-700 dark:text-slate-300 text-sm leading-relaxed", "{alert_message}" }
                        }

                        // Footer
                        div { class: "flex gap-2 p-4 border-t border-slate-200 dark:border-slate-700",
                            button {
                                class: "flex-1 px-4 py-2 rounded-lg font-medium text-sm transition-colors bg-slate-200 dark:bg-slate-700 text-slate-700 dark:text-slate-300 hover:bg-slate-300 dark:hover:bg-slate-600",
                                onclick: move |_| show_alert.set(false),
                                "Close"
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

    #[test]
    fn watch_without_profile_encrypts_everything() {
        let cfg = watch_config(std::path::Path::new("/src"), std::path::Path::new("/bkp"));
        assert!(cfg.encryption_required());
        assert_eq!(cfg.destination_path, "/bkp");
    }

    #[test]
    fn stop_summary_mentions_pending_changes_and_errors() {
        let stats = SyncStats {
            pending_changes: 2,
            synced: 5,
            failed: 1,
            backups_completed: 3,
            backups_failed: 1,
            backup_running: false,
            last_error: Some("disk full".into()),
        };
        let msg = stop_summary(&stats);
        assert!(msg.contains("2 change(s) were NOT backed up"));
        assert!(msg.contains("disk full"));
    }
}
