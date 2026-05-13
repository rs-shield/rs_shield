// manifest.rs - Versão performance + confiabilidade
use super::types::{ChunkMetadata, FileMetadata};
use crate::crypto::{decrypt_data, encrypt_data};
use crate::storage::Storage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};
use zstd::stream::{copy_decode, copy_encode};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChunkReport {
    pub file_path: PathBuf,
    pub chunks: Vec<ChunkMetadata>,
    pub total_chunks: usize,
}

/// Escreve o manifest de forma otimizada (compressão + encriptação)
pub async fn write_manifest(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    encryption_key: Option<&str>,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let timestamp = Utc::now().format("%Y-%m-%dT%H%M%SZ").to_string();
    let snapshot_path = format!("snapshots/{}.toml", timestamp);

    if dry_run {
        info!("[Dry-run] Snapshot manifest not written: {}", snapshot_path);
        return Ok(snapshot_path);
    }

    // Serialização TOML
    let content_str = toml::to_string(manifest)?;

    // Compressão Zstd (muito mais rápida e eficiente que sem compressão)
    let mut compressed = Vec::new();
    {
        let mut encoder = zstd::Encoder::new(&mut compressed, 3)?; // nível 3 = bom equilí   encoder.write_all(content_str.as_bytes())?;
        encoder.finish()?;
    }

    // Encriptação (se aplicável)
    let final_bytes = if let Some(key) = encryption_key {
        encrypt_data(&compressed, key.as_bytes())?
    } else {
        compressed
    };

    storage.write(&snapshot_path, &final_bytes).await?;
    info!(
        "📸 Snapshot manifest written: {} ({} bytes → {} bytes)",
        snapshot_path,
        content_str.len(),
        final_bytes.len()
    );

    Ok(snapshot_path)
}

/// Lê o manifest com suporte a encriptação + compressão
pub async fn read_manifest(
    storage: &dyn Storage,
    path: &str,
    key: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let raw = storage.read(path).await?;

    let decrypted = if let Some(k) = key {
        match decrypt_data(&raw, k.as_bytes()) {
            Ok(data) => data,
            Err(e) => {
                error!("Decryption failed for manifest {}: {}", path, e);
                return Err(format!("Decryption failed: {}", e).into());
            }
        }
    } else {
        raw
    };

    // Tenta descomprimir (Zstd)
    let mut decompressed = Vec::new();
    if copy_decode(&decrypted[..], &mut decompressed).is_ok() {
        return Ok(String::from_utf8(decompressed)?);
    }

    // Fallback: não estava comprimido
    Ok(String::from_utf8(decrypted)?)
}

/// Encontra o snapshot mais recente de forma eficiente
pub async fn find_latest_snapshot(
    storage: &dyn Storage,
    snapshot_id: Option<&str>,
    key: Option<&str>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    if let Some(id) = snapshot_id {
        let path = format!("snapshots/{}.toml", id);
        if storage.exists(&path).await? {
            let content = read_manifest(storage, &path, key).await?;
            return Ok((path, content));
        }
        return Err("Snapshot ID not found".into());
    }

    // Listagem + sort otimizado
    let mut snapshots = storage.list("snapshots/").await?;
    snapshots.retain(|s| s.ends_with(".toml"));

    if snapshots.is_empty() {
        return Err("No snapshots found".into());
    }

    // Ordenação reversa (mais recente primeiro)
    snapshots.sort_by(|a, b| b.cmp(a)); // lexicographical funciona por causa do timestamp
    let latest = snapshots.into_iter().next().unwrap();

    let content = read_manifest(storage, &latest, key).await?;
    Ok((latest, content))
}

// ====================== UTILITÁRIO (opcional) ======================
#[allow(dead_code)]
pub fn log_chunk_metadata(file_path: &Path, chunks: &[ChunkMetadata]) {
    if chunks.len() > 8 {
        info!(
            "📦 {}: {} chunks (total {} bytes)",
            file_path.display(),
            chunks.len(),
            chunks.iter().map(|c| c.stored_size).sum::<u64>()
        );
    } else {
        let report = ChunkReport {
            file_path: file_path.to_path_buf(),
            chunks: chunks.to_vec(),
            total_chunks: chunks.len(),
        };
        if let Ok(json) = serde_json::to_string(&report) {
            debug!("Chunk metadata: {}", json);
        }
    }
}
