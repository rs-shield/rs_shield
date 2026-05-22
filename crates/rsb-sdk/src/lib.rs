pub mod auth;
pub mod backup;
pub mod config;
pub mod core;
pub mod credentials;
pub mod crypto;
pub mod fido2;
pub mod integrity;
pub mod realtime;
pub mod report;
pub mod s3_check;
pub mod server;
pub mod snapshot;
pub mod storage;
pub mod utils;
pub mod operation;
pub mod metrics;

pub use crate::core::cancellation::CancellationToken;
pub use config::{Config, create_profile, load_config};
pub use core::{perform_backup, perform_prune, perform_restore};
pub use credentials::{CredentialsManager, SecureString, encryption};
pub use integrity::perform_verify;
pub use realtime::{
    ChangeQueue, ChangeType, FileChange, RealtimeSync, RealtimeWatcher, SyncStats, SyncStrategy,
};
pub use s3_check::verify_s3_connection;
