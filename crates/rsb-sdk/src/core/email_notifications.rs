use lettre::transport::smtp::SmtpTransport;
use lettre::{Message, Transport};
use serde::{Deserialize, Serialize};
use tracing::{error, info};

/// Email configuration for notifications
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EmailConfig {
    pub smtp_server: String,     // e.g., "smtp.gmail.com"
    pub smtp_port: u16,          // e.g., 587
    pub sender_email: String,    // e.g., "noreply@rs-shield.local"
    pub sender_password: String, // Password or API key
    pub recipient_email: String, // Recipient
    pub use_tls: bool,           // Use TLS
}

/// Email notification structure
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct EmailNotification {
    pub subject: String,
    pub body_html: String,
    pub body_text: String,
}

impl EmailNotification {
    /// Creates a successful synchronization notification
    #[allow(dead_code)]
    pub fn sync_success(files_count: usize, timestamp: &str) -> Self {
        let body_text = format!(
            "Synchronization Complete\n\
             =====================================\n\
             Files synchronized: {}\n\
             Time: {}\n\n\
             RS Shield System\n\
             https://github.com/zebedeu/rs_shield",
            files_count, timestamp
        );
        let body_html = format!(
            "<html><body style=\"font-family: Arial, sans-serif;\">\
             <h2 style=\"color: #10b981;\">✅ Synchronization Complete</h2>\
             <table style=\"border-collapse: collapse;\">\
             <tr><td style=\"padding: 8px;\"><b>Files:</b></td><td style=\"padding: 8px;\">{} synchronized</td></tr>\
             <tr><td style=\"padding: 8px;\"><b>Time:</b></td><td style=\"padding: 8px;\">{}</td></tr>\
             </table>\
             <p style=\"color: #666; font-size: 12px; margin-top: 20px;\">\
             RS Shield System - Backup &amp; Sync\
             </p>\
             </body></html>",
            files_count, timestamp
        );

        Self {
            subject: format!("RS Shield: ✅ Synchronization of {} file(s)", files_count),
            body_html,
            body_text,
        }
    }

    /// Creates a backup created notification
    #[allow(dead_code)]
    pub fn backup_created(backup_name: &str, timestamp: &str) -> Self {
        let body_text = format!(
            "Backup Created Successfully\n\
             =====================================\n\
             Name: {}\n\
             Time: {}\n\n\
             RS Shield System\n\
             https://github.com/zebedeu/rs_shield",
            backup_name, timestamp
        );

        let body_html = format!(
            "<html><body style=\"font-family: Arial, sans-serif;\">\
             <h2 style=\"color: #3b82f6;\">💾 Backup Created Successfully</h2>\
             <table style=\"border-collapse: collapse;\">\
             <tr><td style=\"padding: 8px;\"><b>File:</b></td><td style=\"padding: 8px; font-family: monospace;\">{}</td></tr>\
             <tr><td style=\"padding: 8px;\"><b>Time:</b></td><td style=\"padding: 8px;\">{}</td></tr>\
             </table>\
             <p style=\"color: #666; font-size: 12px; margin-top: 20px;\">\
             RS Shield - Automatic Backup System\
             </p>\
             </body></html>",
            backup_name, timestamp
        );

        Self {
            subject: format!("RS Shield: 💾 Backup '{}' Created", backup_name),
            body_html,
            body_text,
        }
    }

    /// Creates an error notification
    #[allow(dead_code)]
    pub fn error(error_msg: &str, timestamp: &str) -> Self {
        let body_text = format!(
            "Error in Synchronization/Backup\n\
             =====================================\n\
             Error: {}\n\
             Time: {}\n\n\
             RS Shield System",
            error_msg, timestamp
        );

        let body_html = format!(
            "<html><body style=\"font-family: Arial, sans-serif;\">\
             <h2 style=\"color: #ef4444;\">❌ Error Detected</h2>\
             <p style=\"background-color: #fee2e2; padding: 10px; border-left: 4px solid #dc2626;\">\
             <code>{}</code>\
             </p>\
             <p style=\"color: #666; font-size: 14px;\">Time: {}</p>\
             <p style=\"color: #666; font-size: 12px; margin-top: 20px;\">\
             RS Shield - Automatic Backup System\
             </p>\
             </body></html>",
            error_msg, timestamp
        );

        Self {
            subject: "RS Shield: ❌ Error in operation".to_string(),
            body_html,
            body_text,
        }
    }

