use crate::core::{ChatIntegration, EmailConfig, EmailNotification};
use serde::{Deserialize, Serialize};
use std::fmt;
use tracing::{error, info};

/// Notification event types
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum NotificationEvent {
    BackupStarted,
    BackupCompleted,
    BackupFailed,
    RestoreStarted,
    RestoreCompleted,
    RestoreFailed,
    SyncCompleted,
    SyncFailed,
    LowBattery,
    StorageWarning,
    VerificationCompleted,
    VerificationFailed,
}

impl fmt::Display for NotificationEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NotificationEvent::BackupStarted => write!(f, "Backup Started"),
            NotificationEvent::BackupCompleted => write!(f, "Backup Completed"),
            NotificationEvent::BackupFailed => write!(f, "Backup Failed"),
            NotificationEvent::RestoreStarted => write!(f, "Restore Started"),
            NotificationEvent::RestoreCompleted => write!(f, "Restore Completed"),
            NotificationEvent::RestoreFailed => write!(f, "Restore Failed"),
            NotificationEvent::SyncCompleted => write!(f, "Sync Completed"),
            NotificationEvent::SyncFailed => write!(f, "Sync Failed"),
            NotificationEvent::LowBattery => write!(f, "Low Battery"),
            NotificationEvent::StorageWarning => write!(f, "Storage Warning"),
            NotificationEvent::VerificationCompleted => write!(f, "Verification Completed"),
            NotificationEvent::VerificationFailed => write!(f, "Verification Failed"),
        }
    }
}

/// Notification payload with context
#[derive(Debug, Clone)]
pub struct NotificationPayload {
    pub event: NotificationEvent,
    pub title: String,
    pub message: String,
    pub details: Option<String>,
    pub timestamp: String,
}

impl NotificationPayload {
    pub fn new(event: NotificationEvent, title: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            event,
            title: title.into(),
            message: message.into(),
            details: None,
            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }

    fn get_notification_type(&self) -> &str {
        match self.event {
            NotificationEvent::BackupCompleted
            | NotificationEvent::RestoreCompleted
            | NotificationEvent::SyncCompleted
            | NotificationEvent::VerificationCompleted => "success",
            NotificationEvent::BackupFailed
            | NotificationEvent::RestoreFailed
            | NotificationEvent::SyncFailed
            | NotificationEvent::VerificationFailed => "error",
            NotificationEvent::LowBattery | NotificationEvent::StorageWarning => "warning",
            _ => "info",
        }
    }
}

/// Unified notification manager
pub struct NotificationManager {
    email_config: Option<EmailConfig>,
    chat_integrations: Vec<ChatIntegration>,
}

impl NotificationManager {
    pub fn new() -> Self {
        Self {
            email_config: None,
            chat_integrations: Vec::new(),
        }
    }

    pub fn with_email(mut self, config: EmailConfig) -> Self {
        self.email_config = Some(config);
        self
    }

    pub fn with_chat_integration(mut self, integration: ChatIntegration) -> Self {
        self.chat_integrations.push(integration);
        self
    }

    pub fn add_chat_integration(&mut self, integration: ChatIntegration) {
        self.chat_integrations.push(integration);
    }

    pub fn set_email_config(&mut self, config: EmailConfig) {
        self.email_config = Some(config);
    }

    /// Send notification via all configured channels
    pub async fn send(&self, payload: &NotificationPayload) -> Result<(), NotificationError> {
        let mut results = Vec::new();

        // Send email
        if let Some(email_config) = &self.email_config {
            match self.send_email(email_config, payload).await {
                Ok(_) => results.push(Ok("email")),
                Err(e) => results.push(Err(format!("email: {}", e))),
            }
        }

        // Send to chat integrations
        for integration in &self.chat_integrations {
            match self.send_chat(integration, payload).await {
                Ok(_) => results.push(Ok("chat")),
                Err(e) => results.push(Err(format!("chat: {}", e))),
            }
        }

        // Check if any succeeded
        let successes = results.iter().filter(|r| r.is_ok()).count();
        if successes == 0 && !results.is_empty() {
            let errors: Vec<String> = results.iter().filter_map(|r| r.as_ref().err().cloned()).collect();
            return Err(NotificationError::AllChannelsFailed(errors.join("; ")));
        }

        Ok(())
    }

