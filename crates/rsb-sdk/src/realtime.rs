use crate::backup::{BackupOptions, perform_backup};
use crate::config::Config;
use crate::utils::ensure_directory_exists;
use chrono::{DateTime, Utc};
use notify::{RecursiveMode, Result as NotifyResult, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};
use tokio::sync::{Notify, RwLock};
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

/// changes detected in real-time
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChangeType {
    Created,
    Modified,
    Deleted,
    Renamed,
}

/// File change detected in real-time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub change_type: ChangeType,
    pub timestamp: DateTime<Utc>,
    pub size: u64,
    pub hash: Option<String>,
}

/// Queue of changes to synchronize
#[derive(Clone)]
pub struct ChangeQueue {
    changes: Arc<RwLock<VecDeque<FileChange>>>,
    max_size: usize,
    last_added: Arc<StdMutex<Option<Instant>>>,
}

impl ChangeQueue {
    pub fn new(max_size: usize) -> Self {
        ChangeQueue {
            changes: Arc::new(RwLock::new(VecDeque::with_capacity(max_size))),
            max_size,
            last_added: Arc::new(StdMutex::new(None)),
        }
    }

    pub async fn add_change(&self, change: FileChange) {
        let mut q = self.changes.write().await;
        if q.len() >= self.max_size {
            q.pop_front();
        }
        q.push_back(change);
        *self.last_added.lock().unwrap() = Some(Instant::now());
    }

    /// Time since the last change was added (None if nothing was ever added).
    pub fn last_change_age(&self) -> Option<Duration> {
        self.last_added.lock().unwrap().map(|t| t.elapsed())
    }

    pub async fn get_changes(&self) -> Vec<FileChange> {
        self.changes.read().await.iter().cloned().collect()
    }

    /// Removes and returns every queued change (the queue is left empty).
    pub async fn take_all(&self) -> Vec<FileChange> {
        std::mem::take(&mut *self.changes.write().await).into()
    }

    pub async fn clear(&self) {
        self.changes.write().await.clear();
    }

    pub async fn count(&self) -> usize {
        self.changes.read().await.len()
    }
}

/// File monitor in real-time
pub struct RealtimeWatcher {
    watcher: Option<notify::RecommendedWatcher>,
    queue: ChangeQueue,
    watched_paths: Arc<RwLock<Vec<PathBuf>>>,
}

impl RealtimeWatcher {
    pub fn new(queue: ChangeQueue) -> Self {
        RealtimeWatcher {
            watcher: None,
            queue,
            watched_paths: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub async fn start_watching(&mut self, root_path: &Path) -> NotifyResult<()> {
        let queue = self.queue.clone();
        let root = root_path.to_path_buf();
        // The notify callback runs on notify's own thread, outside the tokio
        // runtime: spawn through an explicit handle (tokio::spawn would panic).
        let rt = tokio::runtime::Handle::current();

        let mut watcher =
            notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
                match res {
                    Ok(event) => {
                        let change_type = match event.kind {
                            notify::EventKind::Create(_) => ChangeType::Created,
                            notify::EventKind::Modify(notify::event::ModifyKind::Name(_)) => {
                                ChangeType::Renamed
                            }
                            notify::EventKind::Modify(_) => ChangeType::Modified,
                            notify::EventKind::Remove(_) => ChangeType::Deleted,
                            // Ignore accesses and unknown events
                            _ => return,
                        };
                        let queue_clone = queue.clone();
                        rt.spawn(async move {
                            for path in &event.paths {
                                // Deleted paths (and the old side of a rename)
                                // no longer exist: record them with size 0
                                // instead of silently dropping them.
                                let size = match std::fs::symlink_metadata(path) {
                                    Ok(metadata) => metadata.len(),
                                    Err(_)
                                        if matches!(
                                            change_type,
                                            ChangeType::Deleted | ChangeType::Renamed
                                        ) =>
                                    {
                                        0
                                    }
                                    // Created/modified then removed before we
                                    // looked: nothing to sync.
                                    Err(_) => continue,
                                };

                                let change = FileChange {
                                    path: path.clone(),
                                    change_type: change_type.clone(),
                                    timestamp: Utc::now(),
                                    size,
                                    hash: None, // Calculated later
                                };

                                queue_clone.add_change(change).await;
                            }
                        });
                    }
                    Err(e) => eprintln!("Error monitoring files: {}", e),
                }
            })?;

        watcher.watch(&root, RecursiveMode::Recursive)?;
        self.watcher = Some(watcher);
        self.watched_paths.write().await.push(root);

        Ok(())
    }

