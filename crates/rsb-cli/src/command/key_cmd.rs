//! `rsb key ...`: repository key file (`keys/repo.key`, format v3).
//!
//! Secrets are only ever read from an interactive prompt: there are no
//! password arguments (they would end up in `ps` and shell history).

use crate::helpers::resolve_profile_path;
use anyhow::{Result, anyhow};
use clap::{Args, Subcommand};
use rsb_sdk::CancellationToken;
use rsb_sdk::crypto::keyfile::{self, KdfParams, REPO_KEY_PATH};
use rsb_sdk::crypto::lifecycle;
use rsb_sdk::storage::{LocalStorage, Storage};
use std::io::{IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use zeroize::Zeroizing;

/// Which repository to operate on.
#[derive(Args, Debug, Clone)]
pub struct RepoTarget {
    /// Path to the profile .toml (its destination, local or S3, is used)
    pub config: Option<PathBuf>,

    /// Local repository directory (alternative to a profile)
    #[arg(short = 'r', long)]
    pub repo: Option<PathBuf>,
}

/// Argon2id cost overrides (defaults: 65536 KiB, t=3, p=1).
#[derive(Args, Debug, Clone, Default)]
pub struct KdfArgs {
    /// Argon2 memory cost in KiB (minimum 19456)
    #[arg(long)]
    pub m_cost_kib: Option<u32>,
    /// Argon2 passes (minimum 2)
    #[arg(long)]
    pub t_cost: Option<u32>,
    /// Argon2 lanes
    #[arg(long)]
    pub p_cost: Option<u32>,
}

impl KdfArgs {
    fn params(&self, base: KdfParams) -> Option<KdfParams> {
        if self.m_cost_kib.is_none() && self.t_cost.is_none() && self.p_cost.is_none() {
            return None;
        }
        Some(KdfParams {
            m_cost_kib: self.m_cost_kib.unwrap_or(base.m_cost_kib),
            t_cost: self.t_cost.unwrap_or(base.t_cost),
            p_cost: self.p_cost.unwrap_or(base.p_cost),
        })
    }
}

#[derive(Subcommand)]
pub enum KeyCommand {
    /// Create keys/repo.key and upgrade the repository to format v3
    /// (otherwise done automatically by the first encrypted backup)
    Init {
        #[command(flatten)]
        target: RepoTarget,
        #[command(flatten)]
        kdf: KdfArgs,
        /// Re-encrypt existing v1/v2 data as v3 without asking (for scripts)
        #[arg(long)]
        yes: bool,
    },

    /// Re-encrypt every v1/v2 (pre-v3) object and manifest as v3, verify
    /// the repository, then drop the legacy password from the key file.
    /// Resumable: run it again after an interruption.
    Migrate {
        #[command(flatten)]
        target: RepoTarget,
    },

    /// Generate a new data key and re-encrypt every object and manifest
    /// under it; the old key is dropped once everything verifies. Resumable:
    /// run it again after an interruption
    Rotate {
        #[command(flatten)]
        target: RepoTarget,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
    },

    /// Export keys/repo.key. The data key stays wrapped by the password, but
    /// together with the password the export decrypts every backup: protect
    /// it like the password
    Export {
        #[command(flatten)]
        target: RepoTarget,
        /// Write the key file here (created with mode 0600; must not exist)
        #[arg(
            short,
            long,
            conflicts_with = "paper",
            required_unless_present = "paper"
        )]
        output: Option<PathBuf>,
        /// Print a printable form (base32 in groups, a checksum per line)
        #[arg(long)]
        paper: bool,
    },

    /// Install a key file written by `rsb key export` (file or paper form)
    /// after checking that it opens with the password and decrypts this
    /// repository
    Import {
        /// Exported key file (JSON or paper form)
        file: PathBuf,
        #[command(flatten)]
        target: RepoTarget,
        /// Replace an existing, different keys/repo.key (it is backed up first)
        #[arg(long)]
        force: bool,
    },

    /// Change the repository password (rewraps the key; no data is rewritten)
    ChangePassword {
        #[command(flatten)]
        target: RepoTarget,
        #[command(flatten)]
        kdf: KdfArgs,
    },
}

