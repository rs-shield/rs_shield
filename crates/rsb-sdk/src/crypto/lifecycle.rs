//! Repository key lifecycle: re-encryption of legacy (v1/v2) objects as v3
//! (`rsb key migrate`), export/import of the key file (`rsb key export` /
//! `rsb key import`) and DEK rotation (`rsb key rotate`).
//!
//! # Crash safety
//!
//! Every operation that rewrites blobs holds the exclusive repository lock
//! and relies on two properties:
//!
//! * objects keep their names (content-addressed), and a blob is replaced by
//!   a single atomic write over the old one (local storage: temp file +
//!   rename; S3: one PutObject; memory: one insert). A reader therefore sees
//!   either the old blob or the complete new one, and readers handle v1, v2
//!   and v3 blobs (and blobs of either DEK during a rotation) per object, so
//!   a half-done run leaves a fully readable repository;
//! * each new blob is read back and decrypted, and its plaintext hash is
//!   compared with the old one before moving on (the write is retried from
//!   the local copy if the stored bytes are wrong).
//!
//! Manifests are rewritten last (their `stored_size`/`stored_hash` fields
//! are refreshed from the new objects), and the key file is changed only
//! after a full verification of every snapshot succeeds with the final key
//! alone. Running the command again resumes: work already done is detected
//! from the blob headers and skipped.

use super::keyfile::{
    self, RepoKey, RepoKeyFile, read_key_file_raw, release, replace_key_file, unwrap_blocking,
};
use super::stream::{KeyId, V3_HEADER_LEN, V3Header};
use super::{KeyCache, is_v3};
use crate::core::cancellation::CancellationToken;
use crate::core::manifest::read_manifest_cached;
use crate::core::types::{FileMetadata, ProgressCallback};
use crate::error::{CryptoErrorKind, RsbError};
use crate::integrity::verify::{VerifyObjectsOptions, verify_manifest_objects};
use crate::storage::{Storage, validate_content_hash};
use std::collections::HashMap;
use std::io::{self, ErrorKind};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt, ReadBuf};
use tracing::{info, warn};

/// Prefix of encrypted data objects.
const ENC_PREFIX: &str = "data/enc";
/// Attempts to store a rewritten blob before giving up.
const WRITE_ATTEMPTS: usize = 3;

/// Format of one stored encrypted blob, from its header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlobKind {
    /// v1 or v2 (password-derived key).
    Legacy,
    /// v3, with the key id of its header (if any).
    V3(Option<KeyId>),
    /// Starts with the v3 magic but the header is invalid.
    Malformed,
}

/// Format of a snapshot manifest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ManifestKind {
    Clear,
    Legacy,
    V3(Option<KeyId>),
}

fn other(msg: impl Into<String>) -> RsbError {
    RsbError::Other(msg.into())
}

fn corrupt(msg: impl Into<String>) -> RsbError {
    RsbError::crypto(CryptoErrorKind::Corrupt, msg)
}

fn boxed(e: Box<dyn std::error::Error>) -> RsbError {
    match e.downcast::<RsbError>() {
        Ok(e) => *e,
        Err(e) => match e.downcast::<io::Error>() {
            Ok(e) => RsbError::from(*e),
            Err(e) => RsbError::Other(e.to_string()),
        },
    }
}

fn blake3_hex(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

fn cancelled(token: &Option<CancellationToken>) -> bool {
    token.as_ref().is_some_and(|t| t.is_cancelled())
}

fn report(progress: &Option<ProgressCallback>, i: usize, total: usize, msg: impl Into<String>) {
    if let Some(cb) = progress {
        cb(i, total, msg.into());
    }
}

async fn read_up_to<R: AsyncRead + Unpin + ?Sized>(
    src: &mut R,
    buf: &mut [u8],
) -> io::Result<usize> {
    let mut filled = 0;
    while filled < buf.len() {
        let n = src.read(&mut buf[filled..]).await?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    Ok(filled)
}

fn kind_of_prefix(prefix: &[u8]) -> BlobKind {
    if is_v3(prefix) {
        match V3Header::parse(prefix) {
            Ok(h) => BlobKind::V3(h.key_id()),
            Err(_) => BlobKind::Malformed,
        }
    } else {
        BlobKind::Legacy
    }
}

/// Reads only the header of an encrypted blob to classify it.
pub async fn blob_kind(storage: &dyn Storage, path: &str) -> io::Result<BlobKind> {
    let mut r = storage.read_stream(path).await?;
    let mut buf = [0u8; V3_HEADER_LEN];
    let n = read_up_to(&mut r, &mut buf).await?;
    Ok(kind_of_prefix(&buf[..n]))
}

fn manifest_kind(raw: &[u8]) -> ManifestKind {
    if !keyfile::looks_encrypted(raw) {
        ManifestKind::Clear
    } else {
        match kind_of_prefix(&raw[..raw.len().min(V3_HEADER_LEN)]) {
            BlobKind::V3(id) => ManifestKind::V3(id),
            // A malformed v3 manifest fails to decrypt before we get here.
            BlobKind::Legacy | BlobKind::Malformed => ManifestKind::Legacy,
        }
    }
}

/// Counts and hashes the bytes read through it.
struct HashingReader<R> {
    inner: R,
    hasher: blake3::Hasher,
    len: u64,
}

impl<R: AsyncRead + Unpin> AsyncRead for HashingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let before = buf.filled().len();
        let r = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &r {
            let new = &buf.filled()[before..];
            self.hasher.update(new);
            self.len += new.len() as u64;
        }
        r
    }
}

/// What a stored v3 blob holds, after authenticating every chunk.
#[derive(Debug, Clone)]
struct StoredInfo {
    stored_size: u64,
    stored_hash: String,
    plain_hash: String,
}

