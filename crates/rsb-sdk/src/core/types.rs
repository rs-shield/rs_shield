use serde::{Deserialize, Serialize};

pub const MULTIPART_THRESHOLD: u64 = 4 * 1024 * 1024 * 1024; // 4 GB
pub const CHUNK_SIZE: usize = 512 * 1024 * 1024; // 512 MB

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ChunkMetadata {
    pub hash: String,
    pub stored_size: u64,
    #[serde(default)]
    pub stored_hash: Option<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct FileMetadata {
    pub hash: String,
    pub encrypted: bool,
    pub stored_hash: Option<String>,
    pub stored_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunks: Option<Vec<ChunkMetadata>>,
    #[serde(default = "default_true")]
    pub compressed: bool,
}

pub type ProgressCallback = std::sync::Arc<dyn Fn(usize, usize, String) + Send + Sync>;

pub enum FileStatus {
    Processed,
    Skipped,
}

fn default_true() -> bool {
    true
}
