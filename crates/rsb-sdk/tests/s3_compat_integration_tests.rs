// Testes de integração contra um servidor S3-compatível real (MinIO, SeaweedFS,
// R2, B2, Wasabi, Ceph RGW, Garage, ...). Só correm quando estas variáveis de
// ambiente estão definidas; caso contrário são ignorados (passam sem fazer
// nada):
//
//   RSB_TEST_S3_ENDPOINT    ex.: http://localhost:9000
//   RSB_TEST_S3_BUCKET      bucket DEDICADO a testes (deve estar vazio)
//   RSB_TEST_S3_ACCESS_KEY
//   RSB_TEST_S3_SECRET_KEY
//   RSB_TEST_S3_REGION      opcional (por omissão us-east-1; R2 usa "auto")
//   RSB_TEST_S3_REQUIRED    opcional: falhar (em vez de ignorar) sem as acima
//
// Exemplo com SeaweedFS local (o CI faz o mesmo, ver .github/workflows/test.yml;
// s3.json define a identidade rsbtest/rsbtest-secret-key):
//   docker run -d --name seaweedfs -p 8333:8333 -v "$PWD/sw:/etc/rsb:ro" \
//     chrislusf/seaweedfs:3.80 server -s3 -s3.port=8333 \
//     -s3.config=/etc/rsb/s3.json -dir=/data -ip.bind=0.0.0.0
//   echo "s3.bucket.create -name rsb-test" | docker exec -i seaweedfs weed shell
//   RSB_TEST_S3_ENDPOINT=http://localhost:8333 RSB_TEST_S3_BUCKET=rsb-test \
//   RSB_TEST_S3_ACCESS_KEY=rsbtest RSB_TEST_S3_SECRET_KEY=rsbtest-secret-key \
//   cargo test -p rsb-sdk --test s3_compat_integration_tests
//
// Os testes que criam um repositório usam a raiz do bucket (snapshots/,
// data/, locks/, keys/, meta/): correm um de cada vez (REPO_ROOT) e só num
// bucket vazio ou já marcado por estes testes (MARKER).

use rsb_sdk::backup::perform_backup;
use rsb_sdk::config::Config;
use rsb_sdk::core::dedup_index::DedupIndexOptions;
use rsb_sdk::core::perform_prune_with_storage;
use rsb_sdk::core::restore::restore_from_storage;
use rsb_sdk::crypto::keyfile::{self, KdfParams};
use rsb_sdk::integrity::perform_verify;
use rsb_sdk::report::ReportData;
use rsb_sdk::storage::{InstrumentedStorage, LockKind, RepoLock, S3Storage, Storage};
use rsb_sdk::{BackupOptions, RestoreOptions, VerifyOptions};
use std::collections::BTreeMap;
use std::io::ErrorKind;
use std::path::Path;
use std::sync::Arc;
use tokio::io::AsyncReadExt;

/// Repository tests share the bucket root: one at a time.
static REPO_ROOT: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
/// Written at the bucket root by these tests: a bucket holding a
/// repository without it is not a test bucket and is never touched.
const MARKER: &str = "rsb-s3-compat-test-bucket";
const REPO_DIRS: &[&str] = &["snapshots", "data", "locks", "keys", "meta"];
const PASSWORD: &str = "s3 compat key";
/// More than one ListObjectsV2 page (1000 keys).
const MANY: usize = 1050;

struct Env {
    endpoint: String,
    bucket: String,
    region: Option<String>,
    access_key: String,
    secret_key: String,
}

fn var(k: &str) -> Option<String> {
    std::env::var(k).ok().filter(|v| !v.trim().is_empty())
}

fn env_vars() -> Option<Env> {
    Some(Env {
        endpoint: var("RSB_TEST_S3_ENDPOINT")?,
        bucket: var("RSB_TEST_S3_BUCKET")?,
        region: var("RSB_TEST_S3_REGION"),
        access_key: var("RSB_TEST_S3_ACCESS_KEY")?,
        secret_key: var("RSB_TEST_S3_SECRET_KEY")?,
    })
}

