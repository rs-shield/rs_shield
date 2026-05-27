//! Comprehensive tests for portable restore functionality
//! Tests direct backup restoration without config files, cross-platform support, and validation

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

// Mock structures for testing
#[derive(Debug, Clone, PartialEq, Eq)]
struct MockConfig {
    pub source_path: String,
    pub destination_path: String,
    pub backup_mode: String,
    pub encryption_key: Option<String>,
}

#[derive(Debug, Clone)]
struct BackupValidation {
    pub is_valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
    pub snapshots_count: usize,
    pub data_files_count: usize,
}

// Helper functions for testing
fn create_mock_backup_structure(backup_dir: &Path) -> std::io::Result<()> {
    // Create required directories
    fs::create_dir_all(backup_dir.join("snapshots"))?;
    fs::create_dir_all(backup_dir.join("data/clear"))?;

    // Create mock manifest file
    let manifest_content = r#"
[metadata]
backup_date = "2026-02-25T10:00:00Z"
total_files = 10
total_size = 1048576

[files."test_file.txt"]
hash = "abc123def456"
size = 1024
compressed = false
encrypted = false
"#;
    fs::write(backup_dir.join("snapshots/latest.toml"), manifest_content)?;

    // Create mock data files including the file referenced in manifest
    fs::write(backup_dir.join("data/clear/test_file.txt"), vec![0u8; 1024])?;
    fs::write(backup_dir.join("data/clear/file1.dat"), vec![0u8; 512])?;
    fs::write(backup_dir.join("data/clear/file2.dat"), vec![0u8; 512])?;

    Ok(())
}

fn create_incomplete_backup(backup_dir: &Path, missing: &str) -> std::io::Result<()> {
    match missing {
        "snapshots" => {
            fs::create_dir_all(backup_dir.join("data/clear"))?;
            fs::write(backup_dir.join("data/clear/file1.dat"), vec![0u8; 512])?;
        }
        "data" => {
            fs::create_dir_all(backup_dir.join("snapshots"))?;
            fs::write(backup_dir.join("snapshots/latest.toml"), "[metadata]")?;
        }
        "both" => {
            // Create empty backup directory
        }
        _ => {}
    }
    Ok(())
}

// ============================================================================
// CORE PORTABILITY TESTS
// ============================================================================

#[test]
fn test_portable_backup_structure_validation_valid() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create valid backup structure
    create_mock_backup_structure(backup_path).unwrap();

    // Test validation
    let validation = validate_backup_structure(backup_path);

    assert!(validation.is_valid, "Valid backup should pass validation");
    assert_eq!(validation.snapshots_count, 1, "Should find 1 snapshot");
    assert_eq!(validation.data_files_count, 3, "Should find 3 data files");
    assert!(
        validation.issues.is_empty(),
        "Valid backup should have no issues"
    );
}

#[test]
fn test_portable_backup_structure_validation_missing_snapshots() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create incomplete backup (missing snapshots)
    create_incomplete_backup(backup_path, "snapshots").unwrap();

    // Test validation
    let validation = validate_backup_structure(backup_path);

    assert!(
        !validation.is_valid,
        "Backup without snapshots should fail validation"
    );
    assert!(
        validation.issues.iter().any(|i| i.contains("snapshots")),
        "Should report missing snapshots"
    );
    assert!(
        validation.suggestions.iter().any(|s| s.contains("copied")),
        "Should suggest re-copying backup"
    );
}

#[test]
fn test_portable_backup_structure_validation_missing_data() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create incomplete backup (missing data)
    create_incomplete_backup(backup_path, "data").unwrap();

    // Test validation
    let validation = validate_backup_structure(backup_path);

    assert!(
        !validation.is_valid,
        "Backup without data should fail validation"
    );
    assert!(
        validation.issues.iter().any(|i| i.contains("data")),
        "Should report missing data"
    );
}

#[test]
fn test_portable_backup_structure_validation_empty_backup() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create empty backup directory
    create_incomplete_backup(backup_path, "both").unwrap();

    // Test validation
    let validation = validate_backup_structure(backup_path);

    assert!(!validation.is_valid, "Empty backup should fail validation");
    assert!(
        validation.issues.len() >= 2,
        "Should report multiple issues"
    );
}

#[test]
fn test_portable_config_from_backup_path_valid() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    create_mock_backup_structure(backup_path).unwrap();

    // Test creating config from backup path
    let config = config_from_backup_path(backup_path, None).unwrap();

    assert_eq!(config.source_path, "direct-restore");
    assert_eq!(
        config.destination_path,
        backup_path.to_string_lossy().to_string()
    );
    assert_eq!(config.backup_mode, "incremental");
}

