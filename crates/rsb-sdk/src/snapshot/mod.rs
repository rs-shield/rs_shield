//! Snapshot management shared by the CLI (`rsb snapshots ...`) and the
//! desktop app's Snapshots screen, so both list, show, compare and delete
//! snapshots the same way on every backend (local directory, S3-compatible
//! bucket, [`crate::storage::MemoryStorage`]).
//!
//! A snapshot is the manifest `snapshots/<id>.toml` (see `docs/FORMAT.md`).
//! Its id is the UTC time it was written, `%Y-%m-%dT%H%M%S.mmmZ`
//! (e.g. `2026-05-25T103000.000Z`), older ones without milliseconds, with a
//! `-<n>` suffix when two backups finished in the same millisecond.
//!
//! Listing ids and timestamps needs no key (they are object names). The
//! contents of a manifest (file count, sizes) are encrypted when the backup
//! is: without the key such a snapshot is reported as
//! [`SnapshotContents::Locked`], never as an error or an empty list.
//!
//! Every function takes the storage backend; front-ends build it with
//! [`crate::core::storage_backend::get_storage`] from a profile (local or S3,
//! same credential order as backup/restore) or from a repository directory.

pub mod manifest;

use crate::core::limits::{MAX_MANIFEST_SIZE, too_large, zstd_decode_bounded};
use crate::core::manifest::{
    manifest_decrypt_error, parse_snapshot_timestamp, sort_snapshots_newest_first,
    validate_snapshot_id,
};
use crate::core::prune::prune_with_stats;
use crate::core::types::FileMetadata;
use crate::crypto::{BLOB_MAGIC_V2, KeyCache, is_v3};
use crate::error::{CryptoErrorKind, RsbError};
use crate::storage::Storage;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap};
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Arc;

const SNAPSHOT_DIR: &str = "snapshots";
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xB5, 0x2F, 0xFD];

/// One snapshot of a repository, as listed by [`list_snapshots`].
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotInfo {
    /// Snapshot id (`2026-05-25T103000.000Z`, possibly with a `-<n>` suffix).
    pub id: String,
    /// When the snapshot was written (parsed from the id); `None` for a
    /// name in no known format.
    pub timestamp: Option<DateTime<Utc>>,
    /// The manifest is stored encrypted.
    pub encrypted: bool,
    /// What could be read from the manifest.
    pub contents: SnapshotContents,
}

/// Contents of a manifest as far as they could be read.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SnapshotContents {
    /// The manifest was read.
    Readable(SnapshotSummary),
    /// The manifest is encrypted and no key was given.
    Locked,
    /// The manifest could not be read (corrupt, wrong key, I/O error); the
    /// message says why.
    Unreadable { error: String },
}

/// Totals of one snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotSummary {
    /// Number of files.
    pub file_count: usize,
    /// Of which stored encrypted.
    pub encrypted_files: usize,
    /// Sum of the stored (compressed and/or encrypted) object sizes. Shared
    /// (deduplicated) objects count once per file referencing them.
    pub stored_size: u64,
    /// Sum of the original file sizes, when every entry records it (format
    /// v3 manifests); `None` for older manifests.
    pub original_size: Option<u64>,
}

impl SnapshotSummary {
    /// Size to show: the original size when known, else the stored size.
    pub fn display_size(&self) -> u64 {
        self.original_size.unwrap_or(self.stored_size)
    }
}

impl SnapshotInfo {
    /// Date of the snapshot for display (`2026-05-25 10:30:00 UTC`), or
    /// `-` when its name has no known date format.
    pub fn date_label(&self) -> String {
        format_timestamp(self.timestamp)
    }
}

/// `2026-05-25 10:30:00 UTC`, or `-` for `None`.
pub fn format_timestamp(ts: Option<DateTime<Utc>>) -> String {
    ts.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string())
        .unwrap_or_else(|| "-".to_string())
}

/// Human-readable size with binary units (`1.50 MB` = 1.5 MiB).
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{} B", bytes);
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{:.2} {}", value, UNITS[unit])
}

/// A snapshot with its file list ([`snapshot_details`]).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SnapshotDetails {
    pub id: String,
    pub timestamp: Option<DateTime<Utc>>,
    pub encrypted: bool,
    pub summary: SnapshotSummary,
    /// Files sorted by path.
    pub files: Vec<SnapshotFile>,
}

/// One file of a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotFile {
    /// Path relative to the backup source, `/`-separated.
    pub path: String,
    /// Original size, when recorded (format v3).
    pub size: Option<u64>,
    /// Stored size, when recorded.
    pub stored_size: Option<u64>,
    pub encrypted: bool,
    /// Number of chunks (0 for a file stored as one object).
    pub chunks: usize,
}

