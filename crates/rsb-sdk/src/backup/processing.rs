use crate::config::Config;
use crate::core::dedup_index::{DedupIndex, entry_objects};
use crate::core::file_processor;
use crate::core::manifest::write_manifest_with_key;
use crate::core::resource_monitor::spawn_resource_monitor;
use crate::core::types::FileMetadata;
use crate::core::types::FileStatus;
use crate::crypto::{KeyCache, RepoKey};
use crate::error::RsbError;
use crate::options::BackupOptions;
use crate::report::ReportData;
use crate::storage::{LockKind, RepoLock, Storage};
use crate::utils::expand_path;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error};

use super::discovery;
use super::metadata::{CachedEncryptionKey, load_previous_metadata};
use super::progress;
use super::stats::{Stats, StatsSummary};
use super::threading;

/// Performs a file system backup of `config.source_path`.
///
/// Mode, key, dry-run, threads, progress callback and cancellation are
/// given through [`BackupOptions`].
pub async fn perform_backup(
    config: &Config,
    options: BackupOptions,
) -> crate::error::Result<ReportData> {
    let start_time = Instant::now();
    let source_expanded = expand_path(&config.source_path);
    let source = source_expanded.as_path();

    if !source.is_dir() {
        return Err(RsbError::InvalidInput(format!(
            "Source path is not a directory: {}",
            source.display()
        )));
    }

    let storage =
        crate::core::storage_backend::storage_or_from_config(options.storage.clone(), config)
            .await?;

    // ====================== REPOSITORY LOCK ======================
    // Shared lock: concurrent backups are fine, but prune (exclusive) must
    // not garbage-collect data objects this backup writes before its
    // manifest exists. Dry-runs write nothing and take no lock.
    let dry_run = options.dry_run;

    // ====================== REPOSITORY KEY ======================
    // Unlock keys/repo.key (wrong password -> typed AuthenticationFailed
    // before anything is read or written), or create it on the first
    // encrypted backup. Done before the shared lock: creation takes the
    // exclusive lock so two first backups cannot create two keys.
    let repo_key = match options.key().filter(|k| !k.is_empty()) {
        // An encrypted (format v3) repository never receives unencrypted
        // objects: without a key the backup is refused before anything is
        // read or written (it used to write every file in clear).
        None if crate::crypto::keyfile::repo_key_exists(&*storage).await? => {
            return Err(RsbError::crypto(
                crate::error::CryptoErrorKind::Other,
                format!(
                    "The repository is encrypted ({} exists) and no encryption key was \
                     given: provide the key via {}, the profile's OS keyring entry or the \
                     prompt. Refusing to back up unencrypted.",
                    crate::crypto::keyfile::REPO_KEY_PATH,
                    crate::config::ENCRYPTION_KEY_ENV
                ),
            ));
        }
        Some(pw) => Some(Arc::new(
            crate::crypto::keyfile::open_or_init_repo_key(storage.clone(), pw.as_bytes(), dry_run)
                .await?,
        )),
        None => None,
    };

    let repo_lock = if dry_run {
        None
    } else {
        Some(RepoLock::acquire(storage.clone(), LockKind::Shared, "backup").await?)
    };

    let result = run_backup(config, source, storage, &options, repo_key, start_time)
        .await
        .map_err(RsbError::from);

    if let Some(lock) = repo_lock
        && let Err(e) = lock.release().await
    {
        tracing::warn!("Failed to release repository lock after backup: {}", e);
    }
    result
}

