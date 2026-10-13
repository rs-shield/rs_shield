//! Side effects shared by the operation screens: HTML reports and
//! notifications.

use crate::ui::integrations_screen::IntegrationConfig;
use chrono::Local;
use rsb_sdk::core::{NotificationEvent, NotificationManager, NotificationPayload};
use rsb_sdk::report::ReportData;
use std::path::{Path, PathBuf};

/// Directory for the HTML reports of the desktop app
/// (`~/.rs-shield/reports`). The CLI writes them to its working directory,
/// but a GUI app's working directory is arbitrary (often `/` on macOS).
pub fn reports_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".rs-shield")
        .join("reports")
}

/// Writes the HTML report of an operation (`kind` = backup/restore/verify)
/// into `dir` and returns its path.
pub fn write_report_in(dir: &Path, kind: &str, report: &ReportData) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {}", dir.display(), e))?;
    let path = dir.join(format!(
        "rsb-report-{}-{}.html",
        kind,
        Local::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, rsb_sdk::report::generate_html(report))
        .map_err(|e| format!("Cannot write {}: {}", path.display(), e))?;
    Ok(path)
}

/// [`write_report_in`] the default [`reports_dir`].
pub fn write_report(kind: &str, report: &ReportData) -> Result<PathBuf, String> {
    write_report_in(&reports_dir(), kind, report)
}

/// Sends a notification through the integrations configured in the
/// Integrations screen. Failures are logged (a notification problem must
/// not turn a successful backup into a failure).
pub async fn notify(event: NotificationEvent, title: &str, message: String, details: String) {
    let integrations = IntegrationConfig::load_default();
    let mut manager = NotificationManager::new();
    let email = integrations.to_email_config();
    let chats = integrations.to_chat_integrations();
    if email.is_none() && chats.is_empty() {
        return;
    }
    if let Some(cfg) = email {
        manager.set_email_config(cfg);
    }
    for chat in chats {
        manager.add_chat_integration(chat);
    }
    let payload = NotificationPayload {
        event,
        title: title.to_string(),
        message,
        details: Some(details),
        timestamp: Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
    };
    if let Err(e) = manager.send(&payload).await {
        tracing::warn!("Notification '{}' could not be sent: {}", title, e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_is_written_in_the_reports_dir() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("reports");
        let report = ReportData {
            operation: "Backup".into(),
            status: "Success".into(),
            ..Default::default()
        };
        let p = write_report_in(&sub, "backup", &report).unwrap();
        assert!(p.starts_with(&sub));
        assert!(std::fs::read_to_string(&p).unwrap().contains("Success"));
    }
}