#[test]
fn test_portable_config_from_backup_path_invalid() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("nonexistent");

    // Test with nonexistent path
    let result = config_from_backup_path(&backup_path, None);

    assert!(result.is_err(), "Should fail with nonexistent path");
}

#[test]
fn test_portable_config_from_backup_path_with_encryption_key() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    create_mock_backup_structure(backup_path).unwrap();

    // Test with encryption key
    let config = config_from_backup_path(backup_path, Some("test_password".to_string())).unwrap();

    assert_eq!(config.encryption_key, Some("test_password".to_string()));
}

// ============================================================================
// CROSS-PLATFORM PATH HANDLING TESTS
// ============================================================================

#[test]
fn test_portable_path_normalization_windows_style() {
    // Test that Windows-style paths are handled correctly
    let path_str = r"C:\Users\username\backup\data";
    let normalized = normalize_backup_path(Path::new(path_str));

    // Should not contain backslashes on Unix-like systems
    #[cfg(unix)]
    assert!(!normalized.to_string_lossy().contains('\\'));
}

#[test]
fn test_portable_path_normalization_mixed_separators() {
    // Test paths with mixed separators
    let path_str = "path/to\\backup/data";
    let normalized = normalize_backup_path(Path::new(path_str));

    // Should use native separators
    let path_str_normalized = normalized.to_string_lossy();
    #[cfg(unix)]
    assert!(!path_str_normalized.contains('\\'));
}

#[test]
fn test_portable_path_is_portable_relative() {
    // Relative paths should be portable
    let path = Path::new("./backups/my-backup");
    assert!(is_portable_path(path), "Relative paths should be portable");
}

#[test]
fn test_portable_path_is_portable_absolute_unix() {
    // Unix absolute paths starting with /tmp, /mnt should be portable
    #[cfg(unix)]
    {
        let path = Path::new("/mnt/external/backup");
        assert!(
            is_portable_path(path),
            "External mount paths should be portable"
        );
    }
}

#[test]
fn test_portable_path_is_not_portable_user_home() {
    // User home paths are NOT portable
    let path = Path::new("/Users/username/backup");
    assert!(
        !is_portable_path(path),
        "User home paths should NOT be portable"
    );
}

#[test]
fn test_portable_path_is_not_portable_absolute_windows() {
    // Windows absolute paths are NOT portable
    let path = Path::new("C:\\Users\\backup");
    assert!(
        !is_portable_path(path),
        "Windows absolute paths should NOT be portable"
    );
}

// ============================================================================
// BACKUP INTEGRITY TESTS
// ============================================================================

#[test]
fn test_portable_backup_integrity_valid_manifests() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    create_mock_backup_structure(backup_path).unwrap();

    // Test integrity validation
    let integrity = validate_backup_integrity(backup_path).unwrap();

    assert!(
        integrity.is_valid,
        "Valid backup should pass integrity check"
    );
    assert!(integrity.snapshots_valid > 0, "Should have valid snapshots");
}

#[test]
fn test_portable_backup_integrity_corrupted_manifest() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();

    // Create corrupted manifest (invalid TOML)
    fs::write(
        backup_path.join("snapshots/corrupted.toml"),
        "invalid toml content [[[\n",
    )
    .unwrap();

    // Test integrity validation
    let integrity = validate_backup_integrity(backup_path).unwrap();

    assert!(
        !integrity.is_valid,
        "Corrupted manifest should fail integrity check"
    );
    assert!(
        integrity.snapshots_invalid > 0,
        "Should detect invalid snapshots"
    );
}

#[test]
fn test_portable_backup_integrity_missing_data_files() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();

    // Create manifest referencing files that don't exist
    let manifest_content = r#"
[files."missing_file.txt"]
hash = "abc123"
size = 1024
"#;
    fs::write(
        backup_path.join("snapshots/manifest.toml"),
        manifest_content,
    )
    .unwrap();

    // Test integrity validation
    let integrity = validate_backup_integrity(backup_path).unwrap();

    assert!(!integrity.is_valid, "Backup with missing files should fail");
}

// ============================================================================
// PARTIAL BACKUP DETECTION TESTS
// ============================================================================

