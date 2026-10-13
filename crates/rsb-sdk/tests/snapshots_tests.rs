//! `rsb_sdk::snapshot`: listing, details, diff and delete of snapshots (the
//! code behind `rsb snapshots ...` and the desktop Snapshots screen), on a
//! `MemoryStorage`.

use rsb_sdk::core::types::{ChunkMetadata, FileMetadata};
use rsb_sdk::crypto::keyfile::{self, KdfParams};
use rsb_sdk::snapshot::{
    SnapshotContents, delete_snapshot, diff_snapshots, list_snapshot_ids, list_snapshots,
    open_key_cache, snapshot_details,
};
use rsb_sdk::storage::{LockKind, MemoryStorage, RepoLock, Storage};
use rsb_sdk::{CryptoErrorKind, RsbError};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

const PW: &str = "snapshot test password";

fn h(c: char) -> String {
    c.to_string().repeat(64)
}

fn entry(hash: &str, size: Option<u64>, stored: u64, encrypted: bool) -> FileMetadata {
    let mut m = FileMetadata::new(hash.to_string(), encrypted, true);
    m.size = size;
    m.stored_size = Some(stored);
    m
}

type Manifest = HashMap<PathBuf, FileMetadata>;

fn manifest(entries: &[(&str, FileMetadata)]) -> Manifest {
    entries
        .iter()
        .map(|(p, m)| (PathBuf::from(p), m.clone()))
        .collect()
}

fn zstd_toml(m: &Manifest) -> Vec<u8> {
    zstd::stream::encode_all(toml::to_string(m).unwrap().as_bytes(), 3).unwrap()
}

async fn put_plain(s: &dyn Storage, id: &str, m: &Manifest) {
    s.write(&format!("snapshots/{id}.toml"), &zstd_toml(m))
        .await
        .unwrap();
}

/// Empty repository: `snapshots/` and `data/` exist but hold nothing.
async fn empty_repo() -> Arc<MemoryStorage> {
    let s = Arc::new(MemoryStorage::new());
    for dir in ["snapshots", "data/clear"] {
        let probe = format!("{dir}/probe");
        s.write(&probe, b"").await.unwrap();
        s.delete(&probe).await.unwrap();
    }
    s
}

fn readable(c: &SnapshotContents) -> rsb_sdk::snapshot::SnapshotSummary {
    match c {
        SnapshotContents::Readable(s) => *s,
        other => panic!("expected a readable manifest, got {other:?}"),
    }
}

#[tokio::test]
async fn lists_newest_first_across_name_formats_and_collision_suffixes() {
    let s = empty_repo().await;
    let m = manifest(&[("a.txt", entry(&h('a'), Some(1), 1, false))]);
    // Written out of order on purpose.
    for id in [
        "2026-05-25T103000.500Z-2",
        "2026-05-24T090000.000Z",
        "2026-05-25T103000Z",
        "2026-05-25T103000.500Z-10",
        "2026-05-25T103000.500Z",
    ] {
        put_plain(&*s, id, &m).await;
    }
    // Not snapshots: foreign file, interrupted write, nested object.
    s.write("snapshots/notes.txt", b"x").await.unwrap();
    s.write("snapshots/.2026-05-25T110000.000Z.toml.rsb-tmp-ab12", b"x")
        .await
        .unwrap();

    let ids = list_snapshot_ids(&*s).await.unwrap();
    // A lexicographic sort (what `rsb snapshots list` used to do) puts
    // "-2" before "-10" and the second-resolution name before ".500Z".
    assert_eq!(
        ids,
        [
            "2026-05-25T103000.500Z-10",
            "2026-05-25T103000.500Z-2",
            "2026-05-25T103000.500Z",
            "2026-05-25T103000Z",
            "2026-05-24T090000.000Z",
        ]
    );

    let list = list_snapshots(&*s, None).await.unwrap();
    assert_eq!(list.iter().map(|i| i.id.as_str()).collect::<Vec<_>>(), ids);
    assert!(list.iter().all(|i| i.timestamp.is_some() && !i.encrypted));
    assert_eq!(
        list[2].timestamp.unwrap().to_rfc3339(),
        "2026-05-25T10:30:00.500+00:00"
    );
}

