// Repository key lifecycle end to end: migration of v1/v2 repositories to
// v3, key export/import (file and paper form) and key rotation, including
// interrupted runs (a storage wrapper that fails writes after N objects)
// and restore/verify/prune at every intermediate state.

use async_trait::async_trait;
use rsb_sdk::backup::perform_backup;
use rsb_sdk::config::Config;
use rsb_sdk::core::manifest::write_manifest;
use rsb_sdk::core::perform_restore;
use rsb_sdk::core::prune_with_storage;
use rsb_sdk::core::types::FileMetadata;
use rsb_sdk::crypto::keyfile::{self, KdfParams, REPO_KEY_BACKUP_PREFIX, REPO_KEY_PATH};
use rsb_sdk::crypto::lifecycle::{self, BlobKind};
use rsb_sdk::crypto::{BLOB_MAGIC_V3, EncryptionKey, hash_file_content};
use rsb_sdk::integrity::perform_verify;
use rsb_sdk::storage::{BlobReader, MemoryStorage, Storage};
use rsb_sdk::{BackupOptions, RestoreOptions, RsbError, VerifyOptions};
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

const PW: &str = "correct horse battery staple";

/// MemoryStorage that fails writes under `prefix` once `budget` writes have
/// succeeded (budget < 0: never fails). Simulates a crash in the middle of
/// a migration or rotation.
struct FailingStorage {
    inner: Arc<MemoryStorage>,
    prefix: std::sync::Mutex<String>,
    budget: AtomicI64,
}

impl FailingStorage {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(MemoryStorage::new()),
            prefix: std::sync::Mutex::new(String::new()),
            budget: AtomicI64::new(-1),
        })
    }

    fn fail_after(&self, prefix: &str, writes: i64) {
        *self.prefix.lock().unwrap() = prefix.to_string();
        self.budget.store(writes, Ordering::SeqCst);
    }

    fn heal(&self) {
        self.budget.store(-1, Ordering::SeqCst);
    }

    fn check(&self, path: &str) -> io::Result<()> {
        if !path.starts_with(self.prefix.lock().unwrap().as_str()) {
            return Ok(());
        }
        let b = self.budget.load(Ordering::SeqCst);
        if b < 0 {
            return Ok(());
        }
        if b == 0 {
            return Err(io::Error::other(format!(
                "injected failure writing {}",
                path
            )));
        }
        self.budget.fetch_sub(1, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait]
impl Storage for FailingStorage {
    async fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.check(path)?;
        self.inner.write(path, data).await
    }
    async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        self.inner.read(path).await
    }
    async fn read_stream(&self, path: &str) -> io::Result<BlobReader> {
        self.inner.read_stream(path).await
    }
    async fn write_from_reader(&self, path: &str, reader: BlobReader) -> io::Result<u64> {
        self.check(path)?;
        self.inner.write_from_reader(path, reader).await
    }
    async fn write_file(&self, path: &str, src: &Path) -> io::Result<()> {
        self.check(path)?;
        self.inner.write_file(path, src).await
    }
    async fn exists(&self, path: &str) -> io::Result<bool> {
        self.inner.exists(path).await
    }
    async fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        self.inner.list(prefix).await
    }
    async fn delete(&self, path: &str) -> io::Result<()> {
        self.inner.delete(path).await
    }
    async fn size(&self, path: &str) -> io::Result<Option<u64>> {
        self.inner.size(path).await
    }
}

fn config(src: &Path) -> Config {
    Config {
        source_path: src.to_string_lossy().into_owned(),
        destination_path: "/nonexistent/must-not-be-used".into(),
        exclude_patterns: vec![],
        compression_level: Some(3),
        max_threads: Some(2),
        ..Default::default()
    }
}