impl SnapshotFile {
    /// Size to show: the original size, else the stored size, else 0.
    pub fn display_size(&self) -> u64 {
        self.size.or(self.stored_size).unwrap_or(0)
    }
}

/// Differences between two snapshots ([`diff_snapshots`]), paths sorted.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SnapshotDiff {
    pub from: String,
    pub to: String,
    pub added: Vec<SnapshotFile>,
    pub removed: Vec<SnapshotFile>,
    /// Files whose content changed: (in `from`, in `to`).
    pub modified: Vec<(SnapshotFile, SnapshotFile)>,
}

impl SnapshotDiff {
    /// No file was added, removed or changed.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.modified.is_empty()
    }
}

/// What [`delete_snapshot`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SnapshotDeletion {
    pub id: String,
    /// Data objects only the deleted snapshot referenced, now deleted.
    pub objects_deleted: usize,
    /// `false` when the deleted snapshot was the last one: its data objects
    /// were then kept (like `rsb prune`, which never collects the data of
    /// a repository left without snapshots).
    pub gc_ran: bool,
}

/// Opens the decryption context for `key` (`None` without a key). A wrong
/// password fails here for a repository with `keys/repo.key`.
pub async fn open_key_cache(
    storage: &dyn Storage,
    key: Option<&str>,
) -> Result<Option<KeyCache>, RsbError> {
    match key.filter(|k| !k.is_empty()) {
        Some(k) => Ok(Some(KeyCache::open(storage, k.as_bytes()).await?)),
        None => Ok(None),
    }
}

/// Storage key of the manifest of snapshot `id` (validated).
pub fn snapshot_key(id: &str) -> Result<String, RsbError> {
    let id = validate_snapshot_id(id).map_err(|e| RsbError::InvalidInput(e.to_string()))?;
    Ok(format!("{}/{}.toml", SNAPSHOT_DIR, id))
}

/// Id of a manifest storage key (`snapshots/<id>.toml` → `<id>`).
fn id_of(key: &str) -> &str {
    let name = key.rsplit('/').next().unwrap_or(key);
    name.strip_suffix(".toml").unwrap_or(name)
}

/// Snapshot ids of the repository, newest first.
///
/// Only `snapshots/<id>.toml` objects whose name is a valid id count
/// (leftovers of interrupted writes and foreign files are ignored). A
/// local repository without a `snapshots/` directory is an error (not a
/// repository, or an incomplete copy); an empty one gives an empty list.
pub async fn list_snapshot_ids(storage: &dyn Storage) -> Result<Vec<String>, RsbError> {
    let listed = storage.list(SNAPSHOT_DIR).await.map_err(|e| {
        if e.kind() == ErrorKind::NotFound {
            RsbError::NotFound(format!(
                "Not a backup repository: no '{}/' found ({})",
                SNAPSHOT_DIR, e
            ))
        } else {
            RsbError::from(e)
        }
    })?;
    let prefix = format!("{}/", SNAPSHOT_DIR);
    let mut keys: Vec<String> = listed
        .into_iter()
        .filter(|k| {
            k.strip_prefix(&prefix)
                .and_then(|name| name.strip_suffix(".toml"))
                .is_some_and(|id| !id.contains('/') && validate_snapshot_id(id).is_ok())
        })
        .collect();
    sort_snapshots_newest_first(&mut keys);
    Ok(keys.into_iter().map(|k| id_of(&k).to_string()).collect())
}

/// All snapshots of the repository, newest first, with their totals.
///
/// A manifest that cannot be read is reported in its entry
/// ([`SnapshotContents::Unreadable`], or [`SnapshotContents::Locked`] when
/// it is encrypted and `cache` is `None`); only a failure to list the
/// repository is an error.
pub async fn list_snapshots(
    storage: &dyn Storage,
    cache: Option<&KeyCache>,
) -> Result<Vec<SnapshotInfo>, RsbError> {
    let ids = list_snapshot_ids(storage).await?;
    let mut out = Vec::with_capacity(ids.len());
    for id in ids {
        let key = format!("{}/{}.toml", SNAPSHOT_DIR, id);
        let (encrypted, contents) = match load_manifest(storage, &key, cache).await {
            Ok(Loaded::Plain {
                encrypted,
                manifest,
            }) => (encrypted, SnapshotContents::Readable(summarize(&manifest))),
            Ok(Loaded::Locked) => (true, SnapshotContents::Locked),
            Err((encrypted, e)) => (
                encrypted,
                SnapshotContents::Unreadable {
                    error: e.to_string(),
                },
            ),
        };
        out.push(SnapshotInfo {
            timestamp: parse_snapshot_timestamp(&id),
            id,
            encrypted,
            contents,
        });
    }
    Ok(out)
}