fn prompt(msg: &str) -> Result<Zeroizing<String>> {
    if !std::io::stdin().is_terminal() {
        return Err(anyhow!(
            "Passwords are only read from an interactive prompt (no TTY available)"
        ));
    }
    Ok(Zeroizing::new(rpassword::prompt_password(msg)?))
}

fn prompt_new() -> Result<Zeroizing<String>> {
    let pw = prompt("New password: ")?;
    if pw.is_empty() {
        return Err(anyhow!("The password must not be empty"));
    }
    let confirm = prompt("Confirm new password: ")?;
    if *pw != *confirm {
        return Err(anyhow!("Passwords do not match"));
    }
    Ok(pw)
}

/// Asks a yes/no question on the terminal (default: no).
fn confirm(question: &str) -> Result<bool> {
    print!("{} [y/N] ", question);
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    Ok(matches!(line.trim(), "y" | "Y" | "yes" | "YES" | "Yes"))
}

fn human_bytes(n: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut v = n as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{} B", n)
    } else {
        format!("{:.1} {}", v, UNITS[u])
    }
}

/// Progress on stderr (one line, rewritten) and Ctrl-C → cancellation.
fn progress_and_cancel() -> (rsb_sdk::core::types::ProgressCallback, CancellationToken) {
    let token = CancellationToken::new();
    let t = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            eprintln!(
                "\n⏹️  Stopping after the current object (run the command again to resume)..."
            );
            t.cancel();
        }
    });
    let cb: rsb_sdk::core::types::ProgressCallback = Arc::new(|i, total, msg: String| {
        let short: String = msg.chars().take(60).collect();
        eprint!("\r\x1b[2K  [{}/{}] {}", i + 1, total, short);
    });
    (cb, token)
}

async fn run_migration(storage: Arc<dyn Storage>, pw: &[u8]) -> Result<()> {
    let (cb, token) = progress_and_cancel();
    let rep = lifecycle::migrate_legacy_objects(storage, pw, Some(cb), Some(token)).await;
    eprintln!();
    let rep = rep?;
    println!(
        "✅ Migration complete: {} object(s) re-encrypted as v3 ({} already v3), {} manifest(s) \
         rewritten, {} object(s) verified.",
        rep.objects_migrated, rep.objects_already_v3, rep.manifests_rewritten, rep.objects_verified
    );
    if rep.legacy_secret_removed {
        println!(
            "🔑 The legacy (v1/v2) password was removed from {}.",
            REPO_KEY_PATH
        );
    }
    if !rep.unreferenced_legacy.is_empty() {
        println!(
            "ℹ️  {} v1/v2 object(s) are not referenced by any snapshot (prune garbage-collects them).",
            rep.unreferenced_legacy.len()
        );
    }
    if !rep.unreadable_legacy.is_empty() {
        println!(
            "⚠️  {} unreferenced object(s) could not be decrypted and were left as they are:",
            rep.unreadable_legacy.len()
        );
        for p in rep.unreadable_legacy.iter().take(10) {
            println!("     {}", p);
        }
    }
    Ok(())
}

fn print_legacy_summary(s: &lifecycle::LegacySummary) {
    println!(
        "This repository holds {} v1/v2 (pre-v3) object(s) ({}) and {} v1/v2 manifest(s).",
        s.objects,
        human_bytes(s.bytes),
        s.manifests
    );
}

/// Creates `path` (must not exist) readable only by the owner and writes
/// `data` to it.
fn write_private_file(path: &Path, data: &[u8]) -> Result<()> {
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    let mut f = opts
        .open(path)
        .map_err(|e| anyhow!("Cannot create {}: {}", path.display(), e))?;
    f.write_all(data)?;
    f.sync_all()?;
    Ok(())
}

/// Largest key file accepted by `rsb key import`.
const MAX_IMPORT_SIZE: u64 = 64 * 1024;

