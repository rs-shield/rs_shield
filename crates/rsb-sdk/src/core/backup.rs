use super::cancellation::CancellationToken;
use super::manifest::write_manifest;
use super::resource_monitor::spawn_resource_monitor;
use super::types::{FileMetadata, ProgressCallback};
use crate::config::Config;
use crate::core::types::FileStatus;
use crate::crypto;
use crate::report::ReportData;
use crate::utils::{matches_exclude_pattern, walk_filtered};
use indicatif::{ProgressBar, ProgressStyle};
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::mpsc;
use tracing::{debug, error, info};

pub async fn perform_backup(
    config: &Config,
    mode: &str,
    encryption_key: Option<&str>,
    dry_run: bool,
    resume: bool,
    max_threads: Option<usize>,
    on_progress: Option<ProgressCallback>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    perform_backup_with_cancellation(
        config,
        mode,
        encryption_key,
        dry_run,
        resume,
        max_threads,
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
    max_threads: Option<usize>,

    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<ReportData, Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let source = Path::new(&config.source_path);

    if !source.is_dir() {
        return Err(format!("Source path is not a directory: {}", source.display()).into());
    }

    let storage = super::storage_backend::get_storage(config).await;

    debug!(
        "🚀 Starting backup{}: {} -> {} (mode: {})",
        if dry_run { " (DRY-RUN)" } else { "" },
        source.display(),
        config.destination_path,
        mode
    );

    // ====================== DISCOVERY ======================
    let files = discover_files(source, &config.exclude_patterns)?;

    // ====================== PREVIOUS METADATA ======================
    let previous_metadata_cache = load_previous_metadata(&*storage, encryption_key).await?;

    // ====================== SHARED STATE ======================
    let snapshot_manifest: Arc<Mutex<HashMap<PathBuf, FileMetadata>>> =
        Arc::new(Mutex::new(HashMap::new()));
    let (tx, mut rx) = mpsc::channel::<(PathBuf, FileMetadata)>(8192);

    let stats = Arc::new(Stats::default());
    let errors_list = Arc::new(Mutex::new(Vec::new()));

    let (resource_paused, monitor_running, monitor_handle) =
        spawn_resource_monitor(config.pause_on_low_battery, config.pause_on_high_cpu);

    let pb = create_progress_bar(files.len() as u64);

    // ====================== PARALLELISM CONTROL ======================
    let num_threads = determine_optimal_threads(config, max_threads);
    debug!("⚡ Using {} threads for processing", num_threads);

    // Configura Rayon globalmente para esta operação
    let _pool = rayon::ThreadPoolBuilder::new()
        .num_threads(num_threads)
        .build_global()
        .ok(); // ignora erro se já estiver configurado

    // ====================== PREPARE DATA ======================
    let encryption_key_owned: Option<String> = encryption_key.map(|k| k.to_string());
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

    // ⚡ OPTIMIZATION: Pre-derive encryption key ONCE (not per-file)
    let encryption_key_cached = encryption_key_owned.as_deref().and_then(|pwd| {
        match crate::crypto::EncryptionKey::new(pwd.as_bytes()) {
            Ok(key) => {
                debug!(
                    "✅ Pre-derived encryption key (saved {:?} PBKDF2 iterations per file)",
                    600_000 * files_len
                );
                Some(Arc::new(key))
            }
            Err(e) => {
                error!("Failed to pre-derive encryption key: {}", e);
                None
            }
        }
    });

    let encryption_key_cached_clone = encryption_key_cached.clone();

    // ====================== PROCESSING ======================
    let processing_handle = tokio::task::spawn_blocking(move || {
        files.par_iter().for_each(|(full_path, rel_path)| {
            if let Some(token) = &cancellation_clone {
                if token.is_cancelled() {
                    return;
                }
            }

            while resource_paused.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            let result = super::file_processor::process_file(
                full_path,
                &storage_clone,
                rel_path,
                encryption_key_cached_clone.clone(), // ⚡ Pass pre-derived key
                &previous_cache,
                dry_run,
                &rt_handle,
                &encrypt_patterns,
                compression_level,
            );

            match result {
                Ok((status, metadata)) => {
                    if matches!(status, FileStatus::Processed) {
                        stats_clone.inc_processed();
                    } else {
                        stats_clone.inc_skipped();
                    }
                    let _ = tx_clone.blocking_send((rel_path.clone(), metadata));
                }
                Err(e) => {
                    stats_clone.inc_error();
                    let msg = format!("Failed {}: {}", full_path.display(), e);
                    error!("{}", msg);
                    errors_clone.lock().unwrap().push(msg);
                }
            }

            pb.inc(1);
            update_progress(&pb, &stats_clone, files_len, rel_path, &on_progress_clone);
        });

        pb.finish_with_message("✅ Processing completed");
    });

    // Manifest updater
    let manifest_updater = tokio::spawn(async move {
        while let Some((path, meta)) = rx.recv().await {
            snapshot_clone.lock().unwrap().insert(path, meta);
        }
    });

    processing_handle.await?;
    drop(tx);
    let _ = manifest_updater.await;

    monitor_running.store(false, Ordering::Relaxed);
    let _ = monitor_handle.join();

    // ====================== FINALIZE ======================
    let snapshot_path = if !dry_run {
        let manifest = snapshot_manifest.lock().unwrap().clone();
        write_manifest(&*storage, &manifest, encryption_key, dry_run).await?
    } else {
        "[Dry-run] No snapshot written".to_string()
    };

    let stats_final = stats.finalize();
    debug!(
        "✅ Backup completed | Snapshot: {} | Processed: {} | Skipped: {} | Errors: {}",
        snapshot_path, stats_final.processed, stats_final.skipped, stats_final.errors
    );

    Ok(build_report_data(
        start_time,
        mode,
        stats_final,
        errors_list,
    ))
}

// ====================== PARALLELISM HELPER ======================
fn determine_optimal_threads(config: &Config, max_threads: Option<usize>) -> usize {
    // Prioridade:
    // 1. Configuração explícita no config
    if let Some(threads) = max_threads {
        if threads > 0 {
            return threads.min(256); // limite alto para I/O paralelo
        }
    }

    // 2. Lógica automática baseada em núcleos
    let cores = num_cpus::get();

    // Aumenta threads para I/O-bound operations como backup com criptografia
    // Fórmula: 2x cores para não sobrecarregar, mas permitir paralelismo máximo de I/O
    let optimal = (cores * 2).min(256);

    debug!(
        "📊 System has {} cores, using {} threads for optimal backup parallelism",
        cores, optimal
    );
    optimal
}

// ====================== RESTO DO CÓDIGO (mesmo de antes) ======================
fn discover_files(
    source: &Path,
    exclude_patterns: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>, Box<dyn std::error::Error>> {
    let walker = walk_filtered(source, exclude_patterns, true);

    let mut files: Vec<(PathBuf, PathBuf)> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let full = e.path().to_path_buf();
            let rel = full.strip_prefix(source).ok()?.to_path_buf();

            for pattern in exclude_patterns {
                if matches_exclude_pattern(&rel, pattern) {
                    debug!("Excluding: {}", rel.display());
                    return None;
                }
            }
            Some((full, rel))
        })
        .collect();

    files.par_sort_by_key(|(full, _)| super::file_processor::get_file_priority(full));
    debug!("📊 Found {} files to process", files.len());

    Ok(files)
}
#[derive(Default)]
struct Stats {
    processed: AtomicUsize,
    skipped: AtomicUsize,
    errors: AtomicUsize,
}