#[tokio::test]
async fn summary_counts_files_and_sizes() {
    let s = empty_repo().await;
    let mut big = entry(&h('c'), Some(100), 70, true);
    big.chunks = Some(vec![ChunkMetadata {
        hash: h('d'),
        stored_size: 70,
        stored_hash: None,
    }]);
    put_plain(
        &*s,
        "2026-05-25T103000.000Z",
        &manifest(&[
            ("a.txt", entry(&h('a'), Some(10), 5, false)),
            ("sub/b.bin", big),
        ]),
    )
    .await;
    // Older manifest without the plaintext `size`.
    put_plain(
        &*s,
        "2026-05-24T103000.000Z",
        &manifest(&[("a.txt", entry(&h('a'), None, 5, false))]),
    )
    .await;

    let list = list_snapshots(&*s, None).await.unwrap();
    let new = readable(&list[0].contents);
    assert_eq!(new.file_count, 2);
    assert_eq!(new.encrypted_files, 1);
    assert_eq!(new.stored_size, 75);
    assert_eq!(new.original_size, Some(110));
    let old = readable(&list[1].contents);
    assert_eq!((old.file_count, old.stored_size), (1, 5));
    assert_eq!(old.original_size, None);

    let d = snapshot_details(&*s, "2026-05-25T103000.000Z", None)
        .await
        .unwrap();
    assert_eq!(d.summary, new);
    let paths: Vec<_> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "sub/b.bin"]);
    assert_eq!(d.files[1].chunks, 1);
    assert_eq!(d.files[1].display_size(), 100);
    // `.toml` suffix accepted, like `rsb restore --snapshot`.
    assert!(
        snapshot_details(&*s, "2026-05-25T103000.000Z.toml", None)
            .await
            .is_ok()
    );
}

#[tokio::test]
async fn empty_and_missing_repositories() {
    let s = empty_repo().await;
    assert!(list_snapshot_ids(&*s).await.unwrap().is_empty());
    assert!(list_snapshots(&*s, None).await.unwrap().is_empty());

    // No snapshots/ at all: an error, not a silent empty list.
    let none = MemoryStorage::new();
    let e = list_snapshots(&none, None).await.unwrap_err();
    assert!(matches!(e, RsbError::NotFound(_)), "{e:?}");
    assert!(e.to_string().contains("Not a backup repository"), "{e}");
}

#[tokio::test]
async fn corrupt_manifests_are_reported_not_fatal() {
    let s = empty_repo().await;
    let m = manifest(&[("a.txt", entry(&h('a'), Some(1), 1, false))]);
    put_plain(&*s, "2026-05-25T100000.000Z", &m).await;
    // Valid zstd, invalid TOML.
    s.write(
        "snapshots/2026-05-25T110000.000Z.toml",
        &zstd::stream::encode_all(&b"not = [toml"[..], 3).unwrap(),
    )
    .await
    .unwrap();
    // Truncated zstd frame.
    let mut cut = zstd_toml(&m);
    cut.truncate(cut.len() / 2);
    s.write("snapshots/2026-05-25T120000.000Z.toml", &cut)
        .await
        .unwrap();

    let list = list_snapshots(&*s, None).await.unwrap();
    assert_eq!(list.len(), 3);
    for info in &list[..2] {
        match &info.contents {
            SnapshotContents::Unreadable { error } => {
                assert!(error.contains("corrupt"), "{error}")
            }
            other => panic!("{}: expected unreadable, got {other:?}", info.id),
        }
    }
    assert_eq!(readable(&list[2].contents).file_count, 1);

    assert!(
        snapshot_details(&*s, "2026-05-25T110000.000Z", None)
            .await
            .is_err()
    );
}

#[tokio::test]
async fn invalid_and_unknown_ids_are_refused() {
    let s = empty_repo().await;
    for bad in ["../keys/repo", "a/b", "", ".hidden", "x\\y"] {
        let e = snapshot_details(&*s, bad, None).await.unwrap_err();
        assert!(matches!(e, RsbError::InvalidInput(_)), "{bad}: {e:?}");
        let e = delete_snapshot(s.clone(), bad, None).await.unwrap_err();
        assert!(matches!(e, RsbError::InvalidInput(_)), "{bad}: {e:?}");
    }
    let e = snapshot_details(&*s, "2026-01-01T000000.000Z", None)
        .await
        .unwrap_err();
    assert!(matches!(e, RsbError::NotFound(_)), "{e:?}");
    let e = delete_snapshot(s.clone(), "2026-01-01T000000.000Z", None)
        .await
        .unwrap_err();
    assert!(matches!(e, RsbError::NotFound(_)), "{e:?}");
}

