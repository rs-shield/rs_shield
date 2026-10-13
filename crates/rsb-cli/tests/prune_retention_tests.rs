//! `rsb prune --retention <DAYS>` / `--keep-last <COUNT>`: argument parsing
//! and the prune the command runs (planning with the SDK's retention
//! policy, then preview or delete), on a local repository whose snapshot
//! timestamps are fabricated relative to now. Running the binary itself
//! needs a login, so the command's helpers are called directly.

use clap::Parser;
use rsb_cli::command::main_cmd::Commands;
use rsb_cli::helpers::{DEFAULT_PRUNE_KEEP_LAST, execute_prune, prune_policy};
use rsb_sdk::core::manifest::SNAPSHOT_NAME_FORMAT;
use rsb_sdk::core::types::FileMetadata;
use rsb_sdk::core::{RetentionPolicy, plan_prune_by_policy_with_storage};
use rsb_sdk::storage::{LocalStorage, Storage};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

fn parse(args: &[&str]) -> Result<(Option<usize>, Option<u32>, bool, bool), clap::Error> {
    let mut argv = vec!["rsb", "prune", "-b", "repo"];
    argv.extend_from_slice(args);
    match Cli::try_parse_from(argv)?.command {
        Commands::Prune {
            keep_last,
            retention,
            dry_run,
            confirm,
            ..
        } => Ok((keep_last, retention, dry_run, confirm)),
        _ => panic!("not a prune command"),
    }
}

#[test]
fn retention_is_parsed_as_days() {
    for (arg, days) in [
        ("30", 30),
        ("30d", 30),
        ("2w", 14),
        ("6m", 180),
        ("1y", 365),
    ] {
        let (keep, retention, _, _) = parse(&["--retention", arg]).unwrap();
        assert_eq!(retention, Some(days), "{arg}");
        assert_eq!(keep, None);
    }
    let (keep, retention, dry_run, confirm) =
        parse(&["-r", "7", "-l", "3", "--dry-run", "--confirm"]).unwrap();
    assert_eq!(
        (keep, retention, dry_run, confirm),
        (Some(3), Some(7), true, true)
    );
}

#[test]
fn invalid_retention_is_rejected_by_the_parser() {
    for bad in ["0", "0d", "abc", "30x", "-1", "36501"] {
        let arg = format!("--retention={bad}");
        let err = parse(&[&arg]).expect_err(bad);
        assert_eq!(err.kind(), clap::error::ErrorKind::ValueValidation, "{bad}");
    }
}

#[test]
fn policy_combines_the_flags() {
    assert_eq!(
        prune_policy(None, None),
        RetentionPolicy {
            keep_last: Some(DEFAULT_PRUNE_KEEP_LAST),
            keep_within_days: None
        }
    );
    // --retention alone is purely age based: no hidden count default.
    assert_eq!(
        prune_policy(None, Some(30)),
        RetentionPolicy {
            keep_last: None,
            keep_within_days: Some(30)
        }
    );
    assert_eq!(
        prune_policy(Some(3), Some(30)),
        RetentionPolicy {
            keep_last: Some(3),
            keep_within_days: Some(30)
        }
    );
}

/// A local repository with one snapshot per entry of `days_ago`, each
/// referencing its own data object.
struct Repo {
    _dir: tempfile::TempDir,
    storage: Arc<dyn Storage>,
    /// (snapshot key, data object key), in the order of `days_ago`.
    snapshots: Vec<(String, String)>,
}

async fn repo(days_ago: &[i64]) -> Repo {
    let dir = tempfile::tempdir().unwrap();
    let storage: Arc<dyn Storage> = Arc::new(LocalStorage::new(dir.path().to_str().unwrap()));
    let now = chrono::Utc::now();
    let mut snapshots = Vec::new();
    for (i, d) in days_ago.iter().enumerate() {
        let when = now - chrono::Duration::days(*d) - chrono::Duration::minutes(5);
        let key = format!("snapshots/{}.toml", when.format(SNAPSHOT_NAME_FORMAT));
        let hash = format!("{:064x}", i + 1);
        let mut manifest = HashMap::new();
        manifest.insert(
            PathBuf::from(format!("f{}", i)),
            FileMetadata::new(hash.clone(), false, false),
        );
        storage
            .write(&key, toml::to_string(&manifest).unwrap().as_bytes())
            .await
            .unwrap();
        let data = format!("data/clear/{}", hash);
        storage.write(&data, b"x").await.unwrap();
        snapshots.push((key, data));
    }
    Repo {
        _dir: dir,
        storage,
        snapshots,
    }
}

