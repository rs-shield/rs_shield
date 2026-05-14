// Integration tests - Test multiple components working together

use rsb_sdk::config::Config;
use std::fs;
use tempfile::TempDir;

#[test]
fn test_config_and_paths_integration() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = temp_dir.path().join("source");
    let dest = temp_dir.path().join("dest");

    fs::create_dir_all(&source).expect("Failed to create source");
    fs::create_dir_all(&dest).expect("Failed to create dest");

    // Create test files
    fs::write(source.join("file1.txt"), "content1").expect("Failed to create file1");
    fs::write(source.join("file2.txt"), "content2").expect("Failed to create file2");

    let config = Config {
        source_path: source.to_string_lossy().into_owned(),
        destination_path: dest.to_string_lossy().into_owned(),
        exclude_patterns: vec!["*.tmp".to_string()],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: Some(20),
        pause_on_high_cpu: Some(80),
        compression_level: Some(3),
        channel_buffer_size: 8192,
        max_threads: None,

    };

    assert_eq!(config.source_path, source.to_string_lossy().to_string());
    assert_eq!(config.destination_path, dest.to_string_lossy().to_string());
    assert!(source.exists(), "Source path should exist");
    assert!(dest.exists(), "Destination path should exist");
}

#[test]
fn test_crypto_and_file_operations() {
    use rsb_sdk::crypto;
    use std::fs;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = temp_dir.path().join("secret.txt");

    let original_content = b"This is a secret message that needs encryption";
    fs::write(&file_path, original_content).expect("Failed to write original file");

    // Hash the original file
    let hash = crypto::hash_file_content(original_content).expect("Failed to hash file");

    // Encrypt the content
    let password = b"my-secure-password";
    let encrypted = crypto::encrypt_data(original_content, password).expect("Failed to encrypt");

    // Verify encrypted content is different
    assert_ne!(
        encrypted,
        original_content.to_vec(),
        "Encrypted should differ from original"
    );

    // Decrypt and verify
    let decrypted = crypto::decrypt_data(&encrypted, password).expect("Failed to decrypt");

    assert_eq!(
        decrypted,
        original_content.to_vec(),
        "Decrypted should match original"
    );

    // Verify hash matches
    let hash2 = crypto::hash_file_content(&decrypted).expect("Failed to hash decrypted");
    assert_eq!(hash, hash2, "Hashes should match");
}

#[test]
fn test_file_processing_workflow() {
    use rsb_sdk::crypto;
    use rsb_sdk::utils;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = temp_dir.path().join("source");
    let dest = temp_dir.path().join("dest");

    fs::create_dir_all(&source).expect("Failed to create source");
    fs::create_dir_all(&dest).expect("Failed to create dest");

    // Create multiple test files
    for i in 1..=5 {
        let content = format!("File {} content", i).into_bytes();
        let file_path = source.join(format!("file{}.txt", i));
        fs::write(&file_path, &content).expect("Failed to create file");

        // Hash each file
        let hash = crypto::hash_file_content(&content).expect("Failed to hash file");
        assert_eq!(hash.len(), 64, "Hash should be 64 chars");
    }

    // Use walker to find all files
    let walk = utils::walk_filtered(&source, &[], false);
    let file_count = walk.filter_map(|e| e.ok()).count();

    assert!(file_count > 0, "Should find created files");
}

#[test]
fn test_config_with_exclusions_and_encryption() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = temp_dir.path().join("source");
    let dest = temp_dir.path().join("dest");

    fs::create_dir_all(&source).expect("Failed to create source");
    fs::create_dir_all(&dest).expect("Failed to create dest");

    // Create test structure
    fs::write(source.join("important.pdf"), "PDF content").expect("Failed to create PDF");
    fs::write(source.join("temp.tmp"), "temp content").expect("Failed to create temp");
    fs::create_dir(source.join("node_modules")).expect("Failed to create node_modules");

    let config = Config {
        source_path: source.to_string_lossy().into_owned(),
        destination_path: dest.to_string_lossy().into_owned(),
        exclude_patterns: vec!["*.tmp".to_string(), "node_modules".to_string()],
        encryption_key: Some("secure-key".to_string()),
        backup_mode: "full".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: Some(vec!["*.pdf".to_string()]),
        pause_on_low_battery: Some(15),
        pause_on_high_cpu: Some(75),
        compression_level: Some(6),
        channel_buffer_size: 8192,
        max_threads: None,
    };

    assert!(config.encryption_key.is_some());
    assert!(config.encrypt_patterns.is_some());
    assert!(!config.exclude_patterns.is_empty());
}

