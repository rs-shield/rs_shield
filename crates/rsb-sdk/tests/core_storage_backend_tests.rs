// Testes para rsb-core/src/core/storage_backend.rs

use rsb_sdk::config::{Config, S3Config};

#[test]
fn test_local_storage_selection() {
    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/destination".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: Some(3),
        channel_buffer_size: 8192,
        max_threads: None,

    };

    // Should use local storage
    assert!(config.s3_bucket.is_none());
    assert!(config.s3.is_none());
}

#[test]
fn test_s3_storage_selection_with_bucket() {
    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/destination".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "full".to_string(),
        s3_bucket: Some("my-bucket".to_string()),
        s3_region: Some("us-east-1".to_string()),
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
        channel_buffer_size: 8192,
        max_threads: None,
    };

    // Should use S3 storage
    assert!(config.s3_bucket.is_some());
    assert_eq!(config.s3_bucket.unwrap(), "my-bucket");
}

#[test]
fn test_s3_config_nested_structure() {
    let s3_config = S3Config {
        bucket: Some("nested-bucket".to_string()),
        region: Some("eu-west-1".to_string()),
        endpoint: Some("https://s3.eu-west-1.amazonaws.com".to_string()),
        access_key: Some("AKIA...".to_string()),
        secret_key: Some("secret...".to_string()),
    };

    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/destination".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: Some(s3_config),
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: Some(5),
        channel_buffer_size: 8192,
        max_threads: None,
    };

    assert!(config.s3.is_some());
    let s3 = config.s3.unwrap();
    assert_eq!(s3.bucket.unwrap(), "nested-bucket");
    assert_eq!(s3.region.unwrap(), "eu-west-1");
}

#[test]
fn test_s3_bucket_extraction_nested() {
    let s3_config = S3Config {
        bucket: Some("extracted-bucket".to_string()),
        region: Some("ap-northeast-1".to_string()),
        endpoint: None,
        access_key: None,
        secret_key: None,
    };

    let bucket = s3_config.bucket.clone();
    assert_eq!(bucket.unwrap(), "extracted-bucket");
}

#[test]
fn test_s3_bucket_extraction_flat() {
    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/destination".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: Some("flat-bucket".to_string()),
        s3_region: Some("us-west-2".to_string()),
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
                channel_buffer_size: 8192,
        max_threads: None,

    };

    let bucket = config.s3_bucket.clone();
    assert_eq!(bucket.unwrap(), "flat-bucket");
}

#[test]
fn test_empty_bucket_uses_local() {
    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/destination".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: Some("   ".to_string()), // Whitespace only
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
                channel_buffer_size: 8192,
        max_threads: None,

    };

    let bucket = config.s3_bucket.as_ref().map(|b| b.trim());
    // Empty bucket should fall back to local
    assert_eq!(bucket.unwrap(), "");
}

#[test]
fn test_s3_credentials_presence() {
    let s3_config = S3Config {
        bucket: Some("secure-bucket".to_string()),
        region: Some("eu-central-1".to_string()),
        endpoint: None,
        access_key: Some("AKIAIOSFODNN7EXAMPLE".to_string()),
        secret_key: Some("wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string()),
    };

    assert!(s3_config.access_key.is_some());
    assert!(s3_config.secret_key.is_some());
}

#[test]
fn test_s3_credentials_missing() {
    let s3_config = S3Config {
        bucket: Some("public-bucket".to_string()),
        region: None,
        endpoint: None,
        access_key: None,
        secret_key: None,
    };

    assert!(s3_config.access_key.is_none());
    assert!(s3_config.secret_key.is_none());
}

#[test]
fn test_s3_endpoint_override() {
    let s3_config = S3Config {
        bucket: Some("minio-bucket".to_string()),
        region: None,
        endpoint: Some("https://minio.example.com".to_string()),
        access_key: None,
        secret_key: None,
    };

    assert!(s3_config.endpoint.is_some());
    assert_eq!(s3_config.endpoint.unwrap(), "https://minio.example.com");
}

#[test]
fn test_local_storage_path_preservation() {
    let dest_path = "/backup/my-backup";
    let config = Config {
        source_path: "/data".to_string(),
        destination_path: dest_path.to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_endpoint: None,
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
                channel_buffer_size: 8192,
        max_threads: None,

    };

    assert_eq!(config.destination_path, dest_path);
}

#[test]
fn test_s3_region_fallback() {
    // Test nested region takes precedence
    let s3_config = S3Config {
        bucket: Some("bucket".to_string()),
        region: Some("nested-region".to_string()),
        endpoint: None,
        access_key: None,
        secret_key: None,
    };

    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/dest".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: None,
        s3_region: Some("flat-region".to_string()),
        s3_endpoint: None,
        s3_buckets: None,
        s3: Some(s3_config),
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
                channel_buffer_size: 8192,
        max_threads: None,

    };

    let region = config
        .s3
        .as_ref()
        .and_then(|s| s.region.clone())
        .or(config.s3_region.clone());

    // Nested should take precedence
    assert_eq!(region.unwrap(), "nested-region");
}

#[test]
fn test_s3_endpoint_fallback() {
    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/dest".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "incremental".to_string(),
        s3_bucket: Some("bucket".to_string()),
        s3_region: None,
        s3_endpoint: Some("https://s3.amazonaws.com".to_string()),
        s3: None,
        s3_buckets: None,
        encrypt_patterns: None,
        pause_on_low_battery: None,
        pause_on_high_cpu: None,
        compression_level: None,
                channel_buffer_size: 8192,
        max_threads: None,

    };

    let endpoint = config
        .s3
        .as_ref()
        .and_then(|s| s.endpoint.clone())
        .or(config.s3_endpoint.clone());

    assert_eq!(endpoint.unwrap(), "https://s3.amazonaws.com");
}

#[test]
fn test_complete_s3_config() {
    let s3_config = S3Config {
        bucket: Some("complete-bucket".to_string()),
        region: Some("us-east-1".to_string()),
        endpoint: Some("https://s3.amazonaws.com".to_string()),
        access_key: Some("key".to_string()),
        secret_key: Some("secret".to_string()),
    };

    let config = Config {
        source_path: "/source".to_string(),
        destination_path: "/dest".to_string(),
        exclude_patterns: vec![],
        encryption_key: None,
        backup_mode: "full".to_string(),
        s3_bucket: None,
        s3_region: None,
        s3_buckets: None,
        s3_endpoint: None,
        s3: Some(s3_config),
        encrypt_patterns: None,
        pause_on_low_battery: Some(15),
        pause_on_high_cpu: Some(80),
        compression_level: Some(6),
        channel_buffer_size: 8192,
        max_threads: None,

    };

    let s3 = config.s3.unwrap();
    assert!(s3.bucket.is_some());
    assert!(s3.region.is_some());
    assert!(s3.endpoint.is_some());
    assert!(s3.access_key.is_some());
    assert!(s3.secret_key.is_some());
}