/// Streams a v3 blob through the decryptor (every chunk authenticated with
/// `key`) and returns its stored size/hash and the plaintext hash.
async fn authenticate_v3(
    storage: &dyn Storage,
    path: &str,
    object_id: &str,
    key: &RepoKey,
) -> io::Result<StoredInfo> {
    let mut reader = HashingReader {
        inner: storage.read_stream(path).await?,
        hasher: blake3::Hasher::new(),
        len: 0,
    };
    let plain_hash = {
        let mut dec = key.stream_decryptor(object_id, &[], &mut reader).await?;
        let mut h = blake3::Hasher::new();
        while let Some(chunk) = dec.next_chunk().await? {
            h.update(chunk);
        }
        h.finalize().to_hex().to_string()
    };
    // The decryptor stops at EOF; drain defensively so the hash covers
    // every stored byte.
    let mut rest = Vec::new();
    reader.read_to_end(&mut rest).await?;
    if !rest.is_empty() {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("{}: trailing data after the final chunk", path),
        ));
    }
    Ok(StoredInfo {
        stored_size: reader.len,
        stored_hash: reader.hasher.finalize().to_hex().to_string(),
        plain_hash,
    })
}

/// A rewritten blob waiting to be stored (memory, or a spooled temp file).
enum NewBlob {
    Memory(Vec<u8>),
    Spool(crate::core::file_processor::Spool),
}

impl NewBlob {
    async fn store(&self, storage: &dyn Storage, path: &str) -> io::Result<()> {
        use crate::core::file_processor::Spool;
        match self {
            NewBlob::Memory(b) => storage.write(path, b).await,
            NewBlob::Spool(Spool::Memory { buf, .. }) => storage.write(path, buf).await,
            NewBlob::Spool(Spool::File(f)) => storage.write_file(path, f.path()).await,
        }
    }
}

/// Stores `blob` over `path` (atomic per object on every backend), then
/// reads it back with `verify_key` and checks that it authenticates and
/// decrypts to `plain_hash`. Retries from the local copy if not.
async fn store_verified(
    storage: &dyn Storage,
    path: &str,
    object_id: &str,
    blob: &NewBlob,
    verify_key: &RepoKey,
    plain_hash: &str,
) -> Result<StoredInfo, RsbError> {
    let mut last = String::new();
    for attempt in 1..=WRITE_ATTEMPTS {
        blob.store(storage, path).await?;
        match authenticate_v3(storage, path, object_id, verify_key).await {
            Ok(info) if info.plain_hash == plain_hash => return Ok(info),
            Ok(_) => last = "plaintext hash differs".into(),
            Err(e) => last = e.to_string(),
        }
        warn!(
            "Re-encrypted {} did not verify (attempt {}/{}): {}",
            path, attempt, WRITE_ATTEMPTS, last
        );
    }
    Err(RsbError::Storage(format!(
        "{} could not be stored correctly after {} attempts ({}); the repository is still \
         readable, run the command again",
        path, WRITE_ATTEMPTS, last
    )))
}

/// One snapshot manifest, decrypted and parsed.
struct SnapshotEntry {
    path: String,
    kind: ManifestKind,
    content: String,
    manifest: HashMap<PathBuf, FileMetadata>,
}

/// One encrypted data object.
struct ObjectEntry {
    path: String,
    id: String,
}

/// Everything a lifecycle operation needs to know about the repository.
struct RepoScan {
    snapshots: Vec<SnapshotEntry>,
    /// Content hashes of encrypted objects referenced by any snapshot, with
    /// the stored size recorded by the manifests (first non-empty one).
    referenced: HashMap<String, Option<u64>>,
    objects: Vec<ObjectEntry>,
}

async fn list_or_empty(storage: &dyn Storage, prefix: &str) -> Result<Vec<String>, RsbError> {
    match storage.list(prefix).await {
        Ok(v) => Ok(v),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(e.into()),
    }
}

async fn snapshot_paths(storage: &dyn Storage) -> Result<Vec<String>, RsbError> {
    let mut snaps = list_or_empty(storage, "snapshots").await?;
    snaps.retain(|s| s.ends_with(".toml"));
    crate::core::manifest::sort_snapshots_newest_first(&mut snaps);
    Ok(snaps)
}

async fn object_entries(storage: &dyn Storage) -> Result<Vec<ObjectEntry>, RsbError> {
    let mut out = Vec::new();
    for path in list_or_empty(storage, ENC_PREFIX).await? {
        let id = path
            .strip_prefix(ENC_PREFIX)
            .map(|s| s.trim_start_matches('/'))
            .unwrap_or_default()
            .to_string();
        if validate_content_hash(&id).is_err() {
            warn!("Ignoring unexpected entry {}", path);
            continue;
        }
        out.push(ObjectEntry { path, id });
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Reads and parses one manifest (`cache` for encrypted ones).
async fn read_snapshot(
    storage: &dyn Storage,
    path: &str,
    cache: &KeyCache,
) -> Result<SnapshotEntry, RsbError> {
    let raw = storage.read(path).await?;
    let kind = manifest_kind(&raw);
    let cache = (kind != ManifestKind::Clear).then_some(cache);
    let content = read_manifest_cached(storage, path, cache)
        .await
        .map_err(boxed)?;
    let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&content)
        .map_err(|e| corrupt(format!("Manifest {} cannot be parsed: {}", path, e)))?;
    Ok(SnapshotEntry {
        path: path.to_string(),
        kind,
        content,
        manifest,
    })
}

fn encrypted_refs(manifest: &HashMap<PathBuf, FileMetadata>) -> Vec<(String, Option<u64>)> {
    let mut out = Vec::new();
    for meta in manifest.values().filter(|m| m.encrypted) {
        match &meta.chunks {
            Some(chunks) => {
                out.extend(chunks.iter().map(|c| (c.hash.clone(), Some(c.stored_size))))
            }
            None => out.push((meta.hash.clone(), meta.stored_size)),
        }
    }
    out
}

/// Reads every manifest (fails closed: an unreadable manifest means the
/// set of referenced objects is unknown) and lists the encrypted objects.
async fn scan(storage: &dyn Storage, cache: &KeyCache) -> Result<RepoScan, RsbError> {
    let mut snapshots = Vec::new();
    let mut referenced: HashMap<String, Option<u64>> = HashMap::new();
    for path in snapshot_paths(storage).await? {
        let snap = read_snapshot(storage, &path, cache).await?;
        for (hash, size) in encrypted_refs(&snap.manifest) {
            let slot = referenced.entry(hash).or_insert(None);
            if slot.is_none() {
                *slot = size.filter(|s| *s != 0);
            }
        }
        snapshots.push(snap);
    }
    Ok(RepoScan {
        snapshots,
        referenced,
        objects: object_entries(storage).await?,
    })
}

/// Overview of the v1/v2 data still in a repository (no password needed:
/// only blob headers are read).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LegacySummary {
    /// v1/v2 data objects under `data/enc`.
    pub objects: usize,
    /// Their total stored size in bytes.
    pub bytes: u64,
    /// v1/v2 snapshot manifests.
    pub manifests: usize,
}