    pub async fn stop_watching(&mut self) {
        self.watcher = None;
        self.watched_paths.write().await.clear();
    }

    pub async fn get_watched_paths(&self) -> Vec<PathBuf> {
        self.watched_paths.read().await.clone()
    }
}

/// Synchronization strategy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStrategy {
    /// Legacy flag. The scheduler always debounces by `batch_interval` so a
    /// burst of changes coalesces into a single backup.
    pub immediate_sync: bool,
    /// Debounce window (seconds): a backup starts once no new change has been
    /// seen for this long (0 = as soon as possible). A burst that never goes
    /// quiet is still backed up after `MAX_BATCH_DELAY_FACTOR` windows.
    pub batch_interval: u64,
    /// Legacy field, not used by the backup scheduler.
    pub immediate_threshold: u64,
    /// Ignore patterns (regex) matched against the changed path.
    pub ignore_patterns: Vec<String>,
    /// Direction: "uni" (source → destination) or "bi" (bidirectional)
    pub direction: String,
}

impl Default for SyncStrategy {
    fn default() -> Self {
        SyncStrategy {
            immediate_sync: true,
            batch_interval: 5,
            immediate_threshold: 10_485_760, // 10 MB
            ignore_patterns: vec![".*\\.tmp$".into(), ".*\\.lock$".into()],
            direction: "uni".into(),
        }
    }
}

/// Callback for when changes are processed
pub type OnChangeCallback = Box<dyn Fn(FileChange) + Send + Sync>;

/// Future returned by a [`BackupRunner`]: `Ok(summary)` only when a complete
/// snapshot was written, `Err(message)` otherwise.
pub type BackupFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;

/// Action run for each debounced batch of changes (normally a full
/// incremental backup of the profile, see [`profile_backup_runner`]).
pub type BackupRunner = Arc<dyn Fn() -> BackupFuture + Send + Sync>;

/// A burst that keeps producing changes is still backed up after this many
/// debounce windows (so a constantly-written file cannot starve backups).
pub const MAX_BATCH_DELAY_FACTOR: u32 = 10;
const MIN_POLL: Duration = Duration::from_millis(100);
const MAX_POLL: Duration = Duration::from_secs(1);
const DEFAULT_RETRY_BASE: Duration = Duration::from_secs(2);
const DEFAULT_RETRY_MAX: Duration = Duration::from_secs(300);

/// Builds a runner that performs a real incremental backup of `config` with
/// the SDK backup pipeline (`perform_backup`, which takes the shared
/// repository lock and writes the snapshot manifest atomically).
///
/// The key is kept in a zeroizing buffer and is never logged.
pub fn profile_backup_runner(mut config: Config, encryption_key: Option<String>) -> BackupRunner {
    // The key is passed explicitly; don't keep a second copy in the config.
    config.encryption_key = None;
    let config = Arc::new(config);
    let key: Arc<Option<Zeroizing<String>>> = Arc::new(encryption_key.map(Zeroizing::new));
    Arc::new(move || {
        let config = config.clone();
        let key = key.clone();
        Box::pin(async move {
            // `perform_backup`'s future is not `Send` (it holds a
            // `Box<dyn Error>` across an await), so drive it on a blocking
            // thread. Once started it always runs to completion, even if the
            // caller goes away: the snapshot is all-or-nothing.
            let handle = tokio::runtime::Handle::current();
            let report = tokio::task::spawn_blocking(move || {
                let key_ref = key.as_ref().as_ref().map(|k| k.as_str());
                handle
                    .block_on(perform_backup(
                        &config,
                        BackupOptions {
                            mode: Some("incremental".to_string()),
                            max_threads: config.max_threads,
                            ..Default::default()
                        }
                        .with_encryption_key(key_ref),
                    ))
                    .map_err(|e| format!("backup failed: {}", e))
            })
            .await
            .map_err(|e| format!("backup task failed: {}", e))??;
            if report.files_with_errors > 0 {
                return Err(format!(
                    "backup incomplete: {} file(s) failed{}",
                    report.files_with_errors,
                    report
                        .errors
                        .first()
                        .map(|e| format!(" (first: {})", e))
                        .unwrap_or_default()
                ));
            }
            Ok(format!(
                "{} files ({} new/changed, {} unchanged)",
                report.total_files, report.files_processed, report.files_skipped
            ))
        })
    })
}

