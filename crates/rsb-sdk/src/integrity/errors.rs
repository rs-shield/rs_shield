use thiserror::Error;

#[derive(Debug, Error)]
pub enum IntegrityError {
    #[error("Snapshot not found")]
    SnapshotNotFound,

    #[error("Manifest corrupted")]
    InvalidManifest,

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Hash mismatch")]
    HashMismatch,

    #[error("Chunk missing: {0}")]
    MissingChunk(String),
}