fn env() -> Option<Env> {
    let env = env_vars();
    // CI sets this so that a misconfigured job fails instead of skipping.
    if env.is_none() && var("RSB_TEST_S3_REQUIRED").is_some() {
        panic!("RSB_TEST_S3_REQUIRED is set but RSB_TEST_S3_* is incomplete");
    }
    env
}

async fn storage(env: &Env) -> Arc<dyn Storage> {
    let creds = aws_credential_types::Credentials::new(
        env.access_key.clone(),
        env.secret_key.clone(),
        None,
        None,
        "rsb-test",
    );
    Arc::new(
        S3Storage::new(
            &env.bucket,
            env.region.clone(),
            Some(env.endpoint.clone()),
            creds,
        )
        .await,
    )
}

/// Exclusive use of the bucket root for one repository test. Leftovers of
/// an earlier (failed) run of these tests are removed first.
struct RepoGuard {
    s: Arc<dyn Storage>,
    _lock: tokio::sync::MutexGuard<'static, ()>,
}

impl RepoGuard {
    async fn claim(s: Arc<dyn Storage>) -> Self {
        let lock = REPO_ROOT.lock().await;
        let marked = s.exists(MARKER).await.unwrap();
        for dir in REPO_DIRS {
            assert!(
                marked || s.list(dir).await.unwrap().is_empty(),
                "RSB_TEST_S3_BUCKET must be a dedicated, empty test bucket ('{}/' is not empty)",
                dir
            );
        }
        s.write(MARKER, b"rs-shield S3 integration tests")
            .await
            .unwrap();
        let guard = Self { s, _lock: lock };
        guard.wipe().await;
        guard
    }

    async fn wipe(&self) {
        for dir in REPO_DIRS {
            delete_all(&*self.s, dir).await;
        }
    }

    async fn release(self) {
        self.wipe().await;
        // Keep the marker if anything is left, so the next run can clean up.
        for dir in REPO_DIRS {
            assert!(
                self.s.list(dir).await.unwrap().is_empty(),
                "{dir}/ left behind"
            );
        }
        self.s.delete(MARKER).await.unwrap();
    }
}

async fn delete_all(s: &dyn Storage, prefix: &str) {
    for k in s.list(prefix).await.unwrap() {
        s.delete(&k).await.unwrap();
    }
}

/// Writes `files` (relative path with '/' → bytes) under `root`.
fn write_tree(root: &Path, files: &BTreeMap<String, Vec<u8>>) {
    for (rel, data) in files {
        let path = rel.split('/').fold(root.to_path_buf(), |p, s| p.join(s));
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, data).unwrap();
    }
}

fn read_tree(root: &Path) -> BTreeMap<String, Vec<u8>> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .map(|e| e.unwrap())
        .filter(|e| e.file_type().is_file())
        .map(|e| {
            let rel = e.path().strip_prefix(root).unwrap();
            let key = rel
                .components()
                .map(|c| c.as_os_str().to_str().unwrap().to_string())
                .collect::<Vec<_>>()
                .join("/");
            (key, std::fs::read(e.path()).unwrap())
        })
        .collect()
}

/// `MANY` small files in nested directories (so `data/<kind>` needs more
/// than one listing page), 1 MiB-boundary and multi-chunk files, and
/// duplicate contents.
fn source_files() -> BTreeMap<String, Vec<u8>> {
    let mut files = BTreeMap::new();
    for i in 0..MANY {
        files.insert(
            format!("small/{:02}/f{:04}.txt", i % 16, i),
            format!("small file {i}\n").into_bytes(),
        );
    }
    let big: Vec<u8> = (0..3 * 1024 * 1024 + 5)
        .map(|i: usize| (i.wrapping_mul(2_654_435_761) >> 13) as u8)
        .collect();
    files.insert("big/three-mib.bin".into(), big.clone());
    files.insert("big/one-mib.bin".into(), big[..1024 * 1024].to_vec());
    files.insert("big/copy of three-mib.bin".into(), big);
    files.insert("empty".into(), Vec::new());
    files.insert("ünïcødé/日本語.txt".into(), b"unicode".to_vec());
    files
}

