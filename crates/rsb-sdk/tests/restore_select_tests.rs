// Selective restore (`include` patterns) and restore by date
// (`at_or_before`) end to end on a `MemoryStorage` repository.

use chrono::{DateTime, Utc};
use rsb_sdk::backup::perform_backup;
use rsb_sdk::config::Config;
use rsb_sdk::core::dedup_index::DedupIndexOptions;
use rsb_sdk::core::{find_snapshot_at_or_before, perform_restore};
use rsb_sdk::storage::{MemoryStorage, Storage};
use rsb_sdk::{BackupOptions, RestoreOptions, RsbError};
use std::path::Path;
use std::sync::Arc;

const KEY: &str = "correct horse battery staple";

fn config(src: &Path) -> Config {
    Config {
        source_path: src.to_string_lossy().into_owned(),
        exclude_patterns: vec![],
        compression_level: Some(3),
        max_threads: Some(2),
        ..Default::default()
    }
}

async fn backup(config: &Config, storage: &Arc<dyn Storage>, key: Option<&str>) {
    let report = perform_backup(
        config,
        BackupOptions {
            mode: Some("full".into()),
            max_threads: Some(2),
            dedup_index: DedupIndexOptions::disabled(),
            storage: Some(storage.clone()),
            ..Default::default()
        }
        .with_encryption_key(key),
    )
    .await
    .unwrap();
    assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
}

fn write(root: &Path, rel: &str, content: &[u8]) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, content).unwrap();
}

/// Every regular file under `root`, as sorted `/`-separated relative paths.
fn files_under(root: &Path) -> Vec<String> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(&p, root, out);
            } else {
                let rel = p.strip_prefix(root).unwrap();
                let parts: Vec<_> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    let mut out = Vec::new();
    if root.exists() {
        walk(root, root, &mut out);
    }
    out.sort();
    out
}