async fn open_storage(target: &RepoTarget) -> Result<Arc<dyn Storage>> {
    if let Some(repo) = &target.repo {
        if target.config.is_some() {
            return Err(anyhow!("Use either a profile or --repo, not both"));
        }
        if !repo.is_dir() {
            return Err(anyhow!("Repository not found: {}", repo.display()));
        }
        return Ok(Arc::new(LocalStorage::new(&repo.to_string_lossy())));
    }
    let path = resolve_profile_path(target.config.clone())?;
    let cfg = rsb_sdk::config::load_config(&path).map_err(|e| anyhow!("{}", e))?;
    rsb_sdk::core::storage_backend::get_storage(&cfg)
        .await
        .map_err(|e| anyhow!("{}", e))
}

impl KeyCommand {
    pub async fn execute(&self) -> Result<()> {
        // Mutating commands: same authentication gate as backup/restore/prune.
        rsb_sdk::server::routes::check_fido2_auth()
            .await
            .map_err(|e| anyhow!("{}", e))?;
        match self {
            KeyCommand::Init { target, kdf, yes } => {
                let storage = open_storage(target).await?;
                if keyfile::repo_key_exists(&*storage).await? {
                    return Err(anyhow!(
                        "{} already exists; use `rsb key change-password`",
                        REPO_KEY_PATH
                    ));
                }
                println!("If this repository already has encrypted backups, enter their password.");
                let pw = prompt_new()?;
                let params = kdf.params(KdfParams::DEFAULT).unwrap_or_default();
                keyfile::init_repo_key(storage.clone(), pw.as_bytes(), params).await?;
                println!("✅ Created {} (format v3).", REPO_KEY_PATH);
                println!(
                    "⚠️  Losing {} makes every v3 backup unreadable: keep a copy of it \
                     (`rsb key export`).",
                    REPO_KEY_PATH
                );
                let summary = lifecycle::legacy_summary(&*storage).await?;
                if !summary.is_empty() {
                    print_legacy_summary(&summary);
                    let go = if *yes {
                        true
                    } else if std::io::stdin().is_terminal() {
                        confirm("Re-encrypt them as v3 now (recommended)?")?
                    } else {
                        false
                    };
                    if go {
                        run_migration(storage, pw.as_bytes()).await?;
                    } else {
                        println!(
                            "ℹ️  Run `rsb key migrate` to re-encrypt them; until then the password \
                             cannot be changed."
                        );
                    }
                }
            }
            KeyCommand::Export {
                target,
                output,
                paper,
            } => {
                let storage = open_storage(target).await?;
                let pw = prompt("Repository password: ")?;
                // Checks that the password opens it; the DEK is never shown.
                let raw = lifecycle::export_key_file(&*storage, pw.as_bytes()).await?;
                if *paper {
                    let file = keyfile::RepoKeyFile::from_bytes(&raw)?;
                    let text = rsb_sdk::crypto::paper::encode(&file.to_compact_bytes()?);
                    println!("{}", text);
                    println!(
                        "Keep this page with the same care as the password. Restore it with \
                         `rsb key import <file with this text>`."
                    );
                } else if let Some(out) = output {
                    write_private_file(out, &raw)?;
                    println!("✅ Key file exported to {} (mode 0600).", out.display());
                    println!(
                        "⚠️  Together with the password it decrypts every backup: store it safely."
                    );
                }
            }
            KeyCommand::Import {
                file,
                target,
                force,
            } => {
                let storage = open_storage(target).await?;
                let len = std::fs::metadata(file)?.len();
                if len > MAX_IMPORT_SIZE {
                    return Err(anyhow!("{} is too large for a key file", file.display()));
                }
                let content = Zeroizing::new(std::fs::read(file)?);
                let raw = if rsb_sdk::crypto::paper::is_paper(&content) {
                    let text = std::str::from_utf8(&content)?;
                    rsb_sdk::crypto::paper::decode(text).map_err(|e| anyhow!("Paper key: {}", e))?
                } else {
                    content.to_vec()
                };
                let pw = prompt("Password of the exported key: ")?;
                let out = lifecycle::import_key_file(storage, &raw, pw.as_bytes(), *force).await?;
                if out.unchanged {
                    println!(
                        "ℹ️  {} already holds this key file; nothing changed.",
                        REPO_KEY_PATH
                    );
                } else {
                    println!("✅ Imported {} ({}).", REPO_KEY_PATH, out.matched_by);
                    if let Some(b) = out.replaced_backup {
                        println!("ℹ️  The previous key file was kept at {}.", b);
                    }
                }
            }
            KeyCommand::Rotate { target, yes } => {
                let storage = open_storage(target).await?;
                let status = lifecycle::key_status(&*storage).await?;
                if !status.can_rotate() {
                    return Err(anyhow!(
                        "{} not found: run `rsb key init` or an encrypted backup first",
                        REPO_KEY_PATH
                    ));
                }
                if status.rotating {
                    println!("ℹ️  Resuming the interrupted key rotation.");
                } else if !*yes && std::io::stdin().is_terminal() {
                    println!(
                        "Key rotation re-encrypts every object and manifest of the repository: \
                         {} object(s) ({}) and {} manifest(s) are read and rewritten.",
                        status.encrypted_objects,
                        human_bytes(status.encrypted_bytes),
                        status.encrypted_manifests
                    );
                    if !confirm("Continue?")? {
                        return Ok(());
                    }
                }
                let pw = prompt("Repository password: ")?;
                let (cb, token) = progress_and_cancel();
                let rep =
                    lifecycle::rotate_repo_key(storage, pw.as_bytes(), Some(cb), Some(token)).await;
                eprintln!();
                let rep = rep?;
                println!(
                    "✅ Key rotated: {} -> {} ({} object(s) and {} manifest(s) re-encrypted, {} \
                     object(s) verified).",
                    rep.old_key_id,
                    rep.new_key_id,
                    rep.objects_rotated,
                    rep.manifests_rewritten,
                    rep.objects_verified
                );
                if !rep.unreadable_unreferenced.is_empty() {
                    println!(
                        "⚠️  {} unreferenced object(s) could not be re-encrypted and were left as \
                         they are (prune garbage-collects them).",
                        rep.unreadable_unreferenced.len()
                    );
                }
                println!(
                    "ℹ️  Old copies of {} (exports, backups) no longer open the data: export the \
                     new key file (`rsb key export`).",
                    REPO_KEY_PATH
                );
            }
            KeyCommand::Migrate { target } => {
                let storage = open_storage(target).await?;
                let pw = prompt("Repository password: ")?;
                let summary = lifecycle::legacy_summary(&*storage).await?;
                print_legacy_summary(&summary);
                run_migration(storage, pw.as_bytes()).await?;
            }
            KeyCommand::ChangePassword { target, kdf } => {
                let storage = open_storage(target).await?;
                let current = keyfile::read_key_file(&*storage).await?.ok_or_else(|| {
                    anyhow!(
                        "{} not found: run `rsb key init` or an encrypted backup first",
                        REPO_KEY_PATH
                    )
                })?;
                let old = prompt("Current password: ")?;
                // Fail fast (typed AuthenticationFailed) before asking for the new one.
                current.unwrap_key(old.as_bytes())?;
                let new = prompt_new()?;
                keyfile::change_password(
                    storage,
                    old.as_bytes(),
                    new.as_bytes(),
                    kdf.params(current.params()),
                )
                .await?;
                println!("✅ Repository password changed. Existing backups are unchanged.");
                println!(
                    "ℹ️  Update RSB_ENCRYPTION_KEY / the OS keyring entry used by scheduled backups."
                );
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_file_is_private_and_never_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("repo.key");
        write_private_file(&p, b"{}").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&p).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        assert!(write_private_file(&p, b"x").is_err());
        assert_eq!(std::fs::read(&p).unwrap(), b"{}");
    }

    #[test]
    fn kdf_overrides() {
        assert_eq!(KdfArgs::default().params(KdfParams::DEFAULT), None);
        let k = KdfArgs {
            t_cost: Some(4),
            ..Default::default()
        };
        assert_eq!(
            k.params(KdfParams::DEFAULT),
            Some(KdfParams {
                t_cost: 4,
                ..KdfParams::DEFAULT
            })
        );
    }
}