impl LegacySummary {
    pub fn is_empty(&self) -> bool {
        self.objects == 0 && self.manifests == 0
    }
}

/// Counts the v1/v2 objects and manifests of a repository.
pub async fn legacy_summary(storage: &dyn Storage) -> Result<LegacySummary, RsbError> {
    let mut s = LegacySummary::default();
    for o in object_entries(storage).await? {
        if blob_kind(storage, &o.path).await? == BlobKind::Legacy {
            s.objects += 1;
            s.bytes += storage.size(&o.path).await?.unwrap_or(0);
        }
    }
    for snap in snapshot_paths(storage).await? {
        if manifest_kind(&storage.read(&snap).await?) == ManifestKind::Legacy {
            s.manifests += 1;
        }
    }
    Ok(s)
}

/// State of a repository's key and encrypted data, for showing what a key
/// operation (`rsb key init/migrate/rotate`) will do before it runs. No
/// password needed: only the key file and blob headers are read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyStatus {
    /// `keys/repo.key` exists.
    pub key_file: bool,
    /// Key id of the active data key (hex), if the key file has one.
    pub key_id: Option<String>,
    /// An interrupted rotation is in progress (running rotate resumes it).
    pub rotating: bool,
    /// The key file still stores the legacy (v1/v2) password (migrate
    /// removes it).
    pub legacy_secret: bool,
    /// v1/v2 objects and manifests (what migrate re-encrypts).
    pub legacy: LegacySummary,
    /// Encrypted data objects of any format (what rotate re-encrypts).
    pub encrypted_objects: usize,
    /// Their total stored size in bytes.
    pub encrypted_bytes: u64,
    /// Encrypted snapshot manifests of any format.
    pub encrypted_manifests: usize,
}

impl KeyStatus {
    /// `rsb key init` applies (it refuses when the key file exists).
    pub fn can_init(&self) -> bool {
        !self.key_file
    }

    /// `rsb key migrate` has work to do: v1/v2 blobs remain or the key file
    /// still holds the legacy password.
    pub fn needs_migration(&self) -> bool {
        self.key_file && (!self.legacy.is_empty() || self.legacy_secret)
    }

    /// `rsb key rotate` applies (it needs the key file).
    pub fn can_rotate(&self) -> bool {
        self.key_file
    }
}

/// Reads [`KeyStatus`] (key file, plus one header read per encrypted blob).
pub async fn key_status(storage: &dyn Storage) -> Result<KeyStatus, RsbError> {
    let mut s = KeyStatus::default();
    if let Some(file) = keyfile::read_key_file(storage).await? {
        s.key_file = true;
        s.key_id = file.key_id().map(str::to_string);
        s.rotating = file.is_rotating();
        s.legacy_secret = file.has_legacy_secret();
    }
    for o in object_entries(storage).await? {
        let size = storage.size(&o.path).await?.unwrap_or(0);
        s.encrypted_objects += 1;
        s.encrypted_bytes += size;
        if blob_kind(storage, &o.path).await? == BlobKind::Legacy {
            s.legacy.objects += 1;
            s.legacy.bytes += size;
        }
    }
    for snap in snapshot_paths(storage).await? {
        match manifest_kind(&storage.read(&snap).await?) {
            ManifestKind::Clear => {}
            ManifestKind::Legacy => {
                s.encrypted_manifests += 1;
                s.legacy.manifests += 1;
            }
            ManifestKind::V3(_) => s.encrypted_manifests += 1,
        }
    }
    Ok(s)
}

/// Number of v1/v2 blobs that still need the legacy password: legacy
/// manifests, referenced legacy objects, and unreferenced legacy objects
/// that decrypt (a later backup could deduplicate against them).
/// `rsb key change-password` and `rsb key rotate` refuse while it is not 0.
pub(crate) async fn legacy_blockers(
    storage: &dyn Storage,
    cache: &KeyCache,
) -> Result<usize, RsbError> {
    let scan = scan(storage, cache).await?;
    let mut n = scan
        .snapshots
        .iter()
        .filter(|s| s.kind == ManifestKind::Legacy)
        .count();
    for o in &scan.objects {
        if blob_kind(storage, &o.path).await? != BlobKind::Legacy {
            continue;
        }
        if scan.referenced.contains_key(&o.id)
            || cache.decrypt_owned(storage.read(&o.path).await?).is_ok()
        {
            n += 1;
        }
    }
    Ok(n)
}

/// Refreshes `stored_size`/`stored_hash` of encrypted entries from `info`.
/// Returns true if anything changed.
fn refresh_stored_metadata(
    manifest: &mut HashMap<PathBuf, FileMetadata>,
    info: &HashMap<String, StoredInfo>,
) -> bool {
    let mut changed = false;
    for meta in manifest.values_mut().filter(|m| m.encrypted) {
        match &mut meta.chunks {
            Some(chunks) => {
                let mut touched = false;
                for c in chunks.iter_mut() {
                    if let Some(i) = info.get(&c.hash)
                        && (c.stored_size != i.stored_size
                            || c.stored_hash.as_deref() != Some(i.stored_hash.as_str()))
                    {
                        c.stored_size = i.stored_size;
                        c.stored_hash = Some(i.stored_hash.clone());
                        touched = true;
                    }
                }
                if touched {
                    if meta.stored_size.is_some() {
                        meta.stored_size = Some(chunks.iter().map(|c| c.stored_size).sum());
                    }
                    changed = true;
                }
            }
            None => {
                if let Some(i) = info.get(&meta.hash)
                    && (meta.stored_size != Some(i.stored_size)
                        || meta.stored_hash.as_deref() != Some(i.stored_hash.as_str()))
                {
                    meta.stored_size = Some(i.stored_size);
                    meta.stored_hash = Some(i.stored_hash.clone());
                    changed = true;
                }
            }
        }
    }
    changed
}

