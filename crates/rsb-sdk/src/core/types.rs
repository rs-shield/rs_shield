// types.rs - Versão limpa, performática e bem documentada
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc};

/// Tamanho máximo antes de usar processamento multipart (4GB)
pub const MULTIPART_THRESHOLD: u64 = 4 * 1024 * 1024 * 1024;

/// Tamanho ideal de chunk para ficheiros grandes (512MB - bom equilíbrio entre memória e performance)
pub const CHUNK_SIZE: usize = 512 * 1024 * 1024;

/// Metadados de cada chunk (usado em ficheiros muito grandes)
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChunkMetadata {
    /// Hash do conteúdo original do chunk
    pub hash: String,
    /// Tamanho após compressão + encriptação
    pub stored_size: u64,
    /// Hash do conteúdo armazenado (após compressão/encriptação)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_hash: Option<String>,
}

/// Metadados completos de um ficheiro no snapshot
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileMetadata {
    /// Hash do conteúdo original do ficheiro
    pub hash: String,

    /// Se o ficheiro foi encriptado
    pub encrypted: bool,

    /// Hash do conteúdo final armazenado (compressão + encriptação)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_hash: Option<String>,

    /// Tamanho final armazenado (após compressão + encriptação)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stored_size: Option<u64>,

    /// Chunks (apenas para ficheiros > MULTIPART_THRESHOLD)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chunks: Option<Vec<ChunkMetadata>>,

    /// Indica se compressão foi aplicada
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
    /// Cria um novo FileMetadata para ficheiros normais
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

    /// Cria metadados para ficheiros grandes (multipart)
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

    /// Retorna o tamanho total armazenado (suporta ambos os formatos)
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