/// What a batch of [`mirror_and_backup_sync`] did (for progress output).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WatchEvent {
    /// New/modified files copied to the mirror folder.
    Mirrored { files: usize },
    /// A backup (the `number`-th of this session) wrote a snapshot.
    BackedUp { number: usize, summary: String },
}

/// Receives the [`WatchEvent`]s (called from the backup task).
pub type WatchEventFn = Arc<dyn Fn(WatchEvent) + Send + Sync>;

/// The synchronizer behind `rsb watch` and the desktop Real-Time Sync
/// screen: each debounced batch first mirrors new/modified files of the
/// profile's source to `sync_dst`, then runs an incremental backup of the
/// profile with `cfg.encryption_key`. Neither the mirror nor the
/// repository feed changes back into the watcher (they may live inside the
/// source tree). Start it with [`RealtimeSync::start`] and
/// [`RealtimeSync::spawn_processing_loop`].
pub fn mirror_and_backup_sync(
    cfg: &Config,
    sync_dst: PathBuf,
    strategy: SyncStrategy,
    on_event: Option<WatchEventFn>,
) -> RealtimeSync {
    let source = crate::utils::expand_path(&cfg.source_path);
    let backup_dst = crate::utils::expand_path(&cfg.destination_path);
    let backup = profile_backup_runner(cfg.clone(), cfg.encryption_key.clone());
    let backups = Arc::new(AtomicUsize::new(0));
    let mirror_dst = sync_dst.clone();
    let runner: BackupRunner = Arc::new(move || {
        let backup = backup.clone();
        let backups = backups.clone();
        let on_event = on_event.clone();
        let src = source.clone();
        let dst = mirror_dst.clone();
        Box::pin(async move {
            let copied = crate::backup::diagnostic::sync_changed_files(&src, &dst)
                .await
                .map_err(|e| format!("sync to {} failed: {}", dst.display(), e))?;
            if copied > 0
                && let Some(f) = &on_event
            {
                f(WatchEvent::Mirrored { files: copied });
            }
            let summary = backup().await?;
            let number = backups.fetch_add(1, Ordering::SeqCst) + 1;
            if let Some(f) = &on_event {
                f(WatchEvent::BackedUp {
                    number,
                    summary: summary.clone(),
                });
            }
            Ok(summary)
        })
    });
    RealtimeSync::with_backup_runner(strategy, runner)
        .exclude_root(sync_dst)
        .exclude_root(backup_dst)
}

/// A change waiting for a successful backup.
struct PendingEntry {
    /// Monotonic sequence: a backup only confirms entries whose seq was
    /// assigned before it started.
    seq: u64,
    first_seen: Instant,
    change: FileChange,
}

#[derive(Default)]
struct PendingState {
    entries: HashMap<PathBuf, PendingEntry>,
    next_seq: u64,
}

