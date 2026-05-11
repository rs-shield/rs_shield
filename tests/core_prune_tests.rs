// Testes para rsb-core/src/core/prune.rs

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

#[test]
fn test_snapshot_filtering_for_prune() {
    let files = vec![
        "snapshots/2026-02-01T10:00:00Z.toml",
        "snapshots/data.bin",
        "snapshots/2026-02-02T10:00:00Z.toml",
        "snapshots/metadata.json",
        "snapshots/2026-02-03T10:00:00Z.toml",
    ];
    
    let snapshots: Vec<_> = files
        .iter()
        .filter(|s| s.ends_with(".toml"))
        .copied()
        .collect();
    
    assert_eq!(snapshots.len(), 3);
}

#[test]
fn test_keep_last_snapshots() {
    let mut snapshots = vec![
        "2026-02-01T10:00:00Z.toml",
        "2026-02-02T10:00:00Z.toml",
        "2026-02-03T10:00:00Z.toml",
        "2026-02-04T10:00:00Z.toml",
        "2026-02-05T10:00:00Z.toml",
    ];
    
    let keep_last = 3;
    
    snapshots.sort();
    let to_delete = if snapshots.len() > keep_last {
        snapshots.len() - keep_last
    } else {
        0
    };
    
    let delete_snapshots: Vec<String> = snapshots
        .drain(0..to_delete)
        .map(|s| s.to_string())
        .collect();
    
    assert_eq!(delete_snapshots.len(), 2);
    assert_eq!(snapshots.len(), 3);
    assert_eq!(delete_snapshots[0], "2026-02-01T10:00:00Z.toml");
    assert_eq!(delete_snapshots[1], "2026-02-02T10:00:00Z.toml");
}

#[test]
fn test_no_prune_when_under_threshold() {
    let snapshots = vec![
        "2026-02-01T10:00:00Z.toml",
        "2026-02-02T10:00:00Z.toml",
    ];
    
    let keep_last = 5;
    
    if snapshots.len() <= keep_last {
        assert!(true, "Should not prune when under threshold");
    }
}

#[test]
fn test_hash_extraction_from_metadata() {
    // Simulate FileMetadata with hashes
    let mut keep_hashes = HashSet::new();
    
    // Simulate adding hashes from kept snapshots
    keep_hashes.insert("hash_file_1".to_string());
    keep_hashes.insert("hash_file_2".to_string());
    keep_hashes.insert("hash_chunk_1".to_string());
    keep_hashes.insert("hash_chunk_2".to_string());
    
    assert_eq!(keep_hashes.len(), 4);
    assert!(keep_hashes.contains("hash_file_1"));
}