async fn backup(cfg: &Config, storage: &Arc<dyn Storage>, pw: &str) {
    let r = perform_backup(
        cfg,
        BackupOptions {
            mode: Some("incremental".into()),
            max_threads: Some(2),
            storage: Some(storage.clone()),
            ..Default::default()
        }
        .with_encryption_key(Some(pw)),
    )
    .await
    .unwrap();
    assert_eq!(r.files_with_errors, 0, "{:?}", r.errors);
}

async fn restore_ok(cfg: &Config, storage: &Arc<dyn Storage>, pw: &str, files: &[(&str, &[u8])]) {
    let out = tempfile::tempdir().unwrap();
    let r = perform_restore(
        cfg,
        RestoreOptions {
            force: true,
            storage: Some(storage.clone()),
            ..RestoreOptions::new(out.path())
        }
        .with_encryption_key(Some(pw)),
    )
    .await
    .unwrap();
    assert_eq!(r.files_with_errors, 0, "{:?}", r.errors);
    for (name, data) in files {
        assert_eq!(
            &std::fs::read(out.path().join(name)).unwrap()[..],
            *data,
            "{}",
            name
        );
    }
}

async fn verify_errors(
    cfg: &Config,
    storage: &Arc<dyn Storage>,
    pw: &str,
    fast: bool,
) -> Vec<String> {
    perform_verify(
        cfg,
        VerifyOptions {
            quiet: true,
            fast,
            storage: Some(storage.clone()),
            ..Default::default()
        }
        .with_encryption_key(Some(pw)),
    )
    .await
    .unwrap()
    .errors
}

/// Restore works, a full verify passes, a prune garbage collection (no
/// snapshot deleted) succeeds, and restore still works afterwards.
async fn all_operations_work(
    cfg: &Config,
    storage: &Arc<dyn Storage>,
    pw: &str,
    files: &[(&str, &[u8])],
) {
    restore_ok(cfg, storage, pw, files).await;
    let errs = verify_errors(cfg, storage, pw, false).await;
    assert!(errs.is_empty(), "{:?}", errs);
    prune_with_storage(storage.clone(), Some(pw), &[])
        .await
        .unwrap();
    restore_ok(cfg, storage, pw, files).await;
}

async fn list(storage: &dyn Storage, prefix: &str) -> Vec<String> {
    storage.list(prefix).await.unwrap_or_default()
}

fn write_src(src: &Path, files: &[(&str, &[u8])]) {
    for (name, data) in files {
        let p = src.join(name);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, data).unwrap();
    }
}

fn noise(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

/// v1 layout: salt(16) | nonce(12) | ct | tag, PBKDF2-HMAC-SHA256 600k, no AAD.
fn legacy_v1_encrypt(data: &[u8], pw: &[u8]) -> Vec<u8> {
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes256Gcm, Nonce};
    let salt = [3u8; 16];
    let nonce = [4u8; 12];
    let mut key = [0u8; 32];
    pbkdf2::pbkdf2_hmac::<sha2::Sha256>(pw, &salt, 600_000, &mut key);
    let ct = Aes256Gcm::new_from_slice(&key)
        .unwrap()
        .encrypt(Nonce::from_slice(&nonce), data)
        .unwrap();
    [&salt[..], &nonce[..], &ct[..]].concat()
}

