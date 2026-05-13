use clap::{Parser, Subcommand};
use rsb_sdk::{config, core, integrity, perform_verify};
use std::path::PathBuf;
use tracing::{Level, info};

#[derive(Parser)]
#[command(name = "rsb", version = "0.1.0", about = "Rust Shield Backup")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Creates a new backup profile
    CreateProfile {
        /// Profile name (generates config.toml)
        name: String,
        /// Source path
        source: PathBuf,
        /// Destination path
        dest: PathBuf,
    },
    /// Runs backup with existing profile
    Backup {
        /// Path to config.toml
        config: PathBuf,
        /// Mode: full or incremental
        #[arg(default_value = "incremental")]
        mode: String,
        /// Encryption key (optional)
        #[arg(short, long)]
        key: Option<String>,
        /// Simulates backup without writing files (Dry-run)
        #[arg(long)]
        dry_run: bool,
    },
    Restore {
        /// Path to profile config.toml
        config: PathBuf,
        /// Snapshot ID to restore (default: latest)
        #[arg(long)]
        snapshot: Option<String>,
        /// Restore destination path (default: source_path + "_restored")
        #[arg(short, long)]
        target: Option<PathBuf>,
        /// Decryption key (required if backup is encrypted)
        #[arg(short, long)]
        key: Option<String>,
        /// Force overwrite of existing files
        #[arg(short, long)]
        force: bool,
    },
    Verify {
        /// Path to config.toml
        config: PathBuf,
        /// Snapshot ID to verify (default: latest)
        #[arg(long)]
        snapshot: Option<String>,
        /// Show only files with issues (quiet mode)
        #[arg(short, long)]
        quiet: bool,
        /// Fast check (stored file hash only, no decryption)
        #[arg(long)]
        fast: bool,
    },
    /// Deletes old snapshots according to retention policy
    Prune {
        /// Path to config.toml
        config: PathBuf,
        /// Keep the last N backups.
        #[arg(long, required = true)]
        keep_last: usize,
    },
    /// Generates scheduling commands (Cron/Systemd)
    Schedule {
        /// Path to config.toml
        config: PathBuf,
        /// Output type: 'cron' or 'systemd'
        #[arg(long, default_value = "cron")]
        format: String,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();
    info!("Starting RSB");

    let cli = Cli::parse();

    match cli.command {
        Commands::CreateProfile { name, source, dest } => {
            config::create_profile(&name, &source, &dest)?;
            info!("Profile {} created.", name);
        }
        Commands::Backup {
            config,
            mode,
            key,
            dry_run,
        } => {
            let cfg = config::load_config(&config)?;
            core::perform_backup(&cfg, &mode, key.as_deref(), dry_run, true, None, None).await?;
            info!("Backup completed.");
        }
        Commands::Restore {
            config,
            snapshot,
            target,
            key,
            force,
        } => {
            let cfg = config::load_config(&config)?;
            core::perform_restore(
                &cfg,
                snapshot.as_deref(),
                target,
                key.as_deref(),
                force,
                None,
            )
            .await?;
            info!("Restore completed.");
        }
        Commands::Verify {
            config,
            snapshot,
            quiet,
            fast,
        } => {
            let cfg = config::load_config(&config)?;
            perform_verify(&cfg, snapshot.as_deref(), quiet, fast, None, None).await?;
            info!("Verification completed.");
        }
        Commands::Prune { config, keep_last } => {
            let cfg = config::load_config(&config)?;
            core::perform_prune(&cfg, keep_last).await?;
            info!("Prune completed.");
        }
        Commands::Schedule { config, format } => {
            let abs_config = std::fs::canonicalize(&config).unwrap_or(config.clone());
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rsb"));

            // Quote paths to handle spaces (common on Windows/macOS)
            let exe_str = format!("\"{}\"", exe.display());
            let config_str = format!("\"{}\"", abs_config.display());

            if format == "cron" {
                println!("# Add this line to your crontab (crontab -e):");
                println!(
                    "0 3 * * * {} backup {} --key \"YOUR_KEY_HERE\"",
                    exe_str, config_str
                );
            } else if format == "systemd" {
                println!("# Example of rsb-backup.service:");
                println!(
                    "[Service]\nType=oneshot\nExecStart={} backup {} --key \"YOUR_KEY_HERE\"",
                    exe_str, config_str
                );
            } else {
                println!("Unknown format. Use 'cron' or 'systemd'.");
            }
        }
    }

    Ok(())
}
