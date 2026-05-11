use crate::utils::ensure_directory_exists;
use chrono::Local;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{error, info};

/// Notification event structure for logging
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct NotificationLogEntry {
    pub id: String,         // UUID for tracking
    pub timestamp: String,  // ISO 8601 format
    pub event_type: String, // "sync", "backup", "error", "battery", "webhook"
    pub title: String,
    pub message: String,
    pub status: String,                // "success", "error", "warning", "info"
    pub extra_data: serde_json::Value, // Additional data
}

/// JSON notification log manager
#[allow(dead_code)]
pub struct NotificationLogger {
    log_dir: PathBuf,
}

impl NotificationLogger {
    /// Create a new logger with a specified directory
    pub fn new(log_dir: impl AsRef<Path>) -> std::io::Result<Self> {
        let log_dir = log_dir.as_ref().to_path_buf();

        // Create directory if it doesn't exist
        let log_dir_str = log_dir.to_str().ok_or(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Invalid path characters",
        ))?;
        ensure_directory_exists(log_dir_str).map_err(std::io::Error::other)?;

        Ok(Self { log_dir })
    }

    /// Log a notification
    pub fn log_notification(
        &self,
        event_type: &str,
        title: &str,
        message: &str,
        status: &str,
        extra_data: Option<serde_json::Value>,
    ) -> std::io::Result<()> {
        let entry = NotificationLogEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: event_type.to_string(),
            title: title.to_string(),
            message: message.to_string(),
            status: status.to_string(),
            extra_data: extra_data.unwrap_or(serde_json::json!({})),
        };

        self.append_to_log(&entry)
    }

    /// Add entry to the JSON log file
    fn append_to_log(&self, entry: &NotificationLogEntry) -> std::io::Result<()> {
        let today = Local::now().format("%Y-%m-%d").to_string();
        let log_file = self.log_dir.join(format!("notifications-{}.jsonl", today));

        // Serialize entry to JSON and add a newline
        let json_line = serde_json::to_string(entry).unwrap() + "\n";

        // Append to the file (JSONL - JSON Lines format)
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)?
            .write_all(json_line.as_bytes())?;

        info!("📝 Notification logged: {}", log_file.display());
        Ok(())
    }

    /// Read all notifications for a day
    pub fn read_day_logs(&self, date: &str) -> std::io::Result<Vec<NotificationLogEntry>> {
        let log_file = self.log_dir.join(format!("notifications-{}.jsonl", date));

        if !log_file.exists() {
            return Ok(Vec::new());
        }

        let content = fs::read_to_string(log_file)?;
        let entries: Vec<NotificationLogEntry> = content
            .lines()
            .filter_map(|line| {
                if line.trim().is_empty() {
                    None
                } else {
                    serde_json::from_str(line).ok()
                }
            })
            .collect();

        Ok(entries)
    }

    /// Read all notifications with a filter
    pub fn read_all_logs(&self) -> std::io::Result<Vec<NotificationLogEntry>> {
        let mut entries = Vec::new();

        // Read all JSONL files
        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "jsonl") {
                match fs::read_to_string(&path) {
                    Ok(content) => {
                        for line in content.lines() {
                            if let Ok(entry) = serde_json::from_str::<NotificationLogEntry>(line) {
                                entries.push(entry);
                            }
                        }
                    }
                    Err(e) => error!("Error reading {}: {}", path.display(), e),
                }
            }
        }

        // Sort by timestamp descending
        entries.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        Ok(entries)
    }

    /// Filter logs by event type
    pub fn filter_by_type(&self, event_type: &str) -> std::io::Result<Vec<NotificationLogEntry>> {
        let all = self.read_all_logs()?;
        Ok(all
            .into_iter()
            .filter(|e| e.event_type == event_type)
            .collect())
    }

    /// Filter logs by status
    pub fn filter_by_status(&self, status: &str) -> std::io::Result<Vec<NotificationLogEntry>> {
        let all = self.read_all_logs()?;
        Ok(all.into_iter().filter(|e| e.status == status).collect())
    }

    /// Generate summary report
    pub fn generate_summary(&self) -> std::io::Result<serde_json::Value> {
        let all = self.read_all_logs()?;

        let total = all.len();
        let success = all.iter().filter(|e| e.status == "success").count();
        let errors = all.iter().filter(|e| e.status == "error").count();
        let warnings = all.iter().filter(|e| e.status == "warning").count();

        let by_type: std::collections::HashMap<String, usize> =
            all.iter()
                .fold(std::collections::HashMap::new(), |mut map, e| {
                    *map.entry(e.event_type.clone()).or_insert(0) += 1;
                    map
                });

        Ok(serde_json::json!({
            "total_notifications": total,
            "by_status": {
                "success": success,
                "error": errors,
                "warning": warnings,
                "info": total - success - errors - warnings
            },
            "by_type": by_type,
            "first_notification": all.last().map(|e| e.timestamp.clone()),
            "last_notification": all.first().map(|e| e.timestamp.clone()),
        }))
    }

    /// Clean up old logs (older than N days)
    pub fn cleanup_old_logs(&self, days: u32) -> std::io::Result<usize> {
        let now = Local::now();
        let mut removed = 0;

        for entry in fs::read_dir(&self.log_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "jsonl") {
                if let Ok(metadata) = path.metadata() {
                    if let Ok(modified) = metadata.modified() {
                        let age = now.timestamp().saturating_sub(
                            modified
                                .duration_since(std::time::UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_secs() as i64,
                        );

                        if age > (days as i64 * 86400) {
                            fs::remove_file(&path)?;
                            removed += 1;
                            info!("🗑️ Old log removed: {}", path.display());
                        }
                    }
                }
            }
        }

        Ok(removed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_create_logger() {
        let temp_dir = TempDir::new().unwrap();
        let _logger = NotificationLogger::new(temp_dir.path()).unwrap();
        assert!(temp_dir.path().exists());
    }

    #[test]
    fn test_log_notification() {
        let temp_dir = TempDir::new().unwrap();
        let logger = NotificationLogger::new(temp_dir.path()).unwrap();

        let result = logger.log_notification(
            "sync",
            "Synchronization",
            "5 files synchronized",
            "success",
            None,
        );

        assert!(result.is_ok());
    }

    #[test]
    fn test_read_logs() {
        let temp_dir = TempDir::new().unwrap();
        let logger = NotificationLogger::new(temp_dir.path()).unwrap();

        logger
            .log_notification("sync", "Test", "Test message", "success", None)
            .unwrap();

        let logs = logger.read_all_logs().unwrap();
        assert!(!logs.is_empty());
        assert_eq!(logs[0].event_type, "sync");
    }

    #[test]
    fn test_filter_by_type() {
        let temp_dir = TempDir::new().unwrap();
        let logger = NotificationLogger::new(temp_dir.path()).unwrap();

        logger
            .log_notification("sync", "Test1", "Msg", "success", None)
            .unwrap();
        logger
            .log_notification("backup", "Test2", "Msg", "success", None)
            .unwrap();

        let sync_logs = logger.filter_by_type("sync").unwrap();
        assert_eq!(sync_logs.len(), 1);
    }

    #[test]
    fn test_summary() {
        let temp_dir = TempDir::new().unwrap();
        let logger = NotificationLogger::new(temp_dir.path()).unwrap();

        logger
            .log_notification("sync", "T1", "M", "success", None)
            .unwrap();
        logger
            .log_notification("sync", "T2", "M", "error", None)
            .unwrap();

        let summary = logger.generate_summary().unwrap();
        assert_eq!(summary["total_notifications"], 2);
    }
}