/// Snapshot `id` with its file list. Fails when it does not exist, cannot
/// be read, or is encrypted and `cache` is `None`.
pub async fn snapshot_details(
    storage: &dyn Storage,
    id: &str,
    cache: Option<&KeyCache>,
) -> Result<SnapshotDetails, RsbError> {
    let key = snapshot_key(id)?;
    let id = id_of(&key).to_string();
    let (encrypted, manifest) = read_existing(storage, &key, &id, cache).await?;
    let mut files: Vec<SnapshotFile> = manifest
        .iter()
        .map(|(path, meta)| file_entry(path, meta))
        .collect();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(SnapshotDetails {
        timestamp: parse_snapshot_timestamp(&id),
        summary: summarize(&manifest),
        id,
        encrypted,
        files,
    })
}

/// Files added, removed and changed (different content hash) from snapshot
/// `from` to snapshot `to`.
pub async fn diff_snapshots(
    storage: &dyn Storage,
    from: &str,
    to: &str,
    cache: Option<&KeyCache>,
) -> Result<SnapshotDiff, RsbError> {
    let from_key = snapshot_key(from)?;
    let to_key = snapshot_key(to)?;
    let (from_id, to_id) = (id_of(&from_key).to_string(), id_of(&to_key).to_string());
    let (_, a) = read_existing(storage, &from_key, &from_id, cache).await?;
    let (_, b) = read_existing(storage, &to_key, &to_id, cache).await?;
    let a: BTreeMap<_, _> = a.iter().collect();
    let b: BTreeMap<_, _> = b.iter().collect();

    let mut diff = SnapshotDiff {
        from: from_id,
        to: to_id,
        ..Default::default()
    };
    for (path, new) in &b {
        match a.get(path) {
            None => diff.added.push(file_entry(path, new)),
            Some(old) if old.hash != new.hash => diff
                .modified
                .push((file_entry(path, old), file_entry(path, new))),
            Some(_) => {}
        }
    }
    for (path, old) in &a {
        if !b.contains_key(path) {
            diff.removed.push(file_entry(path, old));
        }
    }
    Ok(diff)
}

/// Deletes snapshot `id` and the data objects no other snapshot references.
///
/// Runs the prune machinery on that one snapshot: under the exclusive
/// repository lock (refused while a backup, prune or verify holds it), the
/// remaining manifests are read first (so the key is needed for an
/// encrypted repository, and an unreadable manifest aborts before anything
/// is deleted), the dedup-index epoch is bumped, then the manifest and the
/// unreferenced data objects are deleted.
pub async fn delete_snapshot(
    storage: Arc<dyn Storage>,
    id: &str,
    encryption_key: Option<&str>,
) -> Result<SnapshotDeletion, RsbError> {
    let key = snapshot_key(id)?;
    let id = id_of(&key).to_string();
    if !storage.exists(&key).await? {
        return Err(not_found(&id));
    }
    let encryption_key = encryption_key.filter(|k| !k.is_empty());
    if encryption_key.is_none() && crate::crypto::keyfile::repo_key_exists(&*storage).await? {
        return Err(key_required(&id));
    }
    let stats = prune_with_stats(storage, encryption_key, std::slice::from_ref(&key)).await?;
    if stats.snapshots_deleted == 0 {
        // Deleted by someone else between the check and the lock.
        return Err(not_found(&id));
    }
    Ok(SnapshotDeletion {
        id,
        objects_deleted: stats.objects_deleted,
        gc_ran: stats.gc_ran,
    })
}

fn not_found(id: &str) -> RsbError {
    RsbError::NotFound(format!("Snapshot '{}' not found", id))
}

fn key_required(id: &str) -> RsbError {
    RsbError::crypto(
        CryptoErrorKind::Other,
        format!(
            "Snapshot '{}' is in an encrypted repository: the encryption key is needed \
             (set {} or enter it)",
            id,
            crate::config::ENCRYPTION_KEY_ENV
        ),
    )
}

/// Reads an existing manifest; a missing one, a locked one or an
/// unreadable one is an error.
async fn read_existing(
    storage: &dyn Storage,
    key: &str,
    id: &str,
    cache: Option<&KeyCache>,
) -> Result<(bool, HashMap<PathBuf, FileMetadata>), RsbError> {
    if !storage.exists(key).await? {
        return Err(not_found(id));
    }
    match load_manifest(storage, key, cache).await {
        Ok(Loaded::Plain {
            encrypted,
            manifest,
        }) => Ok((encrypted, manifest)),
        Ok(Loaded::Locked) => Err(key_required(id)),
        Err((_, e)) => Err(e),
    }
}