fn config(src: &Path, key: Option<&str>) -> Config {
    Config {
        source_path: src.to_string_lossy().into_owned(),
        destination_path: src.join("unused").to_string_lossy().into_owned(),
        exclude_patterns: vec![],
        encryption_key: key.map(str::to_string),
        compression_level: Some(1),
        ..Default::default()
    }
}

async fn backup(
    config: &Config,
    s: &Arc<dyn Storage>,
    key: Option<&str>,
    index: DedupIndexOptions,
) -> ReportData {
    let report = perform_backup(
        config,
        BackupOptions {
            storage: Some(s.clone()),
            max_threads: Some(8),
            dedup_index: index,
            ..Default::default()
        }
        .with_encryption_key(key),
    )
    .await
    .unwrap();
    assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
    report
}

fn index_in(dir: &Path) -> DedupIndexOptions {
    DedupIndexOptions {
        dir: Some(dir.to_path_buf()),
        ..DedupIndexOptions::default()
    }
}

async fn verify(config: &Config, s: &Arc<dyn Storage>, key: Option<&str>) -> ReportData {
    perform_verify(
        config,
        VerifyOptions {
            storage: Some(s.clone()),
            quiet: true,
            ..Default::default()
        }
        .with_encryption_key(key),
    )
    .await
    .unwrap()
}

async fn restore(s: &Arc<dyn Storage>, key: Option<&str>) -> BTreeMap<String, Vec<u8>> {
    let out = tempfile::tempdir().unwrap();
    let report = restore_from_storage(
        &**s,
        RestoreOptions::new(out.path()).with_encryption_key(key),
    )
    .await
    .unwrap();
    assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
    read_tree(out.path())
}

/// Number of distinct contents (= data objects after deduplication).
fn distinct(files: &BTreeMap<String, Vec<u8>>) -> usize {
    files
        .values()
        .collect::<std::collections::HashSet<_>>()
        .len()
}

fn object_key(kind: &str, data: &[u8]) -> String {
    format!("data/{}/{}", kind, blake3::hash(data).to_hex())
}

