//! `rsb snapshots list|show|diff|delete`.
//!
//! The repository comes from a profile (`--config`, or the one profile in
//! the current directory) or from `--backup <DIR>` (portable mode). A
//! profile is opened like `backup`/`restore` do it ([`get_storage`]): its S3
//! bucket when it has one (credentials from the environment, the encrypted
//! credentials file or a prompt), else its `destination_path`. The work
//! itself is done by [`rsb_sdk::snapshot`], shared with the desktop app.

use crate::helpers::{check_repository_dir, obtain_encryption_key, profile_name};
use anyhow::{Result, anyhow};
use clap::Subcommand;
use rsb_sdk::config::{self, Config};
use rsb_sdk::core::storage_backend::get_storage;
use rsb_sdk::snapshot::{
    self, SnapshotContents, SnapshotDetails, SnapshotDiff, SnapshotInfo, format_bytes,
    format_timestamp,
};
use rsb_sdk::storage::Storage;
use std::fmt::Write as _;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// List snapshots (newest first)
    List {
        /// Profile .toml (optional if exactly one profile is in the current dir)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Repository directory (portable mode, no profile needed)
        #[arg(short, long, conflicts_with = "config")]
        backup: Option<PathBuf>,
        /// Decryption key if manifests are encrypted (DEPRECATED: visible in `ps`; prefer env RSB_ENCRYPTION_KEY)
        #[arg(short, long)]
        key: Option<String>,
        /// Print JSON instead of a table
        #[arg(long)]
        json: bool,
    },

    /// Show snapshot details (files and sizes)
    Show {
        /// Profile .toml (optional if exactly one profile is in the current dir)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Repository directory (portable mode, no profile needed)
        #[arg(short, long, conflicts_with = "config")]
        backup: Option<PathBuf>,
        /// Snapshot ID
        id: String,
        /// Decryption key if manifest is encrypted (DEPRECATED: visible in `ps`; prefer env RSB_ENCRYPTION_KEY)
        #[arg(short, long)]
        key: Option<String>,
        /// Print JSON instead of text
        #[arg(long)]
        json: bool,
    },

    /// Delete a snapshot and the data only it references
    Delete {
        /// Profile .toml (optional if exactly one profile is in the current dir)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Repository directory (portable mode, no profile needed)
        #[arg(short, long, conflicts_with = "config")]
        backup: Option<PathBuf>,
        /// Snapshot ID
        id: String,
        /// Show what would be deleted (snapshot, data objects) without deleting
        #[arg(short = 'd', long)]
        dry_run: bool,
        /// Confirm destructive deletion (required unless --dry-run)
        #[arg(long)]
        confirm: bool,
    },

    /// Compare two snapshots
    Diff {
        /// Profile .toml (optional if exactly one profile is in the current dir)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Repository directory (portable mode, no profile needed)
        #[arg(short, long, conflicts_with = "config")]
        backup: Option<PathBuf>,
        /// First (older) snapshot ID
        from: String,
        /// Second (newer) snapshot ID
        to: String,
        /// Decryption key if manifests are encrypted (DEPRECATED: visible in `ps`; prefer env RSB_ENCRYPTION_KEY)
        #[arg(short, long)]
        key: Option<String>,
    },
}

/// Where the snapshots are read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RepositorySource {
    /// A profile: its S3 bucket, or its `destination_path`.
    Profile(PathBuf),
    /// A local repository directory (`--backup`).
    Directory(PathBuf),
}

impl RepositorySource {
    /// `--backup` when given, else `--config`, else the one profile in the
    /// current directory.
    pub fn resolve(config: Option<PathBuf>, backup: Option<PathBuf>) -> Result<Self> {
        match (config, backup) {
            (_, Some(dir)) => Ok(Self::Directory(dir)),
            (config, None) => Ok(Self::Profile(crate::helpers::resolve_profile_path(config)?)),
        }
    }
}

/// An opened repository.
struct Repo {
    cfg: Config,
    profile: Option<String>,
    storage: Arc<dyn Storage>,
}

impl SnapshotCommand {
    fn source(&self) -> Result<RepositorySource> {
        let (config, backup) = match self {
            Self::List { config, backup, .. }
            | Self::Show { config, backup, .. }
            | Self::Delete { config, backup, .. }
            | Self::Diff { config, backup, .. } => (config.clone(), backup.clone()),
        };
        RepositorySource::resolve(config, backup)
    }