/// Rewrites one manifest: refreshed object metadata, encrypted with the
/// active DEK of `key` (when it was encrypted before). Read back and
/// checked before returning.
async fn rewrite_manifest(
    storage: &dyn Storage,
    snap: &SnapshotEntry,
    key: &RepoKey,
    info: &HashMap<String, StoredInfo>,
    must_reencrypt: bool,
) -> Result<bool, RsbError> {
    let mut manifest = snap.manifest.clone();
    let changed = refresh_stored_metadata(&mut manifest, info);
    if !changed && !must_reencrypt {
        return Ok(false);
    }
    let content = if changed {
        toml::to_string(&manifest).map_err(|e| other(format!("serialize manifest: {e}")))?
    } else {
        snap.content.clone()
    };
    let payload = zstd::stream::encode_all(content.as_bytes(), 3)?;
    let encrypt = snap.kind != ManifestKind::Clear;
    let blob = if encrypt {
        key.encrypt_object(&snap.path, &payload)?
    } else {
        payload.clone()
    };
    let want = blake3_hex(&payload);
    for attempt in 1..=WRITE_ATTEMPTS {
        storage.write(&snap.path, &blob).await?;
        let back = storage.read(&snap.path).await?;
        let ok = if encrypt {
            key.decrypt_object(&snap.path, &back)
                .is_ok_and(|p| blake3_hex(&p) == want)
        } else {
            back == blob
        };
        if ok {
            return Ok(true);
        }
        warn!(
            "Rewritten manifest {} did not verify (attempt {}/{})",
            snap.path, attempt, WRITE_ATTEMPTS
        );
    }
    Err(RsbError::Storage(format!(
        "Manifest {} could not be stored correctly; run the command again",
        snap.path
    )))
}

/// Full verification of every snapshot with `key` alone and no legacy
/// password: every encrypted manifest must be v3 (with the key id
/// `require_id`, if given) and every referenced object must authenticate
/// and match its content hash. Returns the number of objects checked.
async fn verify_complete(
    storage: &dyn Storage,
    key: Arc<RepoKey>,
    require_id: Option<KeyId>,
    progress: Option<ProgressCallback>,
    cancel: Option<CancellationToken>,
) -> Result<usize, RsbError> {
    let cache = KeyCache::v3_only(key);
    let mut combined: HashMap<PathBuf, FileMetadata> = HashMap::new();
    let mut problems: Vec<String> = Vec::new();
    for path in snapshot_paths(storage).await? {
        let snap = match read_snapshot(storage, &path, &cache).await {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{}: {}", path, e));
                continue;
            }
        };
        if let (Some(id), ManifestKind::V3(got)) = (require_id, snap.kind)
            && got != Some(id)
        {
            problems.push(format!("{}: not encrypted with the new key", path));
        }
        for (file, meta) in snap.manifest {
            combined.insert(PathBuf::from(format!("{}#{}", path, file.display())), meta);
        }
    }
    let outcome = verify_manifest_objects(
        storage,
        &combined,
        Some(&cache),
        VerifyObjectsOptions::default(),
        progress,
        cancel.clone(),
    )
    .await
    .map_err(|e| other(e.to_string()))?;
    if cancelled(&cancel) {
        return Err(RsbError::Cancelled);
    }
    problems.extend(outcome.errors);
    if !problems.is_empty() {
        let shown: Vec<&str> = problems.iter().take(5).map(String::as_str).collect();
        return Err(other(format!(
            "Final verification failed ({} problem(s)); the key file was not changed:\n  {}",
            problems.len(),
            shown.join("\n  ")
        )));
    }
    Ok(outcome.objects_checked)
}

/// Result of [`migrate_legacy_objects`].
#[derive(Debug, Clone, Default)]
pub struct MigrationReport {
    /// v1/v2 objects re-encrypted as v3 by this run.
    pub objects_migrated: usize,
    /// Encrypted objects that were already v3.
    pub objects_already_v3: usize,
    /// Manifests rewritten (re-encrypted or with refreshed object sizes).
    pub manifests_rewritten: usize,
    /// Objects checked by the final verification.
    pub objects_verified: usize,
    /// Storage keys of v1/v2 objects no snapshot references (migrated if
    /// they decrypt; never deleted: prune garbage-collects them).
    pub unreferenced_legacy: Vec<String>,
    /// Unreferenced v1/v2 objects left as they are because they do not
    /// decrypt with the repository password.
    pub unreadable_legacy: Vec<String>,
    /// True if the key file held the legacy password and it was removed.
    pub legacy_secret_removed: bool,
}

/// Re-encrypts every v1/v2 object and manifest of the repository as v3
/// under the repository key, verifies the whole repository with the v3 key
/// alone and then removes the legacy password from `keys/repo.key` (if an
/// earlier version stored it). Resumable and crash-safe (see the module
/// docs); holds the exclusive repository lock.
///
/// Unreferenced v1/v2 objects are re-encrypted too when they decrypt, so a
/// later backup cannot deduplicate against a blob that needs the old
/// password; they are reported, never deleted.
pub async fn migrate_legacy_objects(
    storage: Arc<dyn Storage>,
    password: &[u8],
    progress: Option<ProgressCallback>,
    cancellation: Option<CancellationToken>,
) -> Result<MigrationReport, RsbError> {
    let lock = keyfile::lock_exclusive(&storage, "key-migrate").await?;
    let result = migrate_locked(&*storage, password, progress, cancellation).await;
    release(lock).await;
    result
}

