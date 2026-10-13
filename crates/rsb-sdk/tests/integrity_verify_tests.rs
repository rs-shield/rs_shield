// Tests for integrity::verify: detection and repair of corrupt objects
// (e.g. truncated by older versions with a plausible, non-zero size).

use rsb_sdk::backup::perform_backup;
use rsb_sdk::config::Config;
use rsb_sdk::integrity::{perform_verify, perform_verify_detailed};
use rsb_sdk::{BackupOptions, VerifyOptions};
use std::path::{Path, PathBuf};

fn config(src: &Path, dst: &Path, key: Option<&str>) -> Config {
    Config {
        source_path: src.to_string_lossy().into_owned(),
        destination_path: dst.to_string_lossy().into_owned(),
        exclude_patterns: vec![],
        encryption_key: key.map(str::to_string),
        compression_level: Some(3),
        max_threads: Some(2),
        ..Default::default()
    }
}

fn backup_opts(key: Option<&str>) -> BackupOptions {
    BackupOptions {
        mode: Some("incremental".to_string()),
        max_threads: Some(2),
        ..Default::default()
    }
    .with_encryption_key(key)
}

fn quiet() -> VerifyOptions {
    VerifyOptions {
        quiet: true,
        ..Default::default()
    }
}

fn repair() -> VerifyOptions {
    VerifyOptions {
        repair: true,
        ..quiet()
    }
}

fn data_objects(dst: &Path, kind: &str) -> Vec<PathBuf> {
    let dir = dst.join("data").join(kind);
    let mut v: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|rd| rd.filter_map(|e| e.ok()).map(|e| e.path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

/// Truncates an object to half its size (non-zero: passes the size-based
/// plausibility check that dedup uses).
fn truncate_half(p: &Path) {
    let data = std::fs::read(p).unwrap();
    assert!(data.len() > 2);
    std::fs::write(p, &data[..data.len() / 2]).unwrap();
}

async fn roundtrip(key: Option<&str>) {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"alpha ".repeat(500)).unwrap();
    std::fs::write(src.path().join("b.txt"), b"bravo ".repeat(500)).unwrap();
    let cfg = config(src.path(), dst.path(), key);
    let kind = if key.is_some() { "enc" } else { "clear" };

    perform_backup(&cfg, backup_opts(key)).await.unwrap();
    let report = perform_verify(&cfg, quiet()).await.unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);

    let objects = data_objects(dst.path(), kind);
    assert_eq!(objects.len(), 2);
    let victim = &objects[0];
    truncate_half(victim);

    // Plain verify detects it, names the object and does not modify anything.
    let report = perform_verify(&cfg, quiet()).await.unwrap();
    assert_eq!(report.files_with_errors, 1, "{:?}", report.errors);
    assert_eq!(report.files_processed, 1);
    assert!(
        report.errors.iter().any(|e| e.contains("Corrupt object")
            && e.contains(&*victim.file_name().unwrap().to_string_lossy())),
        "{:?}",
        report.errors
    );
    assert!(victim.exists());

    // Repair deletes the corrupt object.
    let report = perform_verify(&cfg, repair()).await.unwrap();
    assert!(
        report
            .errors
            .iter()
            .any(|e| e.contains("Deleted corrupt object"))
    );
    assert!(!victim.exists());

    // The next backup re-uploads it even though the source is unchanged
    // (dedup no longer trusts the previous snapshot blindly).
    perform_backup(&cfg, backup_opts(key)).await.unwrap();
    assert!(victim.exists());
    let report = perform_verify(&cfg, quiet()).await.unwrap();
    assert!(report.errors.is_empty(), "{:?}", report.errors);
    assert_eq!(report.files_processed, 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verify_detects_and_repairs_truncated_clear_object() {
    roundtrip(None).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn verify_detects_and_repairs_truncated_encrypted_object() {
    roundtrip(Some("correct horse battery staple")).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn detailed_verify_previews_what_repair_deletes() {
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"alpha ".repeat(500)).unwrap();
    std::fs::write(src.path().join("b.txt"), b"bravo ".repeat(500)).unwrap();
    let cfg = config(src.path(), dst.path(), None);
    perform_backup(&cfg, backup_opts(None)).await.unwrap();
    let objects = data_objects(dst.path(), "clear");
    let victim = &objects[0];
    truncate_half(victim);
    std::fs::remove_file(&objects[1]).unwrap();

    // Without repair: the corrupt object is a candidate, the missing one is
    // reported separately, nothing is deleted.
    let run = perform_verify_detailed(&cfg, quiet()).await.unwrap();
    let name = victim.file_name().unwrap().to_string_lossy().into_owned();
    let candidates = run.repair_candidates();
    assert_eq!(candidates.len(), 1, "{:?}", run.objects);
    assert!(candidates[0].path.ends_with(&name));
    assert_eq!(candidates[0].files.len(), 1);
    assert_eq!(run.objects.missing.len(), 1);
    assert!(run.objects.deleted.is_empty());
    assert_eq!(run.report.files_with_errors, 2);
    assert!(victim.exists());

    // With repair: exactly the previewed object is deleted.
    let run = perform_verify_detailed(&cfg, repair()).await.unwrap();
    assert_eq!(run.objects.deleted.len(), 1);
    assert!(run.objects.deleted[0].ends_with(&name));
    assert!(!victim.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn repair_does_not_delete_with_wrong_key_objects() {
    // Wrong key: the manifest itself fails to decrypt, so nothing is touched.
    let src = tempfile::tempdir().unwrap();
    let dst = tempfile::tempdir().unwrap();
    std::fs::write(src.path().join("a.txt"), b"alpha").unwrap();
    let cfg = config(src.path(), dst.path(), Some("right"));
    perform_backup(&cfg, backup_opts(Some("right")))
        .await
        .unwrap();
    let wrong = config(src.path(), dst.path(), Some("wrong"));
    assert!(perform_verify(&wrong, repair()).await.is_err());
    assert_eq!(data_objects(dst.path(), "enc").len(), 1);
}
