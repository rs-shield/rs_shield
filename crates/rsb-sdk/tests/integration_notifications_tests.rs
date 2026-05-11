/// Integration tests for complete notification system
/// Uses: Email, JSON Logging, History, and Chat Integrations
use rsb_sdk::core::{
    ChatIntegration, EmailConfig, EmailNotification, HistorySummary, NotificationHistory,
    NotificationHistoryEntry, NotificationLogger,
};
use std::fs;
use tempfile::TempDir;

#[test]
fn test_email_notification_creation() {
    let sync_notif = EmailNotification::sync_success(5, "2026-02-07 14:30:00");
    assert!(sync_notif.subject.contains("5"));
    assert!(sync_notif.body_text.contains("Synchronization"));

    let backup_notif = EmailNotification::backup_created("backup.tar.gz", "2026-02-07 14:30:00");
    assert!(backup_notif.subject.contains("backup.tar.gz"));
    assert!(backup_notif.body_html.contains("💾"));

    let error_notif = EmailNotification::error("Critical error", "2026-02-07 14:30:00");
    assert!(error_notif.subject.contains("Error"));
    assert!(error_notif.body_html.contains("❌"));

    let battery_notif = EmailNotification::low_battery(12.5, "2026-02-07 14:30:00");
    assert!(battery_notif.subject.contains("12"));
    assert!(battery_notif.body_html.contains("⚠️"));
}

#[test]
fn test_email_config() {
    let email_config = EmailConfig {
        smtp_server: "smtp.gmail.com".to_string(),
        smtp_port: 587,
        sender_email: "noreply@example.com".to_string(),
        sender_password: "password123".to_string(),
        recipient_email: "user@example.com".to_string(),
        use_tls: true,
    };

    assert_eq!(email_config.smtp_server, "smtp.gmail.com");
    assert_eq!(email_config.smtp_port, 587);
    assert!(email_config.use_tls);
}

#[test]
fn test_notification_logger_workflow() {
    let temp_dir = TempDir::new().unwrap();
    let logger = NotificationLogger::new(temp_dir.path()).unwrap();

    // Log multiple notifications
    logger
        .log_notification("sync", "Synchronization OK", "5 files", "success", None)
        .unwrap();

    logger
        .log_notification("backup", "Backup created", "backup.tar.gz", "success", None)
        .unwrap();

    logger
        .log_notification("error", "Backup error", "No disk space", "error", None)
        .unwrap();

    // Verify log files were created
    let entries = fs::read_dir(temp_dir.path()).unwrap().count();
    assert!(entries > 0, "Logs must be created");

    // Read all logs
    let all_logs = logger.read_all_logs().unwrap();
    assert_eq!(all_logs.len(), 3);

    // Filter by type
    let sync_logs = logger.filter_by_type("sync").unwrap();
    assert_eq!(sync_logs.len(), 1);
    assert_eq!(sync_logs[0].event_type, "sync");

    // Generate summary
    let summary = logger.generate_summary().unwrap();
    assert_eq!(summary["total_notifications"], 3);
}

#[test]
fn test_notification_history_workflow() {
    let mut history = NotificationHistory::new(10);

    // Add multiple events
    history.add_sync_success(5);
    history.add_backup_created("backup.tar.gz", Some(30));
    history.add_error("Synchronization", "Permission denied");
    history.add_battery_alert(12.5);
    history.add_webhook_sent("https://example.com", true);
    history.add_webhook_sent("https://example.com", false);

    // Verify size
    assert_eq!(history.len(), 6);
    assert!(!history.is_empty());

    // Get all
    let all = history.get_all();
    assert_eq!(all.len(), 6);

    // Get recent
    let recent = history.get_recent(3);
    assert_eq!(recent.len(), 3);

    // Filter by type
    let webhooks = history.filter_by_type("webhook");
    assert_eq!(webhooks.len(), 2);

    // Filter by status
    let errors = history.filter_by_status("error");
    assert!(errors.len() > 0);

    // Summary
    let summary = history.get_summary();
    assert_eq!(summary.total_notifications, 6);
    assert!(summary.success_count > 0);
    assert!(summary.error_count > 0);
    assert!(summary.success_rate() <= 100.0);
    assert!(summary.most_common_type().is_some());

    // Clear
    history.clear();
    assert!(history.is_empty());
}

#[test]
fn test_chat_integration_slack() {
    let slack = ChatIntegration::Slack {
        webhook_url: "https://hooks.slack.com/services/T123/B123/ABC".to_string(),
        mention_user: Some("U123456".to_string()),
    };

    match slack {
        ChatIntegration::Slack {
            webhook_url,
            mention_user,
        } => {
            assert!(webhook_url.contains("hooks.slack.com"));
            assert_eq!(mention_user, Some("U123456".to_string()));
        }
        _ => panic!("Expected Slack integration"),
    }
}

#[test]
fn test_chat_integration_telegram() {
    let telegram = ChatIntegration::Telegram {
        bot_token: "123456789:ABCdefGHIjklmnoPQRstuvWXYZ".to_string(),
        chat_id: "-1001234567890".to_string(),
    };

    match telegram {
        ChatIntegration::Telegram { bot_token, chat_id } => {
            assert!(bot_token.contains("ABCdef"));
            assert!(chat_id.contains("-100"));
        }
        _ => panic!("Expected Telegram integration"),
    }
}

#[test]
fn test_chat_integration_discord() {
    let discord = ChatIntegration::Discord {
        webhook_url: "https://discord.com/api/webhooks/123456789/ABC".to_string(),
    };

    match discord {
        ChatIntegration::Discord { webhook_url } => {
            assert!(webhook_url.contains("discord.com"));
        }
        _ => panic!("Expected Discord integration"),
    }
}

