use super::manifest::{
    parse_snapshot_timestamp, read_manifest_cached, sort_snapshots_newest_first,
};
use crate::config::Config;
use crate::core::types::FileMetadata;
use crate::crypto::KeyCache;
use crate::error::RsbError;
use crate::storage::{LockKind, RepoLock, Storage};
use std::collections::{HashMap, HashSet};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};

pub async fn perform_prune(config: &Config, keep_last: usize) -> Result<(), RsbError> {
    // Fail before touching storage: keep_last == 0 would delete everything.
    check_keep_last(keep_last)?;
    let storage = super::storage_backend::get_storage(config).await?;
    perform_prune_with_storage(storage, config.encryption_key.as_deref(), keep_last).await
}

/// [`perform_prune`] on an existing storage backend.
pub async fn perform_prune_with_storage(
    storage: Arc<dyn Storage>,
    encryption_key: Option<&str>,
    keep_last: usize,
) -> Result<(), RsbError> {
    let deleted = plan_prune_by_count_with_storage(&*storage, keep_last).await?;
    if deleted.is_empty() {
        info!("Nothing to prune.");
        return Ok(());
    }
    prune_with_storage(storage, encryption_key, &deleted).await
}

/// Planeia sem deletar (para --dry-run / preview na UI).
pub async fn plan_prune_by_count(
    config: &Config,
    keep_last: usize,
) -> Result<Vec<String>, RsbError> {
    // Fail before touching storage: keep_last == 0 would delete everything.
    check_keep_last(keep_last)?;
    let storage = super::storage_backend::get_storage(config).await?;
    plan_prune_by_count_with_storage(&*storage, keep_last).await
}

/// [`plan_prune_by_count`] on an existing storage backend.
pub async fn plan_prune_by_count_with_storage(
    storage: &dyn Storage,
    keep_last: usize,
) -> Result<Vec<String>, RsbError> {
    check_keep_last(keep_last)?;
    let snapshots = list_snapshots(storage).await?;
    select_by_count(snapshots, keep_last)
}

fn check_keep_last(keep_last: usize) -> Result<(), RsbError> {
    if keep_last == 0 {
        return Err(RsbError::InvalidInput(
            "Refusing to prune with keep_last = 0: that would delete every snapshot. \
             Use keep_last >= 1."
                .to_string(),
        ));
    }
    Ok(())
}

/// Returns the snapshots to delete so that the newest `keep_last` remain.
pub fn select_by_count(
    mut snapshots: Vec<String>,
    keep_last: usize,
) -> Result<Vec<String>, RsbError> {
    check_keep_last(keep_last)?;
    if snapshots.len() <= keep_last {
        return Ok(vec![]);
    }
    sort_snapshots_newest_first(&mut snapshots);
    Ok(snapshots.split_off(keep_last))
}

/// Returns the snapshots older than `cutoff`, never including the newest one.
/// Snapshots whose name cannot be parsed are kept (fail-closed).
pub fn select_by_age(
    mut snapshots: Vec<String>,
    cutoff: chrono::DateTime<chrono::Utc>,
) -> Vec<String> {
    if snapshots.is_empty() {
        return vec![];
    }
    sort_snapshots_newest_first(&mut snapshots);
    snapshots
        .into_iter()
        .skip(1) // nunca apaga o mais recente (disponibilidade)
        .filter(|snap| matches!(parse_snapshot_timestamp(snap), Some(ts) if ts < cutoff))
        .collect()
}

/// Retenção por idade: mantém sempre o snapshot mais recente + os dos
/// últimos `retention_days` dias. Corrige o bug da UI que passava "dias"
/// como `keep_last` (contagem), o que deletava demais ou de menos.
pub async fn perform_prune_by_age(config: &Config, retention_days: u32) -> Result<(), RsbError> {
    let storage = super::storage_backend::get_storage(config).await?;
    perform_prune_by_age_with_storage(storage, config.encryption_key.as_deref(), retention_days)
        .await
}

/// [`perform_prune_by_age`] on an existing storage backend.
pub async fn perform_prune_by_age_with_storage(
    storage: Arc<dyn Storage>,
    encryption_key: Option<&str>,
    retention_days: u32,
) -> Result<(), RsbError> {
    let to_delete = plan_prune_by_age_with_storage(&*storage, retention_days).await?;
    if to_delete.is_empty() {
        info!("Nothing to prune by age.");
        return Ok(());
    }
    prune_with_storage(storage, encryption_key, &to_delete).await
}

