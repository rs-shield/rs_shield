// Re-exports everything for easier external use (e.g., rsb_sdk::core::perform_backup)
pub mod cancellation;
pub mod chat_integrations;
pub mod email_notifications;
pub mod file_processor;
pub mod manifest;
pub mod notification_history;
pub mod notification_logger;
pub mod notifications;
pub mod prune;
pub mod resource_monitor;
pub mod restore;
pub mod storage_backend;
pub mod telegram_validator;
pub mod types;

pub use chat_integrations::ChatIntegration;
pub use email_notifications::{EmailConfig, EmailNotification};
pub use notification_history::{HistorySummary, NotificationHistory, NotificationHistoryEntry};
pub use notification_logger::NotificationLogger;
pub use notifications::{NotificationEvent, NotificationManager, NotificationPayload, TestResults};
pub use prune::perform_prune;
pub use restore::perform_restore;
pub use telegram_validator::{get_telegram_chat_id, validate_telegram_token};