async fn run_backup(
    config: &Config,
    source: &Path,
    storage: Arc<dyn Storage>,
    options: &BackupOptions,
    repo_key: Option<Arc<RepoKey>>,
    start_time: Instant,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let mode = options.mode.as_deref().unwrap_or(&config.backup_mode);
    let encryption_key = options.key();
    let dry_run = options.dry_run;
    let max_threads = options.max_threads;
    let on_progress = options.on_progress.clone();
    let cancellation_token = options.cancellation_token.clone();
    debug!(
        "🚀 Starting backup{}: {} -> {} (mode: {})",
        if dry_run { " (DRY-RUN)" } else { "" },
        source.display(),
        config.destination_path,
        mode
    );

    // ====================== DISCOVERY ======================
    let (files, walk_errors) =
        discovery::discover_files_with_errors(source, &config.exclude_patterns)?;

    if files.is_empty() {
        return Err(format!(
            "No files found to backup. Check path exists and contains files: {}",
            source.display()
        )
        .into());
    }

    debug!("📋 Discovered {} files to backup", files.len());

    // ====================== PREVIOUS METADATA ======================
    // Load metadata indexed by file hash for deduplication
    // Deduplication já funciona no file_processor.rs via hash matching
    let key_cache = match (encryption_key, &repo_key) {
        (Some(pw), Some(k)) => Some(KeyCache::with_repo_key(pw.as_bytes(), k.clone())),
        _ => None,
    };
    let previous_metadata_cache = load_previous_metadata(&*storage, key_cache.as_ref()).await?;
    debug!(
        "✅ Deduplication cache ready: {} files indexed by hash",
        previous_metadata_cache.len()
    );
    // Local index of objects known to be in the repository (no HEAD per
    // known object). `None` (disabled, or any problem opening it) means
    // every lookup asks storage, as without an index.
    let dedup_index = Arc::new(DedupIndex::open(&*storage, &options.dedup_index, dry_run).await);
    if let Some(idx) = dedup_index.as_ref() {
        debug!("🗂️ Dedup index: {} known objects", idx.len());
    }

    // ====================== SHARED STATE ======================
    let snapshot_manifest: Arc<Mutex<HashMap<PathBuf, FileMetadata>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel::<(PathBuf, FileMetadata)>(8192);

    let stats = Arc::new(Stats::new());
    // Walker errors (unreadable dirs, ...) make the backup incomplete:
    // report them instead of claiming a clean success.
    for _ in &walk_errors {
        stats.inc_error();
    }
    let errors_list = Arc::new(Mutex::new(walk_errors));

    let (resource_paused, monitor_running, monitor_handle) =
        spawn_resource_monitor(config.pause_on_low_battery, config.pause_on_high_cpu);

    let pb = progress::create_progress_bar(files.len() as u64);

    // ====================== PARALLELISM CONTROL ======================
    let num_threads = threading::determine_optimal_threads(config, max_threads);
    debug!("⚡ Using {} threads for processing", num_threads);
    threading::setup_rayon_thread_pool(num_threads)?;

    // ====================== PREPARE DATA ======================
    let encryption_key_cached = CachedEncryptionKey::new(repo_key.clone());
    let encrypt_patterns = config.encrypt_patterns.clone();
    let on_progress_clone = on_progress.clone();
    let compression_level = config.compression_level.unwrap_or(3);
    let files_len = files.len();

    let storage_clone = storage.clone();
    let previous_cache = Arc::new(previous_metadata_cache);
    let snapshot_clone = snapshot_manifest.clone();
    let tx_clone = tx.clone();
    let stats_clone = stats.clone();
    let errors_clone = errors_list.clone();
    let cancellation_clone = cancellation_token.clone();
    let rt_handle = tokio::runtime::Handle::current();
    let index_clone = dedup_index.clone();
    let previous_for_recheck = previous_cache.clone();

    // ====================== PROCESSING ======================
    let processing_handle = tokio::task::spawn_blocking(move || {
        use rayon::prelude::*;

        files.par_iter().for_each(|(full_path, rel_path)| {
            if let Some(token) = &cancellation_clone
                && token.is_cancelled()
            {
                return;
            }

            while resource_paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // Process file (includes deduplication by hash)
            let result = file_processor::process_file(
                full_path,
                &storage_clone,
                rel_path,
                encryption_key_cached.as_ref(), // ⚡ Pre-derived key (not PBKDF2 per-file)
                &previous_cache,
                dry_run,
                &rt_handle,
                &encrypt_patterns,
                compression_level,
                index_clone.as_ref().as_ref(),
            );

            match result {
                Ok((status, metadata)) => {
                    if matches!(status, FileStatus::Processed) {
                        stats_clone.inc_processed();
                    } else {
                        stats_clone.inc_skipped();
                    }
                    let key = crate::core::manifest::manifest_key(rel_path);
                    let _ = tx_clone.blocking_send((key, metadata));
                }
                Err(e) => {
                    stats_clone.inc_error();
                    let msg = format!("Failed {}: {}", full_path.display(), e);
                    error!("{}", msg);
                    errors_clone.lock().unwrap().push(msg);
                }
            }

            pb.inc(1);
            progress::update_progress(&pb, &stats_clone, files_len, rel_path, &on_progress_clone);
        });

        progress::finish_progress_bar(&pb, &stats_clone);
    });

    // ====================== MANIFEST UPDATER ======================
    let manifest_updater = tokio::spawn(async move {
        while let Some((path, meta)) = rx.recv().await {
            snapshot_clone.lock().unwrap().insert(path, meta);
        }
    });

    processing_handle.await?;
    drop(tx);
    let _ = manifest_updater.await;

    monitor_running.store(false, Ordering::Relaxed);
    monitor_handle.thread().unpark();
    let _ = monitor_handle.join();

    // ====================== DEDUP INDEX RECHECK ======================
    // If storage contradicted the index during this backup, every object
    // the index answered for is checked again; files whose object is
    // missing are processed again without the index.
    if let Some(idx) = dedup_index.as_ref()
        && idx.is_distrusted()
    {
        let bad = idx.recheck_trusted(&*storage).await?;
        if !bad.is_empty() {
            let affected: Vec<PathBuf> = snapshot_manifest
                .lock()
                .unwrap()
                .iter()
                .filter(|(_, meta)| entry_objects(meta).iter().any(|o| bad.contains(o)))
                .map(|(rel, _)| rel.clone())
                .collect();
            tracing::warn!(
                "{} objects trusted from the dedup index are missing; re-processing {} files",
                bad.len(),
                affected.len()
            );
            let storage_c = storage.clone();
            let previous_c = previous_for_recheck;
            let manifest_c = snapshot_manifest.clone();
            let errors_c = errors_list.clone();
            let stats_c = stats.clone();
            let key_c = repo_key.clone();
            let patterns_c = config.encrypt_patterns.clone();
            let source_c = source.to_path_buf();
            let rt = tokio::runtime::Handle::current();
            tokio::task::spawn_blocking(move || {
                for rel in affected {
                    let full = source_c.join(&rel);
                    match file_processor::process_file(
                        &full,
                        &storage_c,
                        &rel,
                        key_c.clone(),
                        &previous_c,
                        dry_run,
                        &rt,
                        &patterns_c,
                        compression_level,
                        None,
                    ) {
                        Ok((_, meta)) => {
                            manifest_c.lock().unwrap().insert(rel, meta);
                        }
                        Err(e) => {
                            manifest_c.lock().unwrap().remove(&rel);
                            stats_c.inc_error();
                            let msg = format!("Failed {}: {}", full.display(), e);
                            error!("{}", msg);
                            errors_c.lock().unwrap().push(msg);
                        }
                    }
                }
            })
            .await?;
        }
    }

    // ====================== FINALIZE ======================
    let snapshot_path = if !dry_run {
        // Data objects were written with deferred durability: make them
        // durable before the manifest that references them exists.
        storage.sync().await?;
        let manifest = snapshot_manifest.lock().unwrap().clone();
        let path =
            write_manifest_with_key(&*storage, &manifest, repo_key.as_deref(), dry_run).await?;
        if let Some(idx) = dedup_index.as_ref()
            && let Err(e) = idx.save(&manifest, &path)
        {
            tracing::warn!("Could not save the dedup index: {}", e);
        }
        path
    } else {
        "[Dry-run] No snapshot written".to_string()
    };

    let stats_final = stats.finalize();
    debug!(
        "✅ Backup completed | Snapshot: {} | {}",
        snapshot_path, stats_final
    );

    Ok(build_report_data(
        start_time,
        mode,
        stats_final,
        errors_list,
    ))
}