    fn name(&self) -> &'static str {
        match self {
            Self::List { .. } => "snapshots list",
            Self::Show { .. } => "snapshots show",
            Self::Delete { .. } => "snapshots delete",
            Self::Diff { .. } => "snapshots diff",
        }
    }

    pub async fn execute(&self) -> Result<()> {
        if let Self::Delete {
            dry_run, confirm, ..
        } = self
        {
            check_delete_confirmation(*dry_run, *confirm)?;
            // Destructive: needs an authenticated session (like prune, also
            // for a dry run).
            rsb_sdk::server::routes::check_fido2_auth()
                .await
                .map_err(|e| anyhow!("{}", e))?;
        }
        let repo = open_repository(self.name(), self.source()?).await?;

        match self {
            Self::List { key, json, .. } => {
                let key = repository_key(key.clone(), &repo).await?;
                let cache = snapshot::open_key_cache(&*repo.storage, key.as_deref()).await?;
                let list = snapshot::list_snapshots(&*repo.storage, cache.as_ref()).await?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&list)?);
                } else {
                    print!("{}", format_list(&list));
                }
            }
            Self::Show { id, key, json, .. } => {
                let key = repository_key(key.clone(), &repo).await?;
                let cache = snapshot::open_key_cache(&*repo.storage, key.as_deref()).await?;
                let details =
                    snapshot::snapshot_details(&*repo.storage, id, cache.as_ref()).await?;
                if *json {
                    println!("{}", serde_json::to_string_pretty(&details)?);
                } else {
                    print!("{}", format_details(&details));
                }
            }
            Self::Diff { from, to, key, .. } => {
                let key = repository_key(key.clone(), &repo).await?;
                let cache = snapshot::open_key_cache(&*repo.storage, key.as_deref()).await?;
                let diff =
                    snapshot::diff_snapshots(&*repo.storage, from, to, cache.as_ref()).await?;
                print!("{}", format_diff(&diff));
            }
            Self::Delete {
                id, dry_run: true, ..
            } => {
                let key = repository_key(None, &repo).await?;
                let plan =
                    snapshot::preview_delete_snapshot(&*repo.storage, id, key.as_deref()).await?;
                print!("{}", format_delete_preview(&plan));
            }
            Self::Delete { id, .. } => {
                let key = repository_key(None, &repo).await?;
                let done =
                    snapshot::delete_snapshot(repo.storage.clone(), id, key.as_deref()).await?;
                println!("✅ Snapshot deleted: {}", done.id);
                if done.gc_ran {
                    println!(
                        "   {} data object(s) no other snapshot used were deleted",
                        done.objects_deleted
                    );
                } else {
                    println!("   It was the last snapshot: its data objects were kept");
                }
            }
        }
        Ok(())
    }
}

/// `snapshots delete` deletes data like `rsb prune`: it needs `--confirm`
/// unless it is a `--dry-run`.
pub fn check_delete_confirmation(dry_run: bool, confirm: bool) -> Result<()> {
    if !dry_run && !confirm {
        return Err(anyhow!(
            "Refusing to delete a snapshot without --confirm: it also deletes the data \
             objects no other snapshot uses (preview with --dry-run). \
             Re-run with --confirm to proceed."
        ));
    }
    Ok(())
}

/// Text printed by `rsb snapshots delete --dry-run`.
pub fn format_delete_preview(plan: &snapshot::SnapshotDeletion) -> String {
    let mut out = format!("DRY RUN: would delete snapshot {}\n", plan.id);
    if plan.gc_ran {
        let _ = writeln!(
            out,
            "   and {} data object(s) no other snapshot uses",
            plan.objects_deleted
        );
    } else {
        let _ = writeln!(
            out,
            "   It is the last snapshot: its data objects would be kept"
        );
    }
    out.push_str("Nothing was deleted. Re-run with --confirm to delete.\n");
    out
}

/// Opens the repository of `source` (S3 or local, see the module docs).
async fn open_repository(command: &str, source: RepositorySource) -> Result<Repo> {
    let (cfg, profile) = match source {
        RepositorySource::Directory(dir) => {
            let dir = check_repository_dir(command, &dir)?;
            let cfg = rsb_sdk::config_from_backup_path(&dir, None)
                .map_err(|e| anyhow!("Not a backup repository: {}", e))?;
            (cfg, None)
        }
        RepositorySource::Profile(path) => {
            let cfg = config::load_config(&path)
                .map_err(|e| anyhow!("Cannot use profile {}: {}", path.display(), e))?;
            (cfg, profile_name(&path))
        }
    };
    let storage = get_storage(&cfg).await?;
    Ok(Repo {
        cfg,
        profile,
        storage,
    })
}

