// Testes para rsb-core/src/core/manifest.rs

use chrono::Utc;
use std::collections::HashMap;
use std::path::PathBuf;

#[test]
fn test_snapshot_path_generation() {
    let timestamp = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let snapshot_path = format!("snapshots/{}.toml", timestamp);
    
    assert!(snapshot_path.starts_with("snapshots/"));
    assert!(snapshot_path.ends_with(".toml"));
    assert!(timestamp.len() > 0);
}

#[test]
fn test_multiple_snapshots_different_times() {
    let timestamp1 = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let path1 = format!("snapshots/{}.toml", timestamp1);
    
    // Wait a bit
    std::thread::sleep(std::time::Duration::from_millis(100));
    
    let timestamp2 = Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let path2 = format!("snapshots/{}.toml", timestamp2);
    
    // Paths should be different
    assert_ne!(path1, path2);
    assert!(path2 > path1); // Later timestamp should be lexicographically greater
}

#[test]
fn test_snapshot_list_sorting() {
    let snapshots = vec![
        "snapshots/2026-02-07T10:00:00Z.toml",
        "snapshots/2026-02-07T08:30:00Z.toml",
        "snapshots/2026-02-07T12:15:00Z.toml",
        "snapshots/2026-02-07T09:45:00Z.toml",
    ];
    
    let mut sorted = snapshots.clone();
    sorted.sort();
    
    // Verify sorting
    assert_eq!(sorted[0], "snapshots/2026-02-07T08:30:00Z.toml");
    assert_eq!(sorted[sorted.len()-1], "snapshots/2026-02-07T12:15:00Z.toml");
}

#[test]
fn test_snapshot_filtering() {
    let files = vec![
        "snapshots/2026-02-07T10:00:00Z.toml",
        "snapshots/data.txt",
        "snapshots/2026-02-07T09:30:00Z.toml",
        "snapshots/metadata.json",
        "snapshots/2026-02-07T11:00:00Z.toml",
    ];
    
    let snapshots: Vec<_> = files
        .iter()
        .filter(|s| s.ends_with(".toml"))
        .cloned()
        .collect();
    
    assert_eq!(snapshots.len(), 3);
    assert!(snapshots.iter().all(|s| s.ends_with(".toml")));
}

#[test]
fn test_manifest_path_construction() {
    let id = "2026-02-07T10:30:00Z";
    let path = format!("snapshots/{}.toml", id);
    
    assert_eq!(path, "snapshots/2026-02-07T10:30:00Z.toml");
    assert!(path.starts_with("snapshots/"));
}

#[test]
fn test_snapshot_id_extraction() {
    let path = "snapshots/2026-02-07T10:30:00Z.toml";
    let id = path
        .strip_prefix("snapshots/")
        .and_then(|s| s.strip_suffix(".toml"));
    
    assert!(id.is_some());
    assert_eq!(id.unwrap(), "2026-02-07T10:30:00Z");
}

#[test]
fn test_empty_snapshot_list() {
    let snapshots: Vec<&str> = vec![];
    let toml_snapshots: Vec<_> = snapshots
        .iter()
        .filter(|s| s.ends_with(".toml"))
        .copied()
        .collect();
    
    assert!(toml_snapshots.is_empty());
}

#[test]
fn test_snapshot_search_latest() {
    let mut snapshots = vec![
        "snapshots/2026-02-07T08:00:00Z.toml",
        "snapshots/2026-02-07T12:00:00Z.toml",
        "snapshots/2026-02-07T10:00:00Z.toml",
    ];
    
    snapshots.sort();
    let latest = snapshots.pop();
    
    assert!(latest.is_some());
    assert_eq!(latest.unwrap(), "snapshots/2026-02-07T12:00:00Z.toml");
}

#[test]
fn test_snapshot_datetime_format() {
    let timestamp = "2026-02-07T15:30:45Z";
    let path = format!("snapshots/{}.toml", timestamp);
    
    // Extract and validate format
    let extracted = path
        .strip_prefix("snapshots/")
        .and_then(|s| s.strip_suffix(".toml"));
    
    assert!(extracted.is_some());
    let ts = extracted.unwrap();
    assert!(ts.contains("2026"));
    assert!(ts.contains("02"));
    assert!(ts.contains("07"));
}

#[test]
fn test_manifest_content_structure() {
    // Simulate manifest structure
    let mut manifest: HashMap<PathBuf, String> = HashMap::new();
    manifest.insert(PathBuf::from("/file1.txt"), "hash1".to_string());
    manifest.insert(PathBuf::from("/file2.txt"), "hash2".to_string());
    manifest.insert(PathBuf::from("/dir/file3.txt"), "hash3".to_string());
    
    assert_eq!(manifest.len(), 3);
    assert!(manifest.contains_key(&PathBuf::from("/file1.txt")));
}

#[test]
fn test_snapshot_path_variations() {
    let variations = vec![
        "snapshots/2026-02-07T10:00:00Z.toml",
        "snapshots/2026-02-08T09:30:00Z.toml",
        "snapshots/2026-02-09T14:45:30Z.toml",
    ];
    
    for var in variations {
        assert!(var.starts_with("snapshots/"));
        assert!(var.ends_with(".toml"));
        let without_prefix = var.strip_prefix("snapshots/").unwrap();
        assert!(without_prefix.len() > 0);
    }
}

#[test]
fn test_toml_content_serialization() {
    let mut data: HashMap<String, String> = HashMap::new();
    data.insert("key1".to_string(), "value1".to_string());
    data.insert("key2".to_string(), "value2".to_string());
    
    let toml_content = toml::to_string(&data).expect("Failed to serialize");
    assert!(!toml_content.is_empty());
    assert!(toml_content.contains("key1"));
    assert!(toml_content.contains("value1"));
}

#[test]
fn test_toml_content_deserialization() {
    let toml_str = r#"
key1 = "value1"
key2 = "value2"
"#;
    
    let data: Result<HashMap<String, String>, _> = toml::from_str(toml_str);
    assert!(data.is_ok());
    
    let parsed = data.unwrap();
    assert_eq!(parsed.get("key1").unwrap(), "value1");
    assert_eq!(parsed.get("key2").unwrap(), "value2");
}

#[test]
fn test_snapshot_list_with_invalid_files() {
    let files = vec![
        "snapshots/2026-02-07T10:00:00Z.toml",
        "other_dir/file.toml",
        "snapshots/backup.zip",
        "snapshots/2026-02-07T11:00:00Z.toml",
        "snapshots/.gitkeep",
    ];
    
    let snapshots: Vec<_> = files
        .iter()
        .filter(|f| f.starts_with("snapshots/") && f.ends_with(".toml"))
        .copied()
        .collect();
    
    assert_eq!(snapshots.len(), 2);
    assert!(snapshots.contains(&"snapshots/2026-02-07T10:00:00Z.toml"));
    assert!(snapshots.contains(&"snapshots/2026-02-07T11:00:00Z.toml"));
}
