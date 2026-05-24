use super::cancellation::CancellationToken;
use super::manifest::find_latest_snapshot;
use super::types::{ChunkMetadata, FileMetadata, ProgressCallback};
use crate::config::Config;
use crate::crypto::decrypt_data;
use crate::report::ReportData;
use crate::storage::Storage;
use crate::utils::{ensure_directory_exists_async, mmap_file};
use chrono::Utc;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{error, info};
use zstd::stream::copy_decode;

pub async fn perform_restore(
    config: &Config,
    snapshot_id: Option<&str>,
    target_path: PathBuf,
    encryption_key: Option<&str>,
    force: bool,
    versioned: bool,
    on_progress: Option<ProgressCallback>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    perform_restore_with_cancellation(
        config,
        snapshot_id,
        target_path,
        encryption_key,
        force,
        versioned,
        on_progress,
        None,
    )
    .await
}

pub async fn perform_restore_with_cancellation(
    config: &Config,
    snapshot_id: Option<&str>,
    target_path: PathBuf,
    encryption_key: Option<&str>,
    force: bool,
    versioned: bool,
    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let storage = super::storage_backend::get_storage(config).await;

    // 🐛 Fix: Validate backup structure before attempting restore
    validate_backup_structure(&*storage).await?;

    let (manifest_path, manifest_content) =
        find_latest_snapshot(&*storage, snapshot_id, encryption_key).await?;
    info!("Restoring snapshot: {}", manifest_path);

    let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&manifest_content)?;

    let restore_root = if versioned {
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        target_path.join(format!("restore_{}", timestamp))
    } else {
        target_path
    };

    info!("Restoring to: {}", restore_root.display());

    let pb = ProgressBar::new(manifest.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut errors = Vec::new();
    let mut success = 0;
    let total_files = manifest.len();
    let mut current_files = 0;

    for (rel_path, metadata) in manifest {
        // Check if the operation was cancelled
        if let Some(token) = &cancellation_token {
            if token.is_cancelled() {
                info!("⏹️ Restore cancelled by user");
                break;
            }
        }

        pb.set_message(format!("Restoring: {}", rel_path.display()));
        if let Some(cb) = &on_progress {
            cb(
                current_files,
                total_files,
                format!("Restoring: {}", rel_path.display()),
            );
        }

        let restore_file = restore_root.join(&rel_path);

        if let Some(chunks) = &metadata.chunks {
            if let Err(e) = restore_multipart_file(
                &*storage,
                chunks,
                &restore_file,
                if metadata.encrypted {
                    encryption_key
                } else {
                    None
                },
                &metadata.hash,
                metadata.compressed,
            )
            .await
            {
                let msg = format!("Failed to restore multipart {}: {}", rel_path.display(), e);
                error!("{}", msg);
                errors.push(msg);
            } else {
                success += 1;
            }
        } else {
            let data_path = format!(
                "data/{}/{}",
                if metadata.encrypted { "enc" } else { "clear" },
                metadata.hash
            );

            if !storage.exists(&data_path).await? {
                // 🐛 Fix: More informative error about missing data
                let msg = format!(
                    "❌ Missing data for file: {}\n   This file is referenced in the backup metadata but the data file is missing.\n   Your backup may be incomplete or corrupted.",
                    rel_path.display()
                );
                info!("{}", msg);
                errors.push(msg);
                continue;
            }

            if restore_file.exists() && !force {
                let msg = format!(
                    "File already exists (use --force): {}",
                    restore_file.display()
                );
                info!("{}", msg);
                errors.push(msg);
                continue;
            }

            if let Err(e) = restore_single_file(
                &*storage,
                &data_path,
                &restore_file,
                if metadata.encrypted {
                    encryption_key
                } else {
                    None
                },
                &metadata.hash,
                metadata.compressed,
            )
            .await
            {
                let msg = format!("Failed to restore {}: {}", rel_path.display(), e);
                error!("{}", msg);
                errors.push(msg);
            } else {
                success += 1;
            }
        }

        current_files += 1;
        pb.inc(1);
    }

    pb.finish_with_message("Restore completed");

    let status = if errors.is_empty() {
        "Success"
    } else {
        "Failure with errors"
    }
    .to_string();

    let report_data = ReportData {
        operation: "Restore".to_string(),
        profile_path: "".to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: start_time.elapsed(),
        mode: None,
        files_processed: success,
        files_skipped: 0,
        files_with_errors: errors.len(),
        total_files,
        errors,
        status,
    };

    Ok(report_data)
}

