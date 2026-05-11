use std::fs;
use tempfile::TempDir;

#[test]
fn test_create_config_profile() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let source = temp_dir.path().join("source");
    let dest = temp_dir.path().join("dest");

    fs::create_dir_all(&source).expect("Failed to create source dir");
    fs::create_dir_all(&dest).expect("Failed to create dest dir");

    let config_file = temp_dir.path().join("test_profile.toml");

    // Create a test config
    let config_content = r#"
source_path = "/path/to/source"
destination_path = "/path/to/dest"
exclude_patterns = ["*.tmp", "node_modules"]
backup_mode = "incremental"
compression_level = 3
pause_on_low_battery = 20
pause_on_high_cpu = 20
"#;

    fs::write(&config_file, config_content).expect("Failed to write config");

    // Verify the file was created
    assert!(config_file.exists(), "Config file should exist");

    let content = fs::read_to_string(&config_file).expect("Failed to read config");
    assert!(
        content.contains("incremental"),
        "Config should contain backup mode"
    );
    assert!(
        content.contains("node_modules"),
        "Config should contain exclude patterns"
    );
}

#[test]
fn test_s3_config_serialization() {
    let config_content = r#"
source_path = "/source"
destination_path = "/dest"
backup_mode = "full"
exclude_patterns = ["*.tmp"]

[s3]
bucket = "my-bucket"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"
access_key = "test-key"
secret_key = "test-secret"
"#;

    let config: Result<rsb_sdk::config::Config, _> = toml::from_str(config_content);
    assert!(config.is_ok(), "Config with S3 should parse correctly");

    let cfg = config.unwrap();
    assert_eq!(cfg.backup_mode, "full");
    assert!(cfg.s3.is_some(), "S3 config should be present");

    let s3 = cfg.s3.unwrap();
    assert_eq!(s3.bucket.unwrap(), "my-bucket");
    assert_eq!(s3.region.unwrap(), "us-east-1");
}

#[test]
fn test_config_with_encryption() {
    let config_content = r#"
source_path = "/source"
destination_path = "/dest"
backup_mode = "incremental"
exclude_patterns = ["*.tmp", "node_modules"]
encryption_key = "super-secret-key"
encrypt_patterns = ["*.pdf", "*.doc"]
"#;

    let config: rsb_sdk::config::Config =
        toml::from_str(config_content).expect("Failed to parse config");
    assert!(
        config.encryption_key.is_some(),
        "Encryption key should be present"
    );
    assert!(
        config.encrypt_patterns.is_some(),
        "Encrypt patterns should be present"
    );

    let patterns = config.encrypt_patterns.unwrap();
    assert_eq!(patterns.len(), 2);
    assert!(patterns.contains(&"*.pdf".to_string()));
}

#[test]
fn test_config_default_values() {
    let config_content = r#"
source_path = "/source"
destination_path = "/dest"
backup_mode = "incremental"
exclude_patterns = ["*.tmp"]
"#;

    let config: rsb_sdk::config::Config =
        toml::from_str(config_content).expect("Failed to parse config");
    assert!(
        config.encryption_key.is_none(),
        "Encryption key should be None by default"
    );
    assert!(
        config.pause_on_low_battery.is_none(),
        "Battery pause should be None by default"
    );
    assert!(config.s3.is_none(), "S3 config should be None by default");
}
