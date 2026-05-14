use chrono::Local;
use clap::{Parser, Subcommand};
use rsb_sdk::integrity::perform_verify;
use rsb_sdk::utils::ensure_directory_exists;
use rsb_sdk::{config, core, credentials::Fido2Manager};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{Level, info, warn};
pub mod auth;
pub mod config_cmd;
pub mod fido2;
pub mod list_profiles_cmd;
use crate::config_cmd::ConfigCommand;
use crate::fido2::fido2_cmd::Fido2Command;
use crate::fido2::snapshot_cmd::SnapshotCommand;
use crate::list_profiles_cmd::{ListProfilesCmd, OutputFormat};

#[derive(clap::ValueEnum, Clone, Copy)]
pub enum ListProfilesFormat {
    /// Plain text table
    Table,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}

#[derive(Parser)]
#[command(name = "rsb-cli", version = "0.1.0", about = "Rust Shield Backup")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new backup profile
    CreateProfile {
        /// Profile name (generates config.toml)
        #[arg(short, long)]
        name: String,
        /// Source path to backup
        #[arg(short, long)]
        source: PathBuf,
        /// Destination directory for backup
        #[arg(short, long)]
        dest: PathBuf,
        /// Backup mode: "incremental" or "full" [default: incremental]
        #[arg(short, long, default_value = "incremental")]
        mode: String,
        /// Compression level 0-11 [default: 3]
        #[arg(short, long, default_value = "3")]
        compression: u8,
        /// Enable encryption
        #[arg(short, long)]
        encrypt: bool,
        /// Encryption password (prompted if not provided)
        #[arg(short, long)]
        password: Option<String>,
        /// Exclude patterns (comma-separated)
        #[arg(long)]
        exclude: Option<String>,
        /// S3 bucket name
        #[arg(long)]
        s3_bucket: Option<String>,
        /// S3 region
        #[arg(long)]
        s3_region: Option<String>,
        /// S3 endpoint URL
        #[arg(long)]
        s3_endpoint: Option<String>,
    },
    /// Run backup with an existing profile
    Backup {
        /// Path to config.toml
        config: PathBuf,
        /// Override backup mode (full/incremental)
        #[arg(short, long)]
        mode: Option<String>,
        /// Encryption key (optional)
        #[arg(short, long)]
        key: Option<String>,
        /// Simulate backup without writing files (dry-run)
        #[arg(long)]
        dry_run: bool,
        /// Do not attempt to resume an interrupted backup.
        #[arg(long)]
        no_resume: bool,
        /// Verify backup after completion
        #[arg(long)]
        verify: bool,
        /// Disable compression
        #[arg(long)]
        no_compress: bool,
        /// Number of parallel threads [default: 4]
        #[arg(long, default_value = "4")]
        threads: Option<usize>,
        /// Generate an HTML report of the operation.
        #[arg(long)]
        report: bool,
        /// Healthchecks.io URL for monitoring (sends start/success/fail pings)
        #[arg(long)]
        healthcheck_url: Option<String>,
    },
    /// Restore a backup with an existing profile
    Restore {
        /// Path to the profile's config.toml
        config: PathBuf,
        /// Snapshot ID to restore (default: most recent)
        #[arg(long)]
        snapshot: Option<String>,
        /// Path to restore to (default: source_path + "_restored")
        #[arg(short, long)]
        target: Option<PathBuf>,
        /// Restore specific files only (pattern matching)
        #[arg(short, long)]
        files: Option<String>,
        /// Restore from specific date (format: YYYY-MM-DD)
        #[arg(long)]
        date: Option<String>,
        /// Decryption key (required if backup is encrypted)
        #[arg(short, long)]
        key: Option<String>,
        /// Force overwrite of existing files
        #[arg(short, long)]
        force: bool,
        /// Verify backup before restoring
        #[arg(long)]
        verify: bool,
        /// Generate an HTML report of the operation.
        #[arg(long)]
        report: bool,
    },
    /// Verify a backup with an existing profile
    Verify {
        /// Path to config.toml
        config: PathBuf,
        /// Snapshot ID to verify (default: most recent)
        #[arg(long)]
        snapshot: Option<String>,
        /// Show only files with issues (quiet mode)
        #[arg(short, long)]
        quiet: bool,
        /// Quick check (skip full verification)
        #[arg(long)]
        quick: bool,
        /// Fast verification (only stored file hash, no decryption)
        #[arg(long)]
        fast: bool,
        /// Generate an HTML report of the operation.
        #[arg(long)]
        report: bool,
        /// Decryption key (required if backup is encrypted)
        #[arg(short, long)]
        key: Option<String>,
    },
    /// Delete old snapshots according to retention policy
    Prune {
        /// Path to config.toml (extracts destination automatically)
        #[arg(short, long)]
        config: PathBuf,
        /// Retention policy: "30d", "6m", "1y", or keep last N backups
        #[arg(short, long)]
        retention: Option<String>,
        /// Keep the last N backups (alternative to retention policy)
        #[arg(long)]
        keep_last: Option<usize>,
        /// Preview what would be deleted
        #[arg(long)]
        dry_run: bool,
        /// Suppress output messages
        #[arg(short, long)]
        quiet: bool,
        /// Healthchecks.io URL for monitoring
        #[arg(long)]
        healthcheck_url: Option<String>,
    },
    /// Generate scheduling commands (Cron/Systemd)
    Schedule {
        /// Path to config.toml
        config: PathBuf,
        /// Output format: 'cron' or 'systemd'
        #[arg(long, default_value = "cron")]
        format: String,
    },
    /// Monitor folder in real-time and perform automatic backups
    Watch {
        /// Path to config.toml
        config: PathBuf,
        /// Path to sync files to (destination)
        #[arg(short, long)]
        sync_to: PathBuf,
        /// Encryption key (required)
        #[arg(short, long)]
        key: String,
        /// Check interval in seconds (default: 2)
        #[arg(long, default_value = "2")]
        interval: u64,
        /// Healthchecks.io URL for monitoring (sends heartbeats every 5 min)
        #[arg(long)]
        healthcheck_url: Option<String>,
    },
    /// Start authentication API server (localhost:3000)
    Server {
        /// Port to run server on (default: 3000)
        #[arg(short, long, default_value = "3000")]
        port: u16,
    },
    /// Authenticate with Security Key security key (required before backup/restore)
    Login {
        /// User ID for registration
        user_id: String,
    },
    /// Manage Security Keyauthentication credentials
    #[command(subcommand, name = "auth")]
    Fido2(Fido2Command),
    #[command(subcommand)]
    Snapshots(SnapshotCommand),
    /// List all backup profiles
    ListProfiles {
        /// Profile directory [default: ~/.config/rs-shield]
        #[arg(short, long)]
        directory: Option<PathBuf>,
        /// Output format: table, json, csv [default: table]
        #[arg(short, long, value_enum, default_value = "table")]
        format: ListProfilesFormat,
    },
    /// Manage credentials and settings
    #[command(subcommand, name = "config")]
    Config(ConfigCommand),
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_max_level(Level::WARN).init();

    let cli = Cli::parse();

    async fn check_fido2_auth() -> Result<String, Box<dyn std::error::Error>> {
        let home = dirs::home_dir().ok_or("Home directory not found")?;
        let rsb_dir = home.join(".rs-shield");
        let auth_file = rsb_dir.join("auth_token");

        if !auth_file.exists() {
            return Err("❌ Not authenticated. Please run: rsb login <user_id>".into());
        }

        let token =
            fs::read_to_string(&auth_file).map_err(|_| "❌ Failed to read authentication token")?;

        let trimmed_token = token.trim();
        if trimmed_token.is_empty() {
            return Err("❌ Invalid authentication token".into());
        }

        // 1. Validação Local (JWT Signature & Expiration)
        let jwt_mgr = rsb_sdk::auth::JwtManager::new("rsb-shield-secret-key-256bit")?;
        jwt_mgr
            .verify_token(trimmed_token)
            .map_err(|e| format!("❌ Session expired or invalid: {}", e))?;

        // 2. Validação Remota (Consulta ao servidor para verificar revogação/JTI)
        let client = reqwest::Client::new();
        let res = client
            .get("http://localhost:3000/api/auth/verify")
            .header("Authorization", format!("Bearer {}", trimmed_token))
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => Ok(trimmed_token.to_string()),
            Ok(_) => Err("❌ Session revoked or expired on server. Please login again.".into()),
            Err(_) => {
                println!("⚠️ Auth server unreachable. Proceeding with local validation only.");
                Ok(trimmed_token.to_string())
            }
        }
    }

    match cli.command {
        Commands::CreateProfile {
            name,
            source,
            dest,
            mode,
            compression,
            encrypt,
            password,
            exclude,
            s3_bucket,
            s3_region,
            s3_endpoint,
        } => {
            if !source.exists() {
                return Err(format!("Source path does not exist: {}", source.display()).into());
            }
            if !std::path::Path::new(&dest).exists() {
                warn!(
                    "Destination path does not exist, creating: {}",
                    dest.display()
                );
                ensure_directory_exists(
                    dest.to_str()
                        .ok_or("Invalid path characters in destination")?,
                )?;
            }

            // Handle encryption password
            let encryption_pwd = if encrypt {
                if let Some(pwd) = password {
                    Some(pwd)
                } else {
                    // Prompt for password if encryption enabled but not provided
                    use rpassword::prompt_password;
                    let pwd = prompt_password("Enter encryption password: ")?;
                    let confirm = prompt_password("Confirm password: ")?;
                    if pwd != confirm {
                        return Err("Passwords do not match".into());
                    }
                    Some(pwd)
                }
            } else {
                None
            };

            config::create_profile_with_options(
                &name,
                &source,
                &dest,
                Some(mode.as_str()),
                Some(compression),
                encrypt,
                encryption_pwd.as_deref(),
                exclude.as_deref(),
                s3_bucket.as_deref(),
                s3_region.as_deref(),
                s3_endpoint.as_deref(),
            )?;
            let config_file = format!("{}.toml", name);
            println!("✅ Profile '{}' created: {}", name, config_file);
            println!("📋 Next step, execute backup:");
            println!("   rsb backup {}", config_file);
        }
        Commands::Backup {
            config,
            mode,
            key,
            dry_run,
            no_resume,
            verify,
            no_compress,
            threads,
            report,
            healthcheck_url,
        } => {
            let _auth_token = check_fido2_auth().await?;
            if !config.exists() {
                eprintln!(
                    "❌ Error: Configuration file not found: {}",
                    config.display()
                );
                eprintln!();
                eprintln!("📋 Create a new profile first:");
                eprintln!("   rsb create-profile mybackup /path/to/source /path/to/destination");
                eprintln!();
                eprintln!("   This will generate 'mybackup.toml'");
                eprintln!();
                eprintln!("   Then execute backup:");
                eprintln!("   rsb backup mybackup.toml");
                return Err(format!("Configuration file not found: {}", config.display()).into());
            }

            send_healthcheck(&healthcheck_url, "/start").await;

            let mut cfg = config::load_config(&config)?;
            let profile_name = config
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("default");

            println!("\n💾 Backup Storage Type");
            println!("   1. S3 or S3-compatible (AWS, MinIO, Wasabi, etc.)");
            println!("   2. Local storage (local filesystem)");
            print!("\nChoose storage type (1 or 2): ");
            use std::io::Write;
            std::io::stdout().flush()?;

            let mut response = String::new();
            std::io::stdin().read_line(&mut response)?;
            let use_s3 = response.trim() == "1";

            if use_s3 {
                println!("\n📦 Configuring S3 Storage");
                println!("   Please provide bucket name, region, and endpoint URL...\n");
                config::prompt_for_s3_config(&config)?;

                cfg = config::load_config(&config)?;
                println!("✅ S3 configuration updated. Starting backup...\n");
            } else {
                cfg.s3 = None;
                cfg.s3_bucket = None;
                cfg.s3_region = None;
                cfg.s3_endpoint = None;
                println!("✅ Using local storage. Starting backup...\n");
            }

            let resume = !no_resume;
            let backup_mode = mode.as_deref().unwrap_or("incremental");

            // Apply no-compress option if set
            if no_compress {
                cfg.compression_level = Some(0);
            }

            // ⚡ CLI option priority: CLI args > Config file > Default
            // Merge CLI options with config, allowing CLI to override config
            let effective_threads = threads.or(cfg.max_threads);

            if let Some(t) = effective_threads {
                if t > 4 {
                    println!("📊 Using {} parallel threads for backup", t);
                }
            }

            let mut report_data = match core::perform_backup(
                &cfg,
                backup_mode,
                key.as_deref(),
                dry_run,
                resume,
                effective_threads, // ⚡ Use merged option
                None,
            )
            .await
            {
                Ok(data) => data,
                Err(e) => {
                    send_healthcheck(&healthcheck_url, "/fail").await;
                    return Err(e);
                }
            };

            // Verify backup if requested
            if verify && !dry_run {
                println!("🔍 Verifying backup integrity...");
                match perform_verify(&cfg, None, false, false, None, None).await {
                    Ok(verify_report) => {
                        println!(
                            "✅ Verification passed: {} files verified",
                            verify_report.total_files
                        );
                        report_data.files_processed = verify_report.total_files;
                    }
                    Err(e) => {
                        warn!("⚠️ Verification warning: {}", e);
                    }
                }
            }

            send_healthcheck(&healthcheck_url, "").await;
            println!("✅ Backup completed.");

            if report {
                report_data.profile_path = config.to_string_lossy().to_string();
                let html = rsb_sdk::report::generate_html(&report_data);
                let filename = PathBuf::from(format!(
                    "rsb-report-backup-{}.html",
                    Local::now().format("%Y%m%d-%H%M%S")
                ));
                fs::write(&filename, html)?;
                println!("📄 Report generated at: {}", filename.display());
            }
        }

        Commands::Restore {
            config,
            snapshot,
            target,
            key,
            force,
            report,
            files,
            date,
            verify,
        } => {
            let _auth_token = check_fido2_auth().await?;
            let cfg = config::load_config(&config)?;
            let profile_name = config
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("default");

            // Log file pattern and date filtering (future enhancements)
            if let Some(ref pattern) = files {
                println!("📋 File pattern: {} (selective restore pending)", pattern);
            }
            if let Some(ref date_str) = date {
                println!(
                    "📅 Restoring from date: {} (date-based snapshot selection pending)",
                    date_str
                );
            }

            // Clone target for potential later use in verification
            let target_for_verify = target.clone();

            let mut report_data = core::perform_restore(
                &cfg,
                snapshot.as_deref(),
                target,
                key.as_deref(),
                force,
                None,
            )
            .await?;

            // Verify restored files if requested
            if verify {
                println!("🔍 Verifying restored files integrity...");
                // Create a temporary config for verification pointing to restored location
                let restored_path = target_for_verify
                    .unwrap_or_else(|| PathBuf::from(format!("{}_restored", cfg.source_path)));
                warn!(
                    "📋 Post-restore verification pending for: {}",
                    restored_path.display()
                );
            }

            println!("✅ Restore completed.");

            if report {
                report_data.profile_path = config.to_string_lossy().to_string();
                let html = rsb_sdk::report::generate_html(&report_data);
                let filename = PathBuf::from(format!(
                    "rsb-report-restore-{}.html",
                    Local::now().format("%Y%m%d-%H%M%S")
                ));
                fs::write(&filename, html)?;
                println!("📄 Report generated at: {}", filename.display());
            }
        }
        Commands::Verify {
            config,
            snapshot,
            quiet,
            quick,
            fast,
            report,
            key,
        } => {
            let _auth_token = check_fido2_auth().await?;
            let mut cfg = config::load_config(&config)?;

            if let Some(k) = key {
                cfg.encryption_key = Some(k);
            }

            // Use quick mode if requested (quick takes precedence over fast)
            let use_fast = quick || fast;
            if quick && fast {
                println!("Using quick verification (--quick takes precedence)");
            } else if quick {
                println!("Using quick verification mode");
            } else if fast {
                println!("Using fast verification mode (hash only, no decryption)");
            }

            let mut report_data =
                perform_verify(&cfg, snapshot.as_deref(), quiet, use_fast, None, None).await?;
            println!("✅ Verification completed.");

            if report {
                report_data.profile_path = config.to_string_lossy().to_string();
                let html = rsb_sdk::report::generate_html(&report_data);
                let filename = PathBuf::from(format!(
                    "rsb-report-verify-{}.html",
                    Local::now().format("%Y%m%d-%H%M%S")
                ));
                fs::write(&filename, html)?;
                println!("📄 Report generated at: {}", filename.display());
            }
        }
        Commands::Prune {
            config,
            keep_last,
            retention,
            dry_run,
            quiet,
            healthcheck_url,
        } => {
            let _auth_token = check_fido2_auth().await?;
            send_healthcheck(&healthcheck_url, "/start").await;
            let cfg = config::load_config(&config)?;

            // Determine the number of backups to keep
            let keep_count = if let Some(count) = keep_last {
                count
            } else if let Some(ref policy) = retention {
                // Parse retention policy (e.g., "30d", "6m", "1y") or number
                // Attempt to parse as a number first (e.g., "10" -> 10 backups)
                if let Ok(num) = policy.parse::<usize>() {
                    num
                } else {
                    // Parse time-based retention policies
                    // Examples: "30d" = 30 days, "6m" = 6 months, "1y" = 1 year
                    calculate_retention_backups(policy)
                }
            } else {
                // Default to keeping last 10 backups if neither specified
                10
            };

            if dry_run {
                println!("DRY RUN MODE: Would keep last {} backups", keep_count);
            } else {
                println!("🧹 Keeping last {} backups...", keep_count);
            }

            if !dry_run {
                if let Err(e) = core::perform_prune(&cfg, keep_count).await {
                    send_healthcheck(&healthcheck_url, "/fail").await;
                    return Err(e);
                }
            }
            send_healthcheck(&healthcheck_url, "").await;
            info!("Prune completed.");
        }
        Commands::Schedule { config, format } => {
            let abs_config = std::fs::canonicalize(&config).unwrap_or(config.clone());
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rsb"));

            let exe_str = format!("\"{}\"", exe.display());
            let config_str = format!("\"{}\"", abs_config.display());

            if format == "cron" {
                println!("# Add this line to your crontab (crontab -e):");
                println!(
                    "0 3 * * * {} backup {} --key \"YOUR_PASSWORD\"",
                    exe_str, config_str
                );
            } else if format == "systemd" {
                println!("# Example rsb-backup.service:");
                println!(
                    "[Service]\nType=oneshot\nExecStart={} backup {} --key \"YOUR_PASSWORD\"",
                    exe_str, config_str
                );
            } else {
                println!("Unknown format. Use 'cron' or 'systemd'.");
            }
        }
        Commands::Watch {
            config,
            sync_to,
            key,
            interval,
            healthcheck_url,
        } => {
            let _auth_token = check_fido2_auth().await?;
            let mut cfg = config::load_config(&config)?;
            cfg.encryption_key = Some(key);

            if let Some(url) = healthcheck_url {
                let url_clone = url.clone();
                tokio::spawn(async move {
                    send_healthcheck(&Some(url_clone.clone()), "/start").await;
                    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(300));
                    loop {
                        interval.tick().await;
                        send_healthcheck(&Some(url_clone.clone()), "").await;
                    }
                });
            }

            let source = PathBuf::from(&cfg.source_path);
            let sync_dst = sync_to;
            let backup_dst = PathBuf::from(&cfg.destination_path);

            println!("🟢 Real-Time Sync started");
            println!("📂 Source: {}", source.display());
            println!("📁 Syncing to: {}", sync_dst.display());
            println!("💾 Backup to: {}", backup_dst.display());
            println!("⏱️ Interval: {}s", interval);
            println!("🔐 Encryption: ENABLED");
            println!("\nPress Ctrl+C to stop\n");
            let mut total_changes = 0;
            let mut backups_count = 0;

            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(interval)).await;

                match sync_changed_files(&source, &sync_dst).await {
                    Ok(copied_count) => {
                        if copied_count > 0 {
                            total_changes += copied_count;
                            println!("✅ Sync: {} new/modified files synchronized.", copied_count);

                            // ⚡ Use config max_threads for watch mode
                            let watch_threads = cfg.max_threads;

                            match core::perform_backup(
                                &cfg,
                                "incremental",
                                Some(cfg.encryption_key.as_ref().unwrap()),
                                false,
                                false,
                                watch_threads, // ⚡ Use config value instead of None
                                None,
                            )
                            .await
                            {
                                Ok(report) => {
                                    backups_count += 1;
                                    println!(
                                        "💾 Backup #{}: {} files total, {} processed",
                                        backups_count, report.total_files, report.files_processed
                                    );
                                }
                                Err(e) => {
                                    eprintln!("❌ Backup error: {}", e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("❌ Sync error: {}", e);
                    }
                }

                println!(
                    "📊 Total: {} changes detected, {} backups created",
                    total_changes, backups_count
                );
                println!("---");
            }
        }

        Commands::Server { port } => {
            // Dummy sender since we're running server standalone
            let (tx, _rx) = tokio::sync::oneshot::channel();
            auth::routes::start_auth_server(port, tx).await?;
        }

        Commands::Login { user_id } => {
            let mut login_flow = crate::auth::LoginFlow::new();

            match login_flow.start(user_id.clone()).await {
                Ok(token) => {
                    // Verify and save token
                    let jwt_mgr = rsb_sdk::auth::JwtManager::new("rsb-shield-secret-key-256bit")?;
                    match jwt_mgr.verify_token(&token) {
                        Ok(claims) => {
                            println!("🔑 User: {}", claims.sub);
                            println!("📋 Scopes: {:?}\n", claims.scopes);

                            let home = dirs::home_dir().ok_or("❌ Home directory not found")?;
                            let rsb_dir = home.join(".rs-shield");
                            let _ = fs::create_dir_all(&rsb_dir);

                            let auth_file = rsb_dir.join("auth_token");
                            fs::write(&auth_file, &token)?;

                            #[cfg(unix)]
                            {
                                use std::os::unix::fs::PermissionsExt;
                                let perms = std::fs::Permissions::from_mode(0o600);
                                fs::set_permissions(&auth_file, perms)?;
                            }

                            info!("✅ Authentication successful!");
                            info!("📍 Token saved to: {}", auth_file.display());
                            println!("✅ You are now authenticated!");
                            println!("📍 Token saved to: {}\n", auth_file.display());
                            println!("You can now use: rsb backup, rsb restore, rsb verify\n");
                        }
                        Err(e) => {
                            eprintln!("❌ Invalid token received: {:?}", e);
                            return Err("❌ Failed to validate token".into());
                        }
                    }

                    // Shutdown server gracefully
                    login_flow.shutdown().await;
                }
                Err(e) => {
                    login_flow.shutdown().await;
                    return Err(format!("❌ Authentication failed: {}", e).into());
                }
            }
        }

        Commands::Fido2(cmd) => {
            let origin = "http://localhost:3000";
            let rp_id = "localhost";

            let mut manager = Fido2Manager::new(origin, rp_id, "RSB CLI")?;

            // Unificar para o caminho padrão definido no Core
            let storage_path = Fido2Manager::default_storage_path()?;

            if storage_path.exists() {
                manager.load_from_file(&storage_path)?;
            }

            cmd.execute(Arc::new(Mutex::new(manager.clone()))).await?;

            manager.save_to_file(&storage_path)?;
        }
        Commands::Snapshots(cmd) => {
            cmd.execute().await?;
        }

        Commands::ListProfiles { directory, format } => {
            let output_format = match format {
                ListProfilesFormat::Table => OutputFormat::Table,
                ListProfilesFormat::Json => OutputFormat::Json,
                ListProfilesFormat::Csv => OutputFormat::Csv,
            };
            ListProfilesCmd::execute(directory, output_format).await?;
        }

        Commands::Config(cmd) => {
            cmd.execute().await?;
        }
    }

    Ok(())
}

