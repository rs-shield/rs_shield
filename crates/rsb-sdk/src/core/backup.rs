use super::cancellation::CancellationToken;
use super::manifest::write_manifest;
use super::resource_monitor::spawn_resource_monitor;
use super::types::{FileMetadata, ProgressCallback};
use crate::config::Config;
use crate::core::types::FileStatus;
use crate::report::ReportData;
use crate::utils::{matches_exclude_pattern, walk_filtered};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tracing::{error, info};

pub async fn perform_backup(
    config: &Config,
    mode: &str,
    encryption_key: Option<&str>,
    dry_run: bool,
    resume: bool,
    on_progress: Option<ProgressCallback>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    perform_backup_with_cancellation(
        config,
        mode,
        encryption_key,
        dry_run,
        resume,
        on_progress,
        None,
    )
    .await
}

pub async fn perform_backup_with_cancellation(
    config: &Config,
    mode: &str,
    encryption_key: Option<&str>,
    dry_run: bool,
    resume: bool,
    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let source = Path::new(&config.source_path);
    if !source.is_dir() {
        return Err(format!("Source path is not a directory: {}", source.display()).into());
    }

    // If resume is true, continue from a previous backup
    if resume {
        info!("📋 Resuming previous backup...");
    }

    let storage = super::storage_backend::get_storage(config).await;

    info!(
        "Starting backup{}: {} -> {} (mode: {})",
        if dry_run { " (DRY-RUN)" } else { "" },
        source.display(),
        config.destination_path,
        mode
    );

    let walker = walk_filtered(source, &config.exclude_patterns, true);

    // Log of defined exclusion patterns
    if !config.exclude_patterns.is_empty() {
        info!(
            "Exclusion patterns defined ({}):",
            config.exclude_patterns.len()
        );
        for pattern in &config.exclude_patterns {
            info!("   - {}", pattern);
        }
    } else {
        info!("⚠️  No exclusion patterns defined");
    }

    info!("Respect .gitignore: ACTIVE - Files in .gitignore will be automatically filtered");

    let mut files: Vec<(PathBuf, PathBuf)> = walker
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| {
            // Check if it's a file
            if !e.path().is_file() {
                return false;
            }

            // Check if it matches an exclusion pattern
            let rel_path = match e.path().strip_prefix(source) {
                Ok(p) => p,
                Err(_) => return true, // If strip fails, include the file
            };

            // If any exclusion pattern matches, EXCLUDE the file
            for pattern in &config.exclude_patterns {
                if matches_exclude_pattern(rel_path, pattern) {
                    info!("Excluding: {} (pattern: '{}')", rel_path.display(), pattern);
                    return false; // Exclude this file
                }
            }

            true // Include file
        })
        .map(|e| {
            let full = e.path().to_path_buf();
            let rel = full.strip_prefix(source).unwrap().to_path_buf();
            (full, rel)
        })
        .collect();

    files.sort_by(|a, b| {
        let prio_a = super::file_processor::get_file_priority(&a.0);
        let prio_b = super::file_processor::get_file_priority(&b.0);
        prio_a.cmp(&prio_b)
    });

    info!("Found {} files to process", files.len());

    let snapshot_manifest: Arc<Mutex<HashMap<PathBuf, FileMetadata>>> =
        Arc::new(Mutex::new(HashMap::new()));

    let mut previous_metadata_cache: HashMap<String, FileMetadata> = HashMap::new();
    if let Ok((_, content)) =
        super::manifest::find_latest_snapshot(&*storage, None, encryption_key).await
    {
        if let Ok(prev_manifest) = toml::from_str::<HashMap<PathBuf, FileMetadata>>(&content) {
            for meta in prev_manifest.values() {
                previous_metadata_cache.insert(meta.hash.clone(), meta.clone());
            }
        }
    }
    let previous_metadata_cache = Arc::new(previous_metadata_cache);

    let (resource_paused, monitor_running, monitor_handle) =
        spawn_resource_monitor(config.pause_on_low_battery, config.pause_on_high_cpu);

    let pb = ProgressBar::new(files.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );

    let stats_processed = Arc::new(AtomicUsize::new(0));
    let stats_skipped = Arc::new(AtomicUsize::new(0));
    let stats_errors = Arc::new(AtomicUsize::new(0));
    let errors_list = Arc::new(Mutex::new(Vec::new()));

    // Clones needed for the closure
    let storage_clone = storage.clone();
    let snapshot_manifest_clone = snapshot_manifest.clone();
    let previous_metadata_cache_clone = previous_metadata_cache.clone();
    let errors_list_clone = errors_list.clone();
    let stats_processed_clone = stats_processed.clone();
    let stats_skipped_clone = stats_skipped.clone();
    let stats_errors_clone = stats_errors.clone();
    let resource_paused_clone = resource_paused.clone();
    let pb_clone = pb.clone();

    let encryption_key_clone = encryption_key.map(|k| k.to_string());
    let encrypt_patterns_clone = config.encrypt_patterns.clone();
    let on_progress_clone = on_progress.clone();
    let compression_level = config.compression_level.unwrap_or(3);
    let files_len = files.len();
    let cancellation_token_clone = cancellation_token.clone();

    let rt_handle = tokio::runtime::Handle::current();

    tokio::task::spawn_blocking(move || {
        files.par_iter().for_each(|(full_path, rel_path)| {
            // Check if the operation was cancelled
            if let Some(token) = &cancellation_token_clone {
                if token.is_cancelled() {
                    info!("⏹️ Backup cancelled by user");
                    return;
                }
            }

            while resource_paused_clone.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_secs(1));
            }

            match super::file_processor::process_file(
                full_path,
                &storage_clone,
                rel_path,
                encryption_key_clone.as_deref(),
                &snapshot_manifest_clone,
                dry_run,
                &previous_metadata_cache_clone,
                &rt_handle,
                &encrypt_patterns_clone,
                &on_progress_clone,
                compression_level,
            ) {
                Ok(FileStatus::Processed) => {
                    stats_processed_clone.fetch_add(1, Ordering::Relaxed);
                    pb_clone.set_message(format!("Processed: {}", rel_path.display()));
                }
                Ok(FileStatus::Skipped) => {
                    stats_skipped_clone.fetch_add(1, Ordering::Relaxed);
                    pb_clone.set_message(format!("Skipped: {}", rel_path.display()));
                }
                Err(e) => {
                    stats_errors_clone.fetch_add(1, Ordering::Relaxed);
                    let error_msg = format!("Failed to process {}: {}", full_path.display(), e);
                    error!("{}", &error_msg);
                    errors_list_clone.lock().unwrap().push(error_msg);
                }
            }

            if let Some(cb) = &on_progress_clone {
                let current = stats_processed_clone.load(Ordering::Relaxed)
                    + stats_skipped_clone.load(Ordering::Relaxed)
                    + stats_errors_clone.load(Ordering::Relaxed);
                cb(
                    current,
                    files_len,
                    format!("Processing: {}", rel_path.display()),
                );
            }

            pb_clone.inc(1);
        });

        pb_clone.finish_with_message("Processing completed");
    })
    .await?;

    monitor_running.store(false, Ordering::Relaxed);
    let _ = monitor_handle.join();

    let snapshot_path = if !dry_run {
        let snapshot_data = snapshot_manifest.lock().unwrap().clone();
        write_manifest(&*storage, &snapshot_data, encryption_key, dry_run).await?
    } else {
        "[Dry-run] No snapshot written".to_string()
    };

    // Final comprehensive log
    info!(
        "Backup completed. Snapshot created: {} | Processed: {} | Skipped: {} | Errors: {}",
        snapshot_path,
        stats_processed.load(Ordering::Relaxed),
        stats_skipped.load(Ordering::Relaxed),
        stats_errors.load(Ordering::Relaxed)
    );

    let errors = errors_list.lock().unwrap().clone();
    let status = if errors.is_empty() {
        "Success".to_string()
    } else {
        "Failure with errors".to_string()
    };
    let report_data = ReportData {
        operation: "Backup".to_string(),
        profile_path: "".to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: start_time.elapsed(),
        mode: Some(mode.to_string()),
        files_processed: stats_processed.load(Ordering::Relaxed),
        files_skipped: stats_skipped.load(Ordering::Relaxed),
        files_with_errors: stats_errors.load(Ordering::Relaxed),
        total_files: files_len,
        errors,
        status,
    };

    Ok(report_data)
}