#[test]
fn test_report_data_with_file_operations() {
    use rsb_sdk::report::ReportData;
    use std::time::Duration;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let profile_path = temp_dir.path().join("profile.toml");

    fs::write(&profile_path, "test profile").expect("Failed to create profile");

    let report = ReportData {
        operation: "Backup".to_string(),
        profile_path: profile_path.to_string_lossy().into_owned(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: Duration::from_secs(150),
        mode: Some("incremental".to_string()),
        files_processed: 200,
        files_skipped: 30,
        files_with_errors: 2,
        total_files: 232,
        errors: vec![
            "Could not access /path1".to_string(),
            "Permission denied /path2".to_string(),
        ],
        status: "Completed with warnings".to_string(),
    };

    assert!(profile_path.exists(), "Profile should exist");
    assert_eq!(
        report.files_processed + report.files_skipped + report.files_with_errors,
        report.total_files
    );
    assert_eq!(report.errors.len(), 2);
}

#[test]
fn test_complete_backup_scenario() {
    use rsb_sdk::config::Config;
    use rsb_sdk::crypto;
    use rsb_sdk::utils;

    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = temp_dir.path().join("data");
    let dest = temp_dir.path().join("backup");

    fs::create_dir_all(&source).expect("Failed to create source");
    fs::create_dir_all(&dest).expect("Failed to create dest");

    // Create realistic file structure
    fs::create_dir(source.join("documents")).expect("Failed to create documents");
    fs::create_dir(source.join("photos")).expect("Failed to create photos");
    fs::write(source.join("documents/resume.pdf"), "Resume content")
        .expect("Failed to create resume");
    fs::write(source.join("documents/notes.txt"), "Notes content").expect("Failed to create notes");
    fs::write(source.join("photos/vacation.jpg"), "Image data").expect("Failed to create image");

    // Setup config
    let config = Config {
        source_path: source.to_string_lossy().into_owned(),
        destination_path: dest.to_string_lossy().into_owned(),
        exclude_patterns: vec!["*.tmp".to_string()],
        encryption_key: Some("backup-key".to_string()),
        backup_mode: "full".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: Some(vec!["*.pdf".to_string()]),
        pause_on_low_battery: Some(20),
        pause_on_high_cpu: Some(80),
        compression_level: Some(5),
        channel_buffer_size: 8192,
        max_threads: None,

    };

    // Walk the source directory
    let walk = utils::walk_filtered(
        &config.source_path.as_ref(),
        &config.exclude_patterns,
        false,
    );
    let files: Vec<_> = walk.filter_map(|e| e.ok()).collect();

    assert!(files.len() > 0, "Should find files in backup scenario");

    // Process each file
    let password = b"backup-key";
    for entry in files {
        if entry.file_type().map(|ft| ft.is_file()).unwrap_or(false) {
            if let Ok(content) = fs::read(entry.path()) {
                // Hash the file
                let hash = crypto::hash_file_content(&content).expect("Failed to hash");
                assert_eq!(hash.len(), 64);

                // Encrypt if needed
                if entry
                    .path()
                    .extension()
                    .map(|ext| ext == "pdf")
                    .unwrap_or(false)
                {
                    let encrypted =
                        crypto::encrypt_data(&content, password).expect("Failed to encrypt");
                    let decrypted =
                        crypto::decrypt_data(&encrypted, password).expect("Failed to decrypt");
                    assert_eq!(decrypted, content);
                }
            }
        }
    }
}

#[test]
fn test_s3_config_integration() {
    let config_toml = r#"
source_path = "/data"
destination_path = "/backup"
backup_mode = "incremental"
compression_level = 3
exclude_patterns = []

[s3]
bucket = "my-backup-bucket"
region = "eu-west-1"
endpoint = "https://s3.eu-west-1.amazonaws.com"
access_key = "AKIA..."
secret_key = "secret..."
"#;

    let config: rsb_sdk::config::Config =
        toml::from_str(config_toml).expect("Failed to parse S3 config");

    assert_eq!(config.source_path, "/data");
    assert!(config.s3.is_some());

    let s3 = config.s3.unwrap();
    assert_eq!(s3.bucket.unwrap(), "my-backup-bucket");
    assert_eq!(s3.region.unwrap(), "eu-west-1");
    assert_eq!(s3.endpoint.unwrap(), "https://s3.eu-west-1.amazonaws.com");
}
