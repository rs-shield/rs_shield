// manifest.rs - Performance + reliability version
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

/// Writes the manifest in an optimized way (compression + encryption)
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

    // TOML Serialization
    let content_str = toml::to_string(manifest)?;

    // Zstd Compression (much faster and more efficient than without compression)
    let mut compressed = Vec::new();
    {
        let mut encoder = zstd::Encoder::new(&mut compressed, 3)?; // level 3 = good balance
        encoder.finish()?;
    }

    // Encryption (if applicable)
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

/// Reads the manifest with support for encryption + compression
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

    // Tries to decompress (Zstd)
    let mut decompressed = Vec::new();
    if copy_decode(&decrypted[..], &mut decompressed).is_ok() {
        return Ok(String::from_utf8(decompressed)?);
    }

    // Fallback: it wasn't compressed
    Ok(String::from_utf8(decrypted)?)
}

/// Efficiently finds the most recent snapshot
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

    // Optimized listing + sorting
    let mut snapshots = storage.list("snapshots/").await?;
    snapshots.retain(|s| s.ends_with(".toml"));

    if snapshots.is_empty() {
        return Err("No snapshots found".into());
    }

    // Reverse sorting (most recent first)
    snapshots.sort_by(|a, b| b.cmp(a)); // lexicographical works due to timestamp
    let latest = snapshots.into_iter().next().unwrap();

    let content = read_manifest(storage, &latest, key).await?;
    Ok((latest, content))
}

// ====================== UTILITY (optional) ======================
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