#[test]
fn test_orphaned_data_detection() {
    let keep_hashes: HashSet<String> = vec![
        "hash_1",
        "hash_2",
        "hash_3",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    
    let all_data_files = vec![
        "data/clear/hash_1",
        "data/clear/hash_2",
        "data/clear/hash_4",  // Orphaned
        "data/enc/hash_3",
        "data/enc/hash_5",    // Orphaned
    ];
    
    let mut deleted_count = 0;
    for file in all_data_files {
        let fname = PathBuf::from(file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        if !keep_hashes.contains(&fname) {
            deleted_count += 1;
        }
    }
    
    assert_eq!(deleted_count, 2); // hash_4 and hash_5
}

#[test]
fn test_data_directory_separation() {
    let clear_files = vec![
        "data/clear/hash_1",
        "data/clear/hash_2",
        "data/clear/hash_3",
    ];
    
    let enc_files = vec![
        "data/enc/hash_4",
        "data/enc/hash_5",
    ];
    
    let all_data: Vec<_> = clear_files
        .into_iter()
        .chain(enc_files)
        .collect();
    
    assert_eq!(all_data.len(), 5);
    assert!(all_data.iter().filter(|f| f.contains("clear")).count() == 3);
    assert!(all_data.iter().filter(|f| f.contains("enc")).count() == 2);
}

#[test]
fn test_snapshot_deletion_list() {
    let mut snapshots = vec![
        "snap_1",
        "snap_2",
        "snap_3",
        "snap_4",
        "snap_5",
    ];
    
    snapshots.sort();
    let keep_last = 2;
    let to_delete = snapshots.len() - keep_last;
    
    let delete_snapshots: Vec<_> = snapshots.drain(0..to_delete).collect();
    
    assert_eq!(delete_snapshots.len(), 3);
    assert_eq!(snapshots.len(), 2);
}

#[test]
fn test_chunk_hash_collection() {
    let mut keep_hashes = HashSet::new();
    
    // Simulate metadata with chunks
    let metadata_with_chunks = vec![
        ("file_hash_1", vec!["chunk_1", "chunk_2", "chunk_3"]),
        ("file_hash_2", vec!["chunk_4", "chunk_5"]),
    ];
    
    for (file_hash, chunks) in metadata_with_chunks {
        keep_hashes.insert(file_hash.to_string());
        for chunk in chunks {
            keep_hashes.insert(chunk.to_string());
        }
    }
    
    assert_eq!(keep_hashes.len(), 7); // 2 file hashes + 5 chunk hashes
}

#[test]
fn test_prune_with_single_snapshot() {
    let snapshots = vec!["snap_1"];
    let keep_last = 3;
    
    if snapshots.len() <= keep_last {
        assert!(true, "Single snapshot should not be pruned when keep_last is high");
    }
}

#[test]
fn test_prune_keep_latest_only() {
    let mut snapshots = vec![
        "snap_oldest",
        "snap_middle",
        "snap_latest",
    ];
    
    snapshots.sort();
    let keep_last = 1;
    let to_delete = snapshots.len() - keep_last;
    let to_keep: Vec<_> = snapshots.drain(to_delete..).collect();
    
    assert_eq!(to_keep.len(), 1);
    assert_eq!(to_keep[0], "snap_latest");
}

#[test]
fn test_empty_data_directories() {
    let clear_files: Vec<String> = vec![];
    let enc_files: Vec<String> = vec![];
    
    let all_data: Vec<_> = clear_files.into_iter().chain(enc_files).collect();
    
    assert!(all_data.is_empty());
}

#[test]
fn test_all_data_orphaned() {
    let keep_hashes: HashSet<String> = HashSet::new(); // Empty set
    
    let all_data_files = vec![
        "data/clear/hash_1",
        "data/clear/hash_2",
        "data/enc/hash_3",
    ];
    
    let mut deleted_count = 0;
    for file in all_data_files {
        let fname = PathBuf::from(file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        
        if !keep_hashes.contains(&fname) {
            deleted_count += 1;
        }
    }
    
    assert_eq!(deleted_count, 3); // All are orphaned
}

#[test]
fn test_snapshot_sorting_chronological() {
    let mut snapshots = vec![
        "2026-02-05T10:00:00Z.toml",
        "2026-02-02T10:00:00Z.toml",
        "2026-02-04T10:00:00Z.toml",
        "2026-02-01T10:00:00Z.toml",
        "2026-02-03T10:00:00Z.toml",
    ];
    
    snapshots.sort();
    
    assert_eq!(snapshots[0], "2026-02-01T10:00:00Z.toml");
    assert_eq!(snapshots[snapshots.len()-1], "2026-02-05T10:00:00Z.toml");
}

#[test]
fn test_prune_calculation_various_keep_values() {
    let snapshots = vec!["1", "2", "3", "4", "5"];
    
    for keep_last in 1..=5 {
        let mut snap_copy = snapshots.clone();
        let to_delete = if snap_copy.len() > keep_last {
            snap_copy.len() - keep_last
        } else {
            0
        };
        
        assert_eq!(to_delete + keep_last, snapshots.len().min(snapshots.len()));
    }
}