/// Snapshots that [`perform_prune_by_age`] would delete (nothing is
/// deleted): those older than `retention_days`, never the newest one.
pub async fn plan_prune_by_age(
    config: &Config,
    retention_days: u32,
) -> Result<Vec<String>, RsbError> {
    let storage = super::storage_backend::get_storage(config).await?;
    plan_prune_by_age_with_storage(&*storage, retention_days).await
}

/// [`plan_prune_by_age`] on an existing storage backend.
pub async fn plan_prune_by_age_with_storage(
    storage: &dyn Storage,
    retention_days: u32,
) -> Result<Vec<String>, RsbError> {
    let snapshots = list_snapshots(storage).await?;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(retention_days as i64);
    // Snapshot names are `%Y-%m-%dT%H%M%S[.mmm]Z` (not RFC3339); the shared
    // parser handles every format ever written. Unparseable names are kept.
    Ok(select_by_age(snapshots, cutoff))
}

async fn list_snapshots(storage: &dyn Storage) -> Result<Vec<String>, RsbError> {
    let mut snapshots = storage.list("snapshots").await?;
    snapshots.retain(|s| s.ends_with(".toml"));
    Ok(snapshots)
}

/// What [`prune_with_stats`] deleted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PruneStats {
    /// Snapshot manifests deleted.
    pub snapshots_deleted: usize,
    /// Data objects (`data/clear`, `data/enc`) no remaining snapshot
    /// referenced, deleted by the garbage collection.
    pub objects_deleted: usize,
    /// `false` when no snapshot remained, so the garbage collection was
    /// skipped and the data objects were left in place.
    pub gc_ran: bool,
}

/// Deletes `delete_snapshots` and garbage-collects unreferenced data,
/// holding the exclusive repository lock so no backup can be writing data
/// objects whose manifest does not exist yet.
pub async fn prune_with_storage(
    storage: Arc<dyn Storage>,
    encryption_key: Option<&str>,
    delete_snapshots: &[String],
) -> Result<(), RsbError> {
    prune_with_stats(storage, encryption_key, delete_snapshots)
        .await
        .map(|_| ())
}

/// [`prune_with_storage`], returning what was deleted.
pub async fn prune_with_stats(
    storage: Arc<dyn Storage>,
    encryption_key: Option<&str>,
    delete_snapshots: &[String],
) -> Result<PruneStats, RsbError> {
    // Unlock the repository key before deleting anything: a wrong password
    // must fail before any snapshot is removed.
    let cache = match encryption_key {
        Some(k) => Some(KeyCache::open(&*storage, k.as_bytes()).await?),
        None => None,
    };
    let lock = RepoLock::acquire(storage.clone(), LockKind::Exclusive, "prune").await?;
    let result = prune_locked(&*storage, cache.as_ref(), delete_snapshots).await;
    if let Err(e) = lock.release().await {
        warn!("Failed to release repository lock after prune: {}", e);
    }
    result
}

/// What a prune of given snapshots would delete.
struct PrunePlan {
    /// Manifests to delete (valid snapshot keys only).
    doomed: Vec<String>,
    /// Data objects no kept snapshot references; `None` when no snapshot
    /// would remain (garbage collection skipped).
    unreferenced: Option<Vec<String>>,
}