/// Pre-v3 repository: a v2 object (compressed), a v1 object and a v2
/// manifest referencing both, plus an unreferenced v2 object and an
/// unreferenced blob that decrypts with nothing.
async fn legacy_repo(storage: &dyn Storage, a: &[u8], b: &[u8]) -> (String, String) {
    let key = EncryptionKey::new(PW.as_bytes()).unwrap();
    let mut manifest = HashMap::new();
    let ha = hash_file_content(a).unwrap();
    let v2 = key
        .encrypt(&zstd::stream::encode_all(a, 3).unwrap())
        .unwrap();
    storage
        .write(&format!("data/enc/{}", ha), &v2)
        .await
        .unwrap();
    manifest.insert(
        PathBuf::from("a.txt"),
        FileMetadata {
            stored_size: Some(v2.len() as u64),
            stored_hash: Some(hash_file_content(&v2).unwrap()),
            ..FileMetadata::new(ha.clone(), true, true)
        },
    );
    let hb = hash_file_content(b).unwrap();
    let v1 = legacy_v1_encrypt(b, PW.as_bytes());
    storage
        .write(&format!("data/enc/{}", hb), &v1)
        .await
        .unwrap();
    manifest.insert(
        PathBuf::from("b.txt"),
        FileMetadata {
            stored_size: Some(v1.len() as u64),
            stored_hash: Some(hash_file_content(&v1).unwrap()),
            ..FileMetadata::new(hb.clone(), true, false)
        },
    );
    write_manifest(storage, &manifest, Some(PW), false)
        .await
        .unwrap();
    // Unreferenced: one that decrypts, one garbage blob.
    let orphan = b"orphaned legacy content".to_vec();
    storage
        .write(
            &format!("data/enc/{}", hash_file_content(&orphan).unwrap()),
            &key.encrypt(&orphan).unwrap(),
        )
        .await
        .unwrap();
    storage
        .write(&format!("data/enc/{}", "e".repeat(64)), &noise(200, 99))
        .await
        .unwrap();
    (ha, hb)
}

async fn kinds(storage: &dyn Storage) -> Vec<(String, BlobKind)> {
    let mut out = Vec::new();
    for o in list(storage, "data/enc").await {
        out.push((o.clone(), lifecycle::blob_kind(storage, &o).await.unwrap()));
    }
    out
}

fn no_backup_copies(keys: &[String]) -> bool {
    keys.iter().all(|k| !k.starts_with(REPO_KEY_BACKUP_PREFIX))
}

// ---------------------------------------------------------------------------
// Migration

