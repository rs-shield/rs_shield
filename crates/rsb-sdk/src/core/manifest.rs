// manifest.rs - Performance + reliability version
use super::limits::{MAX_MANIFEST_SIZE, too_large, zstd_decode_bounded};
use super::types::{ChunkMetadata, FileMetadata};
use crate::crypto::encrypt_data_with_key;
use crate::crypto::{KeyCache, RepoKey};
use crate::error::{CryptoErrorKind, RsbError};
use crate::storage::Storage;
use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use tracing::{debug, error, info};

/// Legacy snapshot name format (1-second resolution), e.g. `2026-02-07T103000Z`.
pub const SNAPSHOT_NAME_FORMAT_SECS: &str = "%Y-%m-%dT%H%M%SZ";
/// Current snapshot name format (millisecond resolution), e.g. `2026-02-07T103000.123Z`.
pub const SNAPSHOT_NAME_FORMAT: &str = "%Y-%m-%dT%H%M%S%.3fZ";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[allow(dead_code)]
pub struct ChunkReport {
    pub file_path: PathBuf,
    pub chunks: Vec<ChunkMetadata>,
    pub total_chunks: usize,
}

/// Manifest key of a source-relative path: components joined with `/`
/// whatever the platform, so a manifest written on Windows restores
/// everywhere (restore refuses `\` in entries).
pub fn manifest_key(rel: &std::path::Path) -> PathBuf {
    let parts: Vec<String> = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().into_owned())
        .collect();
    PathBuf::from(parts.join("/"))
}

/// Parses the timestamp encoded in a snapshot name or path.
///
/// Accepts `snapshots/<name>.toml` or a bare `<name>` in any of the formats
/// ever produced: `%Y-%m-%dT%H%M%SZ`, `%Y-%m-%dT%H%M%S.<frac>Z`, either one
/// followed by a `-<n>` collision suffix, or RFC3339 (`2026-02-07T10:30:00Z`).
pub fn parse_snapshot_timestamp(name: &str) -> Option<DateTime<Utc>> {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem.strip_suffix(".toml").unwrap_or(stem);

    if let Some(z) = stem.rfind('Z') {
        let (head, tail) = stem.split_at(z);
        let suffix = &tail[1..];
        let suffix_ok = suffix.is_empty()
            || (suffix.len() > 1
                && suffix.starts_with('-')
                && suffix[1..].bytes().all(|b| b.is_ascii_digit()));
        if suffix_ok {
            let (base, frac) = match head.split_once('.') {
                Some((b, f)) => (b, Some(f)),
                None => (head, None),
            };
            if let Ok(naive) = NaiveDateTime::parse_from_str(base, "%Y-%m-%dT%H%M%S") {
                let mut ts = naive.and_utc();
                match frac {
                    None => return Some(ts),
                    Some(f)
                        if !f.is_empty()
                            && f.len() <= 9
                            && f.bytes().all(|b| b.is_ascii_digit()) =>
                    {
                        let nanos: i64 = format!("{:0<9}", f).parse().ok()?;
                        ts += chrono::Duration::nanoseconds(nanos);
                        return Some(ts);
                    }
                    Some(_) => {}
                }
            }
        }
    }

    DateTime::parse_from_rfc3339(stem)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

/// Validates a user-supplied snapshot id before it is turned into a storage
/// key. Accepts an optional `.toml` suffix and returns the bare id.
pub fn validate_snapshot_id(id: &str) -> Result<&str, Box<dyn std::error::Error>> {
    let bare = id.strip_suffix(".toml").unwrap_or(id);
    let ok = !bare.is_empty()
        && bare.len() <= 128
        && !bare.starts_with('.')
        && !bare.contains("..")
        && bare
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':' | b'+'));
    if !ok {
        return Err(format!(
            "Invalid snapshot id '{}': expected a snapshot name such as 2026-02-07T103000.123Z",
            id
        )
        .into());
    }
    Ok(bare)
}