/// Reads every manifest that stays (fail-closed: an unreadable one is an
/// error) and lists the data objects none of them references. Deletes
/// nothing.
async fn plan_prune(
    storage: &dyn Storage,
    cache: Option<&KeyCache>,
    delete_snapshots: &[String],
) -> Result<PrunePlan, RsbError> {
    let doomed: Vec<String> = delete_snapshots
        .iter()
        .filter(|snap| {
            let ok = snap.starts_with("snapshots/") && snap.ends_with(".toml");
            if !ok {
                warn!("Skipping unexpected snapshot path during prune: {}", snap);
            }
            ok
        })
        .cloned()
        .collect();

    let keep_snapshots: Vec<String> = list_snapshots(storage)
        .await?
        .into_iter()
        .filter(|s| !doomed.contains(s))
        .collect();
    if keep_snapshots.is_empty() {
        return Ok(PrunePlan {
            doomed,
            unreferenced: None,
        });
    }

    let mut keep_hashes = HashSet::new();
    for snap in &keep_snapshots {
        let content = read_manifest_cached(storage, snap, cache)
            .await
            .map_err(RsbError::from)?;
        let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&content)?;

        for metadata in manifest.values() {
            keep_hashes.insert(metadata.hash.clone());
            if let Some(chunks) = &metadata.chunks {
                for chunk in chunks {
                    keep_hashes.insert(chunk.hash.clone());
                }
            }
        }
    }

    let mut all_data = Vec::new();
    for prefix in ["data/clear", "data/enc"] {
        match storage.list(prefix).await {
            Ok(list) => all_data.extend(list),
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    let mut unreferenced = Vec::new();
    for data_file in all_data {
        // Sem unwrap: filename estranho/unicode não deve abortar o prune
        // no meio (deleção parcial = perda de disponibilidade).
        let fname = match PathBuf::from(&data_file).file_name() {
            Some(n) => n.to_string_lossy().to_string(),
            None => {
                warn!("Skipping odd data path during prune: {}", data_file);
                continue;
            }
        };
        if !keep_hashes.contains(&fname) {
            unreferenced.push(data_file);
        }
    }
    Ok(PrunePlan {
        doomed,
        unreferenced: Some(unreferenced),
    })
}

/// What [`prune_with_stats`] would delete for `delete_snapshots`, without
/// deleting anything (dry run). Takes no lock: a backup running meanwhile
/// can make the object count differ from the real prune.
pub async fn preview_prune(
    storage: &dyn Storage,
    encryption_key: Option<&str>,
    delete_snapshots: &[String],
) -> Result<PruneStats, RsbError> {
    let cache = match encryption_key {
        Some(k) => Some(KeyCache::open(storage, k.as_bytes()).await?),
        None => None,
    };
    let plan = plan_prune(storage, cache.as_ref(), delete_snapshots).await?;
    let mut snapshots = 0;
    for snap in &plan.doomed {
        if storage.exists(snap).await? {
            snapshots += 1;
        }
    }
    Ok(PruneStats {
        snapshots_deleted: snapshots,
        objects_deleted: plan.unreferenced.as_ref().map_or(0, Vec::len),
        gc_ran: plan.unreferenced.is_some(),
    })
}

async fn prune_locked(
    storage: &dyn Storage,
    cache: Option<&KeyCache>,
    delete_snapshots: &[String],
) -> Result<PruneStats, RsbError> {
    let mut stats = PruneStats::default();

    // 1) Read every manifest that stays BEFORE deleting anything: an
    //    unreadable manifest (missing or wrong key, corruption) aborts the
    //    prune with the repository untouched (fail-closed: we cannot know
    //    what it references).
    let plan = plan_prune(storage, cache, delete_snapshots).await?;

    // Local dedup indexes must not trust what this prune may delete.
    crate::core::dedup_index::bump_repo_epoch(storage).await?;

    // 2) Delete the snapshot manifests first. If we crash afterwards the
    //    worst case is unreferenced data (reclaimed by the next prune), never
    //    a manifest pointing at deleted data.
    for snap in &plan.doomed {
        info!("Pruning snapshot: {}", snap);
        match storage.delete(snap).await {
            Ok(()) => stats.snapshots_deleted += 1,
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    // 3) Garbage-collect data not referenced by any remaining snapshot.
    let Some(unreferenced) = plan.unreferenced else {
        warn!("No snapshots left after prune; skipping data garbage collection.");
        info!(
            "Prune completed. Deleted {} snapshot(s).",
            stats.snapshots_deleted
        );
        return Ok(stats);
    };
    stats.gc_ran = true;

    for data_file in unreferenced {
        info!("Pruning data: {}", data_file);
        match storage.delete(&data_file).await {
            Ok(()) => stats.objects_deleted += 1,
            Err(e) if e.kind() == ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }

    info!(
        "Prune completed. Deleted {} snapshot(s) and {} data object(s).",
        stats.snapshots_deleted, stats.objects_deleted
    );

    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::FileMetadata;
    use crate::storage::LocalStorage;

    fn snaps(names: &[&str]) -> Vec<String> {
        names
            .iter()
            .map(|n| format!("snapshots/{}.toml", n))
            .collect()
    }

    #[test]
    fn keep_last_zero_is_rejected() {
        let s = snaps(&["2026-01-01T000000Z", "2026-01-02T000000Z"]);
        assert!(select_by_count(s, 0).is_err());
    }

    #[test]
    fn keep_last_keeps_newest_across_name_formats() {
        let s = snaps(&[
            "2026-01-03T000000.500Z",
            "2026-01-01T000000Z",
            "2026-01-02T000000Z",
        ]);
        let del = select_by_count(s, 1).unwrap();
        assert_eq!(del.len(), 2);
        assert!(!del.contains(&"snapshots/2026-01-03T000000.500Z.toml".to_string()));
    }

    #[test]
    fn prune_by_age_matches_real_snapshot_names() {
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(40))
            .format(crate::core::manifest::SNAPSHOT_NAME_FORMAT_SECS)
            .to_string();
        let old_ms = (now - chrono::Duration::days(35))
            .format(crate::core::manifest::SNAPSHOT_NAME_FORMAT)
            .to_string();
        let recent = (now - chrono::Duration::days(1))
            .format(crate::core::manifest::SNAPSHOT_NAME_FORMAT_SECS)
            .to_string();
        let s = snaps(&[&old, &old_ms, &recent, "not-a-date"]);
        let cutoff = now - chrono::Duration::days(30);
        let mut del = select_by_age(s, cutoff);
        del.sort();
        let mut expected = snaps(&[&old, &old_ms]);
        expected.sort();
        assert_eq!(del, expected);
    }

    #[test]
    fn prune_by_age_never_deletes_latest() {
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(400))
            .format(crate::core::manifest::SNAPSHOT_NAME_FORMAT_SECS)
            .to_string();
        assert!(select_by_age(snaps(&[&old]), now).is_empty());
    }

    #[tokio::test]
    async fn prune_deletes_manifests_then_unreferenced_data_only() {
        let dir = tempfile::tempdir().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(dir.path().to_str().unwrap()));
        let keep_hash = "a".repeat(64);
        let drop_hash = "b".repeat(64);
        let mk = |h: &str| {
            let mut m = HashMap::new();
            m.insert(
                PathBuf::from("f"),
                FileMetadata::new(h.to_string(), false, false),
            );
            toml::to_string(&m).unwrap()
        };
        storage
            .write(
                "snapshots/2026-01-01T000000Z.toml",
                mk(&drop_hash).as_bytes(),
            )
            .await
            .unwrap();
        storage
            .write(
                "snapshots/2026-01-02T000000Z.toml",
                mk(&keep_hash).as_bytes(),
            )
            .await
            .unwrap();
        storage
            .write(&format!("data/clear/{}", keep_hash), b"k")
            .await
            .unwrap();
        storage
            .write(&format!("data/clear/{}", drop_hash), b"d")
            .await
            .unwrap();

        prune_with_storage(
            storage.clone(),
            None,
            &["snapshots/2026-01-01T000000Z.toml".to_string()],
        )
        .await
        .unwrap();

        assert!(
            !storage
                .exists("snapshots/2026-01-01T000000Z.toml")
                .await
                .unwrap()
        );
        assert!(
            storage
                .exists(&format!("data/clear/{}", keep_hash))
                .await
                .unwrap()
        );
        assert!(
            !storage
                .exists(&format!("data/clear/{}", drop_hash))
                .await
                .unwrap()
        );
    }

    #[tokio::test]
    async fn plan_by_age_lists_old_snapshots_but_keeps_the_newest() {
        let dir = tempfile::tempdir().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(dir.path().to_str().unwrap()));
        for s in ["2020-01-01T000000Z", "2020-01-02T000000Z"] {
            storage
                .write(&format!("snapshots/{}.toml", s), b"")
                .await
                .unwrap();
        }
        let plan = plan_prune_by_age_with_storage(&*storage, 30).await.unwrap();
        assert_eq!(plan, vec!["snapshots/2020-01-01T000000Z.toml".to_string()]);
    }

    #[tokio::test]
    async fn prune_refuses_while_backup_holds_lock() {
        let dir = tempfile::tempdir().unwrap();
        let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(dir.path().to_str().unwrap()));
        let backup = RepoLock::acquire(storage.clone(), LockKind::Shared, "backup")
            .await
            .unwrap();
        assert!(
            prune_with_storage(storage.clone(), None, &[])
                .await
                .is_err()
        );
        backup.release().await.unwrap();
        let prune = RepoLock::acquire(storage.clone(), LockKind::Exclusive, "prune").await;
        assert!(
            prune.is_ok(),
            "lock must be free once the backup released it"
        );
    }
}