#[tokio::test]
async fn encrypted_manifests_are_locked_without_the_key() {
    let s = empty_repo().await;
    let storage: Arc<dyn Storage> = s.clone();
    let key = keyfile::init_repo_key(storage.clone(), PW.as_bytes(), KdfParams::MIN_CREATE)
        .await
        .unwrap();
    let m = manifest(&[
        ("a.txt", entry(&h('a'), Some(3), 40, true)),
        ("b.txt", entry(&h('b'), Some(4), 41, true)),
    ]);
    let id = "2026-05-25T103000.000Z";
    let path = format!("snapshots/{id}.toml");
    let blob = key.encrypt_object(&path, &zstd_toml(&m)).unwrap();
    s.write(&path, &blob).await.unwrap();
    // A plaintext snapshot in the same repository (written before
    // encryption was turned on) stays readable with the key.
    put_plain(
        &*s,
        "2026-05-24T103000.000Z",
        &manifest(&[("old.txt", entry(&h('c'), Some(1), 1, false))]),
    )
    .await;

    // No key: ids and dates are listed, contents are locked.
    let list = list_snapshots(&*s, None).await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].id, id);
    assert!(list[0].encrypted);
    assert_eq!(list[0].contents, SnapshotContents::Locked);
    assert!(!list[1].encrypted);
    assert_eq!(readable(&list[1].contents).file_count, 1);
    let e = snapshot_details(&*s, id, None).await.unwrap_err();
    assert!(e.to_string().contains("encryption key"), "{e}");

    // Right key.
    let cache = open_key_cache(&*s, Some(PW)).await.unwrap();
    let list = list_snapshots(&*s, cache.as_ref()).await.unwrap();
    let sum = readable(&list[0].contents);
    assert_eq!((sum.file_count, sum.encrypted_files), (2, 2));
    assert_eq!(sum.stored_size, 81);
    assert_eq!(readable(&list[1].contents).file_count, 1);
    let d = snapshot_details(&*s, id, cache.as_ref()).await.unwrap();
    assert!(d.encrypted);
    assert_eq!(d.files.len(), 2);

    // Wrong key: refused when the repository key is unlocked.
    let Err(e) = open_key_cache(&*s, Some("wrong")).await else {
        panic!("a wrong password must be refused");
    };
    assert!(e.is_authentication_failure(), "{e:?}");

    // A manifest moved to another name no longer authenticates.
    let moved = "snapshots/2026-05-26T103000.000Z.toml";
    s.write(moved, &blob).await.unwrap();
    let list = list_snapshots(&*s, cache.as_ref()).await.unwrap();
    assert_eq!(list[0].id, "2026-05-26T103000.000Z");
    assert!(matches!(
        list[0].contents,
        SnapshotContents::Unreadable { .. }
    ));
}

#[tokio::test]
async fn diff_reports_added_removed_and_modified() {
    let s = empty_repo().await;
    put_plain(
        &*s,
        "2026-05-24T100000.000Z",
        &manifest(&[
            ("same.txt", entry(&h('a'), Some(1), 1, false)),
            ("changed.txt", entry(&h('b'), Some(2), 2, false)),
            ("gone.txt", entry(&h('c'), Some(3), 3, false)),
        ]),
    )
    .await;
    put_plain(
        &*s,
        "2026-05-25T100000.000Z",
        &manifest(&[
            ("same.txt", entry(&h('a'), Some(1), 1, false)),
            ("changed.txt", entry(&h('d'), Some(20), 9, false)),
            ("new.txt", entry(&h('e'), Some(5), 5, false)),
        ]),
    )
    .await;
    let d = diff_snapshots(
        &*s,
        "2026-05-24T100000.000Z",
        "2026-05-25T100000.000Z",
        None,
    )
    .await
    .unwrap();
    assert_eq!(d.added.len(), 1);
    assert_eq!(d.added[0].path, "new.txt");
    assert_eq!(d.removed.len(), 1);
    assert_eq!(d.removed[0].path, "gone.txt");
    assert_eq!(d.modified.len(), 1);
    assert_eq!(d.modified[0].0.display_size(), 2);
    assert_eq!(d.modified[0].1.display_size(), 20);
    assert!(!d.is_empty());

    let same = diff_snapshots(
        &*s,
        "2026-05-25T100000.000Z",
        "2026-05-25T100000.000Z",
        None,
    )
    .await
    .unwrap();
    assert!(same.is_empty());
    assert!(
        diff_snapshots(&*s, "2026-05-24T100000.000Z", "2026-01-01T000000Z", None)
            .await
            .is_err()
    );
}

/// Two snapshots sharing `shared`; the old one also references `only_old`.
async fn two_snapshot_repo() -> (Arc<MemoryStorage>, String, String) {
    let s = empty_repo().await;
    let (shared, only_old) = (h('a'), h('b'));
    put_plain(
        &*s,
        "2026-05-24T100000.000Z",
        &manifest(&[
            ("s.txt", entry(&shared, Some(1), 1, false)),
            ("o.txt", entry(&only_old, Some(1), 1, false)),
        ]),
    )
    .await;
    put_plain(
        &*s,
        "2026-05-25T100000.000Z",
        &manifest(&[("s.txt", entry(&shared, Some(1), 1, false))]),
    )
    .await;
    for hash in [&shared, &only_old] {
        s.write(&format!("data/clear/{hash}"), b"x").await.unwrap();
    }
    (s, shared, only_old)
}

