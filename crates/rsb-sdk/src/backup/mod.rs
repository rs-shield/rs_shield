// Backup modules organized by responsibility
pub mod discovery;
pub mod metadata;
pub mod progress;
pub mod processing;
pub mod stats;
pub mod threading;

pub use processing::perform_backup;
pub use processing::perform_backup_with_cancellation;
