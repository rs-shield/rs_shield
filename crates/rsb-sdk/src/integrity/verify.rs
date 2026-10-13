use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use tracing::{info, warn};

use crate::core::manifest::find_latest_snapshot_cached;
use crate::core::storage_backend::storage_or_from_config;
use crate::core::types::{FileMetadata, ProgressCallback};
use crate::crypto::{KeyCache, hash_file_content};
use crate::integrity::checksum::checksum;
use crate::options::VerifyOptions;
use crate::report::ReportData;
use crate::storage::Storage;
use crate::{CancellationToken, Config};

/// Verifies every object referenced by a snapshot (latest, or
/// `options.snapshot_id`).
///
/// - `fast`: only checks presence, stored size and stored hash (no
///   download-and-decode); cannot prove an object is corrupt.
/// - full (default): reads each referenced object once, decrypts and
///   decompresses it and checks the content hash, which detects objects
///   truncated or damaged by older versions even when their size looked
///   plausible.
/// - `repair`: implies a full check; objects whose content is proven
///   corrupt are **deleted** so that the next backup re-uploads them (the
///   backup re-checks that objects it dedups against still exist). Missing
///   objects are reported; the next backup re-uploads them if the source
///   file is still present. Without `repair` the repository is never
///   modified.
///
/// The key is `options.encryption_key`, falling back to
/// `config.encryption_key`.
pub async fn perform_verify(
    config: &Config,
    options: VerifyOptions,
) -> crate::error::Result<ReportData> {
    perform_verify_detailed(config, options)
        .await
        .map(|run| run.report)
}

/// Result of [`perform_verify_detailed`]: the report plus the per-object
/// outcome (missing, corrupt and deleted objects).
#[derive(Debug)]
pub struct VerifyRun {
    pub report: ReportData,
    pub objects: VerifyObjectsOutcome,
}

impl VerifyRun {
    /// Objects that a repair would delete: those proven corrupt. Without
    /// `repair` this is the preview of `verify --repair` (a repair also
    /// keeps objects that failed to decrypt when no encrypted object could
    /// be decrypted at all, since that rather means a wrong key).
    pub fn repair_candidates(&self) -> Vec<&CorruptObject> {
        self.objects.corrupt.iter().collect()
    }
}

/// [`perform_verify`], also returning the per-object outcome, e.g. to show
/// which objects a repair would delete (run it without `repair` first).
pub async fn perform_verify_detailed(
    config: &Config,
    options: VerifyOptions,
) -> crate::error::Result<VerifyRun> {
    let VerifyOptions {
        snapshot_id,
        quiet,
        fast,
        repair,
        encryption_key,
        on_progress,
        cancellation_token,
        storage,
    } = options;
    let snapshot_id = snapshot_id.as_deref();
    let start_time = Instant::now();
    let storage = storage_or_from_config(storage, config).await?;
    let password = encryption_key
        .as_ref()
        .map(|k| k.as_str())
        .or(config.encryption_key.as_deref());
    // Unlocks keys/repo.key (if any) before touching objects: a wrong
    // password fails here with a typed authentication error.
    let key_cache = match password {
        Some(k) => Some(KeyCache::open(&*storage, k.as_bytes()).await?),
        None => None,
    };

    // Reading the manifest with the key also proves the key is correct, so
    // a later decryption failure on an object means the object is damaged.
    let (path, content) =
        find_latest_snapshot_cached(&*storage, snapshot_id, key_cache.as_ref()).await?;
    info!("Verifying snapshot: {}", path);
    let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&content)?;

    let fast = fast && !repair;
    let outcome = verify_manifest_objects(
        &*storage,
        &manifest,
        key_cache.as_ref(),
        VerifyObjectsOptions { fast, repair },
        on_progress,
        cancellation_token,
    )
    .await?;

    let status = if outcome.errors.is_empty() {
        "Success"
    } else if repair && !outcome.deleted.is_empty() {
        "Repaired: run a backup to re-upload deleted objects"
    } else {
        "Failure with errors"
    }
    .to_string();

    if !quiet {
        info!("Verification completed.");
        info!("Total files: {}", manifest.len());
        info!("✅ Files successfully verified: {}", outcome.files_ok);
        info!(
            "Objects checked: {} | missing: {} | corrupt: {} | manifest flag mismatches: {}",
            outcome.objects_checked,
            outcome.missing.len(),
            outcome.corrupt.len(),
            outcome.flag_mismatches
        );
        if repair {
            info!("Corrupt objects deleted: {}", outcome.deleted.len());
        }
    }

    let mode = match (fast, repair) {
        (true, _) => "Fast (Lite)",
        (false, false) => "Full",
        (false, true) => "Full + repair",
    };

    let report = ReportData {
        operation: "Verify".to_string(),
        profile_path: "".to_string(),
        timestamp: chrono::Local::now().to_rfc3339(),
        duration: start_time.elapsed(),
        mode: Some(mode.to_string()),
        files_processed: outcome.files_ok,
        files_skipped: 0,
        files_with_errors: outcome.files_with_errors,
        total_files: manifest.len(),
        errors: outcome.errors.clone(),
        status,
    };
    Ok(VerifyRun {
        report,
        objects: outcome,
    })
}

