// types.rs - Clean, performant, and well-documented version
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

/// Maximum size before using multipart processing (4GB)
pub const MULTIPART_THRESHOLD: u64 = 4 * 1024 * 1024 * 1024;

/// Ideal chunk size for large files (512MB - good balance between memory and performance)
pub const CHUNK_SIZE: usize = 512 * 1024 * 1024;

/// Metadata for each chunk (used for very large files)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChunkMetadata {
    /// Hash of the original chunk content
    pub hash: String,
    /// Size after compression + encryption
    pub stored_size: u64,
    /// Hash of the stored content (after compression/encryption)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_hash: Option<String>,
}

/// Full metadata of a file in the snapshot
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    /// Hash of the original file content
    pub hash: String,

    /// Whether the file was encrypted
    pub encrypted: bool,

    /// Hash of the final stored content (compression + encryption)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_hash: Option<String>,

    /// Final stored size (after compression + encryption)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_size: Option<u64>,

    /// Chunks (only for files > MULTIPART_THRESHOLD)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunks: Option<Vec<ChunkMetadata>>,

    /// Indicates if compression was applied
    #[serde(default = "default_true")]
    pub compressed: bool,
}

pub type ProgressCallback = Arc<dyn Fn(usize, usize, String) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Processed,
    Skipped,
}

// ====================== HELPERS ======================

fn default_true() -> bool {
    true
}

impl FileMetadata {
    /// Creates a new FileMetadata for normal files
    pub fn new(hash: String, encrypted: bool, compressed: bool) -> Self {
        Self {
            hash,
            encrypted,
            stored_hash: None,
            stored_size: None,
            chunks: None,
            compressed,
        }
    }

    /// Creates metadata for large files (multipart)
    pub fn new_multipart(
        hash: String,
        encrypted: bool,
        compressed: bool,
        chunks: Vec<ChunkMetadata>,
        total_stored_size: u64,
    ) -> Self {
        Self {
            hash,
            encrypted,
            stored_hash: None,
            stored_size: Some(total_stored_size),
            chunks: Some(chunks),
            compressed,
        }
    }

    /// Returns the total stored size (supports both formats)
    pub fn total_stored_size(&self) -> u64 {
        if let Some(size) = self.stored_size {
            size
        } else if let Some(chunks) = &self.chunks {
            chunks.iter().map(|c| c.stored_size).sum()
        } else {
            0
        }
    }
}