#[tokio::test]
async fn s3_basic_operations_are_provider_compatible() {
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let s = storage(&env).await;
    let base = format!("rsb-compat-test-{}", uuid::Uuid::new_v4().simple());
    let a = format!("{}/dir/a.bin", base);
    let b = format!("{}/dir/b.bin", base);
    let other = format!("{}/dirX/c.bin", base);
    let payload: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();

    s.write(&a, &payload).await.unwrap();
    s.write(&b, b"bee").await.unwrap();
    s.write(&other, b"x").await.unwrap();

    assert_eq!(s.read(&a).await.unwrap(), payload);
    let mut r = s.read_stream(&a).await.unwrap();
    let mut got = Vec::new();
    r.read_to_end(&mut got).await.unwrap();
    assert_eq!(got, payload);

    assert_eq!(s.size(&b).await.unwrap(), Some(3));
    assert!(s.exists(&b).await.unwrap());
    assert!(s.exists(&format!("{}/dir/", base)).await.unwrap());

    // HEAD/GET de objeto inexistente: 404 (sem corpo no HEAD) ⇒ None/NotFound.
    let missing = format!("{}/dir/missing", base);
    assert_eq!(s.size(&missing).await.unwrap(), None);
    assert!(!s.exists(&missing).await.unwrap());
    assert_eq!(
        s.read(&missing).await.unwrap_err().kind(),
        ErrorKind::NotFound
    );
    assert_eq!(
        s.read_stream(&missing).await.err().map(|e| e.kind()),
        Some(ErrorKind::NotFound)
    );
    assert!(!s.exists(&format!("{}/nodir/", base)).await.unwrap());

    // list com semântica de diretório: "dir" não apanha "dirX".
    let mut listed = s.list(&format!("{}/dir", base)).await.unwrap();
    listed.sort();
    assert_eq!(listed, vec![a.clone(), b.clone()]);

    // Streaming upload paths (spooled file and reader) are plain PutObjects.
    let spooled = format!("{}/dir/spooled.bin", base);
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), &payload).unwrap();
    s.write_file(&spooled, tmp.path()).await.unwrap();
    assert_eq!(s.read(&spooled).await.unwrap(), payload);
    let streamed = format!("{}/dir/streamed.bin", base);
    let n = s
        .write_from_reader(&streamed, Box::pin(std::io::Cursor::new(payload.clone())))
        .await
        .unwrap();
    assert_eq!(n, payload.len() as u64);
    assert_eq!(s.size(&streamed).await.unwrap(), Some(payload.len() as u64));

    // Overwrite replaces the whole object.
    s.write(&b, b"b2").await.unwrap();
    assert_eq!(s.read(&b).await.unwrap(), b"b2");

    // Deleting a missing key is not an error (S3 semantics).
    s.delete(&missing).await.unwrap();

    delete_all(&*s, &base).await;
    assert!(s.list(&base).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_list_follows_pagination_past_1000_keys() {
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let s = storage(&env).await;
    let base = format!("rsb-page-test-{}", uuid::Uuid::new_v4().simple());
    let keys: Vec<String> = (0..2 * MANY)
        .map(|i| format!("{}/{}/k{:05}", base, if i % 2 == 0 { "p" } else { "q" }, i))
        .collect();

    // 32 concurrent writers.
    let mut tasks = tokio::task::JoinSet::new();
    for part in keys.chunks(keys.len().div_ceil(32)) {
        let s = s.clone();
        let part = part.to_vec();
        tasks.spawn(async move {
            for k in part {
                s.write(&k, k.as_bytes()).await.unwrap();
            }
        });
    }
    while let Some(r) = tasks.join_next().await {
        r.unwrap();
    }

    // 2 100 keys under base (3 pages), 1 050 under each sub-prefix (2 pages).
    let mut all = s.list(&base).await.unwrap();
    all.sort();
    let mut expected = keys.clone();
    expected.sort();
    assert_eq!(all.len(), 2 * MANY);
    assert_eq!(all, expected);
    for sub in ["p", "q"] {
        let mut listed = s.list(&format!("{}/{}", base, sub)).await.unwrap();
        listed.sort();
        listed.dedup();
        assert_eq!(listed.len(), MANY, "{sub}");
        assert!(
            listed
                .iter()
                .all(|k| k.starts_with(&format!("{}/{}/", base, sub)))
        );
    }

    let mut tasks = tokio::task::JoinSet::new();
    for part in keys.chunks(keys.len().div_ceil(32)) {
        let s = s.clone();
        let part = part.to_vec();
        tasks.spawn(async move {
            for k in part {
                s.delete(&k).await.unwrap();
            }
        });
    }
    while let Some(r) = tasks.join_next().await {
        r.unwrap();
    }
    assert!(s.list(&base).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_repository_lock() {
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let s = storage(&env).await;
    let repo = RepoGuard::claim(s.clone()).await;
    let would_block = |r: std::io::Result<RepoLock>| match r {
        Err(e) => e.kind() == ErrorKind::WouldBlock,
        Ok(_) => false,
    };

    // Only PUT/LIST/GET/DELETE (no conditional writes).
    let b1 = RepoLock::acquire(s.clone(), LockKind::Shared, "backup")
        .await
        .unwrap();
    let b2 = RepoLock::acquire(s.clone(), LockKind::Shared, "backup")
        .await
        .unwrap();
    assert_eq!(s.list("locks").await.unwrap().len(), 2);
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune").await
    ));
    // A refused attempt leaves no lock object behind.
    assert_eq!(s.list("locks").await.unwrap().len(), 2);
    b1.release().await.unwrap();
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune").await
    ));
    b2.release().await.unwrap();
    assert!(s.list("locks").await.unwrap().is_empty());

    let prune = RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune")
        .await
        .unwrap();
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Shared, "backup").await
    ));
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune").await
    ));
    // A backup refuses to run under an exclusive lock and writes nothing.
    let src = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"alpha").unwrap();
    let err = perform_backup(
        &config(src.path(), None),
        BackupOptions {
            storage: Some(s.clone()),
            dedup_index: DedupIndexOptions::disabled(),
            ..Default::default()
        },
    )
    .await;
    assert!(err.is_err(), "backup ran while the repository was locked");
    assert!(s.list("snapshots").await.unwrap().is_empty());
    prune.release().await.unwrap();
    assert!(s.list("locks").await.unwrap().is_empty());

    // A lock from another machine whose heartbeat stopped long ago is stale.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let stale = serde_json::json!({
        "id": "stale", "kind": "exclusive", "operation": "prune",
        "hostname": "some-other-host", "pid": 1,
        "created_unix": now - 7200, "refreshed_unix": now - 7200,
        "machine_id": "some-other-machine",
    });
    s.write("locks/stale.lock", stale.to_string().as_bytes())
        .await
        .unwrap();
    let l = RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune")
        .await
        .unwrap();
    l.release().await.unwrap();

    // ... but a fresh one is honoured, and so is an unreadable one.
    let fresh = serde_json::json!({
        "id": "fresh", "kind": "shared", "operation": "backup",
        "hostname": "some-other-host", "pid": 1,
        "created_unix": now, "refreshed_unix": now,
        "machine_id": "some-other-machine",
    });
    s.write("locks/fresh.lock", fresh.to_string().as_bytes())
        .await
        .unwrap();
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Exclusive, "prune").await
    ));
    s.delete("locks/fresh.lock").await.unwrap();
    s.write("locks/torn.lock", b"{\"id\": \"to").await.unwrap();
    assert!(would_block(
        RepoLock::acquire(s.clone(), LockKind::Shared, "backup").await
    ));

    repo.release().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_encrypted_backup_restore_prune_verify() {
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let s = storage(&env).await;
    let repo = RepoGuard::claim(s.clone()).await;
    let key = Some(PASSWORD);
    let cache = tempfile::tempdir().unwrap();
    keyfile::init_repo_key(s.clone(), PASSWORD.as_bytes(), KdfParams::MIN_CREATE)
        .await
        .unwrap();

    let src = tempfile::tempdir().unwrap();
    let mut files = source_files();
    write_tree(src.path(), &files);
    let cfg = config(src.path(), key);

    let r = backup(&cfg, &s, key, index_in(cache.path())).await;
    assert_eq!(r.files_processed + r.files_skipped, files.len());
    // Everything encrypted, one object per distinct content, and more
    // objects than one listing page.
    assert!(s.list("data/clear").await.unwrap().is_empty());
    let objects = s.list("data/enc").await.unwrap();
    assert_eq!(objects.len(), distinct(&files));
    assert!(objects.len() > 1000);

    // Second snapshot: one file changed, one added, one removed.
    let old_a = files["small/00/f0000.txt"].clone();
    let changed = b"changed after the first snapshot".to_vec();
    files.insert("small/00/f0000.txt".into(), changed.clone());
    files.insert("added.txt".into(), b"added".to_vec());
    files.remove("small/01/f0001.txt");
    std::fs::remove_file(src.path().join("small/01/f0001.txt")).unwrap();
    write_tree(src.path(), &files);
    backup(&cfg, &s, key, index_in(cache.path())).await;
    assert_eq!(s.list("snapshots").await.unwrap().len(), 2);

    let v = verify(&cfg, &s, key).await;
    assert!(v.errors.is_empty(), "{:?}", v.errors);

    // Streaming restore of the latest snapshot: same paths, same bytes.
    let restored = restore(&s, key).await;
    assert_eq!(restored.len(), files.len());
    assert!(restored == files, "restored tree differs");

    // Wrong password: refused before anything is restored.
    let out = tempfile::tempdir().unwrap();
    assert!(
        restore_from_storage(
            &*s,
            RestoreOptions::new(out.path()).with_encryption_key(Some("wrong"))
        )
        .await
        .is_err()
    );
    assert!(read_tree(out.path()).is_empty());
    assert!(
        perform_prune_with_storage(s.clone(), Some("wrong"), 1)
            .await
            .is_err()
    );
    assert_eq!(s.list("snapshots").await.unwrap().len(), 2);

    // Prune keeps the newest snapshot and collects what only the old one
    // referenced (needs the full, paginated listing of data/enc).
    perform_prune_with_storage(s.clone(), key, 1).await.unwrap();
    assert_eq!(s.list("snapshots").await.unwrap().len(), 1);
    assert!(!s.exists(&object_key("enc", &old_a)).await.unwrap());
    assert!(s.exists(&object_key("enc", &changed)).await.unwrap());
    assert_eq!(s.list("data/enc").await.unwrap().len(), distinct(&files));
    assert!(s.list("locks").await.unwrap().is_empty());
    let v = verify(&cfg, &s, key).await;
    assert!(v.errors.is_empty(), "{:?}", v.errors);
    assert!(restore(&s, key).await == files);

    // A corrupted object is reported by verify and fails its file on
    // restore without leaving partial output.
    let victim = object_key("enc", &files["ünïcødé/日本語.txt"]);
    let mut blob = s.read(&victim).await.unwrap();
    let mid = blob.len() / 2;
    blob[mid] ^= 0x01;
    s.write(&victim, &blob).await.unwrap();
    let v = verify(&cfg, &s, key).await;
    assert!(!v.errors.is_empty(), "verify missed a corrupted object");
    let out = tempfile::tempdir().unwrap();
    let r = restore_from_storage(
        &*s,
        RestoreOptions::new(out.path()).with_encryption_key(key),
    )
    .await;
    // Either the whole restore fails or the file is reported as failed.
    if let Ok(report) = r {
        assert!(report.files_with_errors >= 1);
    }
    assert!(!out.path().join("ünïcødé").join("日本語.txt").exists());

    repo.release().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_dedup_index() {
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let raw = storage(&env).await;
    let repo = RepoGuard::claim(raw.clone()).await;
    let counting = Arc::new(InstrumentedStorage::new(raw.clone()));
    let s: Arc<dyn Storage> = counting.clone();
    let cache = tempfile::tempdir().unwrap();
    let index = |sample_one_in| DedupIndexOptions {
        sample_one_in,
        ..index_in(cache.path())
    };

    let src = tempfile::tempdir().unwrap();
    let files = source_files();
    write_tree(src.path(), &files);
    let cfg = config(src.path(), None);
    let distinct = distinct(&files);

    // First backup: every object is looked up (HEAD) and uploaded; the
    // index is keyed by this bucket's identity and repository id.
    counting.reset();
    backup(&cfg, &s, None, index(0)).await;
    let first = counting.counts();
    assert!(first.head >= distinct as u64, "{first:?}");
    assert!(raw.exists("meta/repo-id").await.unwrap());
    assert!(
        std::fs::read_dir(cache.path()).unwrap().count() > 0,
        "index file written"
    );

    // Unchanged second backup: the index answers, (almost) no HEADs.
    counting.reset();
    backup(&cfg, &s, None, index(0)).await;
    let second = counting.counts();
    assert!(second.head < 5, "{second:?}");

    // Without the index every object is HEADed again.
    counting.reset();
    backup(&cfg, &s, None, DedupIndexOptions::disabled()).await;
    assert!(counting.counts().head >= distinct as u64);

    // An object deleted behind the index's back: sampling every hit finds
    // it missing and it is uploaded again.
    let victim = object_key("clear", &files["small/03/f0003.txt"]);
    raw.delete(&victim).await.unwrap();
    backup(&cfg, &s, None, index(1)).await;
    assert!(raw.exists(&victim).await.unwrap(), "object re-uploaded");

    // Prune bumps the repository epoch: the index is discarded and the next
    // backup checks every object again.
    let epoch_before = raw.read("meta/epoch").await.ok();
    perform_prune_with_storage(s.clone(), None, 1)
        .await
        .unwrap();
    assert_ne!(raw.read("meta/epoch").await.ok(), epoch_before);
    let victim = object_key("clear", &files["small/04/f0004.txt"]);
    raw.delete(&victim).await.unwrap();
    counting.reset();
    backup(&cfg, &s, None, index(0)).await;
    assert!(counting.counts().head >= distinct as u64);
    assert!(raw.exists(&victim).await.unwrap(), "object re-uploaded");

    let v = verify(&cfg, &s, None).await;
    assert!(v.errors.is_empty(), "{:?}", v.errors);
    assert!(restore(&s, None).await == files);

    repo.release().await;
}

/// `rsb snapshots` / the desktop Snapshots tab on S3: list (with and
/// without the key), details, diff and delete through `rsb_sdk::snapshot`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn s3_snapshot_management() {
    use rsb_sdk::snapshot::{self, SnapshotContents};
    let Some(env) = env() else {
        eprintln!("skipped: RSB_TEST_S3_* not set");
        return;
    };
    let s = storage(&env).await;
    let repo = RepoGuard::claim(s.clone()).await;
    let key = Some(PASSWORD);
    keyfile::init_repo_key(s.clone(), PASSWORD.as_bytes(), KdfParams::MIN_CREATE)
        .await
        .unwrap();

    // An empty bucket is an empty repository, not an error.
    assert!(snapshot::list_snapshot_ids(&*s).await.unwrap().is_empty());

    let src = tempfile::tempdir().unwrap();
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    files.insert("a.txt".into(), b"first a".to_vec());
    files.insert("b.txt".into(), b"only in the first snapshot".to_vec());
    write_tree(src.path(), &files);
    let cfg = config(src.path(), key);
    backup(&cfg, &s, key, DedupIndexOptions::disabled()).await;
    std::fs::remove_file(src.path().join("b.txt")).unwrap();
    std::fs::write(src.path().join("c.txt"), b"added").unwrap();
    backup(&cfg, &s, key, DedupIndexOptions::disabled()).await;

    let ids = snapshot::list_snapshot_ids(&*s).await.unwrap();
    assert_eq!(ids.len(), 2);
    let (newest, oldest) = (ids[0].clone(), ids[1].clone());

    // Without the key: ids and dates, contents locked.
    let locked = snapshot::list_snapshots(&*s, None).await.unwrap();
    assert!(
        locked.iter().all(|i| i.encrypted
            && i.contents == SnapshotContents::Locked
            && i.timestamp.is_some())
    );

    let cache = snapshot::open_key_cache(&*s, key).await.unwrap();
    let list = snapshot::list_snapshots(&*s, cache.as_ref()).await.unwrap();
    for info in &list {
        match &info.contents {
            SnapshotContents::Readable(sum) => assert_eq!(sum.file_count, 2),
            other => panic!("{}: {other:?}", info.id),
        }
    }
    let d = snapshot::snapshot_details(&*s, &newest, cache.as_ref())
        .await
        .unwrap();
    let paths: Vec<_> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "c.txt"]);
    let diff = snapshot::diff_snapshots(&*s, &oldest, &newest, cache.as_ref())
        .await
        .unwrap();
    assert_eq!(diff.added.len(), 1);
    assert_eq!(diff.removed.len(), 1);
    assert!(diff.modified.is_empty());

    // Delete the old snapshot: its own object goes, the shared one stays.
    assert!(
        snapshot::delete_snapshot(s.clone(), &oldest, None)
            .await
            .is_err(),
        "an encrypted repository needs the key"
    );
    let done = snapshot::delete_snapshot(s.clone(), &oldest, key)
        .await
        .unwrap();
    assert_eq!(done.objects_deleted, 1);
    assert_eq!(snapshot::list_snapshot_ids(&*s).await.unwrap(), [newest]);
    assert!(!s.exists(&object_key("enc", &files["b.txt"])).await.unwrap());
    assert!(s.exists(&object_key("enc", &files["a.txt"])).await.unwrap());
    assert!(s.list("locks").await.unwrap().is_empty());

    repo.release().await;
}