/// Options for [`verify_manifest_objects`].
#[derive(Debug, Clone, Copy, Default)]
pub struct VerifyObjectsOptions {
    /// Presence + stored size/hash only (ignored when `repair` is set).
    pub fast: bool,
    /// Delete objects whose content is proven corrupt.
    pub repair: bool,
}

/// A referenced object whose content does not match its content hash.
#[derive(Debug, Clone)]
pub struct CorruptObject {
    /// Storage key, e.g. `data/enc/<hash>`.
    pub path: String,
    pub reason: String,
    /// Snapshot files that reference the object.
    pub files: Vec<PathBuf>,
}

/// Result of [`verify_manifest_objects`].
#[derive(Debug, Clone, Default)]
pub struct VerifyObjectsOutcome {
    pub objects_checked: usize,
    pub files_ok: usize,
    pub files_with_errors: usize,
    /// Storage keys of referenced objects that do not exist.
    pub missing: Vec<String>,
    pub corrupt: Vec<CorruptObject>,
    /// Corrupt objects deleted by repair.
    pub deleted: Vec<String>,
    /// Intact objects whose manifest entry has the wrong `compressed` flag
    /// (written by older versions); restore of those files fails.
    pub flag_mismatches: usize,
    /// Human-readable error lines (one per problem).
    pub errors: Vec<String>,
}

/// One reference from a manifest entry to an object.
struct ObjectRef {
    file: PathBuf,
    compressed: bool,
    expected_size: Option<u64>,
    expected_stored_hash: Option<String>,
}

/// How an object's content was found to be valid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Decoded {
    Plain,
    Zstd,
}

enum ObjectCheck {
    Ok(Option<Decoded>),
    Corrupt {
        reason: String,
        decrypt_failed: bool,
    },
    Error(String),
}

fn content_matches(data: &[u8], expected: &str) -> bool {
    hash_file_content(data).is_ok_and(|h| h == expected)
}

/// Reads one object and checks that it decodes to `content_hash`, under
/// either interpretation (plain or zstd), so that the result says which
/// one the object really is.
async fn check_object_content(
    storage: &dyn Storage,
    path: &str,
    content_hash: &str,
    encrypted: bool,
    max_plain: u64,
    key: Option<&KeyCache>,
) -> ObjectCheck {
    let raw = match storage.read(path).await {
        Ok(r) => r,
        Err(e) => return ObjectCheck::Error(format!("Failed to read {}: {}", path, e)),
    };
    let payload = if encrypted {
        let Some(key) = key else {
            return ObjectCheck::Error(format!(
                "{} is encrypted but no encryption key was provided",
                path
            ));
        };
        // v3 blobs are bound to their object id (the content hash).
        match key.decrypt_object(content_hash, &raw) {
            Ok(p) => p,
            Err(e) => {
                return ObjectCheck::Corrupt {
                    reason: format!("decryption failed ({} bytes stored): {}", raw.len(), e),
                    decrypt_failed: true,
                };
            }
        }
    } else {
        raw
    };

    // Streamed hash, bounded by the size the manifest allows for this
    // object: a zstd bomb is reported instead of exhausting memory/CPU.
    match crate::core::limits::zstd_hash_bounded(&payload, max_plain, path) {
        Ok((h, _)) if h == content_hash => return ObjectCheck::Ok(Some(Decoded::Zstd)),
        // A plain object may itself be a zstd frame of something larger.
        Err(e)
            if e.kind() == std::io::ErrorKind::FileTooLarge
                && !content_matches(&payload, content_hash) =>
        {
            return ObjectCheck::Corrupt {
                reason: e.to_string(),
                decrypt_failed: false,
            };
        }
        _ => {}
    }
    if content_matches(&payload, content_hash) {
        return ObjectCheck::Ok(Some(Decoded::Plain));
    }
    ObjectCheck::Corrupt {
        reason: format!(
            "content hash mismatch ({} bytes after decryption; truncated or damaged)",
            payload.len()
        ),
        decrypt_failed: false,
    }
}