async fn migrate_locked(
    storage: &dyn Storage,
    password: &[u8],
    progress: Option<ProgressCallback>,
    cancel: Option<CancellationToken>,
) -> Result<MigrationReport, RsbError> {
    let raw = read_key_file_raw(storage).await?;
    let file = RepoKeyFile::from_bytes(&raw)?;
    let key = Arc::new(unwrap_blocking(file.clone(), password).await?);
    let cache = KeyCache::with_repo_key(password, key.clone());
    let scan = scan(storage, &cache).await?;
    // Objects are rewritten with other stored sizes: local dedup indexes
    // must start over.
    crate::core::dedup_index::bump_repo_epoch(storage).await?;
    let mut rep = MigrationReport::default();
    let mut info: HashMap<String, StoredInfo> = HashMap::new();

    // 1) Objects.
    let total = scan.objects.len();
    for (i, o) in scan.objects.iter().enumerate() {
        if cancelled(&cancel) {
            return Err(RsbError::Cancelled);
        }
        report(&progress, i, total, format!("Migrating {}", o.path));
        let referenced = scan.referenced.contains_key(&o.id);
        match blob_kind(storage, &o.path).await? {
            BlobKind::V3(_) => {
                rep.objects_already_v3 += 1;
                continue;
            }
            BlobKind::Malformed => {
                if referenced {
                    return Err(corrupt(format!("{} has a malformed v3 header", o.path)));
                }
                continue;
            }
            BlobKind::Legacy => {}
        }
        if !referenced {
            rep.unreferenced_legacy.push(o.path.clone());
        }
        let plain = match cache.decrypt_owned(storage.read(&o.path).await?) {
            Ok(p) => p,
            Err(e) if referenced => {
                return Err(corrupt(format!(
                    "Referenced object {} cannot be decrypted ({}); run `rsb verify` and fix \
                     the repository before migrating",
                    o.path, e
                )));
            }
            Err(_) => {
                warn!(
                    "Unreferenced legacy object {} does not decrypt; left as is",
                    o.path
                );
                rep.unreadable_legacy.push(o.path.clone());
                continue;
            }
        };
        let plain_hash = blake3_hex(&plain);
        let blob = NewBlob::Memory(key.encrypt_object(&o.id, &plain)?);
        drop(plain);
        let stored = store_verified(storage, &o.path, &o.id, &blob, &key, &plain_hash).await?;
        info.insert(o.id.clone(), stored);
        rep.objects_migrated += 1;
    }

    // 2) Manifests. Objects migrated by an earlier (interrupted) run are
    //    found by their stored size: a v3 blob never has the size of the
    //    v1/v2 blob of the same payload (overheads 61+ vs 44/72 bytes).
    for (hash, recorded) in &scan.referenced {
        if info.contains_key(hash) {
            continue;
        }
        let Some(recorded) = recorded else { continue };
        let path = format!("{}/{}", ENC_PREFIX, hash);
        if storage.size(&path).await? != Some(*recorded)
            && blob_kind(storage, &path).await? != BlobKind::Legacy
        {
            match authenticate_v3(storage, &path, hash, &key).await {
                Ok(i) => {
                    info.insert(hash.clone(), i);
                }
                Err(e) => warn!(
                    "{} does not authenticate ({}); manifests keep its old size",
                    path, e
                ),
            }
        }
    }
    for (i, snap) in scan.snapshots.iter().enumerate() {
        if cancelled(&cancel) {
            return Err(RsbError::Cancelled);
        }
        report(
            &progress,
            i,
            scan.snapshots.len(),
            format!("Rewriting {}", snap.path),
        );
        if rewrite_manifest(
            storage,
            snap,
            &key,
            &info,
            snap.kind == ManifestKind::Legacy,
        )
        .await?
        {
            rep.manifests_rewritten += 1;
        }
    }

    // 3) Full verification with the v3 key alone (no legacy password).
    rep.objects_verified = verify_complete(
        storage,
        Arc::new(key.without_legacy()),
        None,
        progress,
        cancel,
    )
    .await?;

    // 4) Only now drop the legacy password from the key file.
    if file.has_legacy_secret() {
        replace_key_file(
            storage,
            &raw,
            &file.without_legacy_secret(),
            password,
            &key.without_legacy(),
        )
        .await?;
        rep.legacy_secret_removed = true;
    }
    info!(
        "🔑 Migration complete: {} object(s) re-encrypted, {} manifest(s) rewritten",
        rep.objects_migrated, rep.manifests_rewritten
    );
    Ok(rep)
}

// ---------------------------------------------------------------------------
// Export / import of the key file.

/// Returns the bytes of `keys/repo.key` after checking that `password`
/// unlocks it. The DEK stays wrapped by the password-derived KEK: the export
/// is useless without the password, but must be protected like it (with it,
/// it decrypts the whole repository). The unwrapped DEK is never returned.
pub async fn export_key_file(storage: &dyn Storage, password: &[u8]) -> Result<Vec<u8>, RsbError> {
    let raw = read_key_file_raw(storage).await?;
    let file = RepoKeyFile::from_bytes(&raw)?;
    unwrap_blocking(file, password).await?;
    Ok(raw)
}

/// Result of [`import_key_file`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportOutcome {
    /// The repository already had exactly this key file: nothing written.
    pub unchanged: bool,
    /// Where the replaced key file was copied (only with `force`); the copy
    /// is kept.
    pub replaced_backup: Option<String>,
    /// How the key was matched against the repository.
    pub matched_by: String,
}

/// Checks that `key` opens this repository: the newest v3 manifest, else a
/// v3 data object. A repository without any v3 blob only proves that
/// `password` opens its newest v1/v2 manifest (if any).
async fn check_key_matches_repository(
    storage: &dyn Storage,
    key: &RepoKey,
    password: &[u8],
) -> Result<String, RsbError> {
    let mismatch = |what: &str| {
        RsbError::crypto(
            CryptoErrorKind::AuthenticationFailed,
            format!(
                "The imported key file does not belong to this repository: it cannot decrypt \
                 {}. Nothing was written.",
                what
            ),
        )
    };
    for snap in snapshot_paths(storage).await? {
        let raw = storage.read(&snap).await?;
        if !is_v3(&raw) {
            continue;
        }
        return match key.decrypt_object(&snap, &raw) {
            Ok(_) => Ok(format!("decrypts {}", snap)),
            Err(_) => Err(mismatch(&snap)),
        };
    }
    for o in object_entries(storage).await? {
        if let BlobKind::V3(_) = blob_kind(storage, &o.path).await? {
            return match authenticate_v3(storage, &o.path, &o.id, key).await {
                Ok(_) => Ok(format!("decrypts {}", o.path)),
                Err(_) => Err(mismatch(&o.path)),
            };
        }
    }
    if keyfile::probe_legacy_snapshots(storage, password).await? {
        Ok("password opens the newest v1/v2 snapshot".into())
    } else {
        Ok("repository has no encrypted data yet".into())
    }
}