/// Real-time synchronizer: watches a source tree and, after each debounced
/// burst of changes, runs a backup. Changes are only counted as synced after
/// the backup that covers them succeeded; failures keep them pending and are
/// retried with capped exponential backoff. Backups never run concurrently.
pub struct RealtimeSync {
    watcher: tokio::sync::Mutex<RealtimeWatcher>,
    /// Raw changes reported by the watcher (not yet debounced).
    pub queue: ChangeQueue,
    strategy: SyncStrategy,
    ignore_regexes: Vec<regex::Regex>,
    excluded_roots: Vec<PathBuf>,
    runner: Option<BackupRunner>,
    pending: StdMutex<PendingState>,
    /// Serializes backups (loop and manual `sync_now` callers).
    backup_lock: tokio::sync::Mutex<()>,
    backup_running: AtomicBool,
    synced_count: AtomicUsize,
    backups_completed: AtomicUsize,
    backups_failed: AtomicUsize,
    last_error: StdMutex<Option<String>>,
    is_processing: AtomicBool,
    wake: Notify,
    retry_base: Duration,
    retry_max: Duration,
}

impl RealtimeSync {
    /// Synchronizer without a backup target: changes are tracked but never
    /// reported as synced (every backup attempt fails).
    pub fn new(strategy: SyncStrategy) -> Self {
        let queue = ChangeQueue::new(1000);
        let watcher = RealtimeWatcher::new(queue.clone());
        let ignore_regexes = strategy
            .ignore_patterns
            .iter()
            .filter_map(|p| match regex::Regex::new(p) {
                Ok(re) => Some(re),
                Err(e) => {
                    tracing::warn!("realtime: invalid ignore pattern {:?}: {}", p, e);
                    None
                }
            })
            .collect();

        RealtimeSync {
            watcher: tokio::sync::Mutex::new(watcher),
            queue,
            strategy,
            ignore_regexes,
            excluded_roots: Vec::new(),
            runner: None,
            pending: StdMutex::new(PendingState::default()),
            backup_lock: tokio::sync::Mutex::new(()),
            backup_running: AtomicBool::new(false),
            synced_count: AtomicUsize::new(0),
            backups_completed: AtomicUsize::new(0),
            backups_failed: AtomicUsize::new(0),
            last_error: StdMutex::new(None),
            is_processing: AtomicBool::new(false),
            wake: Notify::new(),
            retry_base: DEFAULT_RETRY_BASE,
            retry_max: DEFAULT_RETRY_MAX,
        }
    }

    /// Synchronizer that runs `runner` for every debounced batch.
    pub fn with_backup_runner(strategy: SyncStrategy, runner: BackupRunner) -> Self {
        let mut sync = Self::new(strategy);
        sync.runner = Some(runner);
        sync
    }

    /// Synchronizer that backs up the profile `config` (incremental backup
    /// with `encryption_key`). A local destination inside the source tree is
    /// excluded from change detection so backups don't trigger themselves.
    pub fn for_profile(
        strategy: SyncStrategy,
        config: Config,
        encryption_key: Option<String>,
    ) -> Self {
        let destination = crate::utils::expand_path(&config.destination_path);
        Self::with_backup_runner(strategy, profile_backup_runner(config, encryption_key))
            .exclude_root(destination)
    }

    /// Ignore every change under `root` (e.g. a destination inside the source).
    pub fn exclude_root(mut self, root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        if let Ok(canonical) = std::fs::canonicalize(&root)
            && canonical != root
        {
            self.excluded_roots.push(canonical);
        }
        self.excluded_roots.push(root);
        self
    }

    /// Override the retry backoff after a failed backup (default 2s doubling,
    /// capped at 5 min).
    pub fn with_retry_backoff(mut self, base: Duration, max: Duration) -> Self {
        self.retry_base = base;
        self.retry_max = max.max(base);
        self
    }

    pub async fn start(&self, source: &Path) -> NotifyResult<()> {
        self.watcher.lock().await.start_watching(source).await
    }

    /// Queues a change of the whole `source` tree, so the next batch backs
    /// it up (used at start-up, before only reacting to changes).
    pub async fn queue_full_backup(&self, source: &Path) {
        self.queue
            .add_change(FileChange {
                path: source.to_path_buf(),
                change_type: ChangeType::Modified,
                timestamp: Utc::now(),
                size: 0,
                hash: None,
            })
            .await;
    }

