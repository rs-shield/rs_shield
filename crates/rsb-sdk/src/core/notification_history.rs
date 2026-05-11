use chrono::Local;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

/// Notification history entry
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[allow(dead_code)]
pub struct NotificationHistoryEntry {
    pub id: String,
    pub timestamp: String,
    pub event_type: String, // "sync", "backup", "error", "battery", "webhook"
    pub title: String,
    pub message: String,
    pub status: String,             // "success", "error", "warning", "info"
    pub duration_secs: Option<u64>, // Duração da operação se aplicável
}

/// In-memory notification history manager
#[allow(dead_code)]
pub struct NotificationHistory {
    entries: VecDeque<NotificationHistoryEntry>,
    max_entries: usize,
}

impl NotificationHistory {
    /// Create new history with entry limit
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: VecDeque::with_capacity(max_entries),
            max_entries,
        }
    }

    /// Add entry to history
    pub fn add_entry(&mut self, entry: NotificationHistoryEntry) {
        self.entries.push_front(entry);

        // Keep only the last N entries
        if self.entries.len() > self.max_entries {
            self.entries.pop_back();
        }
    }

    /// Add successful synchronization notification
    pub fn add_sync_success(&mut self, files_count: usize) {
        self.add_entry(NotificationHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: "sync".to_string(),
            title: "📄 Synchronization Complete".to_string(),
            message: format!("{} file(s) synchronized successfully", files_count),
            status: "success".to_string(),
            duration_secs: None,
        });
    }

    /// Add backup created notification
    pub fn add_backup_created(&mut self, backup_name: &str, duration_secs: Option<u64>) {
        self.add_entry(NotificationHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: "backup".to_string(),
            title: "💾 Backup Created".to_string(),
            message: format!("Backup '{}' created successfully", backup_name),
            status: "success".to_string(),
            duration_secs,
        });
    }

    /// Add error notification
    pub fn add_error(&mut self, title: &str, error_msg: &str) {
        self.add_entry(NotificationHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: "error".to_string(),
            title: format!("❌ {}", title),
            message: error_msg.to_string(),
            status: "error".to_string(),
            duration_secs: None,
        });
    }

    /// Add battery alert
    pub fn add_battery_alert(&mut self, percent: f64) {
        self.add_entry(NotificationHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: "battery".to_string(),
            title: "⚠️ Low Battery".to_string(),
            message: format!("Battery at {:.0}% - Synchronization may be paused", percent),
            status: "warning".to_string(),
            duration_secs: None,
        });
    }

    /// Add webhook sent notification
    pub fn add_webhook_sent(&mut self, webhook_url: &str, success: bool) {
        self.add_entry(NotificationHistoryEntry {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp: Local::now().to_rfc3339(),
            event_type: "webhook".to_string(),
            title: "🔗 Webhook".to_string(),
            message: if success {
                format!("Webhook sent successfully to {}", webhook_url)
            } else {
                format!("Failed to send webhook to {}", webhook_url)
            },
            status: if success {
                "success".to_string()
            } else {
                "error".to_string()
            },
            duration_secs: None,
        });
    }

    /// Get all entries
    pub fn get_all(&self) -> Vec<NotificationHistoryEntry> {
        self.entries.iter().cloned().collect()
    }

    /// Get last N entries
    pub fn get_recent(&self, count: usize) -> Vec<NotificationHistoryEntry> {
        self.entries.iter().take(count).cloned().collect()
    }

    /// Filter by event type
    pub fn filter_by_type(&self, event_type: &str) -> Vec<NotificationHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Filter by status
    pub fn filter_by_status(&self, status: &str) -> Vec<NotificationHistoryEntry> {
        self.entries
            .iter()
            .filter(|e| e.status == status)
            .cloned()
            .collect()
    }

    /// Get statistical summary
    pub fn get_summary(&self) -> HistorySummary {
        let total = self.entries.len();
        let success = self
            .entries
            .iter()
            .filter(|e| e.status == "success")
            .count();
        let errors = self.entries.iter().filter(|e| e.status == "error").count();
        let warnings = self
            .entries
            .iter()
            .filter(|e| e.status == "warning")
            .count();

        let mut by_type: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        for entry in self.entries.iter() {
            *by_type.entry(entry.event_type.clone()).or_insert(0) += 1;
        }

        HistorySummary {
            total_notifications: total,
            success_count: success,
            error_count: errors,
            warning_count: warnings,
            info_count: total.saturating_sub(success + errors + warnings),
            by_type,
        }
    }

    /// Limpar histórico
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Obter tamanho atual
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Verificar se está vazio
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Buscar entrada por ID
    pub fn find_by_id(&self, id: &str) -> Option<NotificationHistoryEntry> {
        self.entries.iter().find(|e| e.id == id).cloned()
    }
}

/// Resumo estatístico do histórico
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct HistorySummary {
    pub total_notifications: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub warning_count: usize,
    pub info_count: usize,
    pub by_type: std::collections::HashMap<String, usize>,
}

impl HistorySummary {
    /// Taxa de sucesso em percentual
    pub fn success_rate(&self) -> f64 {
        if self.total_notifications == 0 {
            100.0
        } else {
            (self.success_count as f64 / self.total_notifications as f64) * 100.0
        }
    }

    /// Horário com melhor desempenho (baseado em sucessos)
    pub fn most_common_type(&self) -> Option<String> {
        self.by_type
            .iter()
            .max_by_key(|(_, count)| *count)
            .map(|(k, _)| k.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_history() {
        let history = NotificationHistory::new(10);
        assert_eq!(history.len(), 0);
    }

    #[test]
    fn test_add_sync_success() {
        let mut history = NotificationHistory::new(10);
        history.add_sync_success(5);
        assert_eq!(history.len(), 1);
        assert_eq!(history.get_all()[0].event_type, "sync");
    }

    #[test]
    fn test_add_backup_created() {
        let mut history = NotificationHistory::new(10);
        history.add_backup_created("backup.tar.gz", Some(30));
        assert_eq!(history.len(), 1);
        assert_eq!(history.get_all()[0].event_type, "backup");
    }

    #[test]
    fn test_add_error() {
        let mut history = NotificationHistory::new(10);
        history.add_error("Erro de sincronização", "Permissão negada");
        assert_eq!(history.len(), 1);
        assert_eq!(history.get_all()[0].status, "error");
    }

    #[test]
    fn test_max_entries_limit() {
        let mut history = NotificationHistory::new(3);
        for i in 0..5 {
            history.add_sync_success(i);
        }
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_filter_by_type() {
        let mut history = NotificationHistory::new(10);
        history.add_sync_success(1);
        history.add_backup_created("test.tar", None);
        history.add_sync_success(2);

        let syncs = history.filter_by_type("sync");
        assert_eq!(syncs.len(), 2);
    }

    #[test]
    fn test_summary() {
        let mut history = NotificationHistory::new(10);
        history.add_sync_success(5);
        history.add_error("Erro", "Msg");

        let summary = history.get_summary();
        assert_eq!(summary.total_notifications, 2);
        assert_eq!(summary.success_count, 1);
        assert_eq!(summary.error_count, 1);
    }

    #[test]
    fn test_success_rate() {
        let mut history = NotificationHistory::new(10);
        history.add_sync_success(5);
        history.add_sync_success(3);
        history.add_error("E", "M");

        let summary = history.get_summary();
        assert!((summary.success_rate() - 66.66).abs() < 1.0);
    }

    #[test]
    fn test_clear_history() {
        let mut history = NotificationHistory::new(10);
        history.add_sync_success(5);
        history.clear();
        assert!(history.is_empty());
    }
}