fn file_entry(path: &std::path::Path, meta: &FileMetadata) -> SnapshotFile {
    SnapshotFile {
        path: path.to_string_lossy().into_owned(),
        size: meta.size,
        stored_size: meta.stored_size,
        encrypted: meta.encrypted,
        chunks: meta.chunks.as_ref().map_or(0, Vec::len),
    }
}

fn summarize(manifest: &HashMap<PathBuf, FileMetadata>) -> SnapshotSummary {
    let mut s = SnapshotSummary {
        file_count: manifest.len(),
        original_size: Some(0),
        ..Default::default()
    };
    for meta in manifest.values() {
        if meta.encrypted {
            s.encrypted_files += 1;
        }
        s.stored_size = s.stored_size.saturating_add(meta.stored_size.unwrap_or(0));
        s.original_size = match (s.original_size, meta.size) {
            (Some(total), Some(n)) => Some(total.saturating_add(n)),
            _ => None,
        };
    }
    s
}

enum Loaded {
    Plain {
        encrypted: bool,
        manifest: HashMap<PathBuf, FileMetadata>,
    },
    Locked,
}

/// Reads and decodes one manifest. Unlike
/// [`crate::core::manifest::read_manifest_cached`], a plaintext manifest is
/// read even when a key is given (a repository can hold both), and an
/// encrypted one without a key is [`Loaded::Locked`] instead of "corrupt".
/// The error carries whether the manifest is encrypted.
async fn load_manifest(
    storage: &dyn Storage,
    key: &str,
    cache: Option<&KeyCache>,
) -> Result<Loaded, (bool, RsbError)> {
    if let Some(n) = storage.size(key).await.map_err(|e| (false, e.into()))?
        && n > MAX_MANIFEST_SIZE
    {
        return Err((false, too_large(key, MAX_MANIFEST_SIZE).into()));
    }
    let raw = storage.read(key).await.map_err(|e| (false, e.into()))?;

    let plaintext = if is_v3(&raw) || raw.starts_with(BLOB_MAGIC_V2) {
        None
    } else if raw.starts_with(&ZSTD_MAGIC) {
        let bytes = zstd_decode_bounded(&raw, MAX_MANIFEST_SIZE, key)
            .map_err(|e| (false, corrupt(key, &e.to_string())))?;
        Some(bytes)
    } else if std::str::from_utf8(&raw).is_ok() {
        Some(raw.clone())
    } else {
        // No magic: a legacy (v1) encrypted blob, or garbage.
        None
    };

    let (encrypted, text) = match plaintext {
        Some(bytes) => (false, bytes),
        None => {
            let Some(cache) = cache else {
                return Ok(Loaded::Locked);
            };
            let decrypted = cache
                .decrypt_object(key, &raw)
                .map_err(|e| (true, manifest_decrypt_error(key, &raw, cache, &e)))?;
            let bytes = zstd_decode_bounded(&decrypted, MAX_MANIFEST_SIZE, key)
                .or_else(|e| {
                    if e.kind() == ErrorKind::FileTooLarge {
                        Err(e)
                    } else {
                        Ok(decrypted)
                    }
                })
                .map_err(|e| (true, RsbError::from(e)))?;
            (true, bytes)
        }
    };

    let text = String::from_utf8(text).map_err(|_| (encrypted, corrupt(key, "not UTF-8")))?;
    let manifest: HashMap<PathBuf, FileMetadata> =
        toml::from_str(&text).map_err(|e| (encrypted, corrupt(key, &e.to_string())))?;
    Ok(Loaded::Plain {
        encrypted,
        manifest,
    })
}

fn corrupt(key: &str, why: &str) -> RsbError {
    RsbError::Other(format!(
        "Snapshot manifest {} is corrupt or unreadable: {}",
        key, why
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_formatted_with_binary_units() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1536), "1.50 KB");
        assert_eq!(format_bytes(5 * 1024 * 1024), "5.00 MB");
        assert_eq!(format_bytes(3 << 40), "3.00 TB");
        assert_eq!(format_bytes(u64::MAX), "16777216.00 TB");
    }

    #[test]
    fn timestamps_are_formatted_in_utc() {
        let ts = parse_snapshot_timestamp("2026-05-25T103000.500Z-2");
        assert_eq!(format_timestamp(ts), "2026-05-25 10:30:00 UTC");
        assert_eq!(format_timestamp(None), "-");
    }

    #[test]
    fn ids_map_to_manifest_keys() {
        assert_eq!(
            snapshot_key("2026-05-25T103000.000Z.toml").unwrap(),
            "snapshots/2026-05-25T103000.000Z.toml"
        );
        assert!(snapshot_key("../x").is_err());
        assert_eq!(id_of("snapshots/a-1.toml"), "a-1");
    }
}