/// Verifies the objects referenced by `manifest`, each object once.
pub async fn verify_manifest_objects(
    storage: &dyn Storage,
    manifest: &HashMap<PathBuf, FileMetadata>,
    key: Option<&KeyCache>,
    options: VerifyObjectsOptions,
    on_progress: Option<ProgressCallback>,
    cancellation_token: Option<CancellationToken>,
) -> Result<VerifyObjectsOutcome, Box<dyn std::error::Error>> {
    let fast = options.fast && !options.repair;

    // object path -> (content hash, encrypted, references)
    let mut objects: BTreeMap<String, (String, bool, Vec<ObjectRef>)> = BTreeMap::new();
    // object path -> largest decoded size any reference allows
    let mut max_plain: HashMap<String, u64> = HashMap::new();
    for (file, meta) in manifest {
        let kind = if meta.encrypted { "enc" } else { "clear" };
        let items: Vec<(String, Option<u64>, Option<String>)> = match &meta.chunks {
            Some(chunks) => chunks
                .iter()
                .map(|c| (c.hash.clone(), Some(c.stored_size), c.stored_hash.clone()))
                .collect(),
            None => vec![(
                meta.hash.clone(),
                meta.stored_size,
                meta.stored_hash.clone(),
            )],
        };
        for (hash, size, stored_hash) in items {
            let path = format!("data/{}/{}", kind, hash);
            let limit = max_plain.entry(path.clone()).or_insert(0);
            *limit = (*limit).max(meta.max_object_plain_size());
            objects
                .entry(path)
                .or_insert_with(|| (hash, meta.encrypted, Vec::new()))
                .2
                .push(ObjectRef {
                    file: file.clone(),
                    compressed: meta.compressed,
                    expected_size: size,
                    expected_stored_hash: stored_hash,
                });
        }
    }

    let mut out = VerifyObjectsOutcome::default();
    let mut bad_files: HashSet<PathBuf> = HashSet::new();
    let mut decrypted_any = false;
    let mut delete_candidates: Vec<(String, bool)> = Vec::new();
    let total = objects.len();

    for (i, (path, (content_hash, encrypted, refs))) in objects.iter().enumerate() {
        if cancellation_token
            .as_ref()
            .is_some_and(|t| t.is_cancelled())
        {
            info!("⏹️ Verification cancelled by the user");
            out.errors.push("Verification cancelled".to_string());
            break;
        }
        if let Some(cb) = &on_progress {
            cb(i, total, format!("Verifying: {}", refs[0].file.display()));
        }
        out.objects_checked += 1;
        let files_list = || {
            let mut v: Vec<String> = refs.iter().map(|r| r.file.display().to_string()).collect();
            v.sort();
            v.dedup();
            v.join(", ")
        };

        match storage.exists(path).await {
            Ok(true) => {}
            Ok(false) => {
                out.errors.push(format!(
                    "Missing object {} (used by: {})",
                    path,
                    files_list()
                ));
                out.missing.push(path.clone());
                bad_files.extend(refs.iter().map(|r| r.file.clone()));
                continue;
            }
            Err(e) => {
                out.errors.push(format!("Failed to access {}: {}", path, e));
                bad_files.extend(refs.iter().map(|r| r.file.clone()));
                continue;
            }
        }

        if fast {
            let data = match storage.read(path).await {
                Ok(d) => d,
                Err(e) => {
                    out.errors.push(format!("Failed to read {}: {}", path, e));
                    bad_files.extend(refs.iter().map(|r| r.file.clone()));
                    continue;
                }
            };
            for r in refs {
                if let Err(e) = checksum(
                    &data,
                    r.expected_size.filter(|s| *s != 0),
                    r.expected_stored_hash.as_ref(),
                ) {
                    out.errors
                        .push(format!("{} for {} ({})", e, r.file.display(), path));
                    bad_files.insert(r.file.clone());
                }
            }
            continue;
        }

        let limit = max_plain.get(path).copied().unwrap_or(0);
        match check_object_content(storage, path, content_hash, *encrypted, limit, key).await {
            ObjectCheck::Ok(decoded) => {
                if *encrypted {
                    decrypted_any = true;
                }
                for r in refs {
                    let flag_ok = match decoded {
                        Some(Decoded::Zstd) => r.compressed,
                        Some(Decoded::Plain) => !r.compressed,
                        None => true,
                    };
                    if !flag_ok {
                        out.flag_mismatches += 1;
                        out.errors.push(format!(
                            "Manifest entry for {} says compressed={} but object {} is {}; \
                             the object is intact but this snapshot entry cannot be restored",
                            r.file.display(),
                            r.compressed,
                            path,
                            if r.compressed {
                                "not compressed"
                            } else {
                                "compressed"
                            }
                        ));
                        bad_files.insert(r.file.clone());
                    }
                }
            }
            ObjectCheck::Corrupt {
                reason,
                decrypt_failed,
            } => {
                out.errors.push(format!(
                    "Corrupt object {}: {} (used by: {})",
                    path,
                    reason,
                    files_list()
                ));
                let mut files: Vec<PathBuf> = refs.iter().map(|r| r.file.clone()).collect();
                files.sort();
                files.dedup();
                bad_files.extend(files.iter().cloned());
                out.corrupt.push(CorruptObject {
                    path: path.clone(),
                    reason,
                    files,
                });
                delete_candidates.push((path.clone(), decrypt_failed));
            }
            ObjectCheck::Error(e) => {
                out.errors.push(e);
                bad_files.extend(refs.iter().map(|r| r.file.clone()));
            }
        }
    }

    if options.repair {
        // If no encrypted object could be decrypted at all, decryption
        // failures more likely mean a wrong key than corrupt data: never
        // delete on that evidence alone.
        let encrypted_failures = delete_candidates.iter().filter(|(_, d)| *d).count();
        let distrust_decrypt = encrypted_failures > 0 && !decrypted_any;
        if distrust_decrypt {
            warn!(
                "{} objects failed to decrypt and none succeeded; not deleting them (wrong key?)",
                encrypted_failures
            );
            out.errors.push(format!(
                "Repair skipped {} objects that failed decryption: no encrypted object could be \
                 decrypted, check the encryption key",
                encrypted_failures
            ));
        }
        if delete_candidates
            .iter()
            .any(|(_, d)| !(*d && distrust_decrypt))
            && let Err(e) = crate::core::dedup_index::bump_repo_epoch(storage).await
        {
            // Without the epoch change, local dedup indexes could keep
            // trusting the objects about to be deleted: do not delete.
            out.errors.push(format!(
                "Repair skipped: could not update the repository epoch: {}",
                e
            ));
            delete_candidates.clear();
        }
        for (path, decrypt_failed) in delete_candidates {
            if decrypt_failed && distrust_decrypt {
                continue;
            }
            match storage.delete(&path).await {
                Ok(()) => {
                    warn!("Deleted corrupt object {}", path);
                    out.errors.push(format!(
                        "Deleted corrupt object {} (the next backup will re-upload it)",
                        path
                    ));
                    out.deleted.push(path);
                }
                Err(e) => out
                    .errors
                    .push(format!("Failed to delete corrupt object {}: {}", path, e)),
            }
        }
    }

    out.files_with_errors = bad_files.len();
    out.files_ok = manifest.len().saturating_sub(bad_files.len());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::MemoryStorage;

    #[tokio::test]
    async fn zstd_bomb_object_is_reported_not_inflated() {
        let storage = MemoryStorage::new();
        let content = b"0123456789";
        let hash = hash_file_content(content).unwrap();
        // 256 MiB of zeros in a few KiB, stored where a 10-byte file belongs.
        let bomb = zstd::stream::encode_all(std::io::Read::take(std::io::repeat(0), 256 << 20), 1)
            .unwrap();
        storage
            .write(&format!("data/clear/{}", hash), &bomb)
            .await
            .unwrap();
        let mut manifest = HashMap::new();
        manifest.insert(
            PathBuf::from("f.txt"),
            FileMetadata::new(hash.clone(), false, true).with_size(content.len() as u64),
        );
        let out = verify_manifest_objects(
            &storage,
            &manifest,
            None,
            VerifyObjectsOptions::default(),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(out.corrupt.len(), 1, "{:?}", out.errors);
        assert!(
            out.corrupt[0].reason.contains("exceeds"),
            "{:?}",
            out.corrupt
        );
    }
}