    /// Send email notification
    async fn send_email(
        &self,
        config: &EmailConfig,
        payload: &NotificationPayload,
    ) -> Result<(), String> {
        let notification = match payload.event {
            NotificationEvent::BackupCompleted => {
                let details = payload.details.as_deref().unwrap_or("Backup completed successfully");
                EmailNotification::backup_created(&payload.title, &payload.timestamp)
            }
            NotificationEvent::SyncCompleted => {
                let files_count = payload.details
                    .as_ref()
                    .and_then(|d| d.parse::<usize>().ok())
                    .unwrap_or(0);
                EmailNotification::sync_success(files_count, &payload.timestamp)
            }
            NotificationEvent::LowBattery => {
                let percent = payload.details
                    .as_ref()
                    .and_then(|d| d.parse::<f64>().ok())
                    .unwrap_or(0.0);
                EmailNotification::low_battery(percent, &payload.timestamp)
            }
            _ => {
                // Generic error or info email
                if payload.get_notification_type() == "error" {
                    EmailNotification::error(&payload.message, &payload.timestamp)
                } else {
                    // Create generic notification
                    let body_text = format!(
                        "{}\n\
                         ===========================\n\
                         {}\n\
                         Time: {}",
                        payload.title, payload.message, payload.timestamp
                    );
                    let body_html = format!(
                        "<html><body style=\"font-family: Arial, sans-serif;\">\
                         <h2>{}</h2>\
                         <p>{}</p>\
                         <p style=\"color: #666; font-size: 12px; margin-top: 20px;\">\
                         RS Shield System - Backup &amp; Sync<br/>Time: {}\
                         </p>\
                         </body></html>",
                        payload.title, payload.message, payload.timestamp
                    );
                    EmailNotification {
                        subject: format!("RS Shield: {}", payload.title),
                        body_html,
                        body_text,
                    }
                }
            }
        };

        crate::core::email_notifications::send_email_notification(config, &notification)
            .await
            .map_err(|e| e.to_string())
    }

    /// Send chat notification
    async fn send_chat(
        &self,
        integration: &ChatIntegration,
        payload: &NotificationPayload,
    ) -> Result<(), String> {
        let full_message = if let Some(details) = &payload.details {
            format!("{}\n{}", payload.message, details)
        } else {
            payload.message.clone()
        };

        crate::core::chat_integrations::send_chat_notification(
            integration,
            &payload.title,
            &full_message,
            payload.get_notification_type(),
        )
        .await
        .map_err(|e| e.to_string())
    }

    /// Test notification configuration
    pub async fn test(&self) -> Result<TestResults, Box<dyn std::error::Error>> {
        let mut results = TestResults::default();

        if let Some(email_config) = &self.email_config {
            let test_notification = EmailNotification {
                subject: "RS Shield: 🧪 Test Email".to_string(),
                body_text: "This is a test email from RS Shield to verify your configuration."
                    .to_string(),
                body_html: "<html><body><h2>🧪 Test Email</h2><p>This is a test email from RS Shield to verify your configuration.</p></body></html>".to_string(),
            };

            match crate::core::email_notifications::send_email_notification(
                email_config,
                &test_notification,
            )
            .await
            {
                Ok(_) => {
                    results.email_success = true;
                    info!("✅ Email test passed");
                }
                Err(e) => {
                    results.email_error = Some(e.to_string());
                    error!("❌ Email test failed: {}", e);
                }
            }
        }

        for (idx, integration) in self.chat_integrations.iter().enumerate() {
            let channel_name = match integration {
                ChatIntegration::Slack { .. } => "Slack",
                ChatIntegration::Telegram { .. } => "Telegram",
                ChatIntegration::Discord { .. } => "Discord",
            };

            match crate::core::chat_integrations::send_chat_notification(
                integration,
                "🧪 Test Notification",
                "This is a test notification from RS Shield to verify your configuration.",
                "info",
            )
            .await
            {
                Ok(_) => {
                    results.chat_success.push(channel_name.to_string());
                    info!("✅ {} test passed", channel_name);
                }
                Err(e) => {
                    results
                        .chat_errors
                        .insert(channel_name.to_string(), e.to_string());
                    error!("❌ {} test failed: {}", channel_name, e);
                }
            }
        }

        Ok(results)
    }
}

impl Default for NotificationManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Test results
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TestResults {
    pub email_success: bool,
    pub email_error: Option<String>,
    pub chat_success: Vec<String>,
    pub chat_errors: std::collections::HashMap<String, String>,
}

/// Notification errors
#[derive(Debug)]
pub enum NotificationError {
    AllChannelsFailed(String),
    NoChannelsConfigured,
    InvalidConfiguration(String),
}

impl fmt::Display for NotificationError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NotificationError::AllChannelsFailed(msg) => {
                write!(f, "All notification channels failed: {}", msg)
            }
            NotificationError::NoChannelsConfigured => {
                write!(f, "No notification channels configured")
            }
            NotificationError::InvalidConfiguration(msg) => {
                write!(f, "Invalid notification configuration: {}", msg)
            }
        }
    }
}

impl std::error::Error for NotificationError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_payload_creation() {
        let payload = NotificationPayload::new(
            NotificationEvent::BackupCompleted,
            "Backup Done",
            "All files backed up successfully",
        )
        .with_details("100 files");

        assert_eq!(payload.event, NotificationEvent::BackupCompleted);
        assert_eq!(payload.title, "Backup Done");
        assert!(payload.details.is_some());
    }

    #[test]
    fn test_notification_event_display() {
        assert_eq!(NotificationEvent::BackupCompleted.to_string(), "Backup Completed");
        assert_eq!(NotificationEvent::SyncFailed.to_string(), "Sync Failed");
    }

    #[test]
    fn test_notification_type_classification() {
        let success_event = NotificationPayload::new(
            NotificationEvent::BackupCompleted,
            "test",
            "test",
        );
        assert_eq!(success_event.get_notification_type(), "success");

        let error_event = NotificationPayload::new(NotificationEvent::BackupFailed, "test", "test");
        assert_eq!(error_event.get_notification_type(), "error");
    }
}