/// The key: `--key` (with a warning) → RSB_ENCRYPTION_KEY → the profile's
/// keyring entry → legacy key in the profile; for an encrypted repository
/// without any of these, a prompt on a terminal. `None` otherwise: listing
/// then shows encrypted snapshots as locked, and show/diff/delete explain
/// that the key is needed.
async fn repository_key(cli_key: Option<String>, repo: &Repo) -> Result<Option<String>> {
    if let Some(k) =
        obtain_encryption_key(cli_key, repo.profile.as_deref(), Some(&repo.cfg), false)?
    {
        return Ok(Some(k));
    }
    let encrypted = rsb_sdk::crypto::keyfile::repo_key_exists(&*repo.storage).await?
        || repo.cfg.encryption_required();
    if encrypted && std::io::stdin().is_terminal() {
        return obtain_encryption_key(None, repo.profile.as_deref(), Some(&repo.cfg), true);
    }
    Ok(None)
}

/// Table printed by `rsb snapshots list`.
pub fn format_list(list: &[SnapshotInfo]) -> String {
    let mut out = String::new();
    if list.is_empty() {
        out.push_str("ℹ️  No snapshots found\n");
        return out;
    }
    let _ = writeln!(out, "{} snapshot(s), newest first\n", list.len());
    let _ = writeln!(
        out,
        "{:<28} {:<23} {:>8} {:>12}  NOTES",
        "ID", "DATE", "FILES", "SIZE"
    );
    let _ = writeln!(out, "{}", "-".repeat(90));
    let (mut locked, mut unreadable) = (0, 0);
    for info in list {
        let lock = if info.encrypted { "🔒 encrypted" } else { "" };
        match &info.contents {
            SnapshotContents::Readable(s) => {
                let _ = writeln!(
                    out,
                    "{:<28} {:<23} {:>8} {:>12}  {}",
                    info.id,
                    info.date_label(),
                    s.file_count,
                    format_bytes(s.display_size()),
                    lock
                );
            }
            SnapshotContents::Locked => {
                locked += 1;
                let _ = writeln!(
                    out,
                    "{:<28} {:<23} {:>8} {:>12}  🔒 encrypted (key needed)",
                    info.id,
                    info.date_label(),
                    "?",
                    "?"
                );
            }
            SnapshotContents::Unreadable { error } => {
                unreadable += 1;
                let first_line = error.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
                let _ = writeln!(
                    out,
                    "{:<28} {:<23} {:>8} {:>12}  ⚠️  unreadable: {}",
                    info.id,
                    info.date_label(),
                    "?",
                    "?",
                    first_line.trim()
                );
            }
        }
    }
    if locked > 0 {
        let _ = writeln!(
            out,
            "\n🔒 {} snapshot(s) are encrypted: set {} (or run in a terminal to be asked) \
             to see their contents.",
            locked,
            config::ENCRYPTION_KEY_ENV
        );
    }
    if unreadable > 0 {
        let _ = writeln!(
            out,
            "\n⚠️  {} snapshot(s) could not be read; `rsb verify` checks the repository.",
            unreadable
        );
    }
    out
}

/// Text printed by `rsb snapshots show`.
pub fn format_details(d: &SnapshotDetails) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "📦 Snapshot {}", d.id);
    let _ = writeln!(out, "Date:         {}", format_timestamp(d.timestamp));
    let _ = writeln!(
        out,
        "Encrypted:    {}",
        if d.encrypted { "yes" } else { "no" }
    );
    let _ = writeln!(
        out,
        "Files:        {} ({} encrypted)",
        d.summary.file_count, d.summary.encrypted_files
    );
    if let Some(n) = d.summary.original_size {
        let _ = writeln!(out, "Size:         {}", format_bytes(n));
    }
    let _ = writeln!(out, "Stored size:  {}", format_bytes(d.summary.stored_size));
    let _ = writeln!(out, "\n📄 Files:");
    for f in &d.files {
        let _ = writeln!(out, "  {:>12}  {}", format_bytes(f.display_size()), f.path);
    }
    out
}

