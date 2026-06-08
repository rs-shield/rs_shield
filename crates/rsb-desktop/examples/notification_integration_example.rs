// Example: How to integrate notifications in RSB Desktop

// In your backup operation (e.g., backup_screen.rs):
use rsb_sdk::core::{NotificationManager, NotificationEvent, NotificationPayload, EmailConfig, ChatIntegration};

async fn perform_backup_with_notifications() {
    // Load integration config
    let integration_config = rsb_desktop::ui::integrations_screen::IntegrationConfig::load(
        &std::path::PathBuf::from("~/.rs-shield/default.toml")
    );

    // Create notification manager with configured integrations
    let mut notif_manager = NotificationManager::new();
    
    // Add email if configured
    if let Some(email_cfg) = integration_config.to_email_config() {
        notif_manager.set_email_config(email_cfg);
    }

    // Add chat integrations if configured
    for chat_integration in integration_config.to_chat_integrations() {
        notif_manager.add_chat_integration(chat_integration);
    }

    // Notify backup started
    let start_payload = NotificationPayload::new(
        NotificationEvent::BackupStarted,
        "Backup Started",
        "Your backup operation has started"
    );
    let _ = notif_manager.send(&start_payload).await;

    // Perform backup...
    match perform_backup().await {
        Ok(result) => {
            // Notify success
            let success_payload = NotificationPayload::new(
                NotificationEvent::BackupCompleted,
                "Backup Completed Successfully",
                format!("Backed up {} files successfully", result.file_count)
            )
            .with_details(format!("Size: {:.2} MB", result.total_size));

            let _ = notif_manager.send(&success_payload).await;
        }
        Err(e) => {
            // Notify error
            let error_payload = NotificationPayload::new(
                NotificationEvent::BackupFailed,
                "Backup Failed",
                format!("Error: {}", e)
            );

            let _ = notif_manager.send(&error_payload).await;
        }
    }
}

// Example for restore operation:
async fn perform_restore_with_notifications() {
    let integration_config = rsb_desktop::ui::integrations_screen::IntegrationConfig::load(
        &std::path::PathBuf::from("~/.rs-shield/default.toml")
    );

    let mut notif_manager = NotificationManager::new();
    
    if let Some(email_cfg) = integration_config.to_email_config() {
        notif_manager.set_email_config(email_cfg);
    }

    for chat_integration in integration_config.to_chat_integrations() {
        notif_manager.add_chat_integration(chat_integration);
    }

    // Notify restore started
    let start_payload = NotificationPayload::new(
        NotificationEvent::RestoreStarted,
        "Restore Started",
        "Restore operation has started"
    );
    let _ = notif_manager.send(&start_payload).await;

    // Perform restore...
    match perform_restore().await {
        Ok(result) => {
            let success_payload = NotificationPayload::new(
                NotificationEvent::RestoreCompleted,
                "Restore Completed",
                format!("Restored {} files successfully", result.file_count)
            )
            .with_details(format!("To: {}", result.destination_path));

            let _ = notif_manager.send(&success_payload).await;
        }
        Err(e) => {
            let error_payload = NotificationPayload::new(
                NotificationEvent::RestoreFailed,
                "Restore Failed",
                format!("Error: {}", e)
            );

            let _ = notif_manager.send(&error_payload).await;
        }
    }
}

// Example for real-time sync:
async fn perform_sync_with_notifications() {
    let integration_config = rsb_desktop::ui::integrations_screen::IntegrationConfig::load(
        &std::path::PathBuf::from("~/.rs-shield/default.toml")
    );

    let mut notif_manager = NotificationManager::new();
    
    if let Some(email_cfg) = integration_config.to_email_config() {
        notif_manager.set_email_config(email_cfg);
    }

    for chat_integration in integration_config.to_chat_integrations() {
        notif_manager.add_chat_integration(chat_integration);
    }

    match perform_realtime_sync().await {
        Ok(sync_stats) => {
            let success_payload = NotificationPayload::new(
                NotificationEvent::SyncCompleted,
                "Sync Completed",
                "Real-time synchronization completed"
            )
            .with_details(format!("{}", sync_stats.total_files_synced));

            let _ = notif_manager.send(&success_payload).await;
        }
        Err(e) => {
            let error_payload = NotificationPayload::new(
                NotificationEvent::SyncFailed,
                "Sync Failed",
                format!("Real-time sync error: {}", e)
            );

            let _ = notif_manager.send(&error_payload).await;
        }
    }
}

// Example for low battery notification:
async fn check_battery_and_notify() {
    use battery::Manager;

    let integration_config = rsb_desktop::ui::integrations_screen::IntegrationConfig::load(
        &std::path::PathBuf::from("~/.rs-shield/default.toml")
    );

    let mut notif_manager = NotificationManager::new();
    
    if let Some(email_cfg) = integration_config.to_email_config() {
        notif_manager.set_email_config(email_cfg);
    }

    for chat_integration in integration_config.to_chat_integrations() {
        notif_manager.add_chat_integration(chat_integration);
    }

    if let Ok(manager) = Manager::new() {
        if let Ok(battery) = manager.iter().next() {
            let percent = battery.state_of_charge().value * 100.0;
            
            if percent < 15.0 {
                let warning_payload = NotificationPayload::new(
                    NotificationEvent::LowBattery,
                    "Low Battery Warning",
                    format!("Battery is at {:.0}%. Backup may pause.", percent)
                );

                let _ = notif_manager.send(&warning_payload).await;
            }
        }
    }
}

// Testing notifications from integration screen:
async fn test_all_notifications() {
    let integration_config = rsb_desktop::ui::integrations_screen::IntegrationConfig::load(
        &std::path::PathBuf::from("~/.rs-shield/default.toml")
    );

    let mut notif_manager = NotificationManager::new();
    
    if let Some(email_cfg) = integration_config.to_email_config() {
        notif_manager.set_email_config(email_cfg);
    }

    for chat_integration in integration_config.to_chat_integrations() {
        notif_manager.add_chat_integration(chat_integration);
    }

    // Test all event types
    let test_events = vec![
        (NotificationEvent::BackupCompleted, "Backup Complete", "Test backup notification"),
        (NotificationEvent::SyncCompleted, "Sync Complete", "Test sync notification"),
        (NotificationEvent::LowBattery, "Battery Warning", "Test battery notification"),
    ];

    for (event, title, message) in test_events {
        let payload = NotificationPayload::new(event, title, message);
        match notif_manager.send(&payload).await {
            Ok(_) => println!("✅ {} sent", title),
            Err(e) => println!("❌ {} failed: {}", title, e),
        }
    }
}
