use super::types::{ChunkMetadata, FileMetadata};
use crate::crypto::{decrypt_data, encrypt_data};
use crate::storage::Storage;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{error, info};
use zstd::stream::copy_decode;

/// Chunk detail reporting structure for logging/auditing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChunkReport {
    pub file_path: PathBuf,
    pub chunks: Vec<ChunkMetadata>,
    pub total_chunks: usize,
}

/// Core manifest management: writing/reading snapshot manifests, handling encryption, and finding latest snapshots.
#[allow(dead_code)]
pub fn log_chunk_metadata(
    file_path: &Path,
    chunks: &[ChunkMetadata],
) -> Result<(), Box<dyn std::error::Error>> {
    let report = ChunkReport {
        file_path: file_path.to_path_buf(),
        chunks: chunks.to_vec(),
        total_chunks: chunks.len(),
    };

    let json = serde_json::to_string(&report)?;
    info!("📦 Chunk metadata: {}", json);
    Ok(())
}

pub async fn write_manifest(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    encryption_key: Option<&str>,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    // timestamp format compatible with Windows (without ":" which is invalid in file names)

    let timestamp = Utc::now().format("%Y-%m-%dT%H%M%SZ").to_string();
    let snapshot_path = format!("snapshots/{}.toml", timestamp);

    if dry_run {
        info!("[Dry-run] Snapshot manifest not written.");
        return Ok(snapshot_path);
    }

    let content_str = toml::to_string(manifest)?;
    let content_bytes = if let Some(k) = encryption_key {
        encrypt_data(content_str.as_bytes(), k.as_bytes())?
    } else {
        content_str.into_bytes()
    };

    storage.write(&snapshot_path, &content_bytes).await?;
    info!("Snapshot manifest written to {}", snapshot_path);

    Ok(snapshot_path)
}

pub async fn read_manifest(
    storage: &dyn Storage,
    path: &str,
    key: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let raw = storage.read(path).await?;
    if let Some(k) = key {
        match decrypt_data(&raw, k.as_bytes()) {
            Ok(decrypted) => return Ok(String::from_utf8(decrypted)?),
            Err(e) => {
                error!("Decryption manifest failed : {:?}", e);
                return Err(format!("Decryption failed: {}", e).into());
            }
        }
    }

    // try to decompress if decryption is not applied, in case the manifest was compressed without encryption

    let mut decompressed = Vec::new();
    if copy_decode(&raw[..], &mut decompressed).is_ok() {
        info!("Decompressed manifest content successfully");
        return Ok(String::from_utf8(decompressed)?);
    }

    Ok(String::from_utf8(raw)?)
}

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

    let mut snapshots = storage.list("snapshots").await?;
    snapshots.retain(|s| s.ends_with(".toml"));
    if snapshots.is_empty() {
        return Err("No snapshots found".into());
    }

    snapshots.sort();
    let latest = snapshots.pop().unwrap();
    let content = read_manifest(storage, &latest, key).await?;
    Ok((latest, content))
}