struct MixedRepo {
    cfg: Config,
    _src: tempfile::TempDir,
    files: Vec<(&'static str, Vec<u8>)>,
}

impl MixedRepo {
    fn files(&self) -> Vec<(&str, &[u8])> {
        self.files.iter().map(|(n, d)| (*n, &d[..])).collect()
    }
}

/// v1 + v2 objects and a v2 manifest, then a v3 backup on top (creates the
/// key file; a.txt/b.txt dedup onto the v2/v1 objects, c.txt is v3).
async fn mixed_repo(storage: &Arc<dyn Storage>) -> MixedRepo {
    let a = b"alpha alpha alpha alpha alpha".repeat(50);
    let b = b"bravo (v1 object)".to_vec();
    let c = noise(3 * 1024 * 1024 + 7, 5); // multi-chunk v3 blob
    legacy_repo(&**storage, &a, &b).await;
    let src = tempfile::tempdir().unwrap();
    let cfg = config(src.path());
    let files = vec![("a.txt", a), ("b.txt", b), ("c.bin", c)];
    let r = MixedRepo {
        cfg,
        _src: src,
        files,
    };
    write_src(r._src.path(), &r.files());
    backup(&r.cfg, storage, PW).await;
    r
}

async fn assert_all_v3(storage: &dyn Storage, except: &[String]) {
    for (o, k) in kinds(storage).await {
        if except.contains(&o) {
            continue;
        }
        assert!(matches!(k, BlobKind::V3(Some(_))), "{} is {:?}", o, k);
    }
    for s in list(storage, "snapshots").await {
        assert!(
            storage.read(&s).await.unwrap().starts_with(BLOB_MAGIC_V3),
            "{}",
            s
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn migration_rewrites_v1_v2_as_v3_and_unblocks_password_change() {
    let mem = FailingStorage::new();
    let storage: Arc<dyn Storage> = mem.clone();
    let repo = mixed_repo(&storage).await;
    let summary = lifecycle::legacy_summary(&*storage).await.unwrap();
    assert_eq!(summary.objects, 4, "{:?}", summary); // v1, v2, orphan, garbage
    assert_eq!(summary.manifests, 1);

    // change-password is refused while v1/v2 data remains.
    let e = keyfile::change_password(
        storage.clone(),
        PW.as_bytes(),
        b"pw2",
        Some(KdfParams::MIN_CREATE),
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("rsb key migrate"), "{e}");
    assert!(
        keyfile::load_repo_key(&*storage, PW.as_bytes())
            .await
            .unwrap()
            .is_some()
    );

    // Wrong password: nothing happens.
    assert!(
        lifecycle::migrate_legacy_objects(storage.clone(), b"wrong", None, None)
            .await
            .unwrap_err()
            .is_authentication_failure()
    );

    let rep = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert_eq!(rep.objects_migrated, 3, "{:?}", rep);
    assert_eq!(rep.unreferenced_legacy.len(), 2, "{:?}", rep);
    let garbage = format!("data/enc/{}", "e".repeat(64));
    assert_eq!(rep.unreadable_legacy, vec![garbage.clone()]);
    assert_eq!(rep.manifests_rewritten, 2, "{:?}", rep);
    assert!(rep.objects_verified >= 3);
    assert!(!rep.legacy_secret_removed); // this version never stored it
    assert_all_v3(&*storage, std::slice::from_ref(&garbage)).await;
    // The garbage blob is reported, not deleted.
    assert!(storage.exists(&garbage).await.unwrap());

    let raw = storage.read(REPO_KEY_PATH).await.unwrap();
    assert!(!String::from_utf8(raw).unwrap().contains("legacy_secret"));
    assert!(no_backup_copies(&mem.inner.keys()));

    // Fast verify agrees too: manifests carry the new stored sizes/hashes.
    assert!(
        verify_errors(&repo.cfg, &storage, PW, true)
            .await
            .is_empty()
    );
    all_operations_work(&repo.cfg, &storage, PW, &repo.files()).await;

    // Idempotent; and the password can now be changed.
    let again = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert_eq!(again.objects_migrated, 0);
    assert_eq!(again.manifests_rewritten, 0);
    keyfile::change_password(
        storage.clone(),
        PW.as_bytes(),
        b"pw2",
        Some(KdfParams::MIN_CREATE),
    )
    .await
    .unwrap();
    all_operations_work(&repo.cfg, &storage, "pw2", &repo.files()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_migration_resumes_and_repo_stays_usable() {
    let mem = FailingStorage::new();
    let storage: Arc<dyn Storage> = mem.clone();
    let repo = mixed_repo(&storage).await;

    // Crash after one re-encrypted object.
    mem.fail_after("data/enc/", 1);
    let e = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("injected"), "{e}");
    mem.heal();
    let n_v3 = kinds(&*storage)
        .await
        .iter()
        .filter(|(_, k)| matches!(k, BlobKind::V3(_)))
        .count();
    assert_eq!(n_v3, 2, "c.bin + one migrated object");
    // Half-migrated: every operation works (mixed v1/v2/v3 objects).
    restore_ok(&repo.cfg, &storage, PW, &repo.files()).await;
    assert!(
        verify_errors(&repo.cfg, &storage, PW, false)
            .await
            .is_empty()
    );

    // Crash again, this time while rewriting manifests.
    mem.fail_after("snapshots/", 0);
    let e = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("injected"), "{e}");
    mem.heal();
    restore_ok(&repo.cfg, &storage, PW, &repo.files()).await;
    assert!(
        verify_errors(&repo.cfg, &storage, PW, false)
            .await
            .is_empty()
    );
    // A backup between two runs is fine too.
    backup(&repo.cfg, &storage, PW).await;

    let rep = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert_eq!(rep.objects_migrated, 0, "{:?}", rep);
    assert!(rep.manifests_rewritten >= 1, "{:?}", rep);
    let garbage = format!("data/enc/{}", "e".repeat(64));
    assert_all_v3(&*storage, &[garbage]).await;
    assert!(
        verify_errors(&repo.cfg, &storage, PW, true)
            .await
            .is_empty()
    );
    all_operations_work(&repo.cfg, &storage, PW, &repo.files()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn migration_cancellation_leaves_a_readable_repo() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let repo = mixed_repo(&storage).await;
    let token = rsb_sdk::CancellationToken::new();
    token.cancel();
    let e = lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, Some(token))
        .await
        .unwrap_err();
    assert!(matches!(e, RsbError::Cancelled), "{e:?}");
    all_operations_work(&repo.cfg, &storage, PW, &repo.files()).await;
}

// ---------------------------------------------------------------------------
// Export / import

async fn fresh_repo(storage: &Arc<dyn Storage>) -> MixedRepo {
    let src = tempfile::tempdir().unwrap();
    let cfg = config(src.path());
    let files = vec![
        ("one.txt", b"first file, first file".to_vec()),
        ("big.bin", noise(2 * 1024 * 1024 + 3, 11)),
        ("empty", Vec::new()),
    ];
    let r = MixedRepo {
        cfg,
        _src: src,
        files,
    };
    write_src(r._src.path(), &r.files());
    backup(&r.cfg, storage, PW).await;
    r
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_import_roundtrip_file_and_paper() {
    let mem = Arc::new(MemoryStorage::new());
    let storage: Arc<dyn Storage> = mem.clone();
    let repo = fresh_repo(&storage).await;

    // Export needs the right password and returns the stored file as is.
    assert!(
        lifecycle::export_key_file(&*storage, b"wrong")
            .await
            .unwrap_err()
            .is_authentication_failure()
    );
    let exported = lifecycle::export_key_file(&*storage, PW.as_bytes())
        .await
        .unwrap();
    assert_eq!(exported, storage.read(REPO_KEY_PATH).await.unwrap());

    // Lose the key file: the repository is unreadable...
    storage.delete(REPO_KEY_PATH).await.unwrap();
    let out = tempfile::tempdir().unwrap();
    assert!(
        perform_restore(
            &repo.cfg,
            RestoreOptions {
                force: true,
                storage: Some(storage.clone()),
                ..RestoreOptions::new(out.path())
            }
            .with_encryption_key(Some(PW)),
        )
        .await
        .is_err()
    );
    // ...until the export is imported (wrong password refused first).
    assert!(
        lifecycle::import_key_file(storage.clone(), &exported, b"wrong", false)
            .await
            .unwrap_err()
            .is_authentication_failure()
    );
    assert!(!storage.exists(REPO_KEY_PATH).await.unwrap());
    let o = lifecycle::import_key_file(storage.clone(), &exported, PW.as_bytes(), false)
        .await
        .unwrap();
    assert!(!o.unchanged && o.replaced_backup.is_none(), "{:?}", o);
    assert!(o.matched_by.contains("snapshots/"), "{:?}", o);
    all_operations_work(&repo.cfg, &storage, PW, &repo.files()).await;
    // Importing the same file again is a no-op.
    let o = lifecycle::import_key_file(storage.clone(), &exported, PW.as_bytes(), false)
        .await
        .unwrap();
    assert!(o.unchanged);

    // Paper form: round trip, and a typo is caught with its line number.
    let file = keyfile::RepoKeyFile::from_bytes(&exported).unwrap();
    let paper = rsb_sdk::crypto::paper::encode(&file.to_compact_bytes().unwrap());
    assert!(rsb_sdk::crypto::paper::is_paper(paper.as_bytes()));
    let decoded = rsb_sdk::crypto::paper::decode(&paper).unwrap();
    let mut lines: Vec<String> = paper.lines().map(String::from).collect();
    let target = &mut lines[2];
    let pos = target.find("  ").unwrap() + 2;
    let ch = target.as_bytes()[pos];
    target.replace_range(pos..pos + 1, if ch == b'Q' { "R" } else { "Q" });
    let err = rsb_sdk::crypto::paper::decode(&lines.join("\n")).unwrap_err();
    assert!(err.to_string().contains("line 2"), "{err}");

    // An older export (before a password change) differs: refused without
    // --force; with --force the current file is backed up and kept.
    keyfile::change_password(
        storage.clone(),
        PW.as_bytes(),
        b"pw2",
        Some(KdfParams::MIN_CREATE),
    )
    .await
    .unwrap();
    let e = lifecycle::import_key_file(storage.clone(), &decoded, PW.as_bytes(), false)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("--force"), "{e}");
    restore_ok(&repo.cfg, &storage, "pw2", &repo.files()).await;
    let current = storage.read(REPO_KEY_PATH).await.unwrap();
    let o = lifecycle::import_key_file(storage.clone(), &decoded, PW.as_bytes(), true)
        .await
        .unwrap();
    let bak = o.replaced_backup.expect("backup copy");
    assert_eq!(storage.read(&bak).await.unwrap(), current);
    restore_ok(&repo.cfg, &storage, PW, &repo.files()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn import_into_another_repository_is_refused() {
    let a: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let b: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    fresh_repo(&a).await;
    let repo_b = fresh_repo(&b).await;
    let key_a = lifecycle::export_key_file(&*a, PW.as_bytes())
        .await
        .unwrap();
    let key_b = b.read(REPO_KEY_PATH).await.unwrap();
    for force in [false, true] {
        let e = lifecycle::import_key_file(b.clone(), &key_a, PW.as_bytes(), force)
            .await
            .unwrap_err();
        assert!(e.is_authentication_failure(), "{e:?}");
        assert!(e.to_string().contains("does not belong"), "{e}");
    }
    assert_eq!(b.read(REPO_KEY_PATH).await.unwrap(), key_b);
    // Also refused when B lost its key file.
    b.delete(REPO_KEY_PATH).await.unwrap();
    assert!(
        lifecycle::import_key_file(b.clone(), &key_a, PW.as_bytes(), false)
            .await
            .is_err()
    );
    assert!(!b.exists(REPO_KEY_PATH).await.unwrap());
    lifecycle::import_key_file(b.clone(), &key_b, PW.as_bytes(), false)
        .await
        .unwrap();
    restore_ok(&repo_b.cfg, &b, PW, &repo_b.files()).await;
    // Garbage is refused before anything is written.
    assert!(
        lifecycle::import_key_file(b.clone(), b"{not a key}", PW.as_bytes(), true)
            .await
            .is_err()
    );
}

// ---------------------------------------------------------------------------
// Rotation

/// Every encrypted object and manifest fails to open with `old` (an
/// authentication failure, not a format error) and opens with `new`.
async fn only_new_key_opens(
    storage: &dyn Storage,
    old: &rsb_sdk::crypto::RepoKey,
    new: &rsb_sdk::crypto::RepoKey,
) {
    let mut blobs: Vec<(String, String)> = list(storage, "data/enc")
        .await
        .into_iter()
        .map(|p| {
            let id = p.rsplit('/').next().unwrap().to_string();
            (p, id)
        })
        .collect();
    blobs.extend(
        list(storage, "snapshots")
            .await
            .into_iter()
            .map(|p| (p.clone(), p)),
    );
    assert!(blobs.len() > 3);
    for (path, id) in blobs {
        let raw = storage.read(&path).await.unwrap();
        let e = old.decrypt_object(&id, &raw).unwrap_err();
        assert!(rsb_sdk::crypto::is_authentication_error(&e), "{path}: {e}");
        assert!(new.decrypt_object(&id, &raw).is_ok(), "{path}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rotation_full_cycle() {
    let mem = FailingStorage::new();
    let storage: Arc<dyn Storage> = mem.clone();
    let repo = fresh_repo(&storage).await;
    // A second snapshot sharing objects with the first.
    std::fs::write(repo._src.path().join("two.txt"), b"second").unwrap();
    backup(&repo.cfg, &storage, PW).await;
    let mut files = repo.files();
    files.push(("two.txt", b"second"));
    let old = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();
    let sizes_before: Vec<(String, Option<u64>)> = {
        let mut v = Vec::new();
        for o in list(&*storage, "data/enc").await {
            let s = storage.size(&o).await.unwrap();
            v.push((o, s));
        }
        v
    };

    let rep = lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert!(!rep.resumed);
    assert_eq!(rep.old_key_id, old.key_id_hex());
    assert_ne!(rep.new_key_id, rep.old_key_id);
    assert_eq!(rep.objects_rotated, sizes_before.len(), "{:?}", rep);
    assert_eq!(rep.manifests_rewritten, 2, "{:?}", rep);

    let new = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();
    assert!(!new.is_rotating());
    assert_eq!(new.key_id_hex(), rep.new_key_id);
    let file = keyfile::read_key_file(&*storage).await.unwrap().unwrap();
    assert!(!file.is_rotating());
    assert_eq!(file.key_id(), Some(rep.new_key_id.as_str()));
    assert!(no_backup_copies(&mem.inner.keys()));
    only_new_key_opens(&*storage, &old, &new).await;
    for (o, k) in kinds(&*storage).await {
        assert_eq!(k, BlobKind::V3(Some(new.key_id())), "{o}");
    }
    // Object sizes (and names) are unchanged.
    for (o, s) in &sizes_before {
        assert_eq!(&storage.size(o).await.unwrap(), s, "{o}");
    }
    assert!(
        verify_errors(&repo.cfg, &storage, PW, true)
            .await
            .is_empty()
    );
    all_operations_work(&repo.cfg, &storage, PW, &files).await;
    // Backups after the rotation use the new key; the password is unchanged.
    std::fs::write(repo._src.path().join("three.txt"), b"third").unwrap();
    backup(&repo.cfg, &storage, PW).await;
    for (o, k) in kinds(&*storage).await {
        assert_eq!(k, BlobKind::V3(Some(new.key_id())), "{o}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn interrupted_rotation_resumes() {
    let mem = FailingStorage::new();
    let storage: Arc<dyn Storage> = mem.clone();
    let repo = fresh_repo(&storage).await;
    let files = repo.files();
    let old = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();

    // Crash after one re-encrypted object.
    mem.fail_after("data/enc/", 1);
    let e = lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("injected"), "{e}");
    mem.heal();
    let file = keyfile::read_key_file(&*storage).await.unwrap().unwrap();
    assert!(file.is_rotating());
    let rotating = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();
    assert!(rotating.is_rotating());
    let ids: Vec<BlobKind> = kinds(&*storage).await.into_iter().map(|(_, k)| k).collect();
    assert!(
        ids.contains(&BlobKind::V3(Some(rotating.key_id()))),
        "{ids:?}"
    );
    assert!(ids.contains(&BlobKind::V3(Some(old.key_id()))), "{ids:?}");

    // Mixed old/new objects: everything works, and a backup during the
    // interrupted rotation writes with the new key.
    all_operations_work(&repo.cfg, &storage, PW, &files).await;
    std::fs::write(repo._src.path().join("during.txt"), b"written mid-rotation").unwrap();
    backup(&repo.cfg, &storage, PW).await;
    let mut files = files;
    files.push(("during.txt", b"written mid-rotation"));
    restore_ok(&repo.cfg, &storage, PW, &files).await;
    // No password change while the key file holds two keys.
    let e = keyfile::change_password(
        storage.clone(),
        PW.as_bytes(),
        b"x",
        Some(KdfParams::MIN_CREATE),
    )
    .await
    .unwrap_err();
    assert!(e.to_string().contains("rsb key rotate"), "{e}");

    // Crash again while rewriting manifests.
    mem.fail_after("snapshots/", 0);
    assert!(
        lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
            .await
            .is_err()
    );
    mem.heal();
    all_operations_work(&repo.cfg, &storage, PW, &files).await;

    // Resume: same new key, finishes.
    let rep = lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert!(rep.resumed);
    assert_eq!(rep.new_key_id, rotating.key_id_hex());
    assert_eq!(rep.objects_rotated, 0, "{:?}", rep);
    let new = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();
    assert!(!new.is_rotating());
    only_new_key_opens(&*storage, &old, &new).await;
    assert!(
        verify_errors(&repo.cfg, &storage, PW, true)
            .await
            .is_empty()
    );
    all_operations_work(&repo.cfg, &storage, PW, &files).await;
    // A second rotation works and retires the intermediate key.
    lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    let newer = keyfile::load_repo_key(&*storage, PW.as_bytes())
        .await
        .unwrap()
        .unwrap();
    only_new_key_opens(&*storage, &new, &newer).await;
    restore_ok(&repo.cfg, &storage, PW, &files).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rotation_requires_completed_migration() {
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let repo = mixed_repo(&storage).await;
    let e = lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap_err();
    assert!(e.to_string().contains("rsb key migrate"), "{e}");
    assert!(
        !keyfile::read_key_file(&*storage)
            .await
            .unwrap()
            .unwrap()
            .is_rotating()
    );
    lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    // The garbage blob left by the migration does not block the rotation
    // (it is unreferenced and opens with nothing).
    let rep = lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    assert_eq!(rep.unreadable_unreferenced.len(), 1, "{:?}", rep);
    all_operations_work(&repo.cfg, &storage, PW, &repo.files()).await;
}

// ---------------------------------------------------------------------------
// Key status (what the desktop and `rsb key rotate` show before running)

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn key_status_reports_what_migrate_and_rotate_will_do() {
    // Empty repository: only init applies.
    let empty: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    let s = lifecycle::key_status(&*empty).await.unwrap();
    assert!(s.can_init() && !s.can_rotate() && !s.needs_migration());
    assert_eq!(s.encrypted_objects, 0);

    // Mixed v1/v2 + v3 repository: the legacy counts match legacy_summary.
    let mem = FailingStorage::new();
    let storage: Arc<dyn Storage> = mem.clone();
    let _repo = mixed_repo(&storage).await;
    let s = lifecycle::key_status(&*storage).await.unwrap();
    assert_eq!(
        s.legacy,
        lifecycle::legacy_summary(&*storage).await.unwrap()
    );
    assert!(!s.can_init() && s.can_rotate() && s.needs_migration());
    assert_eq!(s.encrypted_objects, list(&*storage, "data/enc").await.len());
    assert!(s.encrypted_objects > s.legacy.objects);
    assert!(s.encrypted_bytes >= s.legacy.bytes);
    assert_eq!(s.encrypted_manifests, 2, "{s:?}");
    assert!(!s.rotating);
    let key_id = s.key_id.clone().expect("key id");

    // After the migration only the unreadable garbage blob stays v1/v2.
    lifecycle::migrate_legacy_objects(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    let s = lifecycle::key_status(&*storage).await.unwrap();
    assert_eq!(s.legacy.objects, 1, "{s:?}");
    assert_eq!(s.legacy.manifests, 0, "{s:?}");

    // An interrupted rotation is reported (running rotate resumes it).
    mem.fail_after("data/enc/", 1);
    assert!(
        lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
            .await
            .is_err()
    );
    mem.heal();
    let s = lifecycle::key_status(&*storage).await.unwrap();
    assert!(s.rotating, "{s:?}");
    lifecycle::rotate_repo_key(storage.clone(), PW.as_bytes(), None, None)
        .await
        .unwrap();
    let s = lifecycle::key_status(&*storage).await.unwrap();
    assert!(!s.rotating);
    assert_ne!(s.key_id.as_deref(), Some(key_id.as_str()));
}
