use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrityIssue {
    pub path: PathBuf,
    pub kind: IntegrityIssueKind,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum IntegrityIssueKind {
    Missing,
    HashMismatch,
    SizeMismatch,
    DecryptionFailed,
    DecompressionFailed,
    InvalidChunk,
    InvalidManifest,
}

#[derive(Debug, Clone)]
pub enum VerifyMode {
    Quick,
    Standard,
    Deep,
}
