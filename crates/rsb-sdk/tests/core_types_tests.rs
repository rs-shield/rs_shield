// Testes para rsb-core/src/core/types.rs

use rsb_sdk::core::types::{ChunkMetadata, FileMetadata, CHUNK_SIZE, MULTIPART_THRESHOLD};

#[test]
fn test_constants() {
    assert_eq!(MULTIPART_THRESHOLD, 4 * 1024 * 1024 * 1024);
    assert_eq!(CHUNK_SIZE, 512 * 1024 * 1024);
}

#[test]
fn test_chunk_metadata_creation() {
    let chunk = ChunkMetadata {
        hash: "abc123def456".to_string(),
        stored_size: 1024 * 1024,
        stored_hash: Some("xyz789".to_string()),
    };

    assert_eq!(chunk.hash, "abc123def456");
    assert_eq!(chunk.stored_size, 1024 * 1024);
    assert!(chunk.stored_hash.is_some());
    assert_eq!(chunk.stored_hash.unwrap(), "xyz789");
}

#[test]
fn test_chunk_metadata_without_stored_hash() {
    let chunk = ChunkMetadata {
        hash: "hash123".to_string(),
        stored_size: 512 * 1024,
        stored_hash: None,
    };

    assert!(chunk.stored_hash.is_none());
    assert_eq!(chunk.stored_size, 512 * 1024);
}

#[test]
fn test_file_metadata_basic() {
    let metadata = FileMetadata {
        hash: "file_hash_123".to_string(),
        encrypted: false,
        stored_hash: None,
        stored_size: None,
        chunks: None,
        compressed: true,
    };

    assert_eq!(metadata.hash, "file_hash_123");
    assert!(!metadata.encrypted);
    assert!(metadata.compressed);
    assert!(metadata.chunks.is_none());
}

#[test]
fn test_file_metadata_encrypted() {
    let metadata = FileMetadata {
        hash: "file_hash_456".to_string(),
        encrypted: true,
        stored_hash: Some("stored_hash_789".to_string()),
        stored_size: Some(2048),
        chunks: None,
        compressed: true,
    };

    assert!(metadata.encrypted);
    assert!(metadata.stored_hash.is_some());
    assert!(metadata.stored_size.is_some());
    assert_eq!(metadata.stored_size.unwrap(), 2048);
}

#[test]
fn test_file_metadata_with_chunks() {
    let chunks = vec![
        ChunkMetadata {
            hash: "chunk1".to_string(),
            stored_size: CHUNK_SIZE as u64,
            stored_hash: Some("chunk1_stored".to_string()),
        },
        ChunkMetadata {
            hash: "chunk2".to_string(),
            stored_size: CHUNK_SIZE as u64,
            stored_hash: Some("chunk2_stored".to_string()),
        },
    ];

    let metadata = FileMetadata {
        hash: "large_file_hash".to_string(),
        encrypted: true,
        stored_hash: Some("stored".to_string()),
        stored_size: Some(2 * CHUNK_SIZE as u64),
        chunks: Some(chunks.clone()),
        compressed: true,
    };

    assert!(metadata.chunks.is_some());
    assert_eq!(metadata.chunks.unwrap().len(), 2);
}

#[test]
fn test_file_metadata_no_compression() {
    let metadata = FileMetadata {
        hash: "no_compress_hash".to_string(),
        encrypted: false,
        stored_hash: None,
        stored_size: None,
        chunks: None,
        compressed: false,
    };

    assert!(!metadata.compressed);
}

#[test]
fn test_file_metadata_serialization() {
    let metadata = FileMetadata {
        hash: "serialize_test".to_string(),
        encrypted: true,
        stored_hash: Some("stored".to_string()),
        stored_size: Some(5000),
        chunks: None,
        compressed: true,
    };

    let json = serde_json::to_string(&metadata).expect("Failed to serialize");
    let deserialized: FileMetadata = serde_json::from_str(&json).expect("Failed to deserialize");

    assert_eq!(deserialized.hash, metadata.hash);
    assert_eq!(deserialized.encrypted, metadata.encrypted);
    assert_eq!(deserialized.stored_size, metadata.stored_size);
}

#[test]
fn test_chunk_metadata_large_file() {
    let chunk = ChunkMetadata {
        hash: "large_chunk_hash".to_string(),
        stored_size: CHUNK_SIZE as u64,
        stored_hash: Some("large_chunk_stored".to_string()),
    };

    assert_eq!(chunk.stored_size, CHUNK_SIZE as u64);
    assert_eq!(chunk.stored_size, 512 * 1024 * 1024);
}

#[test]
fn test_file_metadata_large_file() {
    let num_chunks = 10;
    let total_size = (CHUNK_SIZE as u64) * num_chunks;

    let mut chunks = Vec::new();
    for i in 0..num_chunks {
        chunks.push(ChunkMetadata {
            hash: format!("chunk_{}", i),
            stored_size: CHUNK_SIZE as u64,
            stored_hash: Some(format!("stored_chunk_{}", i)),
        });
    }

    let metadata = FileMetadata {
        hash: "very_large_file".to_string(),
        encrypted: true,
        stored_hash: Some("large_file_stored".to_string()),
        stored_size: Some(total_size),
        chunks: Some(chunks),
        compressed: true,
    };

    assert_eq!(metadata.stored_size.unwrap(), total_size);
    assert!(metadata.chunks.is_some());
    assert_eq!(metadata.chunks.unwrap().len(), num_chunks as usize);
}

#[test]
fn test_file_metadata_clone() {
    let original = FileMetadata {
        hash: "clone_test".to_string(),
        encrypted: true,
        stored_hash: Some("clone_stored".to_string()),
        stored_size: Some(1000),
        chunks: None,
        compressed: true,
    };

    let cloned = original.clone();

    assert_eq!(original.hash, cloned.hash);
    assert_eq!(original.encrypted, cloned.encrypted);
    assert_eq!(original.stored_size, cloned.stored_size);
}

#[test]
fn test_chunk_metadata_clone() {
    let original = ChunkMetadata {
        hash: "chunk_clone".to_string(),
        stored_size: 2048,
        stored_hash: Some("stored_clone".to_string()),
    };

    let cloned = original.clone();

    assert_eq!(original.hash, cloned.hash);
    assert_eq!(original.stored_size, cloned.stored_size);
    assert_eq!(original.stored_hash, cloned.stored_hash);
}