/// Text printed by `rsb snapshots diff`.
pub fn format_diff(d: &SnapshotDiff) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "📊 Snapshot Diff: {} → {}", d.from, d.to);
    let _ = writeln!(out, "{}", "=".repeat(80));
    if !d.added.is_empty() {
        let _ = writeln!(out, "\n✅ Added ({} files):", d.added.len());
        for f in &d.added {
            let _ = writeln!(out, "  + {} ({})", f.path, format_bytes(f.display_size()));
        }
    }
    if !d.removed.is_empty() {
        let _ = writeln!(out, "\n❌ Removed ({} files):", d.removed.len());
        for f in &d.removed {
            let _ = writeln!(out, "  - {} ({})", f.path, format_bytes(f.display_size()));
        }
    }
    if !d.modified.is_empty() {
        let _ = writeln!(out, "\n🔄 Modified ({} files):", d.modified.len());
        for (old, new) in &d.modified {
            let _ = writeln!(
                out,
                "  ~ {} ({} → {})",
                new.path,
                format_bytes(old.display_size()),
                format_bytes(new.display_size())
            );
        }
    }
    if d.is_empty() {
        let _ = writeln!(out, "ℹ️  No changes between snapshots");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsb_sdk::snapshot::{SnapshotFile, SnapshotSummary};

    fn info(id: &str, contents: SnapshotContents, encrypted: bool) -> SnapshotInfo {
        SnapshotInfo {
            id: id.into(),
            timestamp: rsb_sdk::core::manifest::parse_snapshot_timestamp(id),
            encrypted,
            contents,
        }
    }

    #[test]
    fn list_table_shows_every_state() {
        let ok = SnapshotContents::Readable(SnapshotSummary {
            file_count: 3,
            encrypted_files: 0,
            stored_size: 10,
            original_size: Some(2048),
        });
        let text = format_list(&[
            info("2026-05-25T103000.000Z-1", ok, false),
            info("2026-05-24T103000.000Z", SnapshotContents::Locked, true),
            info(
                "2026-05-23T103000.000Z",
                SnapshotContents::Unreadable {
                    error: "\nboom\nmore".into(),
                },
                false,
            ),
        ]);
        assert!(text.starts_with("3 snapshot(s)"), "{text}");
        assert!(text.contains("2026-05-25T103000.000Z-1"));
        assert!(text.contains("2026-05-25 10:30:00 UTC"));
        assert!(text.contains("2.00 KB"));
        assert!(text.contains("key needed"));
        assert!(text.contains("unreadable: boom"));
        assert!(text.contains("1 snapshot(s) are encrypted"));
        assert!(text.contains("1 snapshot(s) could not be read"));
        assert!(format_list(&[]).contains("No snapshots found"));
    }

    #[test]
    fn diff_text_lists_changes() {
        let f = |p: &str, n| SnapshotFile {
            path: p.into(),
            size: Some(n),
            stored_size: None,
            encrypted: false,
            chunks: 0,
        };
        let d = SnapshotDiff {
            from: "a".into(),
            to: "b".into(),
            added: vec![f("new.txt", 1)],
            removed: vec![f("old.txt", 2)],
            modified: vec![(f("m.txt", 1), f("m.txt", 2048))],
        };
        let text = format_diff(&d);
        assert!(text.contains("+ new.txt (1 B)"));
        assert!(text.contains("- old.txt (2 B)"));
        assert!(text.contains("~ m.txt (1 B → 2.00 KB)"));
        assert!(format_diff(&SnapshotDiff::default()).contains("No changes"));
    }

    #[test]
    fn delete_needs_confirm_unless_dry_run() {
        let e = check_delete_confirmation(false, false).unwrap_err();
        assert!(e.to_string().contains("--confirm"), "{e}");
        assert!(check_delete_confirmation(true, false).is_ok());
        assert!(check_delete_confirmation(false, true).is_ok());

        let mut plan = snapshot::SnapshotDeletion {
            id: "2026-05-25T103000.000Z".into(),
            objects_deleted: 4,
            gc_ran: true,
        };
        let text = format_delete_preview(&plan);
        assert!(text.contains("would delete snapshot 2026-05-25T103000.000Z"));
        assert!(text.contains("4 data object(s)"));
        assert!(text.contains("Nothing was deleted"));
        plan.gc_ran = false;
        assert!(format_delete_preview(&plan).contains("last snapshot"));
    }

    #[test]
    fn backup_dir_wins_and_config_is_kept() {
        assert_eq!(
            RepositorySource::resolve(None, Some("repo".into())).unwrap(),
            RepositorySource::Directory("repo".into())
        );
        assert_eq!(
            RepositorySource::resolve(Some("p.toml".into()), None).unwrap(),
            RepositorySource::Profile("p.toml".into())
        );
    }
}
