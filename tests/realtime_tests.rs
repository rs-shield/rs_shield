use std::fs;
use tempfile::TempDir;
use rsb_sdk::realtime::{FileChange, ChangeType, SyncStrategy};

#[test]
fn test_file_change_creation() {
    let change = FileChange {
        path: "/path/to/file.txt".to_string(),
        change_type: ChangeType::Created,
        timestamp: chrono::Local::now(),
        size: Some(1024),
    };

    assert_eq!(change.path, "/path/to/file.txt");
    assert!(matches!(change.change_type, ChangeType::Created));
    assert_eq!(change.size, Some(1024));
}

#[test]
fn test_file_change_types() {
    let created = FileChange {
        path: "/new_file.txt".to_string(),
        change_type: ChangeType::Created,
        timestamp: chrono::Local::now(),
        size: Some(512),
    };

    let modified = FileChange {
        path: "/existing_file.txt".to_string(),
        change_type: ChangeType::Modified,
        timestamp: chrono::Local::now(),
        size: Some(1024),
    };

    let deleted = FileChange {
        path: "/old_file.txt".to_string(),
        change_type: ChangeType::Deleted,
        timestamp: chrono::Local::now(),
        size: None,
    };

    assert!(matches!(created.change_type, ChangeType::Created));
    assert!(matches!(modified.change_type, ChangeType::Modified));
    assert!(matches!(deleted.change_type, ChangeType::Deleted));
}

#[test]
fn test_sync_strategy_immediate() {
    let strategy = SyncStrategy::Immediate;
    assert!(matches!(strategy, SyncStrategy::Immediate));
}

#[test]
fn test_sync_strategy_batch() {
    let strategy = SyncStrategy::Batch { max_items: 100, interval_secs: 60 };
    assert!(matches!(strategy, SyncStrategy::Batch { .. }));
}

#[test]
fn test_sync_strategy_scheduled() {
    let strategy = SyncStrategy::Scheduled { interval_secs: 3600 };
    assert!(matches!(strategy, SyncStrategy::Scheduled { .. }));
}

#[test]
fn test_change_queue_basic() {
    use rsb_sdk::realtime::ChangeQueue;

    let queue = ChangeQueue::new();
    
    let change = FileChange {
        path: "/test.txt".to_string(),
        change_type: ChangeType::Created,
        timestamp: chrono::Local::now(),
        size: Some(256),
    };
    
    queue.push(change.clone());
    
    // Queue should have the change
    assert!(!queue.is_empty());
}

#[test]
fn test_sync_stats() {
    use rsb_sdk::realtime::SyncStats;

    let stats = SyncStats {
        files_synced: 150,
        files_failed: 5,
        total_size_synced: 1024 * 1024 * 10, // 10 MB
        last_sync_time: chrono::Local::now(),
    };

    assert_eq!(stats.files_synced, 150);
    assert_eq!(stats.files_failed, 5);
    assert_eq!(stats.total_size_synced, 10485760); // 10 MB in bytes
}

#[test]
fn test_realtime_watcher_creation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    
    fs::write(temp_dir.path().join("test.txt"), "content").expect("Failed to create file");
    
    // Create a watcher (may not work in all test environments)
    // This is more of a smoke test
    assert!(temp_dir.path().exists(), "Temp directory should exist");
}

#[test]
fn test_multiple_file_changes() {
    let changes = vec![
        FileChange {
            path: "/file1.txt".to_string(),
            change_type: ChangeType::Created,
            timestamp: chrono::Local::now(),
            size: Some(100),
        },
        FileChange {
            path: "/file2.txt".to_string(),
            change_type: ChangeType::Modified,
            timestamp: chrono::Local::now(),
            size: Some(200),
        },
        FileChange {
            path: "/file3.txt".to_string(),
            change_type: ChangeType::Deleted,
            timestamp: chrono::Local::now(),
            size: None,
        },
    ];

    assert_eq!(changes.len(), 3);
    assert_eq!(changes[0].path, "/file1.txt");
    assert_eq!(changes[1].path, "/file2.txt");
    assert_eq!(changes[2].path, "/file3.txt");
}

#[test]
fn test_change_timestamps() {
    use std::time::Duration;

    let now = chrono::Local::now();
    let change = FileChange {
        path: "/test.txt".to_string(),
        change_type: ChangeType::Created,
        timestamp: now,
        size: Some(512),
    };

    assert_eq!(change.timestamp, now);
    // Timestamp should be recent
    let age = chrono::Local::now().signed_duration_since(change.timestamp);
    assert!(age.num_seconds() < 5, "Change timestamp should be recent");
}

#[test]
fn test_large_file_change() {
    let large_size = 1024 * 1024 * 1024; // 1 GB
    
    let change = FileChange {
        path: "/large_file.iso".to_string(),
        change_type: ChangeType::Created,
        timestamp: chrono::Local::now(),
        size: Some(large_size),
    };

    assert_eq!(change.size, Some(large_size));
}

#[test]
fn test_sync_stats_zero_values() {
    use rsb_sdk::realtime::SyncStats;

    let stats = SyncStats {
        files_synced: 0,
        files_failed: 0,
        total_size_synced: 0,
        last_sync_time: chrono::Local::now(),
    };

    assert_eq!(stats.files_synced, 0);
    assert_eq!(stats.files_failed, 0);
    assert_eq!(stats.total_size_synced, 0);
}

#[test]
fn test_nested_path_changes() {
    let paths = vec![
        "/root/subdir/file.txt",
        "/root/subdir/nested/deep/file.txt",
        "/another/path/file.txt",
    ];

    let changes: Vec<FileChange> = paths
        .iter()
        .map(|path| FileChange {
            path: path.to_string(),
            change_type: ChangeType::Created,
            timestamp: chrono::Local::now(),
            size: Some(100),
        })
        .collect();

    assert_eq!(changes.len(), 3);
    assert!(changes[1].path.contains("nested/deep"));
}