/// Picks a snapshot path that does not exist yet for `now`.
async fn unique_snapshot_path(
    storage: &dyn Storage,
    now: DateTime<Utc>,
) -> Result<String, Box<dyn std::error::Error>> {
    let base = now.format(SNAPSHOT_NAME_FORMAT).to_string();
    let first = format!("snapshots/{}.toml", base);
    if !storage.exists(&first).await? {
        return Ok(first);
    }
    for n in 1..1000u32 {
        let candidate = format!("snapshots/{}-{}.toml", base, n);
        if !storage.exists(&candidate).await? {
            return Ok(candidate);
        }
    }
    Err("Could not allocate a unique snapshot name".into())
}

/// Writes the manifest (zstd + encryption) given a password.
///
/// Uses the repository key (v3) if `keys/repo.key` exists; otherwise falls
/// back to the legacy v2 format (the key file is only created by a backup
/// or `rsb key init`). Backups use [`write_manifest_with_key`].
pub async fn write_manifest(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    encryption_key: Option<&str>,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    match encryption_key {
        Some(pw) => match crate::crypto::keyfile::load_repo_key(storage, pw.as_bytes()).await? {
            Some(k) => write_manifest_with_key(storage, manifest, Some(&k), dry_run).await,
            None => {
                write_manifest_inner(storage, manifest, ManifestCipher::Legacy(pw), dry_run).await
            }
        },
        None => write_manifest_inner(storage, manifest, ManifestCipher::None, dry_run).await,
    }
}

/// Writes the manifest, encrypted as a v3 blob bound to its snapshot path
/// when `repo_key` is given.
pub async fn write_manifest_with_key(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    repo_key: Option<&RepoKey>,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let cipher = match repo_key {
        Some(k) => ManifestCipher::V3(k),
        None => ManifestCipher::None,
    };
    write_manifest_inner(storage, manifest, cipher, dry_run).await
}

