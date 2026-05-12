use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Instant;
use tracing::info;

use crate::core::manifest::find_latest_snapshot;
use crate::core::storage_backend::get_storage;
use crate::core::types::{FileMetadata, ProgressCallback};
use crate::integrity::checksum::checksum;
use crate::integrity::hash::verify_content_hash;
use crate::integrity::progress_bar::progress_bar;
use crate::report::ReportData;
use crate::{CancellationToken, Config};

pub async fn perform_verify(
    config: &Config,
    snapshot_id: Option<&str>,
    quiet: bool,
    fast: bool,
    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let storage = get_storage(config).await;

    let (path, content) =
        find_latest_snapshot(&*storage, snapshot_id, config.encryption_key.as_deref()).await?;
    info!("Verifying snapshot: {}", path);

    let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&content)?;
    let pb = progress_bar(manifest.clone());
    let total_files = manifest.len();

    let mut errors = Vec::new();
    let (mut stats_missing, mut stats_size_error, mut stats_hash_error, mut stats_ok) =
        (0, 0, 0, 0);

    for (current_files, (rel_path, metadata)) in manifest.into_iter().enumerate() {
        if let Some(token) = &cancellation_token {
            if token.is_cancelled() {
                info!("⏹️ Verification cancelled by the user");
                break;
            }
        }

        pb.set_message(format!("Verifying: {}", rel_path.display()));
        if let Some(cb) = &on_progress {
            cb(
                current_files,
                total_files,
                format!("Verifying: {}", rel_path.display()),
            );
        }

        // Determine items to verify (chunks or single file)
        let items_to_verify = if let Some(chunks) = &metadata.chunks {
            chunks
                .iter()
                .map(|c| {
                    (
                        format!(
                            "data/{}/{}",
                            if metadata.encrypted { "enc" } else { "clear" },
                            c.hash
                        ),
                        Some(c.stored_size),
                        c.stored_hash.as_ref(),
                    )
                })
                .collect()
        } else {
            vec![(
                format!(
                    "data/{}/{}",
                    if metadata.encrypted { "enc" } else { "clear" },
                    metadata.hash
                ),
                metadata.stored_size,
                metadata.stored_hash.as_ref(),
            )]
        };

        let mut file_has_error = false;
        for (data_path, exp_size, exp_hash) in items_to_verify {
            if !storage.exists(&data_path).await? {
                errors.push(format!(
                    "Missing data for {}: {}",
                    rel_path.display(),
                    data_path
                ));
                stats_missing += 1;
                file_has_error = true;
                continue;
            }

            let data = storage.read(&data_path).await?;

            if fast {
                if let Err(e) = checksum(&data, exp_size, exp_hash) {
                    errors.push(format!("{} for {}", e, rel_path.display()));
                    if e.contains("Size") {
                        stats_size_error += 1;
                    } else {
                        stats_hash_error += 1;
                    }
                    file_has_error = true;
                }
            } else {
                if let Err(e) = verify_content_hash(&data, &metadata, config) {
                    errors.push(format!("{} for {}", e, rel_path.display()));
                    stats_hash_error += 1;
                    file_has_error = true;
                }
            }
        }

        if !file_has_error {
            stats_ok += 1;
        }
        pb.inc(1);
    }

    pb.finish_and_clear();

    let status = if errors.is_empty() {
        "Success"
    } else {
        "Failure with errors"
    }
    .to_string();

    if !quiet {
        info!("✅ Files successfully verified: {}", stats_ok);
    }

    let report_data = ReportData {
        operation: "Verify".to_string(),
        profile_path: "".to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: start_time.elapsed(),
        mode: Some(if fast { "Fast (Lite)" } else { "Full" }.to_string()),
        files_processed: stats_ok,
        files_skipped: 0,
        files_with_errors: errors.len(),
        total_files,
        errors,
        status,
    };

    if !quiet {
        info!("Verification completed.");
        info!("Total files: {}", total_files);
        if fast {
            info!("- Missing: {}", stats_missing);
            info!("- Size errors: {}", stats_size_error);
            info!("- Hash errors: {}", stats_hash_error);
        }
    }

    Ok(report_data)
}
