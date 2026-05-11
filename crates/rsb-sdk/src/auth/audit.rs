use crate::auth::types::AuditLogEntry;
use chrono::Utc;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

pub struct AuditLogger {
    entries: Arc<Mutex<Vec<AuditLogEntry>>>,
    file_path: Option<String>,
}

impl AuditLogger {
    pub fn new(file_path: Option<String>) -> Self {
        Self {
            entries: Arc::new(Mutex::new(Vec::new())),
            file_path,
        }
    }

    pub async fn log_auth_success(&self, user_id: &str, ip_address: &str, user_agent: &str) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "auth_success".to_string(),
            status: "success".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({}),
        };

        self.log_entry(entry).await;
    }

    pub async fn log_auth_failure(
        &self,
        user_id: &str,
        ip_address: &str,
        user_agent: &str,
        reason: &str,
    ) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "auth_failure".to_string(),
            status: "failure".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({"reason": reason}),
        };

        self.log_entry(entry).await;
    }

    pub async fn log_backup_start(&self, user_id: &str, ip_address: &str, user_agent: &str) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "backup_start".to_string(),
            status: "success".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({}),
        };

        self.log_entry(entry).await;
    }

    pub async fn log_backup_complete(
        &self,
        user_id: &str,
        ip_address: &str,
        user_agent: &str,
        files_count: u64,
    ) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "backup_complete".to_string(),
            status: "success".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({"files_count": files_count}),
        };

        self.log_entry(entry).await;
    }

    pub async fn log_credentials_unlock(&self, user_id: &str, ip_address: &str, user_agent: &str) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "credentials_unlock".to_string(),
            status: "success".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({}),
        };

        self.log_entry(entry).await;
    }

    pub async fn log_logout(&self, user_id: &str, ip_address: &str, user_agent: &str) {
        let entry = AuditLogEntry {
            timestamp: Utc::now(),
            user_id: user_id.to_string(),
            event_type: "logout".to_string(),
            status: "success".to_string(),
            ip_address: ip_address.to_string(),
            user_agent: user_agent.to_string(),
            details: json!({}),
        };

        self.log_entry(entry).await;
    }

    async fn log_entry(&self, entry: AuditLogEntry) {
        let mut entries = self.entries.lock().await;

        // Log via tracing
        info!(
            event = entry.event_type,
            user = entry.user_id,
            status = entry.status,
            ip = entry.ip_address,
            "Audit event"
        );

        entries.push(entry.clone());

        // Se tiver file_path, salvar em arquivo
        if let Some(path) = &self.file_path {
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = tokio::fs::write(path, format!("{}\n", json)).await;
            }
        }
    }

    pub async fn get_all_entries(&self) -> Vec<AuditLogEntry> {
        self.entries.lock().await.clone()
    }

    pub async fn get_entries_for_user(&self, user_id: &str) -> Vec<AuditLogEntry> {
        self.entries
            .lock()
            .await
            .iter()
            .filter(|e| e.user_id == user_id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_audit_logger_records_events() {
        let logger = AuditLogger::new(None);

        logger
            .log_auth_success("user1", "127.0.0.1", "Mozilla/5.0")
            .await;

        let entries = logger.get_all_entries().await;
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].event_type, "auth_success");
    }

    #[tokio::test]
    async fn test_audit_logger_filters_by_user() {
        let logger = AuditLogger::new(None);

        logger
            .log_auth_success("user1", "127.0.0.1", "Mozilla/5.0")
            .await;
        logger
            .log_auth_success("user2", "127.0.0.1", "Mozilla/5.0")
            .await;

        let user1_entries = logger.get_entries_for_user("user1").await;
        assert_eq!(user1_entries.len(), 1);
        assert_eq!(user1_entries[0].user_id, "user1");
    }
}
