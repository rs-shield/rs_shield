//! Portable Restore Module
//!
//! Provides unified restore functionality for both CLI and Desktop UI.
//! Allows restoring backups directly from a backup path without requiring a config file.
//! This is essential for cross-platform portability.

use crate::config::Config;
use crate::core;
use crate::error::RsbError;
use crate::options::RestoreOptions;
use crate::report::ReportData;
use std::path::Path;

/// Creates a minimal config from a backup path
///
/// This is used when restoring directly from a backup folder without a config file.
/// The config includes the backup path as the destination and accepts encryption key.
///
/// # Arguments
/// * `backup_path` - Path to the backup folder
/// * `encryption_key` - Optional encryption key for encrypted backups
///
/// # Returns
/// A minimal Config structure suitable for restore operations
///
/// # Example
/// ```ignore
/// let config = config_from_backup_path("/path/to/backup", Some("password".to_string()))?;
/// let opts = RestoreOptions::new(target_path).with_encryption_key(Some("password"));
/// let report = perform_restore(&config, opts).await?;
/// ```
pub fn config_from_backup_path(
    backup_path: &Path,
    encryption_key: Option<String>,
) -> crate::error::Result<Config> {
    if !backup_path.exists() {
        return Err(RsbError::NotFound(format!(
            "Backup path not found: {}",
            backup_path.display()
        )));
    }

    // Validate backup structure
    let snapshots_dir = backup_path.join("snapshots");
    let data_dir = backup_path.join("data");

    if !snapshots_dir.exists() || !data_dir.exists() {
        return Err(RsbError::InvalidInput(
            "Invalid backup structure: missing 'snapshots' and/or 'data' directories".to_string(),
        ));
    }

    // Create minimal config for restore
    Ok(Config {
        source_path: "direct-restore".to_string(),
        destination_path: backup_path.to_string_lossy().to_string(),
        exclude_patterns: Vec::new(),
        encryption_key,
        ..Default::default()
    })
}

/// Minimal config to restore straight from an S3 repository, without a
/// profile file. Credentials are resolved by `storage_backend` (env vars,
/// session credentials, or the encrypted credentials file) — never from here.
pub fn config_for_s3_restore(
    bucket: &str,
    region: Option<String>,
    endpoint: Option<String>,
) -> crate::error::Result<Config> {
    if bucket.trim().is_empty() {
        return Err(RsbError::InvalidInput(
            "S3 bucket is required for an S3 restore".to_string(),
        ));
    }
    Ok(Config {
        source_path: "direct-restore".to_string(),
        destination_path: String::new(),
        exclude_patterns: Vec::new(),
        s3_bucket: Some(bucket.trim().to_string()),
        s3_region: region.filter(|r| !r.trim().is_empty()),
        s3_endpoint: endpoint.filter(|e| !e.trim().is_empty()),
        ..Default::default()
    })
}

/// Unified restore function supporting both config files and direct backup paths
///
/// This function provides a single entry point for restore operations that works
/// with either:
/// 1. Traditional config file (config_path is Some)
/// 2. Direct backup path (backup_path is Some)
///
/// # Arguments
/// * `config_path` - Optional path to config.toml file
/// * `backup_path` - Optional path to backup folder
/// * `options` - Snapshot, target, key, force/versioned, progress callback
///   and cancellation token (see [`RestoreOptions`])
///
/// # Returns
/// ReportData with restore operation details
///
/// # Errors
/// Returns an error if:
/// - Neither config_path nor backup_path is provided
/// - Both are provided but conflict
/// - Config file cannot be loaded
/// - Backup structure is invalid
/// - Restore operation fails
pub async fn restore_from_config_or_backup(
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
    options: RestoreOptions,
) -> crate::error::Result<ReportData> {
    // Validate that at least one source is provided
    if config_path.is_none() && backup_path.is_none() {
        return Err(RsbError::InvalidInput(
            "Either config file or backup path must be provided".to_string(),
        ));
    }

    // Create config from appropriate source
    let config = if let Some(cfg_path) = config_path {
        // Traditional mode: load from config file
        crate::config::load_config(cfg_path)?
    } else if let Some(backup) = backup_path {
        // Portable mode: create config from backup path
        config_from_backup_path(backup, options.key().map(String::from))?
    } else {
        return Err(RsbError::InvalidInput(
            "No valid restore source provided".to_string(),
        ));
    };

    // Perform restore using core function
    core::perform_restore(&config, options).await
}

/// Validates a backup structure without performing full restore
///
/// This is useful for CLI diagnose and Desktop UI verification operations.
///
/// # Returns
/// BackupValidation structure with validation details
pub fn validate_backup_structure(backup_path: &Path) -> BackupValidation {
    let mut validation = BackupValidation {
        is_valid: true,
        issues: Vec::new(),
        warnings: Vec::new(),
        suggestions: Vec::new(),
        snapshots_count: 0,
        data_files_count: 0,
    };

    // Check basic structure
    let snapshots_dir = backup_path.join("snapshots");
    let data_dir = backup_path.join("data");

    if !snapshots_dir.exists() {
        validation.is_valid = false;
        validation
            .issues
            .push("Missing 'snapshots' directory".to_string());
        validation
            .suggestions
            .push("Ensure entire backup folder was copied".to_string());
    }

    if !data_dir.exists() {
        validation.is_valid = false;
        validation
            .issues
            .push("Missing 'data' directory".to_string());
        validation
            .suggestions
            .push("Re-copy backup from original computer".to_string());
    }

    // Count files if structure valid
    if snapshots_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&snapshots_dir) {
            validation.snapshots_count = entries
                .filter_map(Result::ok)
                .filter(|e| e.file_type().map(|t| t.is_file()).unwrap_or(false))
                .filter(|e| is_snapshot_manifest_name(&e.file_name().to_string_lossy()))
                .count();
        }

        if validation.snapshots_count == 0 {
            validation.is_valid = false;
            validation
                .issues
                .push("No snapshots found in backup".to_string());
            validation
                .suggestions
                .push("Backup may be corrupted or incomplete".to_string());
        }
    }

    if data_dir.exists() {
        // Data objects live under `data/clear/<hash>` and `data/enc/<hash>`;
        // count the objects themselves, not the `clear`/`enc` subdirectories.
        validation.data_files_count = walkdir::WalkDir::new(&data_dir)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
            .filter(|e| !is_temp_or_hidden(&e.file_name().to_string_lossy()))
            .count();

        if validation.data_files_count == 0 {
            validation.warnings.push("No data files found".to_string());
        }
    }

    validation
}

/// True for leftovers of an interrupted atomic write (`.<name>.rsb-tmp-<id>`)
/// and other hidden files, which are not part of the backup contents.
fn is_temp_or_hidden(name: &str) -> bool {
    name.starts_with('.') || name.contains(crate::storage::TMP_MARKER)
}

/// True for a snapshot manifest file (`<timestamp>[-n].toml`), ignoring
/// temp files from atomic writes and unrelated entries.
pub(crate) fn is_snapshot_manifest_name(name: &str) -> bool {
    !is_temp_or_hidden(name) && name.len() > ".toml".len() && name.ends_with(".toml")
}

/// Backup validation result structure
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct BackupValidation {
    pub is_valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
    pub snapshots_count: usize,
    pub data_files_count: usize,
}