    /// Stops the file watcher and the processing loop.
    pub async fn stop(&self) {
        self.stop_processing().await;
        self.watcher.lock().await.stop_watching().await;
    }

    /// Start the automatic processing loop in the background.
    pub async fn start_processing_loop(self: Arc<Self>) {
        // Detach: dropping a JoinHandle does not cancel the spawned task.
        drop(self.spawn_processing_loop());
    }

    /// Start the processing loop and return its handle. After
    /// [`stop_processing`](Self::stop_processing) the loop lets a running
    /// backup finish (it is never interrupted half-way) and then exits.
    pub fn spawn_processing_loop(self: Arc<Self>) -> JoinHandle<()> {
        self.is_processing.store(true, Ordering::SeqCst);
        tokio::spawn(self.processing_loop())
    }

    async fn processing_loop(self: Arc<Self>) {
        let debounce = Duration::from_secs(self.strategy.batch_interval);
        let max_delay = (debounce * MAX_BATCH_DELAY_FACTOR).max(MAX_POLL);
        let poll = debounce.clamp(MIN_POLL, MAX_POLL);
        let mut failures: u32 = 0;
        let mut retry_at: Option<Instant> = None;

        while self.is_processing.load(Ordering::SeqCst) {
            tokio::select! {
                _ = self.wake.notified() => {}
                _ = tokio::time::sleep(poll) => {}
            }
            if !self.is_processing.load(Ordering::SeqCst) {
                break;
            }

            self.collect_changes().await;
            let Some(oldest) = self.oldest_pending() else {
                continue;
            };
            let quiet = self
                .queue
                .last_change_age()
                .is_none_or(|age| age >= debounce);
            if !quiet && oldest.elapsed() < max_delay {
                continue;
            }
            if retry_at.is_some_and(|t| Instant::now() < t) {
                continue;
            }

            match self.sync_now().await {
                Ok(n) => {
                    failures = 0;
                    retry_at = None;
                    if n > 0 {
                        tracing::info!("realtime: backup covered {} change(s)", n);
                    }
                }
                Err(e) => {
                    failures = failures.saturating_add(1);
                    let delay = self.backoff(failures);
                    retry_at = Some(Instant::now() + delay);
                    tracing::warn!(
                        "realtime: backup failed (attempt {}), changes kept pending, retrying in {:?}: {}",
                        failures,
                        delay,
                        e
                    );
                }
            }
        }
    }

    fn backoff(&self, failures: u32) -> Duration {
        let exp = failures.saturating_sub(1).min(16);
        self.retry_base
            .saturating_mul(1u32 << exp)
            .min(self.retry_max)
    }

    /// Stop the processing loop (a running backup is allowed to finish).
    pub async fn stop_processing(&self) {
        self.is_processing.store(false, Ordering::SeqCst);
        self.wake.notify_one();
    }

    /// Check if the processing loop is active
    pub async fn is_processing(&self) -> bool {
        self.is_processing.load(Ordering::SeqCst)
    }

    /// True while a backup is in progress.
    pub fn is_backup_running(&self) -> bool {
        self.backup_running.load(Ordering::SeqCst)
    }

