use clap::{Subcommand};
use std::path::{PathBuf};

use crate::command::{config_cmd::ConfigCommand, fido2_cmd::Fido2Command, snapshot_cmd::SnapshotCommand};

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ListProfilesFormat {
    /// Plain text table
    Table,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}
#[derive(Subcommand)]
pub enum Commands {
    /// Create a new backup profile
    CreateProfile {
        /// Profile name (generates config.toml)
        #[arg(short = 'n', long)]
        name: String,

        /// Source path to backup
        #[arg(short = 's', long)]
        source: PathBuf,

        /// Destination directory for backup
        #[arg(short = 'd', long)]
        dest: PathBuf,

        /// Backup mode: "incremental" or "full"
        #[arg(short = 'm', long, default_value = "incremental")]
        mode: String,

        /// Compression level 0-11
        #[arg(short = 'z', long, default_value = "3")]
        compression: u8,

        /// Enable encryption
        #[arg(short = 'e', long)]
        encrypt: bool,

        /// Encryption password
        #[arg(short = 'k', long)]
        password: Option<String>,

        /// Exclude patterns (comma-separated)
        #[arg(short = 'x', long)]
        exclude: Option<String>,

        /// S3 bucket name
        #[arg(short = 'b', long)]
        s3_bucket: Option<String>,

        /// S3 region
        #[arg(short = 'r', long)]
        s3_region: Option<String>,

        /// S3 endpoint URL
        #[arg(short = 'E', long)]
        s3_endpoint: Option<String>,

        /// Portable mode (store relative paths for cross-computer compatibility)
        #[arg(long)]
        portable: bool,
    },

    /// Run backup with an existing profile
    Backup {
        /// Path to config.toml
        config: PathBuf,

        /// Override backup mode
        #[arg(short = 'm', long)]
        mode: Option<String>,

        /// Encryption key
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Simulate backup without writing files
        #[arg(short = 'd', long)]
        dry_run: bool,

        /// Do not resume interrupted backup
        #[arg(long)]
        no_resume: bool,

        /// Verify backup after completion
        #[arg(short = 'v', long)]
        verify: bool,

        /// Disable compression
        #[arg(long)]
        no_compress: bool,

        /// Number of parallel threads
        #[arg(short = 't', long, default_value = "4")]
        threads: Option<usize>,

        /// Generate HTML report
        #[arg(short = 'r', long)]
        report: bool,

        /// Healthchecks.io URL
        #[arg(short = 'H', long)]
        healthcheck_url: Option<String>,
    },

    /// Restore a backup
    Restore {
        /// Path to config.toml (optional, use --backup for direct backup restore)
        config: Option<PathBuf>,

        /// Direct backup path (alternative to config, useful for portable backups)
        #[arg(short = 'b', long)]
        backup: Option<PathBuf>,

        /// Snapshot ID
        #[arg(short = 's', long)]
        snapshot: Option<String>,

        /// Restore target path
        #[arg(short = 't', long, required = true)]
        target: PathBuf,

        /// Restore specific files
        #[arg(short = 'i', long)]
        files: Option<String>,

        /// Restore from date
        #[arg(short = 'd', long)]
        date: Option<String>,

        /// Decryption key
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Force overwrite
        #[arg(short = 'f', long)]
        force: bool,

        /// Create a timestamped folder inside target instead of direct restore
        #[arg(short = 'V', long)]
        versioned: bool,

        /// Verify before restore
        #[arg(short = 'v', long)]
        verify: bool,

        /// Generate HTML report
        #[arg(short = 'r', long)]
        report: bool,
    },

    /// Verify a backup (by config file or direct path)
    Verify {
        /// Path to config.toml (optional, use --backup for direct path)
        config: Option<PathBuf>,

        /// Direct backup path (alternative to config)
        #[arg(short = 'b', long)]
        backup: Option<PathBuf>,

        /// Snapshot ID
        #[arg(short = 's', long)]
        snapshot: Option<String>,

        /// Quiet mode
        #[arg(short = 'q', long)]
        quiet: bool,

        /// Quick verification
        #[arg(long)]
        quick: bool,

        /// Fast verification
        #[arg(short = 'f', long)]
        fast: bool,

        /// Generate HTML report
        #[arg(short = 'r', long)]
        report: bool,

        /// Decryption key
        #[arg(short = 'k', long)]
        key: Option<String>,
    },

    /// Delete old snapshots
    Prune {
        /// Path to config.toml
        #[arg(short = 'c', long)]
        config: PathBuf,

        /// Retention policy
        #[arg(short = 'r', long)]
        retention: Option<String>,

        /// Keep last N backups
        #[arg(short = 'k', long)]
        keep_last: Option<usize>,

        /// Preview deletion
        #[arg(short = 'd', long)]
        dry_run: bool,

        /// Quiet mode
        #[arg(short = 'q', long)]
        quiet: bool,

        /// Healthchecks.io URL
        #[arg(short = 'H', long)]
        healthcheck_url: Option<String>,
    },

    /// Generate scheduling commands
    Schedule {
        /// Path to config.toml
        config: PathBuf,

        /// Output format
        #[arg(short = 'f', long, default_value = "cron")]
        format: String,
    },

    /// Watch filesystem changes
    Watch {
        /// Path to config.toml
        config: PathBuf,

        /// Sync destination
        #[arg(short = 's', long)]
        sync_to: PathBuf,

        /// Encryption key
        #[arg(short = 'k', long)]
        key: String,

        /// Poll interval in seconds
        #[arg(short = 'i', long, default_value = "2")]
        interval: u64,

        /// Healthchecks.io URL
        #[arg(short = 'H', long)]
        healthcheck_url: Option<String>,
    },

    /// Start authentication API server
    Server {
        /// Server port
        #[arg(short = 'p', long, default_value = "3000")]
        port: u16,
    },

    /// Authenticate with Security Key
    Login {
        /// User ID
        user_id: String,

        /// Use recovery code instead of FIDO2
        #[arg(short = 'r', long)]
        recovery: bool,
    },

    /// Manage Security Key credentials
    #[command(subcommand, name = "auth")]
    Fido2(Fido2Command),

    #[command(subcommand)]
    Snapshots(SnapshotCommand),

    /// List backup profiles
    ListProfiles {
        /// Profile directory
        #[arg(short, long = "dir")]
        directory: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, value_enum, default_value = "table")]
        format: ListProfilesFormat,
    },



    /// Diagnose backup issues
    Diagnose {
        /// Path to backup folder
        #[arg(short = 'b', long)]
        backup: PathBuf,

        /// Encryption key (if encrypted)
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Verbose output
        #[arg(short = 'v', long)]
        verbose: bool,

        /// Generate JSON report
        #[arg(short = 'j', long)]
        json: bool,

        /// Repair mode (attempt to fix issues)
        #[arg(long)]
        repair: bool,
    },

    /// Manage credentials and settings
    #[command(subcommand, name = "config")]
    Config(ConfigCommand),
}