async fn restore(
    config: &Config,
    storage: &Arc<dyn Storage>,
    target: &Path,
    key: Option<&str>,
    edit: impl FnOnce(&mut RestoreOptions),
) -> rsb_sdk::error::Result<rsb_sdk::report::ReportData> {
    let mut opts = RestoreOptions {
        storage: Some(storage.clone()),
        ..RestoreOptions::new(target)
    }
    .with_encryption_key(key);
    edit(&mut opts);
    perform_restore(config, opts).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn include_patterns_restore_only_matching_files() {
    let src = tempfile::tempdir().unwrap();
    write(src.path(), "a.txt", b"top");
    write(src.path(), "docs/report.pdf", b"pdf");
    write(src.path(), "docs/sub/notes.txt", b"notes");
    write(src.path(), "docs2/other.txt", b"other");
    write(src.path(), "fotos/verão/praia.jpg", b"jpg");
    let cfg = config(src.path());
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    backup(&cfg, &storage, Some(KEY)).await;

    let cases: &[(&[&str], &[&str])] = &[
        (&["docs/report.pdf"], &["docs/report.pdf"]),
        (&["docs"], &["docs/report.pdf", "docs/sub/notes.txt"]),
        (&["docs/"], &["docs/report.pdf", "docs/sub/notes.txt"]),
        (&["*.txt"], &["a.txt"]),
        (
            &["**/*.txt"],
            &["a.txt", "docs/sub/notes.txt", "docs2/other.txt"],
        ),
        (&["fotos/verão"], &["fotos/verão/praia.jpg"]),
        // Several patterns, one matching nothing: the others still restore.
        (
            &["a.txt", "docs/sub", "missing.bin"],
            &["a.txt", "docs/sub/notes.txt"],
        ),
    ];
    for (patterns, expected) in cases {
        let out = tempfile::tempdir().unwrap();
        let report = restore(&cfg, &storage, out.path(), Some(KEY), |o| {
            o.include = patterns.iter().map(|s| s.to_string()).collect();
        })
        .await
        .unwrap();
        assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
        assert_eq!(report.total_files, expected.len(), "{patterns:?}");
        assert_eq!(report.files_processed, expected.len(), "{patterns:?}");
        assert_eq!(files_under(out.path()), *expected, "{patterns:?}");
    }
    // Contents and relative paths are preserved.
    let out = tempfile::tempdir().unwrap();
    restore(&cfg, &storage, out.path(), Some(KEY), |o| {
        o.include = vec!["docs/sub/notes.txt".into()];
    })
    .await
    .unwrap();
    assert_eq!(
        std::fs::read(out.path().join("docs/sub/notes.txt")).unwrap(),
        b"notes"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn include_matching_nothing_or_unsafe_fails_without_writing() {
    let src = tempfile::tempdir().unwrap();
    write(src.path(), "a.txt", b"top");
    let cfg = config(src.path());
    let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
    backup(&cfg, &storage, None).await;

    let work = tempfile::tempdir().unwrap();
    let target = work.path().join("restore-here");
    let err = restore(&cfg, &storage, &target, None, |o| {
        o.include = vec!["nope.txt".into(), "dir/*".into()];
    })
    .await
    .unwrap_err();
    assert!(matches!(err, RsbError::NotFound(_)), "{err:?}");
    let msg = err.to_string();
    assert!(
        msg.contains("'nope.txt'") && msg.contains("'dir/*'"),
        "{msg}"
    );
    assert!(msg.contains("nothing was restored"), "{msg}");
    assert!(!target.exists(), "target must not be created");

    for bad in ["../a.txt", "/a.txt", "docs/../../a.txt"] {
        let err = restore(&cfg, &storage, &target, None, |o| {
            o.include = vec![bad.into()];
        })
        .await
        .unwrap_err();
        assert!(matches!(err, RsbError::InvalidInput(_)), "{bad}: {err:?}");
        assert!(!target.exists());
    }
}

fn utc(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

/// Three backups of `v.txt` (contents "v1", "v2", "v3") whose (clear)
/// manifests are renamed to fixed snapshot names, oldest first.
async fn dated_repo(names: [&str; 3]) -> (tempfile::TempDir, Config, Arc<dyn Storage>) {
    let src = tempfile::tempdir().unwrap();
    let cfg = config(src.path());
    let mem = Arc::new(MemoryStorage::new());
    let storage: Arc<dyn Storage> = mem.clone();
    for (i, name) in names.iter().enumerate() {
        write(src.path(), "v.txt", format!("v{}", i + 1).as_bytes());
        write(src.path(), &format!("only{}.txt", i + 1), b"x");
        backup(&cfg, &storage, None).await;
        let fresh = mem
            .keys()
            .into_iter()
            .find(|k| {
                k.starts_with("snapshots/")
                    && k.ends_with(".toml")
                    && !names.iter().any(|n| k == &format!("snapshots/{n}.toml"))
            })
            .unwrap();
        let bytes = storage.read(&fresh).await.unwrap();
        storage
            .write(&format!("snapshots/{name}.toml"), &bytes)
            .await
            .unwrap();
        storage.delete(&fresh).await.unwrap();
    }
    (src, cfg, storage)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restore_by_date_picks_latest_snapshot_at_or_before() {
    let names = [
        "2026-10-01T080000.000Z",
        "2026-10-01T180000.000Z",
        "2026-10-03T090000.000Z",
    ];
    let (_src, cfg, storage) = dated_repo(names).await;

    let cases = [
        ("2026-10-01T08:00:00Z", "v1"),           // exactly equal
        ("2026-10-01T17:59:59.999Z", "v1"),       // just before the 2nd
        ("2026-10-01T23:59:59.999999999Z", "v2"), // end of a day with two
        ("2026-10-02T12:00:00Z", "v2"),
        ("2026-10-03T09:00:00Z", "v3"),
        ("2030-01-01T00:00:00Z", "v3"),
    ];
    for (when, expected) in cases {
        let when = utc(when);
        let out = tempfile::tempdir().unwrap();
        let report = restore(&cfg, &storage, out.path(), None, |o| {
            o.at_or_before = Some(when);
        })
        .await
        .unwrap();
        assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
        assert_eq!(
            std::fs::read_to_string(out.path().join("v.txt")).unwrap(),
            expected,
            "{when}"
        );
    }

    assert_eq!(
        find_snapshot_at_or_before(&*storage, utc("2026-10-02T00:00:00Z"))
            .await
            .unwrap(),
        names[1]
    );

    // Before the oldest snapshot: error naming it, nothing written.
    let work = tempfile::tempdir().unwrap();
    let target = work.path().join("t");
    let err = restore(&cfg, &storage, &target, None, |o| {
        o.at_or_before = Some(utc("2026-09-30T00:00:00Z"));
    })
    .await
    .unwrap_err();
    assert!(matches!(err, RsbError::NotFound(_)), "{err:?}");
    assert!(err.to_string().contains(names[0]), "{err}");
    assert!(!target.exists());

    // A date and an explicit snapshot id are exclusive.
    let err = restore(&cfg, &storage, &target, None, |o| {
        o.at_or_before = Some(utc("2030-01-01T00:00:00Z"));
        o.snapshot_id = Some(names[0].into());
    })
    .await
    .unwrap_err();
    assert!(matches!(err, RsbError::InvalidInput(_)), "{err:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn date_and_files_combine() {
    let names = [
        "2026-10-01T080000Z",
        "2026-10-02T080000Z",
        "2026-10-03T080000Z",
    ];
    let (_src, cfg, storage) = dated_repo(names).await;
    let out = tempfile::tempdir().unwrap();
    let report = restore(&cfg, &storage, out.path(), None, |o| {
        o.at_or_before = Some(utc("2026-10-02T23:00:00Z"));
        o.include = vec!["v.txt".into(), "only*.txt".into()];
    })
    .await
    .unwrap();
    assert_eq!(report.files_with_errors, 0, "{:?}", report.errors);
    assert_eq!(files_under(out.path()), ["only1.txt", "only2.txt", "v.txt"]);
    assert_eq!(
        std::fs::read_to_string(out.path().join("v.txt")).unwrap(),
        "v2"
    );

    // The file only exists in the newest snapshot: not in the dated one.
    let err = restore(&cfg, &storage, out.path(), None, |o| {
        o.at_or_before = Some(utc("2026-10-02T23:00:00Z"));
        o.include = vec!["only3.txt".into()];
    })
    .await
    .unwrap_err();
    assert!(matches!(err, RsbError::NotFound(_)), "{err:?}");
    assert!(err.to_string().contains(names[1]), "{err}");
}

/// `--files` typed in one Unicode normalization form selects a file stored
/// in the other (NFC on Linux/Windows, NFD from macOS), and the file is
/// restored under the name stored in the manifest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn include_matches_across_unicode_normalization_forms() {
    use unicode_normalization::UnicodeNormalization;
    const NFC: &str = "fotos/ver\u{e3}o/caf\u{e9}.txt";
    const NFD: &str = "fotos/vera\u{303}o/cafe\u{301}.txt";
    assert_ne!(NFC, NFD);

    for written in [NFC, NFD] {
        let src = tempfile::tempdir().unwrap();
        write(src.path(), written, b"cafe");
        write(src.path(), "other.txt", b"x");
        // The name as the file system reports it (what the manifest stores).
        let stored: Vec<String> = files_under(src.path())
            .into_iter()
            .filter(|f| f != "other.txt")
            .collect();
        assert_eq!(stored.len(), 1);
        let stored = &stored[0];
        let cfg = config(src.path());
        let storage: Arc<dyn Storage> = Arc::new(MemoryStorage::new());
        backup(&cfg, &storage, None).await;

        // Patterns in the *other* form than the stored name.
        let is_nfc = stored.nfc().collect::<String>() == *stored;
        let other = |s: &str| -> String {
            if is_nfc {
                s.nfd().collect()
            } else {
                s.nfc().collect()
            }
        };
        for pattern in [
            other(stored),
            other("fotos/ver\u{e3}o"),
            other("fotos/ver\u{e3}o/*.txt"),
            other("**/caf\u{e9}.txt"),
        ] {
            assert_ne!(&pattern, stored, "pattern must differ in bytes");
            let out = tempfile::tempdir().unwrap();
            let report = restore(&cfg, &storage, out.path(), None, |o| {
                o.include = vec![pattern.clone()];
            })
            .await
            .unwrap_or_else(|e| panic!("{pattern:?} must match {stored:?}: {e}"));
            assert_eq!(report.files_processed, 1, "{pattern:?}");
            // Written under the stored name, byte for byte.
            assert_eq!(files_under(out.path()), vec![stored.clone()], "{pattern:?}");
            assert_eq!(std::fs::read(out.path().join(stored)).unwrap(), b"cafe");
        }
    }
}