enum ManifestCipher<'a> {
    None,
    Legacy(&'a str),
    V3(&'a RepoKey),
}

async fn write_manifest_inner(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    cipher: ManifestCipher<'_>,
    dry_run: bool,
) -> Result<String, Box<dyn std::error::Error>> {
    let now = Utc::now();

    if dry_run {
        let snapshot_path = format!("snapshots/{}.toml", now.format(SNAPSHOT_NAME_FORMAT));
        info!("[Dry-run] Snapshot manifest not written: {}", snapshot_path);
        return Ok(snapshot_path);
    }

    // Millisecond resolution + existence check: two backups finishing in
    // the same second no longer overwrite each other's manifest.
    let snapshot_path = unique_snapshot_path(storage, now).await?;

    // TOML Serialization
    let content_str = toml::to_string(manifest)?;

    // Zstd Compression (efficient for text-based manifest)
    let mut compressed = Vec::new();
    {
        let mut encoder = zstd::Encoder::new(&mut compressed, 3)?; // level 3 = good compression/speed balance
        encoder.write_all(content_str.as_bytes())?;
        encoder.finish()?;
    }

    // Encryption (if applicable)
    let final_bytes = match cipher {
        ManifestCipher::None => compressed,
        ManifestCipher::Legacy(pw) => {
            let enc_key = crate::crypto::EncryptionKey::new(pw.as_bytes())
                .map_err(|e| format!("Failed to derive key for manifest: {}", e))?;
            encrypt_data_with_key(&compressed, &enc_key)?
        }
        // Object id = storage key: the manifest cannot be renamed/swapped.
        ManifestCipher::V3(k) => k.encrypt_object(&snapshot_path, &compressed)?,
    };

    // Storage::write is atomic + fsync'ed for local storage.
    storage.write(&snapshot_path, &final_bytes).await?;
    info!(
        "📸 Snapshot manifest written: {} ({} bytes → {} bytes)",
        snapshot_path,
        content_str.len(),
        final_bytes.len()
    );

    Ok(snapshot_path)
}

/// Reads the manifest with support for encryption + compression
pub async fn read_manifest(
    storage: &dyn Storage,
    path: &str,
    key: Option<&str>,
) -> Result<String, Box<dyn std::error::Error>> {
    let cache = match key {
        Some(k) => Some(KeyCache::open(storage, k.as_bytes()).await?),
        None => None,
    };
    read_manifest_cached(storage, path, cache.as_ref()).await
}

/// Like [`read_manifest`], but reuses derived keys across calls (one PBKDF2
/// per salt instead of one per manifest).
pub async fn read_manifest_cached(
    storage: &dyn Storage,
    path: &str,
    cache: Option<&KeyCache>,
) -> Result<String, Box<dyn std::error::Error>> {
    // Bound what a malicious repository can make us read and inflate.
    if let Some(n) = storage.size(path).await?
        && n > MAX_MANIFEST_SIZE
    {
        return Err(Box::new(too_large(path, MAX_MANIFEST_SIZE)));
    }
    let raw = storage.read(path).await?;

    let decrypted = if let Some(cache) = cache {
        // v3 manifests are bound to their storage key (object id).
        match cache.decrypt_object(path, &raw) {
            Ok(data) => data,
            Err(e) => return Err(Box::new(manifest_decrypt_error(path, &raw, cache, &e))),
        }
    } else {
        raw
    };

    // Tries to decompress (Zstd) - check magic number or just try decoding
    // Bounded: a tiny zstd frame of zeros would otherwise inflate to GiBs.
    let decoded = zstd_decode_bounded(&decrypted, MAX_MANIFEST_SIZE, path);
    if let Err(e) = &decoded
        && e.kind() == std::io::ErrorKind::FileTooLarge
    {
        error!("{}", e);
        return Err(decoded.unwrap_err().into());
    }
    if let Ok(decompressed) = decoded {
        return String::from_utf8(decompressed).map_err(|e| {
            debug!("Manifest UTF-8 decode failed after decompression: {:?}", e);

            "Backup metadata is corrupted or unreadable".into()
        });
    }

    // Fallback: it wasn't compressed, try reading as raw string
    String::from_utf8(decrypted).map_err(|_| {
        debug!("Manifest parsing failed: {}", path);

        "Backup metadata is corrupted or unreadable".into()
    })
}

/// Typed, user-facing error for a manifest that cannot be decrypted.
///
/// Classification is by type, never by message text: AEAD tag mismatch →
/// `AuthenticationFailed`, malformed blob → `Corrupt`, v3 blob without a
/// repository key → `Other`.
pub(crate) fn manifest_decrypt_error(
    path: &str,
    raw: &[u8],
    cache: &KeyCache,
    e: &std::io::Error,
) -> RsbError {
    use crate::crypto::CryptoError;
    let crypto = crate::crypto::crypto_error_of(e);
    let kind = match crypto {
        Some(CryptoError::AuthenticationFailed) => CryptoErrorKind::AuthenticationFailed,
        Some(CryptoError::Malformed) => CryptoErrorKind::Corrupt,
        Some(CryptoError::RepoKeyRequired | CryptoError::KdfLimitExceeded) => {
            CryptoErrorKind::Other
        }
        None if e.kind() == std::io::ErrorKind::InvalidData => CryptoErrorKind::Corrupt,
        None => CryptoErrorKind::Other,
    };
    // With a v3 manifest the password was already proven by unlocking the
    // repository key, so a tag mismatch here means tampering/corruption.
    let key_verified = cache.repo_key().is_some() && crate::crypto::is_v3(raw);
    let user_message = match kind {
        CryptoErrorKind::AuthenticationFailed if key_verified => format!(
            "❌ Backup metadata failed authentication\n\n\
             The repository key is correct but {} does not authenticate: it was \
             modified, truncated or moved from another name.\n\n\
             Try: rsb verify --backup /path/to/backup\n\n\
             Technical: {}",
            path, e
        ),
        CryptoErrorKind::AuthenticationFailed => format!(
            "❌ Decryption failed (wrong key)\n\n\
             The backup is encrypted but the key provided is incorrect.\n\
             Use the SAME encryption key that was used when creating this backup.\n\n\
             Technical: {}",
            e
        ),
        CryptoErrorKind::Corrupt => format!(
            "⚠️  Backup metadata may be corrupted\n\n\
             The backup data cannot be read, even with the provided key.\n\
             This may indicate:\n\
             - Incomplete backup copy\n\
             - Corrupted backup files\n\
             - Wrong backup directory\n\n\
             Try: rsb verify --backup /path/to/backup\n\n\
             Technical: {}",
            e
        ),
        CryptoErrorKind::Other => format!(
            "❌ Decryption error\n\n\
             Could not decrypt backup metadata.\n\
             Ensure:\n\
             - The backup is not corrupted\n\
             - The correct encryption key is provided\n\
             - The entire backup folder was copied (including keys/repo.key)\n\n\
             Technical: {}",
            e
        ),
    };
    error!("Decryption error for {}: {}", path, e);
    RsbError::crypto(kind, user_message)
}

/// Sorts snapshot paths newest first: by parsed timestamp, then by name
/// (collision suffixes / unparseable names fall back to lexicographic order).
pub fn sort_snapshots_newest_first(snapshots: &mut [String]) {
    snapshots.sort_by(|a, b| {
        let ka = (parse_snapshot_timestamp(a), collision_suffix(a));
        let kb = (parse_snapshot_timestamp(b), collision_suffix(b));
        kb.cmp(&ka).then_with(|| b.cmp(a))
    });
}

/// `-<n>` suffix added by [`write_manifest`] on name collisions (0 if none).
fn collision_suffix(name: &str) -> u32 {
    let stem = name.rsplit('/').next().unwrap_or(name);
    let stem = stem.strip_suffix(".toml").unwrap_or(stem);
    stem.rfind('Z')
        .and_then(|z| stem[z + 1..].strip_prefix('-'))
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

/// Efficiently finds the most recent snapshot
pub async fn find_latest_snapshot(
    storage: &dyn Storage,
    snapshot_id: Option<&str>,
    key: Option<&str>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    let cache = match key {
        Some(k) => Some(KeyCache::open(storage, k.as_bytes()).await?),
        None => None,
    };
    find_latest_snapshot_cached(storage, snapshot_id, cache.as_ref()).await
}

/// Like [`find_latest_snapshot`], sharing a key cache with the caller.
pub async fn find_latest_snapshot_cached(
    storage: &dyn Storage,
    snapshot_id: Option<&str>,
    cache: Option<&KeyCache>,
) -> Result<(String, String), Box<dyn std::error::Error>> {
    if let Some(id) = snapshot_id {
        let id = validate_snapshot_id(id)?;
        let path = format!("snapshots/{}.toml", id);
        if storage.exists(&path).await? {
            let content = read_manifest_cached(storage, &path, cache).await?;
            return Ok((path, content));
        }
        return Err("Snapshot ID not found".into());
    }

    // Full listing (S3 is paginated) + timestamp-aware sorting
    let mut snapshots = storage.list("snapshots").await?;
    snapshots.retain(|s| s.ends_with(".toml"));

    if snapshots.is_empty() {
        return Err("No snapshots found".into());
    }

    sort_snapshots_newest_first(&mut snapshots);
    let latest = snapshots.into_iter().next().unwrap();

    let content = read_manifest_cached(storage, &latest, cache).await?;
    Ok((latest, content))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::LocalStorage;

    #[test]
    fn parses_all_snapshot_name_formats() {
        let legacy = parse_snapshot_timestamp("snapshots/2026-02-07T103000Z.toml").unwrap();
        assert_eq!(legacy.to_rfc3339(), "2026-02-07T10:30:00+00:00");
        let ms = parse_snapshot_timestamp("2026-02-07T103000.250Z").unwrap();
        assert_eq!(ms.timestamp_subsec_millis(), 250);
        assert!(parse_snapshot_timestamp("2026-02-07T103000.250Z-2.toml").is_some());
        assert!(parse_snapshot_timestamp("2026-02-07T10:30:00Z").is_some());
        assert!(parse_snapshot_timestamp("garbage.toml").is_none());
        // Names produced by the writer round-trip through the parser.
        let now = Utc::now();
        let name = now.format(SNAPSHOT_NAME_FORMAT).to_string();
        let parsed = parse_snapshot_timestamp(&name).unwrap();
        assert_eq!(parsed.timestamp_millis(), now.timestamp_millis());
    }

    #[test]
    fn snapshot_id_validation() {
        assert_eq!(
            validate_snapshot_id("2026-02-07T103000Z").unwrap(),
            "2026-02-07T103000Z"
        );
        assert_eq!(
            validate_snapshot_id("2026-02-07T103000.1Z.toml").unwrap(),
            "2026-02-07T103000.1Z"
        );
        assert!(validate_snapshot_id("../../etc/passwd").is_err());
        assert!(validate_snapshot_id("a/b").is_err());
        assert!(validate_snapshot_id("..").is_err());
        assert!(validate_snapshot_id("").is_err());
        assert!(validate_snapshot_id("x\\y").is_err());
    }

    #[tokio::test]
    async fn manifest_zstd_bomb_is_refused() {
        let storage = crate::storage::MemoryStorage::new();
        // Valid unencrypted manifest framing: a zstd frame inflating past
        // MAX_MANIFEST_SIZE from a few KiB of input.
        let bomb = zstd::stream::encode_all(
            std::io::Read::take(std::io::repeat(b' '), MAX_MANIFEST_SIZE + 1),
            1,
        )
        .unwrap();
        assert!(bomb.len() < 1024 * 1024);
        storage
            .write("snapshots/2026-01-01T000000Z.toml", &bomb)
            .await
            .unwrap();
        let err = find_latest_snapshot(&storage, None, None)
            .await
            .unwrap_err();
        let io = err.downcast_ref::<std::io::Error>().expect("io error");
        assert_eq!(io.kind(), std::io::ErrorKind::FileTooLarge);
    }

    #[tokio::test]
    async fn oversized_manifest_object_is_refused_before_reading() {
        struct Huge;
        #[async_trait::async_trait]
        impl Storage for Huge {
            async fn write(&self, _: &str, _: &[u8]) -> std::io::Result<()> {
                Ok(())
            }
            async fn read(&self, _: &str) -> std::io::Result<Vec<u8>> {
                panic!("must not read an oversized manifest");
            }
            async fn exists(&self, _: &str) -> std::io::Result<bool> {
                Ok(true)
            }
            async fn list(&self, _: &str) -> std::io::Result<Vec<String>> {
                Ok(vec!["snapshots/2026-01-01T000000Z.toml".into()])
            }
            async fn delete(&self, _: &str) -> std::io::Result<()> {
                Ok(())
            }
            async fn size(&self, _: &str) -> std::io::Result<Option<u64>> {
                Ok(Some(MAX_MANIFEST_SIZE + 1))
            }
        }
        assert!(find_latest_snapshot(&Huge, None, None).await.is_err());
    }

    #[tokio::test]
    async fn manifests_written_in_same_instant_do_not_collide() {
        let dir = tempfile::tempdir().unwrap();
        let storage = LocalStorage::new(dir.path().to_str().unwrap());
        let m = HashMap::new();
        let a = write_manifest(&storage, &m, None, false).await.unwrap();
        let b = write_manifest(&storage, &m, None, false).await.unwrap();
        assert_ne!(a, b);
        assert_eq!(storage.list("snapshots").await.unwrap().len(), 2);
        let (latest, _) = find_latest_snapshot(&storage, None, None).await.unwrap();
        assert_eq!(latest, b, "the most recently written manifest must win");
    }

    #[test]
    fn manifest_key_uses_forward_slashes() {
        let rel: PathBuf = ["sub", "dir", "b.txt"].iter().collect();
        assert_eq!(manifest_key(&rel), PathBuf::from("sub/dir/b.txt"));
        assert_eq!(
            manifest_key(std::path::Path::new("a.txt")),
            PathBuf::from("a.txt")
        );
    }
}