#[tokio::test]
async fn delete_removes_manifest_and_unreferenced_data_under_lock() {
    let (s, shared, only_old) = two_snapshot_repo().await;
    s.write("meta/epoch", b"before").await.unwrap();

    let r = delete_snapshot(s.clone(), "2026-05-24T100000.000Z", None)
        .await
        .unwrap();
    assert_eq!(r.id, "2026-05-24T100000.000Z");
    assert_eq!(r.objects_deleted, 1);
    assert!(r.gc_ran);
    assert_eq!(
        list_snapshot_ids(&*s).await.unwrap(),
        ["2026-05-25T100000.000Z"]
    );
    assert!(s.exists(&format!("data/clear/{shared}")).await.unwrap());
    assert!(!s.exists(&format!("data/clear/{only_old}")).await.unwrap());
    // Dedup indexes are invalidated, the lock is released.
    assert_ne!(s.read("meta/epoch").await.unwrap(), b"before");
    assert!(s.keys().iter().all(|k| !k.starts_with("locks/")));

    // Deleting the last snapshot keeps the data (like prune).
    let r = delete_snapshot(s.clone(), "2026-05-25T100000.000Z", None)
        .await
        .unwrap();
    assert!(!r.gc_ran);
    assert_eq!(r.objects_deleted, 0);
    assert!(list_snapshot_ids(&*s).await.unwrap().is_empty());
    assert!(s.exists(&format!("data/clear/{shared}")).await.unwrap());
}

#[tokio::test]
async fn delete_is_refused_while_a_backup_holds_the_lock() {
    let (s, _, only_old) = two_snapshot_repo().await;
    let storage: Arc<dyn Storage> = s.clone();
    let backup = RepoLock::acquire(storage.clone(), LockKind::Shared, "backup")
        .await
        .unwrap();
    assert!(
        delete_snapshot(storage.clone(), "2026-05-24T100000.000Z", None)
            .await
            .is_err()
    );
    assert_eq!(list_snapshot_ids(&*s).await.unwrap().len(), 2);
    assert!(s.exists(&format!("data/clear/{only_old}")).await.unwrap());
    backup.release().await.unwrap();
    delete_snapshot(storage, "2026-05-24T100000.000Z", None)
        .await
        .unwrap();
}

#[tokio::test]
async fn delete_touches_nothing_when_a_remaining_manifest_is_unreadable() {
    let (s, _, only_old) = two_snapshot_repo().await;
    s.write("snapshots/2026-05-26T100000.000Z.toml", b"\xff\xfe garbage")
        .await
        .unwrap();
    assert!(
        delete_snapshot(s.clone(), "2026-05-24T100000.000Z", None)
            .await
            .is_err()
    );
    assert_eq!(list_snapshot_ids(&*s).await.unwrap().len(), 3);
    assert!(s.exists(&format!("data/clear/{only_old}")).await.unwrap());
}

#[tokio::test]
async fn delete_in_an_encrypted_repository_needs_the_key() {
    let s = empty_repo().await;
    let storage: Arc<dyn Storage> = s.clone();
    let key = keyfile::init_repo_key(storage.clone(), PW.as_bytes(), KdfParams::MIN_CREATE)
        .await
        .unwrap();
    for (id, hash) in [
        ("2026-05-24T100000.000Z", h('a')),
        ("2026-05-25T100000.000Z", h('b')),
    ] {
        let path = format!("snapshots/{id}.toml");
        let m = manifest(&[("f", entry(&hash, Some(1), 1, true))]);
        s.write(&path, &key.encrypt_object(&path, &zstd_toml(&m)).unwrap())
            .await
            .unwrap();
        s.write(&format!("data/enc/{hash}"), b"x").await.unwrap();
    }

    let e = delete_snapshot(storage.clone(), "2026-05-24T100000.000Z", None)
        .await
        .unwrap_err();
    assert!(
        matches!(
            e,
            RsbError::Crypto {
                kind: CryptoErrorKind::Other,
                ..
            }
        ),
        "{e:?}"
    );
    let e = delete_snapshot(storage.clone(), "2026-05-24T100000.000Z", Some("wrong"))
        .await
        .unwrap_err();
    assert!(e.is_authentication_failure(), "{e:?}");
    assert_eq!(list_snapshot_ids(&*s).await.unwrap().len(), 2);

    let r = delete_snapshot(storage, "2026-05-24T100000.000Z", Some(PW))
        .await
        .unwrap();
    assert_eq!(r.objects_deleted, 1);
    assert!(!s.exists(&format!("data/enc/{}", h('a'))).await.unwrap());
    assert!(s.exists(&format!("data/enc/{}", h('b'))).await.unwrap());
}