    /// Runs one backup now if there are pending changes (waiting for any
    /// backup already in progress first). Returns how many pending changes
    /// the successful backup covered; on failure they stay pending.
    pub async fn sync_now(&self) -> Result<usize, String> {
        let _guard = self.backup_lock.lock().await;
        self.collect_changes().await;

        let (covered_seq, count) = {
            let pending = self.pending.lock().unwrap();
            (pending.next_seq, pending.entries.len())
        };
        if count == 0 {
            return Ok(0);
        }

        let result = match &self.runner {
            Some(runner) => {
                self.backup_running.store(true, Ordering::SeqCst);
                let result = runner().await;
                self.backup_running.store(false, Ordering::SeqCst);
                result
            }
            None => Err("no backup target configured".to_string()),
        };

        match result {
            Ok(summary) => {
                // Only confirm changes seen before this backup started;
                // anything newer stays pending ("dirty") for the next run.
                let confirmed = {
                    let mut pending = self.pending.lock().unwrap();
                    let before = pending.entries.len();
                    pending.entries.retain(|_, e| e.seq >= covered_seq);
                    before - pending.entries.len()
                };
                self.synced_count.fetch_add(confirmed, Ordering::SeqCst);
                self.backups_completed.fetch_add(1, Ordering::SeqCst);
                *self.last_error.lock().unwrap() = None;
                tracing::debug!("realtime: backup ok: {}", summary);
                Ok(confirmed)
            }
            Err(e) => {
                self.backups_failed.fetch_add(1, Ordering::SeqCst);
                *self.last_error.lock().unwrap() = Some(e.clone());
                Err(e)
            }
        }
    }

    /// Moves newly detected (non-ignored) changes from the queue into the
    /// pending set, deduplicated by path.
    async fn collect_changes(&self) -> Vec<FileChange> {
        let changes: Vec<FileChange> = self
            .queue
            .take_all()
            .await
            .into_iter()
            .filter(|change| !self.should_ignore(&change.path))
            .collect();
        if changes.is_empty() {
            return changes;
        }
        let now = Instant::now();
        let mut pending = self.pending.lock().unwrap();
        for change in &changes {
            let seq = pending.next_seq;
            pending.next_seq += 1;
            let first_seen = pending
                .entries
                .get(&change.path)
                .map_or(now, |e| e.first_seen);
            pending.entries.insert(
                change.path.clone(),
                PendingEntry {
                    seq,
                    first_seen,
                    change: change.clone(),
                },
            );
        }
        changes
    }

    fn oldest_pending(&self) -> Option<Instant> {
        self.pending
            .lock()
            .unwrap()
            .entries
            .values()
            .map(|e| e.first_seen)
            .min()
    }

    /// Changes not yet covered by a successful backup (pending set plus
    /// changes still in the raw queue).
    pub async fn peek_pending_changes(&self) -> Vec<FileChange> {
        let mut out: Vec<FileChange> = {
            let pending = self.pending.lock().unwrap();
            let mut entries: Vec<&PendingEntry> = pending.entries.values().collect();
            entries.sort_by_key(|e| e.seq);
            entries.into_iter().map(|e| e.change.clone()).collect()
        };
        out.extend(
            self.queue
                .get_changes()
                .await
                .into_iter()
                .filter(|c| !self.should_ignore(&c.path)),
        );
        out
    }

    pub async fn get_detailed_stats(&self) -> DetailedSyncStats {
        let stats = self.get_stats().await;
        let attempts = stats.backups_completed + stats.backups_failed;
        DetailedSyncStats {
            pending_changes: stats.pending_changes,
            synced: stats.synced,
            failed: stats.failed,
            sync_ratio: if attempts == 0 {
                0.0
            } else {
                stats.backups_completed as f64 / attempts as f64
            },
        }
    }

    /// Moves newly detected changes into the pending set and returns them
    /// (ignored paths filtered out). They remain pending until a backup
    /// succeeds.
    pub async fn process_batch(&self) -> Vec<FileChange> {
        self.collect_changes().await
    }

    fn should_ignore(&self, path: &Path) -> bool {
        if self
            .excluded_roots
            .iter()
            .any(|root| path.starts_with(root))
        {
            return true;
        }
        let path_str = path.to_string_lossy();
        self.ignore_regexes.iter().any(|re| re.is_match(&path_str))
    }