impl Repo {
    /// Which of the snapshots (by index) and their data still exist.
    async fn remaining(&self) -> Vec<usize> {
        let mut out = Vec::new();
        for (i, (snap, data)) in self.snapshots.iter().enumerate() {
            let s = self.storage.exists(snap).await.unwrap();
            let d = self.storage.exists(data).await.unwrap();
            assert_eq!(s, d, "snapshot {i} and its data must go together");
            if s {
                out.push(i);
            }
        }
        out
    }

    async fn prune(&self, keep_last: Option<usize>, days: Option<u32>, dry_run: bool) -> usize {
        let policy = prune_policy(keep_last, days);
        let doomed = plan_prune_by_policy_with_storage(&*self.storage, policy)
            .await
            .unwrap();
        let stats = execute_prune(self.storage.clone(), None, false, &doomed, dry_run)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stats.snapshots_deleted, doomed.len());
        assert_eq!(stats.objects_deleted, doomed.len());
        doomed.len()
    }
}

#[tokio::test]
async fn retention_deletes_snapshots_older_than_n_days() {
    // Many recent snapshots: the old count-based reading of "30" would have
    // kept 30 snapshots and deleted nothing older than 30 days here.
    let r = repo(&[0, 1, 2, 10, 29, 31, 45, 90]).await;
    assert_eq!(r.prune(None, Some(30), true).await, 3, "dry run plans 3");
    assert_eq!(r.remaining().await, (0..8).collect::<Vec<_>>(), "dry run");
    assert_eq!(r.prune(None, Some(30), false).await, 3);
    assert_eq!(r.remaining().await, vec![0, 1, 2, 3, 4]);

    // Few snapshots, all old: "30 days" no longer keeps them as a count.
    let r = repo(&[40, 50, 60]).await;
    r.prune(None, Some(30), false).await;
    assert_eq!(r.remaining().await, vec![0], "the newest is always kept");
}

#[tokio::test]
async fn retention_and_keep_last_keep_the_union() {
    let r = repo(&[0, 1, 40, 50, 60, 70]).await;
    // 30 days keeps 0,1; keep-last 4 keeps 0,1,40,50 -> union.
    r.prune(Some(4), Some(30), false).await;
    assert_eq!(r.remaining().await, vec![0, 1, 2, 3]);

    let r = repo(&[0, 1, 40, 50, 60, 70]).await;
    // keep-last 1 keeps 0; 55 days keeps 0,1,40,50 -> union.
    r.prune(Some(1), Some(55), false).await;
    assert_eq!(r.remaining().await, vec![0, 1, 2, 3]);

    let r = repo(&[0, 1, 2, 3]).await;
    r.prune(Some(2), None, false).await;
    assert_eq!(r.remaining().await, vec![0, 1]);
}

#[tokio::test]
async fn encrypted_repository_without_key() {
    let r = repo(&[0, 40]).await;
    let doomed = plan_prune_by_policy_with_storage(&*r.storage, prune_policy(None, Some(30)))
        .await
        .unwrap();
    assert_eq!(doomed.len(), 1);
    // Preview lists the snapshots but cannot count objects; a real prune
    // is refused before anything is deleted.
    let preview = execute_prune(r.storage.clone(), None, true, &doomed, true)
        .await
        .unwrap();
    assert!(preview.is_none());
    assert!(
        execute_prune(r.storage.clone(), None, true, &doomed, false)
            .await
            .is_err()
    );
    assert_eq!(r.remaining().await, vec![0, 1]);
}
