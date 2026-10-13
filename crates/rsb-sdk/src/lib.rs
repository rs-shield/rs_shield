pub mod auth;
pub mod backup;
pub mod config;
pub mod core;
pub mod credentials;
pub mod crypto;
pub mod error;
pub mod fido2;
#[cfg(any(test, feature = "fuzzing"))]
#[doc(hidden)]
pub mod fuzzing;
pub mod integrity;
pub mod metrics;
pub mod operation;
pub mod options;
pub mod portable_restore;
pub mod realtime;
pub mod report;
pub mod s3_check;
pub mod schedule;
pub mod server;
pub mod snapshot;
pub mod storage;
pub mod utils;

pub use crate::core::cancellation::CancellationToken;
pub use config::{Config, create_profile, load_config};
pub use core::{perform_prune, perform_prune_by_age, perform_restore, plan_prune_by_count};
pub use credentials::{CredentialsManager, S3Credentials, SecureString, encryption};
pub use error::{CryptoErrorKind, RsbError};
pub use integrity::perform_verify;
pub use options::{BackupOptions, RestoreOptions, VerifyOptions};
pub use portable_restore::{
    BackupValidation, config_from_backup_path, restore_from_config_or_backup,
    validate_backup_structure,
};
pub use realtime::{
    ChangeQueue, ChangeType, FileChange, RealtimeSync, RealtimeWatcher, SyncStats, SyncStrategy,
};
pub use s3_check::verify_s3_connection;