fn build_report_data(
    start: Instant,
    mode: &str,
    stats: StatsSummary,
    errors_list: Arc<Mutex<Vec<String>>>,
) -> ReportData {
    let errors = errors_list.lock().unwrap().clone();
    let has_errors = !errors.is_empty();

    ReportData {
        operation: "Backup".to_string(),
        profile_path: String::new(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: start.elapsed(),
        mode: Some(mode.to_string()),
        files_processed: stats.processed,
        files_skipped: stats.skipped,
        files_with_errors: stats.errors,
        total_files: stats.processed + stats.skipped + stats.errors,
        status: if has_errors {
            "Failure with errors".to_string()
        } else {
            "Success".to_string()
        },
        errors,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn backup_into(
        storage: Arc<dyn Storage>,
        key: Option<&str>,
    ) -> crate::error::Result<ReportData> {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "secret").unwrap();
        let cfg = Config::new(dir.path().to_string_lossy(), "unused");
        perform_backup(
            &cfg,
            BackupOptions {
                storage: Some(storage),
                ..Default::default()
            }
            .with_encryption_key(key),
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn encrypted_repository_refuses_a_backup_without_key() {
        let mem = Arc::new(crate::storage::MemoryStorage::new());
        let storage: Arc<dyn Storage> = mem.clone();
        storage
            .write(crate::crypto::keyfile::REPO_KEY_PATH, b"{}")
            .await
            .unwrap();
        for key in [None, Some("")] {
            let err = backup_into(storage.clone(), key).await.unwrap_err();
            assert!(err.to_string().contains("repository is encrypted"), "{err}");
        }
        // Nothing but the key file is in the repository.
        for prefix in ["data", "snapshots", "locks"] {
            assert!(
                storage.list(prefix).await.unwrap_or_default().is_empty(),
                "{prefix}"
            );
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn unencrypted_repository_still_accepts_a_backup_without_key() {
        let storage: Arc<dyn Storage> = Arc::new(crate::storage::MemoryStorage::new());
        let report = backup_into(storage.clone(), None).await.unwrap();
        assert_eq!(report.files_processed, 1);
        assert!(!storage.list("data/clear").await.unwrap().is_empty());
    }

    #[test]
    fn test_build_report_no_errors() {
        let start = Instant::now();
        let stats = StatsSummary {
            processed: 10,
            skipped: 2,
            errors: 0,
        };
        let errors = Arc::new(Mutex::new(Vec::new()));

        let report = build_report_data(start, "full", stats, errors);
        assert_eq!(report.status, "Success");
        assert_eq!(report.files_processed, 10);
    }

    #[test]
    fn test_build_report_with_errors() {
        let start = Instant::now();
        let stats = StatsSummary {
            processed: 10,
            skipped: 2,
            errors: 1,
        };
        let errors = Arc::new(Mutex::new(vec!["Error 1".to_string()]));

        let report = build_report_data(start, "full", stats, errors);
        assert_eq!(report.status, "Failure with errors");
        assert_eq!(report.files_with_errors, 1);
    }
}