/// Installs `raw` (a key file, e.g. from `rsb key export`) as
/// `keys/repo.key`: the file must parse, `password` must unlock it and the
/// key must decrypt the repository's newest v3 manifest (or object). If a
/// different key file exists, refuses unless `force`; then the existing file
/// is first copied to `keys/repo.key.bak-<ts>` (kept). The new file is read
/// back and checked. Holds the exclusive repository lock.
pub async fn import_key_file(
    storage: Arc<dyn Storage>,
    raw: &[u8],
    password: &[u8],
    force: bool,
) -> Result<ImportOutcome, RsbError> {
    let file = RepoKeyFile::from_bytes(raw)?;
    let key = unwrap_blocking(file.clone(), password).await?;
    let lock = keyfile::lock_exclusive(&storage, "key-import").await?;
    let result = import_locked(&*storage, &file, &key, password, force).await;
    release(lock).await;
    result
}

async fn import_locked(
    storage: &dyn Storage,
    file: &RepoKeyFile,
    key: &RepoKey,
    password: &[u8],
    force: bool,
) -> Result<ImportOutcome, RsbError> {
    let matched_by = check_key_matches_repository(storage, key, password).await?;
    let existing = match storage.read(keyfile::REPO_KEY_PATH).await {
        Ok(r) => Some(r),
        Err(e) if e.kind() == ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    let mut out = ImportOutcome {
        matched_by,
        ..Default::default()
    };
    if let Some(old) = &existing {
        if RepoKeyFile::from_bytes(old).is_ok_and(|f| f == *file) {
            out.unchanged = true;
            return Ok(out);
        }
        if !force {
            return Err(RsbError::InvalidInput(format!(
                "{} already exists and differs from the imported file; use --force to replace \
                 it (the current file is backed up first)",
                keyfile::REPO_KEY_PATH
            )));
        }
        let backup = keyfile::backup_copy_path();
        storage.write(&backup, old).await?;
        out.replaced_backup = Some(backup);
    }
    storage
        .write(keyfile::REPO_KEY_PATH, &file.to_bytes()?)
        .await?;
    let ok = matches!(
        keyfile::load_repo_key(storage, password).await,
        Ok(Some(k)) if k.same_key(key)
    );
    if !ok {
        match &existing {
            Some(old) => storage.write(keyfile::REPO_KEY_PATH, old).await?,
            None => storage.delete(keyfile::REPO_KEY_PATH).await?,
        }
        return Err(RsbError::Storage(format!(
            "The imported {} could not be read back; the previous state was restored",
            keyfile::REPO_KEY_PATH
        )));
    }
    info!(
        "🔑 Imported {} ({})",
        keyfile::REPO_KEY_PATH,
        out.matched_by
    );
    Ok(out)
}

// ---------------------------------------------------------------------------
// Key rotation.

/// Result of [`rotate_repo_key`].
#[derive(Debug, Clone, Default)]
pub struct RotationReport {
    /// Key id (hex) of the retired DEK.
    pub old_key_id: String,
    /// Key id (hex) of the new DEK.
    pub new_key_id: String,
    /// True if this run resumed an interrupted rotation.
    pub resumed: bool,
    /// Objects re-encrypted by this run.
    pub objects_rotated: usize,
    /// Objects already under the new key (written by an earlier run, or by
    /// a backup during an interrupted rotation).
    pub objects_already_rotated: usize,
    /// Manifests rewritten.
    pub manifests_rewritten: usize,
    /// Objects checked by the final verification.
    pub objects_verified: usize,
    /// Unreferenced objects that decrypt with neither key (left as they
    /// are; prune garbage-collects them).
    pub unreadable_unreferenced: Vec<String>,
}

/// Re-encrypts `o` (a v3 blob of either DEK) under the active (new) DEK of
/// `key`, streaming through a spool so memory stays bounded.
async fn rotate_object(
    storage: &dyn Storage,
    o: &ObjectEntry,
    key: &RepoKey,
    verify_key: &RepoKey,
) -> Result<StoredInfo, RsbError> {
    use crate::core::file_processor::{HashingWriter, SPOOL_MEM_LIMIT, Spool};
    use std::io::Write;
    let reader = storage.read_stream(&o.path).await?;
    let mut dec = key.stream_decryptor(&o.id, &[], reader).await?;
    let mut enc = key.encryptor(&o.id, HashingWriter::new(Spool::new(SPOOL_MEM_LIMIT)))?;
    let mut plain = blake3::Hasher::new();
    while let Some(chunk) = dec.next_chunk().await? {
        plain.update(chunk);
        enc.write_all(chunk)?;
    }
    let (spool, _, _) = enc.finish()?.into_parts();
    let plain_hash = plain.finalize().to_hex().to_string();
    store_verified(
        storage,
        &o.path,
        &o.id,
        &NewBlob::Spool(spool),
        verify_key,
        &plain_hash,
    )
    .await
}

/// Generates a new DEK and re-encrypts every v3 object and manifest under
/// it; then, after a full verification with the new DEK alone, removes the
/// old DEK from the key file.
///
/// The key file first enters a "rotating" state that holds both DEKs
/// (wrapped by the same password-derived KEK); from then on new blobs use
/// the new DEK and readers pick the DEK by the key id in each blob header.
/// Running it again during an interrupted rotation resumes it. Requires the
/// legacy migration to be complete (no v1/v2 blob that still needs the
/// password). Holds the exclusive repository lock.
pub async fn rotate_repo_key(
    storage: Arc<dyn Storage>,
    password: &[u8],
    progress: Option<ProgressCallback>,
    cancellation: Option<CancellationToken>,
) -> Result<RotationReport, RsbError> {
    let lock = keyfile::lock_exclusive(&storage, "key-rotate").await?;
    let result = rotate_locked(&*storage, password, progress, cancellation).await;
    release(lock).await;
    result
}

fn rotation_params(file: &RepoKeyFile) -> keyfile::KdfParams {
    let p = file.params();
    if p.validate_for_create().is_ok() {
        p
    } else {
        keyfile::KdfParams::DEFAULT
    }
}

async fn rotate_locked(
    storage: &dyn Storage,
    password: &[u8],
    progress: Option<ProgressCallback>,
    cancel: Option<CancellationToken>,
) -> Result<RotationReport, RsbError> {
    let mut raw = read_key_file_raw(storage).await?;
    let mut file = RepoKeyFile::from_bytes(&raw)?;
    let mut key = Arc::new(unwrap_blocking(file.clone(), password).await?);
    let mut rep = RotationReport::default();
    // Objects are rewritten: local dedup indexes must start over.
    crate::core::dedup_index::bump_repo_epoch(storage).await?;

    if key.is_rotating() {
        rep.resumed = true;
        info!("🔑 Resuming the interrupted key rotation");
    } else {
        let cache = KeyCache::with_repo_key(password, key.clone());
        let legacy = legacy_blockers(storage, &cache).await?;
        if legacy > 0 || file.has_legacy_secret() {
            return Err(RsbError::InvalidInput(format!(
                "The repository still holds {} v1/v2 (pre-v3) object(s) or manifest(s){}: run \
                 `rsb key migrate` first, then `rsb key rotate`",
                legacy,
                if file.has_legacy_secret() {
                    " or its key file still stores the legacy password"
                } else {
                    ""
                }
            )));
        }
        let rotating = key.begin_rotation()?;
        let mut new_file = keyfile::wrap_blocking(
            Arc::new(rotating.without_legacy()),
            password,
            rotation_params(&file),
        )
        .await?;
        new_file.set_dates_from(&file);
        key = Arc::new(replace_key_file(storage, &raw, &new_file, password, &rotating).await?);
        raw = read_key_file_raw(storage).await?;
        file = RepoKeyFile::from_bytes(&raw)?;
        info!(
            "🔑 Key rotation started: new key id {} (old {})",
            key.key_id_hex(),
            key.only_primary().key_id_hex()
        );
    }
    rep.new_key_id = key.key_id_hex();
    rep.old_key_id = key.only_primary().key_id_hex();
    let new_id = key.key_id();
    let final_key = Arc::new(key.only_active());

    let cache = KeyCache::with_repo_key(password, key.clone());
    let scan = scan(storage, &cache).await?;
    let mut info: HashMap<String, StoredInfo> = HashMap::new();

    // 1) Objects.
    let total = scan.objects.len();
    for (i, o) in scan.objects.iter().enumerate() {
        if cancelled(&cancel) {
            return Err(RsbError::Cancelled);
        }
        report(&progress, i, total, format!("Re-encrypting {}", o.path));
        let referenced = scan.referenced.contains_key(&o.id);
        match blob_kind(storage, &o.path).await? {
            BlobKind::V3(Some(id)) if id == new_id => {
                rep.objects_already_rotated += 1;
                if referenced {
                    match authenticate_v3(storage, &o.path, &o.id, &final_key).await {
                        Ok(s) => {
                            info.insert(o.id.clone(), s);
                        }
                        Err(e) => {
                            return Err(corrupt(format!(
                                "Referenced object {} does not authenticate: {}",
                                o.path, e
                            )));
                        }
                    }
                }
            }
            BlobKind::V3(_) => match rotate_object(storage, o, &key, &final_key).await {
                Ok(s) => {
                    info.insert(o.id.clone(), s);
                    rep.objects_rotated += 1;
                }
                Err(e) if referenced => return Err(e),
                Err(e) => {
                    warn!("Unreferenced object {} left as is: {}", o.path, e);
                    rep.unreadable_unreferenced.push(o.path.clone());
                }
            },
            BlobKind::Legacy | BlobKind::Malformed if referenced => {
                return Err(RsbError::InvalidInput(format!(
                    "Referenced object {} is not a v3 blob: run `rsb key migrate` (or `rsb \
                     verify`) first, then `rsb key rotate` again to resume",
                    o.path
                )));
            }
            BlobKind::Legacy | BlobKind::Malformed => {
                rep.unreadable_unreferenced.push(o.path.clone());
            }
        }
    }

    // 2) Manifests.
    for (i, snap) in scan.snapshots.iter().enumerate() {
        if cancelled(&cancel) {
            return Err(RsbError::Cancelled);
        }
        report(
            &progress,
            i,
            scan.snapshots.len(),
            format!("Rewriting {}", snap.path),
        );
        let must = match snap.kind {
            ManifestKind::Clear => false,
            ManifestKind::V3(id) => id != Some(new_id),
            ManifestKind::Legacy => {
                return Err(RsbError::InvalidInput(format!(
                    "{} is a v1/v2 manifest: run `rsb key migrate` first",
                    snap.path
                )));
            }
        };
        if rewrite_manifest(storage, snap, &key, &info, must).await? {
            rep.manifests_rewritten += 1;
        }
    }

    // 3) Full verification with the new DEK alone.
    rep.objects_verified =
        verify_complete(storage, final_key.clone(), Some(new_id), progress, cancel).await?;

    // 4) Drop the old DEK from the key file.
    let mut final_file =
        keyfile::wrap_blocking(final_key.clone(), password, rotation_params(&file)).await?;
    final_file.set_dates_from(&file);
    replace_key_file(storage, &raw, &final_file, password, &final_key).await?;
    info!(
        "🔑 Key rotation complete: {} object(s) and {} manifest(s) re-encrypted under key {}",
        rep.objects_rotated, rep.manifests_rewritten, rep.new_key_id
    );
    Ok(rep)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::manifest::write_manifest;
    use crate::crypto::EncryptionKey;
    use crate::crypto::keyfile::{KdfParams, REPO_KEY_PATH, change_password, load_repo_key};
    use crate::storage::MemoryStorage;

    /// A key file written by an earlier version after a password change:
    /// the DEK opens with "new", the v1/v2 objects need "old" (stored as
    /// the legacy secret). Migration uses it, then removes it.
    #[tokio::test]
    async fn migration_removes_legacy_secret_of_old_key_files() {
        let mem = Arc::new(MemoryStorage::new());
        let storage: Arc<dyn Storage> = mem.clone();
        let data = b"legacy payload".to_vec();
        let hash = crate::crypto::hash_file_content(&data).unwrap();
        let obj = format!("data/enc/{}", hash);
        let v2 = EncryptionKey::new(b"old").unwrap().encrypt(&data).unwrap();
        storage.write(&obj, &v2).await.unwrap();
        let mut m = HashMap::new();
        m.insert(
            PathBuf::from("f.txt"),
            FileMetadata::new(hash.clone(), true, false),
        );
        write_manifest(&*storage, &m, Some("old"), false)
            .await
            .unwrap();
        let key = RepoKey::generate().unwrap();
        let mut file = key.wrap(b"new", KdfParams::MIN_CREATE).unwrap();
        file.add_legacy_secret(&key, b"old");
        storage
            .write(REPO_KEY_PATH, &file.to_bytes().unwrap())
            .await
            .unwrap();

        // Readable before, but no password change yet.
        let cache = KeyCache::open(&*storage, b"new").await.unwrap();
        let raw = storage.read(&obj).await.unwrap();
        assert_eq!(cache.decrypt_object(&hash, &raw).unwrap(), data);
        assert!(
            change_password(storage.clone(), b"new", b"x", Some(KdfParams::MIN_CREATE))
                .await
                .is_err()
        );

        let rep = migrate_legacy_objects(storage.clone(), b"new", None, None)
            .await
            .unwrap();
        assert_eq!(rep.objects_migrated, 1);
        assert_eq!(rep.manifests_rewritten, 1);
        assert!(rep.legacy_secret_removed);
        let f = keyfile::read_key_file(&*storage).await.unwrap().unwrap();
        assert!(!f.has_legacy_secret());
        let k = load_repo_key(&*storage, b"new").await.unwrap().unwrap();
        assert!(k.legacy_password().is_none());
        assert!(
            mem.keys()
                .iter()
                .all(|p| !p.starts_with(keyfile::REPO_KEY_BACKUP_PREFIX))
        );

        // Everything opens with the v3 key alone.
        let v3 = KeyCache::v3_only(Arc::new(k));
        let snaps = snapshot_paths(&*storage).await.unwrap();
        let content = read_manifest_cached(&*storage, &snaps[0], Some(&v3))
            .await
            .unwrap();
        assert!(content.contains(&hash));
        let raw = storage.read(&obj).await.unwrap();
        assert_eq!(v3.decrypt_object(&hash, &raw).unwrap(), data);
        // The legacy password is no longer involved.
        change_password(storage.clone(), b"new", b"x", Some(KdfParams::MIN_CREATE))
            .await
            .unwrap();
    }

    /// After a rotation, the raw old DEK fails AEAD authentication on every
    /// blob (not only the key-id lookup), and sizes are unchanged.
    #[tokio::test]
    async fn rotated_blobs_do_not_open_with_the_old_dek() {
        use crate::core::manifest::write_manifest_with_key;
        use crate::core::types::ChunkMetadata;
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        let old = keyfile::init_repo_key(storage.clone(), b"pw", KdfParams::MIN_CREATE)
            .await
            .unwrap();
        let old_dek = old.active_dek();
        let mut m = HashMap::new();
        let mut chunks = Vec::new();
        let mut sizes = Vec::new();
        for i in 0..3u8 {
            let data = vec![i; 1024 * 1024 + 5 * i as usize];
            let hash = crate::crypto::hash_file_content(&data).unwrap();
            let blob = old.encrypt_object(&hash, &data).unwrap();
            storage
                .write(&format!("data/enc/{}", hash), &blob)
                .await
                .unwrap();
            sizes.push(blob.len());
            chunks.push(ChunkMetadata {
                hash: hash.clone(),
                stored_size: blob.len() as u64,
                stored_hash: Some(blake3_hex(&blob)),
            });
        }
        let total: u64 = chunks.iter().map(|c| c.stored_size).sum();
        m.insert(
            PathBuf::from("big"),
            FileMetadata::new_multipart("f".repeat(64), true, false, chunks.clone(), total),
        );
        write_manifest_with_key(&*storage, &m, Some(&old), false)
            .await
            .unwrap();

        let rep = rotate_repo_key(storage.clone(), b"pw", None, None)
            .await
            .unwrap();
        assert_eq!(rep.objects_rotated, 3);
        let new = load_repo_key(&*storage, b"pw").await.unwrap().unwrap();
        let new_dek = new.active_dek();
        assert_ne!(old_dek, new_dek);
        for (c, size) in chunks.iter().zip(sizes) {
            let blob = storage.read(&format!("data/enc/{}", c.hash)).await.unwrap();
            assert_eq!(blob.len(), size);
            let e = crate::crypto::stream::decrypt_v3(&old_dek, &c.hash, &blob).unwrap_err();
            assert!(crate::crypto::is_authentication_error(&e));
            assert!(crate::crypto::stream::decrypt_v3(&new_dek, &c.hash, &blob).is_ok());
        }
        // The manifest records the new stored hashes.
        let snap = &snapshot_paths(&*storage).await.unwrap()[0];
        let raw = storage.read(snap).await.unwrap();
        assert!(crate::crypto::stream::decrypt_v3(&old_dek, snap, &raw).is_err());
        let cache = KeyCache::v3_only(Arc::new(new));
        let content = read_manifest_cached(&*storage, snap, Some(&cache))
            .await
            .unwrap();
        let after: HashMap<PathBuf, FileMetadata> = toml::from_str(&content).unwrap();
        for c in after[&PathBuf::from("big")].chunks.as_ref().unwrap() {
            let blob = storage.read(&format!("data/enc/{}", c.hash)).await.unwrap();
            assert_eq!(c.stored_hash.as_deref(), Some(blake3_hex(&blob).as_str()));
        }
    }
}
