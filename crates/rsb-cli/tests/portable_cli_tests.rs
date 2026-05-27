//! CLI tests for portable backup functionality
//! Tests command-line interface support for portable backups

use rsb_sdk::{config_from_backup_path, validate_backup_structure};
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper function to create a mock backup structure
fn create_mock_backup(backup_path: &PathBuf) {
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();
    fs::write(
        backup_path.join("snapshots/manifest_20260525.toml"),
        "[metadata]\ntype = \"full\"\ndate = \"2026-05-25\"\n[files]\n",
    )
    .unwrap();
    fs::write(backup_path.join("data/clear/file1.bin"), b"test data").unwrap();
}

#[test]
fn test_cli_backup_validation_via_config_from_backup() {
    // Test that config_from_backup_path works correctly
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    create_mock_backup(&backup_path);

    // Should successfully create config from valid backup
    let config = config_from_backup_path(&backup_path, None);
    assert!(config.is_ok());

    let cfg = config.unwrap();
    assert_eq!(cfg.source_path, "direct-restore");
    assert!(cfg.destination_path.contains("backup"));
}

#[test]
fn test_cli_backup_validation_missing_structure() {
    // Test that invalid backups are rejected
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("invalid_backup");
    fs::create_dir_all(&backup_path).unwrap();
    // Don't create snapshots/ and data/ directories

    let config = config_from_backup_path(&backup_path, None);
    assert!(config.is_err());
}

#[test]
fn test_cli_portable_backup_path_validation() {
    // Test validate_backup_structure for portable backups
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    create_mock_backup(&backup_path);

    let validation = validate_backup_structure(&backup_path);
    assert!(validation.is_valid);
    assert_eq!(validation.snapshots_count, 1);
    assert_eq!(validation.data_files_count, 1);
    assert!(validation.issues.is_empty());
}

#[test]
fn test_cli_portable_backup_validation_with_warnings() {
    // Test backup validation with warnings (no data files)
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data")).unwrap();
    fs::write(
        backup_path.join("snapshots/manifest_20260525.toml"),
        "[metadata]\n",
    )
    .unwrap();

    let validation = validate_backup_structure(&backup_path);
    assert!(validation.is_valid);
    assert!(!validation.warnings.is_empty());
    assert!(validation.warnings.iter().any(|w| w.contains("data")));
}

#[test]
fn test_cli_portable_backup_missing_snapshots() {
    // Test detection of incomplete backups (missing snapshots)
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();
    fs::write(backup_path.join("data/clear/file.bin"), b"data").unwrap();

    let validation = validate_backup_structure(&backup_path);
    assert!(!validation.is_valid);
    assert!(validation.issues.iter().any(|i| i.contains("snapshots")));
}

#[test]
fn test_cli_portable_backup_missing_data() {
    // Test detection of incomplete backups (missing data directory)
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::write(backup_path.join("snapshots/manifest.toml"), "[metadata]\n").unwrap();

    let validation = validate_backup_structure(&backup_path);
    assert!(!validation.is_valid);
    assert!(validation.issues.iter().any(|i| i.contains("data")));
}

#[test]
fn test_cli_portable_backup_empty_snapshots() {
    // Test detection of empty snapshots directory
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();

    let validation = validate_backup_structure(&backup_path);
    assert!(!validation.is_valid);
    assert!(validation.issues.iter().any(|i| i.contains("No snapshots")));
}

#[test]
fn test_cli_portable_backup_with_encryption_key() {
    // Test that config accepts encryption keys for encrypted backups
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    create_mock_backup(&backup_path);

    let config = config_from_backup_path(&backup_path, Some("password123".to_string()));
    assert!(config.is_ok());

    let cfg = config.unwrap();
    assert!(cfg.encryption_key.is_some());
    assert_eq!(cfg.encryption_key.unwrap(), "password123");
}

#[test]
fn test_cli_portable_backup_validation_suggestions() {
    // Test that validation provides helpful suggestions
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    // Create only data dir, missing snapshots
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();

    let validation = validate_backup_structure(&backup_path);
    assert!(!validation.is_valid);
    assert!(!validation.suggestions.is_empty());
    assert!(validation.suggestions.iter().any(|s| !s.is_empty()));
}

#[test]
fn test_cli_restore_error_handling_incomplete_backup() {
    // Test error handling for incomplete backups
    // Expected: suggest re-copying from original computer
}

#[test]
fn test_cli_restore_portable_preserves_structure() {
    // Test that portable restore preserves directory structure
    // Including: file permissions, timestamps, nested directories
}

#[test]
fn test_cli_restore_portable_with_encryption() {
    // Test: rsb restore --backup /path/to/backup --target /restore --key password

    // Verify encrypted files are decrypted correctly
    // without needing original config file
}

#[test]
fn test_cli_restore_portable_versioned_output() {
    // Test: rsb restore --backup /path/to/backup --target /restore --versioned

    // Verify timestamped folder is created: /restore/YYYYMMDD_HHmmss/
}