    /// Creates a low battery notification
    #[allow(dead_code)]
    pub fn low_battery(percent: f64, timestamp: &str) -> Self {
        let body_text = format!(
            "Alert: Low Battery\n\
             =====================================\n\
             Level: {:.0}%\n\
             Time: {}\n\n\
             Consider connecting the charger to continue the backup.\n\
             RS Shield System",
            percent, timestamp
        );

        let body_html = format!(
            "<html><body style=\"font-family: Arial, sans-serif;\">\
             <h2 style=\"color: #f59e0b;\">⚠️ Alert: Low Battery</h2>\
             <div style=\"background-color: #fef3c7; padding: 15px; border-left: 4px solid #d97706; border-radius: 4px;\">\
             <p style=\"margin: 0; font-size: 18px;\"><b>{:.0}%</b> battery remaining</p>\
             <p style=\"margin: 10px 0 0 0; color: #92400e;\">Synchronization may be paused automatically</p>\
             </div>\
             <p style=\"color: #666; font-size: 14px; margin-top: 15px;\">Time: {}</p>\
             <p style=\"color: #666; font-size: 12px; margin-top: 20px;\">\
             RS Shield - Automatic Backup System\
             </p>\
             </body></html>",
            percent, timestamp
        );

        Self {
            subject: format!("RS Shield: ⚠️ Alert: Low Battery ({:.0}%)", percent),
            body_html,
            body_text,
        }
    }
}

/// Send notification by email
#[allow(dead_code)]
pub async fn send_email_notification(
    config: &EmailConfig,
    notification: &EmailNotification,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create message
    let email = Message::builder()
        .from(config.sender_email.parse()?)
        .to(config.recipient_email.parse()?)
        .subject(&notification.subject)
        .multipart(
            lettre::message::MultiPart::alternative()
                .singlepart(lettre::message::SinglePart::plain(
                    notification.body_text.clone(),
                ))
                .singlepart(lettre::message::SinglePart::html(
                    notification.body_html.clone(),
                )),
        )?;

    // Create SMTP transport
    let transport = if config.use_tls {
        SmtpTransport::starttls_relay(&config.smtp_server)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
            .port(config.smtp_port)
            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                config.sender_email.clone(),
                config.sender_password.clone(),
            ))
            .build()
    } else {
        SmtpTransport::builder_dangerous(&config.smtp_server)
            .port(config.smtp_port)
            .build()
    };

    // Send email
    match transport.send(&email) {
        Ok(_) => {
            info!("✅ Email sent successfully to {}", config.recipient_email);
            Ok(())
        }
        Err(e) => {
            error!("❌ Failed to send email: {:?}", e);
            Err(Box::new(e))
        }
    }
}

/// Blocking version of send_email_notification (for use in threads)
#[allow(dead_code)]
pub fn send_email_notification_blocking(
    config: &EmailConfig,
    notification: &EmailNotification,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create message
    let email = Message::builder()
        .from(config.sender_email.parse()?)
        .to(config.recipient_email.parse()?)
        .subject(&notification.subject)
        .multipart(
            lettre::message::MultiPart::alternative()
                .singlepart(lettre::message::SinglePart::plain(
                    notification.body_text.clone(),
                ))
                .singlepart(lettre::message::SinglePart::html(
                    notification.body_html.clone(),
                )),
        )?;

    // Create SMTP transport
    let transport = if config.use_tls {
        SmtpTransport::starttls_relay(&config.smtp_server)
            .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?
            .port(config.smtp_port)
            .credentials(lettre::transport::smtp::authentication::Credentials::new(
                config.sender_email.clone(),
                config.sender_password.clone(),
            ))
            .build()
    } else {
        SmtpTransport::builder_dangerous(&config.smtp_server)
            .port(config.smtp_port)
            .build()
    };

    // Send email
    match transport.send(&email) {
        Ok(_) => {
            println!("✅ Email sent successfully to {}", config.recipient_email);
            Ok(())
        }
        Err(e) => {
            eprintln!("❌ Failed to send email: {:?}", e);
            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_sync_success_notification() {
        let notif = EmailNotification::sync_success(5, "2026-02-07 14:30:00");
        assert!(notif.subject.contains("5"));
        assert!(notif.body_text.contains("Synchronization Complete"));
        assert!(notif.body_html.contains("✅"));
    }

    #[test]
    fn test_create_backup_notification() {
        let notif =
            EmailNotification::backup_created("backup-2026-02-07.tar.gz", "2026-02-07 14:30:00");
        assert!(notif.subject.contains("backup-2026-02-07.tar.gz"));
        assert!(notif.body_html.contains("💾"));
    }

    #[test]
    fn test_create_error_notification() {
        let notif = EmailNotification::error(
            "Permission denied when accessing file",
            "2026-02-07 14:30:00",
        );
        assert!(notif.subject.contains("Error"));
        assert!(notif.body_html.contains("❌"));
    }

    #[test]
    fn test_create_battery_notification() {
        let notif = EmailNotification::low_battery(12.5, "2026-02-07 14:30:00");
        assert!(notif.subject.contains("12"));
        assert!(notif.body_html.contains("⚠️"));
    }
}