    pub async fn get_stats(&self) -> SyncStats {
        let pending = self.pending.lock().unwrap().entries.len();
        SyncStats {
            pending_changes: pending + self.queue.count().await,
            synced: self.synced_count.load(Ordering::SeqCst),
            failed: self.backups_failed.load(Ordering::SeqCst),
            backups_completed: self.backups_completed.load(Ordering::SeqCst),
            backups_failed: self.backups_failed.load(Ordering::SeqCst),
            backup_running: self.is_backup_running(),
            last_error: self.last_error.lock().unwrap().clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedSyncStats {
    pub pending_changes: usize,
    pub synced: usize,
    pub failed: usize,
    /// Successful backups / backup attempts.
    pub sync_ratio: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncStats {
    /// Changes not yet covered by a successful backup (deduplicated by path
    /// once collected).
    pub pending_changes: usize,
    /// Changes confirmed by a successful backup.
    pub synced: usize,
    /// Failed backup attempts (same as `backups_failed`).
    pub failed: usize,
    pub backups_completed: usize,
    pub backups_failed: usize,
    pub backup_running: bool,
    pub last_error: Option<String>,
}

pub async fn sync_all_files(src: &Path, dst: &Path) -> Result<usize, String> {
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();

    tokio::task::spawn_blocking(move || {
        use std::fs;
        use walkdir::WalkDir;

        if !src.exists() {
            return Err("Source folder does not exist".to_string());
        }

        let dst_str = dst
            .to_str()
            .ok_or("Invalid path characters in destination")?;
        ensure_directory_exists(dst_str)?;

        let mut count = 0;
        let mut walk_errors: Vec<String> = Vec::new();
        let mut skipped_symlinks = 0usize;

        // follow_links(false) is the default; we additionally use the
        // entry's own file type (lstat) so symlinks are never followed on
        // the source side.
        for entry in WalkDir::new(&src).follow_links(false) {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    tracing::warn!("sync_all_files: walk error: {}", e);
                    walk_errors.push(e.to_string());
                    continue;
                }
            };
            let file_type = entry.file_type();
            if file_type.is_symlink() {
                skipped_symlinks += 1;
                continue;
            }
            if !file_type.is_file() {
                continue;
            }

            let path = entry.path();
            let relative = path.strip_prefix(&src).map_err(|e| e.to_string())?;
            let dst_file = dst.join(relative);

            if let Some(parent) = dst_file.parent() {
                // Plain mkdir: no ~/$VAR expansion of names taken from the source tree.
                fs::create_dir_all(parent)
                    .map_err(|e| format!("Error creating {}: {}", parent.display(), e))?;
            }

            // Never write through a symlink planted at the destination.
            if let Ok(meta) = fs::symlink_metadata(&dst_file)
                && meta.file_type().is_symlink()
            {
                return Err(format!(
                    "Refusing to overwrite symlink at destination: {}",
                    dst_file.display()
                ));
            }

            fs::copy(path, &dst_file).map_err(|e| format!("Error copying: {}", e))?;
            count += 1;
        }

        if skipped_symlinks > 0 {
            tracing::info!("sync_all_files: skipped {} symlinks", skipped_symlinks);
        }
        if !walk_errors.is_empty() {
            return Err(format!(
                "Copied {} files, but {} entries could not be read (first: {})",
                count,
                walk_errors.len(),
                walk_errors[0]
            ));
        }

        Ok(count)
    })
    .await
    .map_err(|e| format!("Error in synchronization task: {}", e))?
}

/// Create automatic backup with encryption and chunking
///
/// # Arguments
/// * `src` - Path of the source folder to backup
/// * `backup_dst` - Path of the backup destination folder
/// * `password` - Optional password for encryption
///
/// # Returns
/// Name/description of the backup created or error
pub async fn create_backup(
    src: &Path,
    backup_dst: &Path,
    password: Option<&str>,
) -> Result<String, String> {
    let src = src.to_path_buf();
    let backup_dst = backup_dst.to_path_buf();
    let password = password.map(|p| p.to_string());

    // Check if source exists
    if !src.exists() {
        return Err("Source does not exist".to_string());
    }

    // Create directory in blocking thread to avoid UI freeze
    let backup_dst_clone = backup_dst.clone();
    tokio::task::spawn_blocking(move || {
        let dst_str = backup_dst_clone
            .to_str()
            .ok_or("Invalid path characters in destination".to_string())?;
        ensure_directory_exists(dst_str).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Error in backup task: {}", e))?
    .map_err(|e| e.to_string())?;

    let timestamp = chrono::Local::now().format("%Y-%m-%d_%H%M%S").to_string();

    // Create Config to use perform_backup from rsb-core
    // Chunking is done automatically (files > MULTIPART_THRESHOLD, CHUNK_SIZE chunks)
    let config = Config {
        source_path: src.to_string_lossy().to_string(),
        destination_path: backup_dst.to_string_lossy().to_string(),
        encryption_key: password.clone(),
        encrypt_patterns: Some(vec!["*".to_string()]), // Encrypt all files
        compression_level: Some(3),
        ..Default::default()
    };

    // Use perform_backup to create backup with encryption and chunking
    match perform_backup(
        &config,
        BackupOptions {
            mode: Some("incremental".to_string()),
            ..Default::default()
        }
        .with_encryption_key(password.as_deref()),
    )
    .await
    {
        Ok(report) => Ok(format!(
            "✅ Backup: {} with encryption\n📊 Files: {}/{}\n🔐 {}MB chunks applied",
            timestamp,
            report.files_processed,
            report.total_files,
            crate::core::types::CHUNK_SIZE / (1024 * 1024)
        )),
        Err(e) => Err(format!("Error creating backup: {}", e)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn mirror_and_backup_sync_mirrors_then_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src");
        let repo = dir.path().join("repo");
        let mirror = dir.path().join("mirror");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("a.txt"), "hello").unwrap();
        let cfg = Config::new(src.to_string_lossy(), repo.to_string_lossy());

        let events = Arc::new(StdMutex::new(Vec::new()));
        let sink = events.clone();
        let rt = mirror_and_backup_sync(
            &cfg,
            mirror.clone(),
            SyncStrategy::default(),
            Some(Arc::new(move |e| sink.lock().unwrap().push(e))),
        );
        rt.queue_full_backup(&src).await;
        assert_eq!(rt.sync_now().await.unwrap(), 1);

        assert_eq!(
            std::fs::read_to_string(mirror.join("a.txt")).unwrap(),
            "hello"
        );
        assert!(std::fs::read_dir(repo.join("snapshots")).unwrap().count() >= 1);
        let events = events.lock().unwrap();
        assert_eq!(events[0], WatchEvent::Mirrored { files: 1 });
        assert!(matches!(events[1], WatchEvent::BackedUp { number: 1, .. }));
    }

    #[tokio::test]
    async fn process_batch_drains_queue() {
        let sync = RealtimeSync::new(SyncStrategy::default());
        sync.queue
            .add_change(FileChange {
                path: PathBuf::from("/a.txt"),
                change_type: ChangeType::Deleted,
                timestamp: Utc::now(),
                size: 0,
                hash: None,
            })
            .await;
        assert_eq!(sync.process_batch().await.len(), 1);
        assert_eq!(sync.queue.count().await, 0);
        assert!(sync.process_batch().await.is_empty());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sync_all_files_skips_symlinks_and_refuses_dst_symlinks() {
        let src = tempfile::tempdir().unwrap();
        let dst = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(outside.path().join("secret"), b"s").unwrap();
        std::fs::write(src.path().join("f.txt"), b"f").unwrap();
        std::os::unix::fs::symlink(outside.path().join("secret"), src.path().join("link")).unwrap();
        assert_eq!(sync_all_files(src.path(), dst.path()).await.unwrap(), 1);
        assert!(!dst.path().join("link").exists());

        let victim = outside.path().join("victim");
        std::fs::write(&victim, b"untouched").unwrap();
        std::fs::remove_file(dst.path().join("f.txt")).unwrap();
        std::os::unix::fs::symlink(&victim, dst.path().join("f.txt")).unwrap();
        assert!(sync_all_files(src.path(), dst.path()).await.is_err());
        assert_eq!(std::fs::read(&victim).unwrap(), b"untouched");
    }
}