#[test]
fn test_notification_logger_with_extra_data() {
    let temp_dir = TempDir::new().unwrap();
    let logger = NotificationLogger::new(temp_dir.path()).unwrap();

    // Log with extra data
    logger
        .log_notification(
            "sync",
            "Synchronization",
            "Files synchronized",
            "success",
            Some(serde_json::json!({
                "files_count": 5,
                "total_bytes": 1024000,
                "duration_secs": 30
            })),
        )
        .unwrap();

    let logs = logger.read_all_logs().unwrap();
    assert_eq!(logs.len(), 1);
    assert!(!logs[0].extra_data.is_null());
}

#[test]
fn test_notification_history_max_entries_limit() {
    let mut history = NotificationHistory::new(3);

    // Add more than the limit
    for i in 0..5 {
        history.add_sync_success(i);
    }

    // Should keep only 3 last
    assert_eq!(history.len(), 3);
}

#[test]
fn test_notification_entry_with_duration() {
    let mut history = NotificationHistory::new(10);

    history.add_backup_created("backup.tar.gz", Some(125));

    let all = history.get_all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].duration_secs, Some(125));
}

#[test]
fn test_history_summary_success_rate() {
    let mut history = NotificationHistory::new(10);

    // 2 sucessos, 1 erro = 66.67% sucesso
    history.add_sync_success(5);
    history.add_sync_success(10);
    history.add_error("Test", "Error");

    let summary = history.get_summary();
    let rate = summary.success_rate();

    assert!(rate > 60.0 && rate < 70.0);
}

#[test]
fn test_email_notification_html_format() {
    let notif = EmailNotification::sync_success(10, "2026-02-07 15:00:00");

    // Check well-formed HTML
    assert!(notif.body_html.contains("<html>"));
    assert!(notif.body_html.contains("</html>"));
    assert!(notif.body_html.contains("<table"));
    assert!(notif.body_html.contains("synchronized")); // Check relevant content
    assert!(notif.body_html.contains("2026-02-07 15:00:00")); // Check timestamp
}

#[test]
fn test_chat_integration_enum_pattern_matching() {
    let integrations = vec![
        ChatIntegration::Slack {
            webhook_url: "https://slack.com".to_string(),
            mention_user: None,
        },
        ChatIntegration::Telegram {
            bot_token: "token123".to_string(),
            chat_id: "123456".to_string(),
        },
        ChatIntegration::Discord {
            webhook_url: "https://discord.com".to_string(),
        },
    ];

    let mut slack_count = 0;
    let mut telegram_count = 0;
    let mut discord_count = 0;

    for integration in integrations {
        match integration {
            ChatIntegration::Slack { .. } => slack_count += 1,
            ChatIntegration::Telegram { .. } => telegram_count += 1,
            ChatIntegration::Discord { .. } => discord_count += 1,
        }
    }

    assert_eq!(slack_count, 1);
    assert_eq!(telegram_count, 1);
    assert_eq!(discord_count, 1);
}

#[test]
fn test_notification_logger_filter_by_type() {
    let temp_dir = TempDir::new().unwrap();
    let logger = NotificationLogger::new(temp_dir.path()).unwrap();

    // Add different types
    logger
        .log_notification("sync", "S1", "M", "success", None)
        .unwrap();
    logger
        .log_notification("backup", "B1", "M", "success", None)
        .unwrap();
    logger
        .log_notification("sync", "S2", "M", "success", None)
        .unwrap();
    logger
        .log_notification("error", "E1", "M", "error", None)
        .unwrap();

    // Filter
    let syncs = logger.filter_by_type("sync").unwrap();
    let backups = logger.filter_by_type("backup").unwrap();
    let errors = logger.filter_by_type("error").unwrap();

    assert_eq!(syncs.len(), 2);
    assert_eq!(backups.len(), 1);
    assert_eq!(errors.len(), 1);
}

#[test]
fn test_history_find_by_id() {
    let mut history = NotificationHistory::new(10);

    history.add_sync_success(5);
    let all = history.get_all();
    let id = all[0].id.clone();

    let found = history.find_by_id(&id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().id, id);
}

#[test]
fn test_notification_history_entry_structure() {
    let entry = NotificationHistoryEntry {
        id: "test-id".to_string(),
        timestamp: "2026-02-07T14:30:00".to_string(),
        event_type: "sync".to_string(),
        title: "Test".to_string(),
        message: "Test message".to_string(),
        status: "success".to_string(),
        duration_secs: Some(30),
    };

    assert_eq!(entry.event_type, "sync");
    assert_eq!(entry.status, "success");
    assert_eq!(entry.duration_secs, Some(30));
}

#[test]
fn test_multiple_email_configs() {
    let gmail = EmailConfig {
        smtp_server: "smtp.gmail.com".to_string(),
        smtp_port: 587,
        sender_email: "user@gmail.com".to_string(),
        sender_password: "password".to_string(),
        recipient_email: "dest@example.com".to_string(),
        use_tls: true,
    };

    let outlook = EmailConfig {
        smtp_server: "smtp-mail.outlook.com".to_string(),
        smtp_port: 587,
        sender_email: "user@outlook.com".to_string(),
        sender_password: "password".to_string(),
        recipient_email: "dest@example.com".to_string(),
        use_tls: true,
    };

    assert_ne!(gmail.smtp_server, outlook.smtp_server);
    assert_eq!(gmail.smtp_port, outlook.smtp_port);
}
