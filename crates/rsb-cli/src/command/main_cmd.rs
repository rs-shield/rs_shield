use clap::Subcommand;
use std::path::PathBuf;

use crate::command::{
    config_cmd::ConfigCommand, fido2_cmd::Fido2Command, key_cmd::KeyCommand,
    snapshot_cmd::SnapshotCommand,
};

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ListProfilesFormat {
    /// Plain text table
    Table,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}
/// Tipo de armazenamento para o backup (override do perfil)
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageChoice {
    /// S3 ou compatível (usa a configuração S3 do perfil)
    S3,
    /// Sistema de ficheiros local
    Local,
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

        /// Compression level 0-22 (zstd; 0 = default)
        #[arg(short = 'z', long, default_value = "3", value_parser = clap::value_parser!(u8).range(0..=22))]
        compression: u8,

        /// Enable encryption
        #[arg(short = 'e', long)]
        encrypt: bool,

        /// Encryption password (DEPRECATED: visible in `ps`/shell history; omit to be prompted)
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
        /// Path to the profile .toml (optional: auto-discovered when the
        /// current directory holds exactly one profile)
        config: Option<PathBuf>,

        /// Repository directory: uses the profile (in that directory or in
        /// the current one) whose destination_path is DIR. Fails if there is
        /// none or several; it is never read as a profile file
        #[arg(short = 'b', long, value_name = "DIR", conflicts_with = "config")]
        backup: Option<PathBuf>,

        /// Override backup mode
        #[arg(short = 'm', long)]
        mode: Option<String>,

        /// Encryption key (DEPRECATED: visible in `ps`/history; prefer env RSB_ENCRYPTION_KEY, keyring or prompt)
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Storage override: "s3" or "local" (default: what the profile configures)
        #[arg(long, value_enum)]
        storage: Option<StorageChoice>,

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

        /// Snapshot ID (default: the latest snapshot)
        #[arg(short = 's', long)]
        snapshot: Option<String>,

        /// Restore target path
        #[arg(short = 't', long, required = true)]
        target: PathBuf,

        /// Restore only matching files: exact path (docs/a.txt), directory
        /// (docs or docs/) or glob (docs/*.pdf, '**/*.jpg'), relative to the
        /// backup source with '/' separators. Repeatable; quote globs.
        /// Unicode NFC/NFD spellings match each other. Fails if nothing
        /// matches
        #[arg(short = 'i', long, value_name = "PATTERN", num_args = 1.., action = clap::ArgAction::Append)]
        files: Vec<String>,

        /// Restore the latest snapshot taken at or before WHEN: RFC 3339
        /// (2026-10-01T12:00:00Z), YYYY-MM-DD (end of that day, local time)
        /// or "YYYY-MM-DD HH:MM" (local time)
        #[arg(short = 'd', long, value_name = "WHEN", conflicts_with = "snapshot")]
        date: Option<String>,

        /// Decryption key (DEPRECATED: visible in `ps`/history; prefer env RSB_ENCRYPTION_KEY or prompt)
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

        /// Decryption key (DEPRECATED: visible in `ps`/history; prefer env RSB_ENCRYPTION_KEY or prompt)
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Delete objects proven corrupt (forces a full check) so the next backup re-uploads them
        #[arg(long)]
        repair: bool,
    },

    /// Delete old snapshots
    Prune {
        /// Path to the profile .toml (optional: auto-discovered; or use --backup)
        config: Option<PathBuf>,

        /// Repository directory to prune directly, without a profile (key
        /// from RSB_ENCRYPTION_KEY or the prompt)
        #[arg(short = 'b', long, value_name = "DIR", conflicts_with = "config")]
        backup: Option<PathBuf>,

        /// Retention policy
        #[arg(short = 'r', long)]
        retention: Option<String>,

        /// Keep last N backups
        #[arg(short = 'l', long)]
        keep_last: Option<usize>,

        /// Preview deletion
        #[arg(short = 'd', long)]
        dry_run: bool,

        /// Confirm destructive deletion (required unless --dry-run)
        #[arg(long)]
        confirm: bool,

        /// Quiet mode
        #[arg(short = 'q', long)]
        quiet: bool,

        /// Healthchecks.io URL
        #[arg(short = 'H', long)]
        healthcheck_url: Option<String>,
    },

    /// Generate scheduling commands
    Schedule {
        /// Path to the profile .toml (optional: auto-discovered when the
        /// current directory holds exactly one profile)
        config: Option<PathBuf>,

        /// Repository directory: uses the profile (in that directory or in
        /// the current one) whose destination_path is DIR. Fails if there is
        /// none or several; it is never read as a profile file
        #[arg(short = 'b', long, value_name = "DIR", conflicts_with = "config")]
        backup: Option<PathBuf>,

        /// Output format
        #[arg(short = 'f', long, default_value = "cron")]
        format: String,
    },

    /// Watch filesystem changes
    Watch {
        /// Path to the profile .toml (optional: auto-discovered when the
        /// current directory holds exactly one profile)
        config: Option<PathBuf>,

        /// Repository directory: uses the profile (in that directory or in
        /// the current one) whose destination_path is DIR. Fails if there is
        /// none or several; it is never read as a profile file
        #[arg(short = 'b', long, value_name = "DIR", conflicts_with = "config")]
        backup: Option<PathBuf>,

        /// Sync destination
        #[arg(short = 's', long)]
        sync_to: PathBuf,

        /// Encryption key (DEPRECATED: visible in `ps`/history; prefer env RSB_ENCRYPTION_KEY, keyring or prompt)
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Quiet period in seconds: a backup starts once no change has arrived for this long
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

    /// Manage the repository key (format v3): init, change-password
    #[command(subcommand)]
    Key(KeyCommand),

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

        /// Encryption key if encrypted (DEPRECATED: visible in `ps`/history; prefer env RSB_ENCRYPTION_KEY or prompt)
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