impl Stats {
    fn inc_processed(&self) {
        self.processed.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_skipped(&self) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
    }
    fn inc_error(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }
    fn finalize(&self) -> StatsSummary {
        StatsSummary {
            processed: self.processed.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
        }
    }
}

#[derive(Clone, Copy)]
struct StatsSummary {
    pub processed: usize,
    pub skipped: usize,
    pub errors: usize,
}

async fn load_previous_metadata(
    storage: &dyn crate::storage::Storage,
    key: Option<&str>,
) -> Result<HashMap<String, FileMetadata>, Box<dyn std::error::Error>> {
    let mut cache = HashMap::new();
    if let Ok((_, content)) = super::manifest::find_latest_snapshot(storage, None, key).await {
        if let Ok(prev) = toml::from_str::<HashMap<PathBuf, FileMetadata>>(&content) {
            cache.reserve(prev.len());
            for meta in prev.values() {
                cache.insert(meta.hash.clone(), meta.clone());
            }
        }
    }
    Ok(cache)
}

fn create_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb
}

fn update_progress(
    pb: &ProgressBar,
    stats: &Stats,
    total: usize,
    rel_path: &Path,
    on_progress: &Option<ProgressCallback>,
) {
    // Shorten path: show only last 2 path components for cleaner UI
    let display_path = shorten_path(rel_path);
    pb.set_message(format!("📄 {}", display_path));

    if let Some(cb) = on_progress {
        let current = stats.processed.load(Ordering::Relaxed)
            + stats.skipped.load(Ordering::Relaxed)
            + stats.errors.load(Ordering::Relaxed);
        cb(
            current,
            total,
            format!("📄 {}", display_path),
        );
    }
}

/// Shorten path to show only the last 1-2 components for cleaner logs
/// E.g. "/home/user/project/src/main.rs" → "src/main.rs"
fn shorten_path(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();
    if components.len() > 2 {
        // Show last 2 components
        let parent = path.parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("...");
        let file = path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        format!("{}/{}", parent, file)
    } else {
        path.display().to_string()
    }
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
