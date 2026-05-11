use super::cancellation::CancellationToken;
use super::manifest::find_latest_snapshot;
use super::types::{ChunkMetadata, FileMetadata, ProgressCallback};
use crate::config::Config;
use crate::crypto::decrypt_data;
use crate::report::ReportData;
use crate::storage::Storage;
use crate::utils::{ensure_directory_exists_async, mmap_file};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;
use tracing::{error, info, warn};
use zstd::stream::copy_decode;

pub async fn perform_restore(
    config: &Config,
    snapshot_id: Option<&str>,
    target_path: Option<PathBuf>,
    encryption_key: Option<&str>,
    force: bool,
    on_progress: Option<ProgressCallback>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    perform_restore_with_cancellation(
        config,
        snapshot_id,
        target_path,
        encryption_key,
        force,
        on_progress,
        None,
    )
    .await
}

pub async fn perform_restore_with_cancellation(
    config: &Config,
    snapshot_id: Option<&str>,
    target_path: Option<PathBuf>,
    encryption_key: Option<&str>,
    force: bool,
    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let storage = super::storage_backend::get_storage(config).await;

    let (manifest_path, manifest_content) =
        find_latest_snapshot(&*storage, snapshot_id, encryption_key).await?;
    info!("Restoring snapshot: {}", manifest_path);

    let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&manifest_content)?;

    let restore_root =
        target_path.unwrap_or_else(|| Path::new(&config.source_path).join("_restored"));

    info!("Restoring to: {}", restore_root.display());

    let pb = ProgressBar::new(manifest.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut errors = Vec::new();
    let mut success = 0;
    let total_files = manifest.len();
    let mut current_files = 0;

    for (rel_path, metadata) in manifest {
        // Verificar se a operação foi cancelada
        if let Some(token) = &cancellation_token {
            if token.is_cancelled() {
                info!("⏹️ Restauro cancelado pelo usuário");
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
                let msg = format!("Missing data for {}: {}", rel_path.display(), metadata.hash);
                warn!("{}", msg);
                errors.push(msg);
                continue;
            }

            if restore_file.exists() && !force {
                let msg = format!(
                    "File already exists (use --force): {}",
                    restore_file.display()
                );
                warn!("{}", msg);
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