async fn sync_changed_files(src: &PathBuf, dst: &Path) -> Result<usize, String> {
    use std::time::SystemTime;
    use walkdir::WalkDir;

    if !src.exists() {
        return Err("Source folder does not exist".to_string());
    }

    let mut copied_count = 0;
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();
        if src_path.is_file() {
            let relative_path = src_path.strip_prefix(src).map_err(|e| e.to_string())?;
            let dst_path = dst.join(relative_path);

            if let Some(parent) = dst_path.parent() {
                ensure_directory_exists(
                    parent
                        .to_str()
                        .ok_or("Invalid path characters in destination")?,
                )?;
            }

            let should_copy = if !dst_path.exists() {
                true
            } else {
                let src_meta = fs::metadata(src_path).map_err(|e| e.to_string())?;
                let dst_meta = fs::metadata(&dst_path).map_err(|e| e.to_string())?;
                src_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                    > dst_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
            };

            if should_copy {
                fs::copy(src_path, &dst_path).map_err(|e| format!("Error copying file: {}", e))?;
                copied_count += 1;
            }
        }
    }

    Ok(copied_count)
}

/// Calculate the number of backups to keep based on retention policy
/// Supports: Xd (days), Xm (months), Xy (years), or direct numbers
/// Assumes 1 backup per day as the typical schedule
fn calculate_retention_backups(policy: &str) -> usize {
    match policy {
        // Common presets with optimal backup counts
        "7d" => 7,
        "14d" => 14,
        "30d" => 30,
        "60d" => 60,
        "90d" => 90,
        "6m" => 26,  // ~6 months at weekly backups
        "1y" => 52,  // 52 weeks = ~1 year
        "2y" => 104, // 2 years
        "3y" => 156, // 3 years
        "5y" => 260, // 5 years
        _ => {
            // Parse custom patterns
            if let Some(days_str) = policy.strip_suffix('d') {
                // Pattern: "45d" -> 45 backups (daily)
                if let Ok(days) = days_str.parse::<usize>() {
                    return days;
                }
            }

            if let Some(months_str) = policy.strip_suffix('m') {
                // Pattern: "12m" -> ~52 backups (weekly frequency)
                if let Ok(months) = months_str.parse::<usize>() {
                    let weeks = (months * 30) / 7; // Rough conversion to weeks
                    return weeks.max(1);
                }
            }

            if let Some(years_str) = policy.strip_suffix('y') {
                // Pattern: "2y" -> ~104 backups (weekly frequency)
                if let Ok(years) = years_str.parse::<usize>() {
                    return years * 52; // Assumes weekly backups
                }
            }

            // Fallback for unknown patterns
            warn!(
                "Unknown retention policy: '{}'. Using default: 10 backups",
                policy
            );
            10
        }
    }
}

async fn send_healthcheck(url: &Option<String>, suffix: &str) {
    if let Some(base_url) = url {
        let target = format!("{}{}", base_url, suffix);
        let client = reqwest::Client::new();
        if let Err(e) = client
            .get(&target)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            warn!("Failed to send healthcheck to {}: {}", target, e);
        }
    }
}