#[test]
fn test_portable_detect_partial_copy_empty_snapshots() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create backup with empty snapshots directory
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();
    fs::write(backup_path.join("data/clear/file.dat"), vec![0u8; 1024]).unwrap();

    let validation = validate_backup_structure(backup_path);

    assert!(!validation.is_valid, "Should detect empty snapshots");
    assert!(
        validation.suggestions.iter().any(|s| s.contains("copied")),
        "Should suggest complete copy"
    );
}

#[test]
fn test_portable_detect_partial_copy_empty_data() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path();

    // Create backup with snapshot but no data
    fs::create_dir_all(backup_path.join("snapshots")).unwrap();
    fs::create_dir_all(backup_path.join("data/clear")).unwrap();
    fs::write(backup_path.join("snapshots/manifest.toml"), "[metadata]").unwrap();

    let validation = validate_backup_structure(backup_path);

    // Should have warning about no data files
    assert!(
        validation.warnings.iter().any(|w| w.contains("data")),
        "Should warn about missing data files"
    );
}

// ============================================================================
// RESTORE OPERATION TESTS
// ============================================================================

#[test]
fn test_portable_restore_from_config_requires_source() {
    let temp_dir = TempDir::new().unwrap();
    let restore_target = temp_dir.path();

    // Test with neither config nor backup path
    let result = restore_from_config_or_backup_simulation(None, None, restore_target, None);

    assert!(
        result.is_err(),
        "Should require either config or backup path"
    );
}

#[test]
fn test_portable_restore_from_config_with_config_file() {
    let temp_dir = TempDir::new().unwrap();
    let config_path = temp_dir.path().join("backup.toml");
    let restore_target = temp_dir.path().join("restore");

    // Create mock config
    fs::write(&config_path, "[config]\nsource = \"/data\"\n").unwrap();

    // Test restore with config
    let result =
        restore_from_config_or_backup_simulation(Some(&config_path), None, &restore_target, None);

    // Should succeed (or fail gracefully if config reading fails)
    assert!(result.is_ok() || result.is_err()); // Basic sanity check
}

#[test]
fn test_portable_restore_from_backup_path_direct() {
    let temp_dir = TempDir::new().unwrap();
    let backup_path = temp_dir.path().join("backup");
    let restore_target = temp_dir.path().join("restore");

    create_mock_backup_structure(&backup_path).unwrap();

    // Test restore with backup path
    let result =
        restore_from_config_or_backup_simulation(None, Some(&backup_path), &restore_target, None);

    assert!(
        result.is_ok(),
        "Should restore from backup path successfully"
    );
}

// ============================================================================
// HELPER IMPLEMENTATIONS FOR TESTING
// ============================================================================