async fn restore_single_file(
    storage: &dyn Storage,
    backup_path: &str,
    restore_path: &Path,
    key: Option<&str>,
    expected_hash: &str,
    compressed: bool,
) -> io::Result<()> {
    if let Some(parent) = restore_path.parent() {
        let parent_str = parent.to_str().ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid path characters",
        ))?;
        ensure_directory_exists_async(parent_str)
            .await
            .map_err(io::Error::other)?;
    }

    let data = storage.read(backup_path).await?;

    let decrypted = if let Some(k) = key {
        decrypt_data(&data, k.as_bytes())?
    } else {
        data
    };

    let final_data = if compressed {
        let mut decompressed = Vec::new();
        copy_decode(&decrypted[..], &mut decompressed)?;
        decompressed
    } else {
        decrypted
    };

    let computed_hash = crate::crypto::hash_file_content(&final_data)?;
    if computed_hash != expected_hash {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "Hash mismatch"));
    }

    tokio::fs::write(restore_path, final_data).await?;
    Ok(())
}

async fn restore_multipart_file(
    storage: &dyn Storage,
    chunks: &[ChunkMetadata],
    restore_path: &Path,
    key: Option<&str>,
    expected_full_hash: &str,
    compressed: bool,
) -> io::Result<()> {
    if let Some(parent) = restore_path.parent() {
        let parent_str = parent.to_str().ok_or(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid path characters",
        ))?;
        ensure_directory_exists_async(parent_str)
            .await
            .map_err(io::Error::other)?;
    }

    let mut file = File::create(restore_path).await?;

    for chunk in chunks {
        let data_path = format!(
            "data/{}/{}",
            if key.is_some() { "enc" } else { "clear" },
            chunk.hash
        );

        let data = storage.read(&data_path).await?;
        let decrypted = if let Some(k) = key {
            decrypt_data(&data, k.as_bytes())?
        } else {
            data
        };

        let chunk_data = if compressed {
            let mut decompressed = Vec::new();
            copy_decode(&decrypted[..], &mut decompressed)?;
            decompressed
        } else {
            decrypted
        };

        file.write_all(&chunk_data).await?;
    }

    file.flush().await?;

    let mapped = mmap_file(restore_path)?;
    let computed = crate::crypto::hash_file_content(&mapped)?;
    if computed != expected_full_hash {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Full file hash mismatch after restore",
        ));
    }

    Ok(())
}

/// 🐛 Fix: Validate backup structure before attempting restore
/// This catches issues early and provides clear error messages
async fn validate_backup_structure(
    storage: &dyn Storage,
) -> Result<(), Box<dyn std::error::Error>> {
    info!("🔍 Validating backup structure...");

    // Check for snapshots directory
    match storage.list("snapshots/").await {
        Ok(snapshots) => {
            if snapshots.is_empty() {
                return Err(
                    "❌ No backups found\n\n\
                    The snapshots/ directory is empty.\n\
                    This indicates:\n\
                    - The backup folder is empty or corrupt\n\
                    - The backup was never completed\n\
                    - Only parts of the backup were copied\n\n\
                    Solution: Ensure the entire backup folder was copied from the original computer."
                        .into(),
                );
            }
            info!("✅ Snapshots found: {}", snapshots.len());
        }
        Err(e) => {
            return Err(format!(
                "❌ Cannot read backup metadata\n\n\
                    The snapshots directory is missing or inaccessible.\n\
                    This likely means:\n\
                    - The backup folder structure is incomplete\n\
                    - The entire backup wasn't copied\n\
                    - Permission issues accessing the backup\n\n\
                    Error: {}\n\n\
                    Solution: Re-copy the entire backup folder from the original computer.",
                e
            )
            .into());
        }
    }

    // Check for data directory
    if !storage.exists("data/").await? {
        return Err("❌ Backup data directory missing\n\n\
            The data/ folder is not found in the backup.\n\
            This indicates:\n\
            - Incomplete backup copy (only copied metadata, not data)\n\
            - Corrupted backup structure\n\
            - Wrong backup folder selected\n\n\
            Solution: \n\
            1. Verify the backup folder has these subdirectories:\n\
               - snapshots/\n\
               - data/clear/ (or data/enc/ for encrypted backups)\n\
            2. If missing, re-copy the entire backup from the original computer"
            .into());
    }

    info!("✅ Backup structure is valid");
    Ok(())
}
