//! Portable Restore Module
//!
//! Provides unified restore functionality for both CLI and Desktop UI.
//! Allows restoring backups directly from a backup path without requiring a config file.
//! This is essential for cross-platform portability.

use crate::config::Config;
use crate::core;
use crate::report::ReportData;
use std::path::{Path, PathBuf};

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
/// let report = perform_restore(&config, None, target_path, Some("password"), false, false, None).await?;
/// ```
pub fn config_from_backup_path(
    backup_path: &Path,
    encryption_key: Option<String>,
) -> Result<Config, Box<dyn std::error::Error>> {
    if !backup_path.exists() {
        return Err(format!("Backup path not found: {}", backup_path.display()).into());
    }

    // Validate backup structure
    let snapshots_dir = backup_path.join("snapshots");
    let data_dir = backup_path.join("data");

    if !snapshots_dir.exists() || !data_dir.exists() {
        return Err(
            "Invalid backup structure: missing 'snapshots' and/or 'data' directories".into(),
        );
    }

    // Create minimal config for restore
    Ok(Config {
        source_path: "direct-restore".to_string(),
        destination_path: backup_path.to_string_lossy().to_string(),
        exclude_patterns: Vec::new(),
        encryption_key,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        max_threads: None,
        channel_buffer_size: 8192,
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
/// * `snapshot_id` - Optional snapshot ID to restore
/// * `target_path` - Target directory for restore
/// * `encryption_key` - Optional encryption key
/// * `force` - Force overwrite existing files
/// * `versioned` - Create timestamped folder
/// * `on_progress` - Optional progress callback
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
    snapshot_id: Option<&str>,
    target_path: PathBuf,
    encryption_key: Option<&str>,
    force: bool,
    versioned: bool,
    on_progress: Option<crate::core::types::ProgressCallback>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    // Validate that at least one source is provided
    if config_path.is_none() && backup_path.is_none() {
        return Err("Either config file or backup path must be provided".into());
    }

    // Create config from appropriate source
    let config = if let Some(cfg_path) = config_path {
        // Traditional mode: load from config file
        crate::config::load_config(cfg_path)?
    } else if let Some(backup) = backup_path {
        // Portable mode: create config from backup path
        config_from_backup_path(backup, encryption_key.map(String::from))?
    } else {
        return Err("No valid restore source provided".into());
    };

    // Perform restore using core function
    core::perform_restore(
        &config,
        snapshot_id,
        target_path,
        encryption_key,
        force,
        versioned,
        on_progress,
    )
    .await
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
            validation.snapshots_count = entries.count();
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
        if let Ok(entries) = std::fs::read_dir(&data_dir) {
            validation.data_files_count = entries.count();
        }

        if validation.data_files_count == 0 {
            validation.warnings.push("No data files found".to_string());
        }
    }

    validation
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

/// Unified restore function with cancellation support
///
/// Extended version of `restore_from_config_or_backup` with cancellation token support.
/// Useful for UI applications that need to allow user cancellation.
///
/// # Arguments
/// * `config_path` - Optional path to config.toml file
/// * `backup_path` - Optional path to backup folder
/// * `snapshot_id` - Optional snapshot ID to restore
/// * `target_path` - Target directory for restore
/// * `encryption_key` - Optional encryption key
/// * `force` - Force overwrite existing files
/// * `versioned` - Create timestamped folder
/// * `on_progress` - Optional progress callback
/// * `cancellation_token` - Optional cancellation token for user interruption
///
/// # Returns
/// ReportData with restore operation details
pub async fn restore_from_config_or_backup_with_cancellation(
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
    snapshot_id: Option<&str>,
    target_path: PathBuf,
    encryption_key: Option<&str>,
    force: bool,
    versioned: bool,
    on_progress: Option<crate::core::types::ProgressCallback>,
    cancellation_token: Option<crate::CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    // Validate that at least one source is provided
    if config_path.is_none() && backup_path.is_none() {
        return Err("Either config file or backup path must be provided".into());
    }

    // Create config from appropriate source
    let config = if let Some(cfg_path) = config_path {
        // Traditional mode: load from config file
        crate::config::load_config(cfg_path)?
    } else if let Some(backup) = backup_path {
        // Portable mode: create config from backup path
        config_from_backup_path(backup, encryption_key.map(String::from))?
    } else {
        return Err("No valid restore source provided".into());
    };

    // Perform restore using core function with cancellation support
    core::restore::perform_restore_with_cancellation(
        &config,
        snapshot_id,
        target_path,
        encryption_key,
        force,
        versioned,
        on_progress,
        cancellation_token,
    )
    .await
}