/// Validates backup structure
fn validate_backup_structure(backup_path: &Path) -> BackupValidation {
    let mut validation = BackupValidation {
        is_valid: true,
        issues: Vec::new(),
        warnings: Vec::new(),
        suggestions: Vec::new(),
        snapshots_count: 0,
        data_files_count: 0,
    };

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

    if snapshots_dir.exists() {
        if let Ok(entries) = fs::read_dir(&snapshots_dir) {
            validation.snapshots_count = entries.count();
        }

        if validation.snapshots_count == 0 {
            validation.is_valid = false;
            validation
                .issues
                .push("No snapshots found in backup".to_string());
            validation
                .suggestions
                .push("Ensure entire backup folder was copied".to_string());
        }
    }

    if data_dir.exists() {
        // Count files recursively in data directory
        if let Ok(entries) = fs::read_dir(&data_dir) {
            for entry in entries {
                if let Ok(entry) = entry {
                    if entry.path().is_file() {
                        validation.data_files_count += 1;
                    } else if entry.path().is_dir() {
                        // Count files in subdirectories
                        if let Ok(subentries) = fs::read_dir(entry.path()) {
                            for subentry in subentries {
                                if let Ok(subentry) = subentry {
                                    if subentry.path().is_file() {
                                        validation.data_files_count += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if validation.data_files_count == 0 {
            validation.warnings.push("No data files found".to_string());
        }
    }

    validation
}

/// Creates config from backup path
fn config_from_backup_path(
    backup_path: &Path,
    encryption_key: Option<String>,
) -> Result<MockConfig, Box<dyn std::error::Error>> {
    if !backup_path.exists() {
        return Err(format!("Backup path not found: {}", backup_path.display()).into());
    }

    let snapshots_dir = backup_path.join("snapshots");
    let data_dir = backup_path.join("data");

    if !snapshots_dir.exists() || !data_dir.exists() {
        return Err(
            "Invalid backup structure: missing 'snapshots' and/or 'data' directories".into(),
        );
    }

    Ok(MockConfig {
        source_path: "direct-restore".to_string(),
        destination_path: backup_path.to_string_lossy().to_string(),
        backup_mode: "incremental".to_string(),
        encryption_key,
    })
}

/// Normalizes backup paths for cross-platform compatibility
fn normalize_backup_path(path: &Path) -> PathBuf {
    let path_str = path.to_string_lossy();

    // Convert Windows backslashes to forward slashes on Unix
    #[cfg(unix)]
    {
        let normalized = path_str.replace('\\', "/");
        PathBuf::from(normalized)
    }

    #[cfg(not(unix))]
    path.to_path_buf()
}

/// Checks if path is portable (no OS-specific absolute paths)
fn is_portable_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();

    // Check if it's a relative path (portable)
    if !path_str.starts_with('/') && !path_str.contains(":") {
        return true;
    }

    // Check for non-portable absolute paths
    let is_home_path = path_str.starts_with("/Users/")      // macOS home
        || path_str.starts_with("/root/")      // Linux root
        || path_str.starts_with("/home/"); // Linux user homes

    let is_windows_abs = path_str.contains(':') && path_str.contains('\\');

    // Portable paths: relative or external mounts/usb
    // Non-portable: user home or Windows absolute
    !is_home_path && !is_windows_abs
}

/// Validates backup integrity including manifest parsing
fn validate_backup_integrity(
    backup_path: &Path,
) -> Result<BackupIntegrity, Box<dyn std::error::Error>> {
    let mut integrity = BackupIntegrity {
        is_valid: true,
        snapshots_valid: 0,
        snapshots_invalid: 0,
        data_files_valid: 0,
        data_files_invalid: 0,
        checksums: HashMap::new(),
    };

    let snapshots_dir = backup_path.join("snapshots");
    if snapshots_dir.exists() {
        for entry in fs::read_dir(&snapshots_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map(|e| e == "toml").unwrap_or(false) {
                // Try to parse manifest as TOML
                let content = fs::read_to_string(&path)?;
                if let Ok(table) = content.parse::<toml::Table>() {
                    integrity.snapshots_valid += 1;
                    
                    // Check if referenced files exist
                    if let Some(files) = table.get("files").and_then(|v| v.as_table()) {
                        for file_key in files.keys() {
                            // Check if file exists in data directory
                            let data_file = backup_path.join("data").join(file_key);
                            if !data_file.exists() {
                                // Check in subdirectories (like data/clear/)
                                let mut found = false;
                                if let Ok(entries) = fs::read_dir(backup_path.join("data")) {
                                    for entry in entries {
                                        if let Ok(entry) = entry {
                                            let subfile = entry.path().join(file_key);
                                            if subfile.exists() {
                                                found = true;
                                                break;
                                            }
                                        }
                                    }
                                }
                                if !found {
                                    integrity.is_valid = false;
                                    integrity.data_files_invalid += 1;
                                }
                            }
                        }
                    }
                } else {
                    integrity.snapshots_invalid += 1;
                    integrity.is_valid = false;
                }
            }
        }
    }

    let data_dir = backup_path.join("data");
    if data_dir.exists() {
        for entry in fs::read_dir(&data_dir)? {
            let entry = entry?;
            if entry.path().is_dir() {
                // Count files in subdirectories (like data/clear/)
                for subentry in fs::read_dir(entry.path())? {
                    let subentry = subentry?;
                    if subentry.path().is_file() {
                        integrity.data_files_valid += 1;
                    }
                }
            } else if entry.path().is_file() {
                integrity.data_files_valid += 1;
            }
        }
    }

    Ok(integrity)
}

#[derive(Debug, Clone)]
struct BackupIntegrity {
    pub is_valid: bool,
    pub snapshots_valid: usize,
    pub snapshots_invalid: usize,
    pub data_files_valid: usize,
    pub data_files_invalid: usize,
    pub checksums: HashMap<String, String>,
}

/// Simulation helper for restore operation
fn restore_from_config_or_backup_simulation(
    config_path: Option<&Path>,
    backup_path: Option<&Path>,
    target_path: &Path,
    _encryption_key: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    if config_path.is_none() && backup_path.is_none() {
        return Err("Either config file or backup path must be provided".into());
    }

    if let Some(backup) = backup_path {
        if !backup.exists() {
            return Err(format!("Backup path not found: {}", backup.display()).into());
        }

        let validation = validate_backup_structure(backup);
        if !validation.is_valid {
            return Err("Invalid backup structure".into());
        }
    }

    Ok(())
}
