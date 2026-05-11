use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotInfo {
    pub id: String,
    pub path: String,
    pub created_at: DateTime<Utc>,
    pub total_files: usize,
    pub total_chunks: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotList {
    pub snapshots: Vec<SnapshotInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotFileEntry {
    pub path: PathBuf,
    pub size: u64,
    pub chunks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDetails {
    pub snapshot: SnapshotInfo,
    pub files: Vec<SnapshotFileEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetadata {
    pub size: u64,
    pub modified: i64,
    pub checksum: String,
    pub chunks: Vec<ChunkMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkMetadata {
    pub hash: String,
    pub size: u64,
}

pub type Manifest = HashMap<PathBuf, FileMetadata>;