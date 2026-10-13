// Re-exports everything for easier external use (e.g., rsb_sdk::core::perform_backup)
pub mod cancellation;
pub mod chat_integrations;
pub mod dedup_index;
pub mod email_notifications;
pub mod file_processor;
pub mod limits;
pub mod manifest;
pub mod notification_history;
pub mod notification_logger;
pub mod notifications;
pub mod prune;
pub mod resource_monitor;
pub mod restore;
pub mod restore_select;
pub mod storage_backend;
pub mod telegram_validator;
pub mod types;

pub use chat_integrations::ChatIntegration;
pub use email_notifications::{EmailConfig, EmailNotification};
pub use notification_history::{HistorySummary, NotificationHistory, NotificationHistoryEntry};
pub use notification_logger::NotificationLogger;
pub use notifications::{NotificationEvent, NotificationManager, NotificationPayload, TestResults};
pub use prune::{
    PruneStats, perform_prune, perform_prune_by_age, perform_prune_by_age_with_storage,
    perform_prune_with_storage, plan_prune_by_count, plan_prune_by_count_with_storage,
    preview_prune, prune_with_stats, prune_with_storage,
};
pub use restore::perform_restore;
pub use restore_select::{
    FileSelector, find_snapshot_at_or_before, find_snapshot_at_or_before_in, parse_restore_date,
};
pub use telegram_validator::{get_telegram_chat_id, validate_telegram_token};
