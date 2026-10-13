use anyhow::Result;
use chrono::Local;
use clap::Parser;
use rsb_cli::command::list_profiles_cmd::{ListProfilesCmd, OutputFormat};
use rsb_cli::command::main_cmd::{Commands, ListProfilesFormat, StorageChoice};
use rsb_cli::helpers::{
    check_repository_dir, is_s3_url, obtain_encryption_key, plan_profile_destination, profile_name,
    profile_uses_relative_paths, render_schedule, resolve_command_profile, resolve_profile_path,
    warn_secret_arg,
};
use rsb_sdk::backup::diagnostic::{print_diagnostics, run_backup_diagnostics};
use rsb_sdk::backup::perform_backup;
use rsb_sdk::integrity::perform_verify;
use rsb_sdk::server;
use rsb_sdk::server::LoginFlow;
use rsb_sdk::server::routes::check_fido2_auth;
use rsb_sdk::utils::{calculate_retention_backups, ensure_directory_exists, send_healthcheck};
use rsb_sdk::{BackupOptions, RestoreOptions, VerifyOptions};
use rsb_sdk::{config, core, credentials::Fido2Manager};
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Parser)]
#[command(name = "rsb-cli", version, about = "Rust Shield Backup")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    rsb_cli::logging::init_logging();

    let cli = Cli::parse();

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
            portable,
        } => {
            if !source.exists() {
                return Err(format!("Source path does not exist: {}", source.display()).into());
            }
            // `--dest s3://bucket` (or --s3-bucket) is an S3 profile: nothing
            // is created locally (this used to create a local "s3:" folder).
            let target = plan_profile_destination(&dest, s3_bucket.as_deref())?;
            for w in &target.warnings {
                eprintln!("⚠️  {}", w);
            }
            let s3_bucket = target.s3_bucket.clone();
            let dest = target.destination.clone();
            if !target.is_s3() && !dest.exists() {
                warn!(
                    "Destination path does not exist, creating: {}",
                    dest.display()
                );
                ensure_directory_exists(
                    dest.to_str()
                        .ok_or("Invalid path characters in destination")?,
                )?;
            }

            // Handle encryption password: é guardada no keyring do SO (nunca no
            // TOML). Sem TTY e sem -k, o perfil fica só com encrypt_patterns e a
            // chave terá de vir de RSB_ENCRYPTION_KEY no momento do backup.
            if password.is_some() {
                warn_secret_arg("-k/--password");
            }
            let encryption_pwd = if encrypt {
                if let Some(pwd) = password {
                    Some(pwd)
                } else if std::io::stdin().is_terminal() {
                    // Prompt for password if encryption enabled but not provided
                    use rpassword::prompt_password;
                    let pwd = prompt_password(
                        "Enter encryption password (stored in OS keyring; empty = use RSB_ENCRYPTION_KEY at backup time): ",
                    )?;
                    if pwd.is_empty() {
                        None
                    } else {
                        let confirm = prompt_password("Confirm password: ")?;
                        if pwd != confirm {
                            return Err("Passwords do not match".into());
                        }
                        Some(pwd)
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Convert paths to relative if portable mode is enabled
            let (source_for_profile, dest_for_profile) = if portable {
                let source_rel = source
                    .canonicalize()
                    .ok()
                    .and_then(|p| pathdiff::diff_paths(&p, std::env::current_dir().ok()?))
                    .unwrap_or(source.clone());
                let dest_rel = if is_s3_url(&dest.to_string_lossy()) {
                    dest.clone()
                } else {
                    dest.canonicalize()
                        .ok()
                        .and_then(|p| pathdiff::diff_paths(&p, std::env::current_dir().ok()?))
                        .unwrap_or(dest.clone())
                };
                (source_rel, dest_rel)
            } else {
                (source.clone(), dest.clone())
            };

            config::create_profile_with_options(
                &name,
                &source_for_profile,
                &dest_for_profile,
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
            if portable {
                println!("📱 Portable mode enabled: using relative paths");
                println!("   This profile can be moved between computers!");
            }
            if let Some(bucket) = &s3_bucket {
                println!("☁️  Backups go to S3 bucket '{}'", bucket);
                println!(
                    "   Credentials: AWS_ACCESS_KEY_ID / AWS_SECRET_ACCESS_KEY or `rsb config s3-credentials`"
                );
            }
            println!("📋 Next step, execute backup:");
            println!("   rsb backup {}", config_file);
        }
        Commands::Backup {
            config,
            backup,
            mode,
            key,
            storage,
            dry_run,
            no_resume,
            verify,
            no_compress,
            threads,
            report,
            healthcheck_url,
        } => {
            let _auth_token = check_fido2_auth().await?;

            // CONFIG, or the profile of the repository given with --backup
            // (never read as a profile file; Cargo.toml is never a profile).
            let config_path = resolve_command_profile("backup", config, backup)?;

            if !config_path.exists() {
                eprintln!(
                    "❌ Error: Configuration file not found: {}",
                    config_path.display()
                );
                eprintln!();
                eprintln!("📋 Create a new profile first:");
                eprintln!("   rsb create-profile mybackup /path/to/source /path/to/destination");
                eprintln!();
                eprintln!("   This will generate 'mybackup.toml'");
                eprintln!();
                eprintln!("   Then execute backup:");
                eprintln!("   rsb backup mybackup.toml");
                return Err(
                    format!("Configuration file not found: {}", config_path.display()).into(),
                );
            }

            let mut cfg = config::load_config(&config_path)?;

            if let Some(level) = cfg.compression_level {
                config::validate_compression_level(level)?;
            }

            // Storage: --storage > configuração do perfil. Só se pergunta em
            // modo interativo (TTY) quando o perfil não tem S3 configurado;
            // em cron/systemd nunca se lê stdin.
            let s3_configured = cfg.s3.as_ref().and_then(|s| s.bucket.as_ref()).is_some()
                || cfg.s3_bucket.is_some();
            let interactive = std::io::stdin().is_terminal();
            let choice = match storage {
                Some(c) => c,
                None if s3_configured => StorageChoice::S3,
                None if interactive => {
                    println!("\n💾 Backup Storage Type (profile has no S3 configured)");
                    println!("   1. S3 or S3-compatible (AWS, MinIO, Wasabi, etc.)");
                    println!("   2. Local storage (local filesystem) [default]");
                    print!("\nChoose storage type (1 or 2): ");
                    use std::io::Write;
                    std::io::stdout().flush()?;

                    let mut response = String::new();
                    std::io::stdin().read_line(&mut response)?;
                    if response.trim() == "1" {
                        StorageChoice::S3
                    } else {
                        StorageChoice::Local
                    }
                }
                None => StorageChoice::Local,
            };

            match choice {
                StorageChoice::S3 => {
                    if !s3_configured {
                        if !interactive {
                            return Err(
                                "--storage s3 requested but the profile has no S3 configuration (run interactively to configure it)"
                                    .into(),
                            );
                        }
                        println!("\n📦 Configuring S3 Storage");
                        println!("   Please provide bucket name, region, and endpoint URL...\n");
                        config::prompt_for_s3_config(&config_path)?;
                        cfg = config::load_config(&config_path)?;
                        println!("✅ S3 configuration updated.");
                    }
                    println!("✅ Using S3 storage. Starting backup...\n");
                }
                StorageChoice::Local => {
                    if is_s3_url(&cfg.destination_path) {
                        return Err(format!(
                            "--storage local: this profile has no local destination ({}); \
                             set destination_path to a directory to back up locally",
                            cfg.destination_path
                        )
                        .into());
                    }
                    cfg.s3 = None;
                    cfg.s3_bucket = None;
                    cfg.s3_region = None;
                    cfg.s3_endpoint = None;
                    println!("✅ Using local storage. Starting backup...\n");
                }
            }

            // Chave: falha (em vez de backup em claro) se o perfil exige cifra.
            let profile = profile_name(&config_path);
            let key = obtain_encryption_key(
                key,
                profile.as_deref(),
                Some(&cfg),
                cfg.encryption_required(),
            );
            let key = match key {
                Ok(k) => k,
                Err(e) => {
                    // Sinalizar falha no healthcheck (ex.: cron sem chave).
                    send_healthcheck(&healthcheck_url, "/fail").await;
                    return Err(e.into());
                }
            };

            send_healthcheck(&healthcheck_url, "/start").await;

            let resume = !no_resume;
            let backup_mode = mode.as_deref().unwrap_or("incremental");

            // Apply no-compress option if set
            if no_compress {
                cfg.compression_level = Some(0);
            }

            // ⚡ CLI option priority: CLI args > Config file > Default
            // Merge CLI options with config, allowing CLI to override config
            let effective_threads = threads.or(cfg.max_threads);

            if let Some(t) = effective_threads
                && t > 4
            {
                println!("📊 Using {} parallel threads for backup", t);
            }

            // Debug info
            println!("📂 Source: {}", cfg.source_path);
            println!("💾 Destination: {}", cfg.destination_path);
            if !cfg.exclude_patterns.is_empty() {
                println!("🚫 Exclude patterns: {}", cfg.exclude_patterns.join(", "));
            }
            println!();

            let mut report_data = match perform_backup(
                &cfg,
                BackupOptions {
                    mode: Some(backup_mode.to_string()),
                    dry_run,
                    resume,
                    max_threads: effective_threads, // ⚡ Use merged option
                    ..Default::default()
                }
                .with_encryption_key(key.as_deref()),
            )
            .await
            {
                Ok(data) => data,
                Err(e) => {
                    send_healthcheck(&healthcheck_url, "/fail").await;
                    return Err(e.into());
                }
            };

            // Verify backup if requested
            if verify && !dry_run {
                println!("🔍 Verifying backup integrity...");
                match perform_verify(
                    &cfg,
                    VerifyOptions::default().with_encryption_key(key.as_deref()),
                )
                .await
                {
                    Ok(verify_report) => {
                        println!(
                            "✅ Verification passed: {} files verified",
                            verify_report.total_files
                        );
                        report_data.files_processed = verify_report.total_files;
                    }
                    Err(e) => {
                        send_healthcheck(&healthcheck_url, "/fail").await;
                        return Err(format!("❌ Backup verification failed: {}", e).into());
                    }
                }
            }

            send_healthcheck(&healthcheck_url, "").await;
            println!("✅ Backup completed.");

            if report {
                report_data.profile_path = config_path.to_string_lossy().to_string();
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
            backup,
            snapshot,
            target,
            key,
            force,
            versioned,
            report,
            files,
            date,
            verify,
        } => {
            let _auth_token = check_fido2_auth().await?;

            // User input is validated before any repository access.
            rsb_sdk::core::FileSelector::new(&files)?;
            let date = date
                .as_deref()
                .map(rsb_sdk::core::parse_restore_date)
                .transpose()?;

            if key.is_some() {
                warn_secret_arg("--key");
            }
            let key = config::resolve_encryption_key(
                key,
                config.as_deref().and_then(profile_name).as_deref(),
                None,
            );

            // Either config or backup must be provided
            let (cfg, _profile_name) = if let Some(ref config_path) = config {
                // Traditional mode: use config file
                let cfg = config::load_config(config_path)?;
                let name = config_path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("default")
                    .to_string();
                (cfg, name)
            } else if let Some(ref backup_path) = backup {
                // Direct backup mode: use portable restore function
                if !backup_path.exists() {
                    eprintln!("❌ Error: Backup path not found: {}", backup_path.display());
                    return Err(format!("Backup not found: {}", backup_path.display()).into());
                }

                println!("📁 Restoring from backup: {}", backup_path.display());

                // Validate backup structure before attempting restore
                let validation = rsb_sdk::validate_backup_structure(backup_path);
                if !validation.is_valid {
                    eprintln!("⚠️  Backup validation issues detected:");
                    for issue in &validation.issues {
                        eprintln!("  ❌ {}", issue);
                    }
                    for warning in &validation.warnings {
                        eprintln!("  ⚠️  {}", warning);
                    }
                    println!("💡 Suggestions:");
                    for suggestion in &validation.suggestions {
                        println!("  • {}", suggestion);
                    }
                    return Err("Backup validation failed".into());
                }

                println!(
                    "✅ Backup validation passed. Found {} snapshots, {} data files",
                    validation.snapshots_count, validation.data_files_count
                );

                // Create config using portable restore function
                let cfg = rsb_sdk::config_from_backup_path(backup_path, key.clone())
                    .map_err(|e| format!("Failed to create config from backup: {}", e))?;

                let name = backup_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or("backup")
                    .to_string();
                (cfg, name)
            } else {
                return Err(
                    "❌ Error: Either --config or --backup is required for restore operation"
                        .into(),
                );
            };

            // Clone target for potential later use in verification
            let _target_for_verify = target.clone();

            let key = key.or_else(|| cfg.encryption_key.clone());
            // --date is resolved here (not through RestoreOptions::at_or_before)
            // so the chosen snapshot is shown and --verify checks the same one.
            let snapshot = match date {
                Some(when) => {
                    let id = rsb_sdk::core::find_snapshot_at_or_before_in(&cfg, None, when).await?;
                    println!(
                        "📅 Latest snapshot at or before {}: {}",
                        when.with_timezone(&Local).format("%Y-%m-%d %H:%M:%S %:z"),
                        id
                    );
                    Some(id)
                }
                None => snapshot,
            };
            if !files.is_empty() {
                println!("🔎 Restoring only files matching: {}", files.join(", "));
            }
            let mut report_data = core::perform_restore(
                &cfg,
                RestoreOptions {
                    snapshot_id: snapshot.clone(),
                    include: files,
                    force,
                    versioned,
                    ..RestoreOptions::new(target)
                }
                .with_encryption_key(key.as_deref()),
            )
            .await?;

            // Verify restored files if requested
            if verify {
                println!("🔍 Verifying restored files integrity...");

                // Run verification on the backup data to ensure integrity
                match perform_verify(
                    &cfg,
                    VerifyOptions {
                        snapshot_id: snapshot.clone(),
                        ..Default::default()
                    }
                    .with_encryption_key(key.as_deref()),
                )
                .await
                {
                    Ok(verify_report) => {
                        println!("✅ Verification passed!");
                        println!("   📊 Snapshots verified: {}", verify_report.total_files);
                        println!("   ✔️  Files processed: {}", verify_report.files_processed);

                        // Merge verification data into restore report
                        report_data.total_files = verify_report.total_files;
                        report_data.files_processed = verify_report.files_processed;
                    }
                    Err(e) => {
                        eprintln!("   Files may have integrity issues. Please review manually.");
                        return Err(format!("❌ Restore verification failed: {}", e).into());
                    }
                }
            }

            println!("✅ Restore completed.");

            if report {
                report_data.profile_path = if let Some(ref cfg_path) = config {
                    cfg_path.to_string_lossy().to_string()
                } else if let Some(ref backup_path) = backup {
                    format!("Backup: {}", backup_path.display())
                } else {
                    "Unknown".to_string()
                };

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
            backup,
            snapshot,
            quiet,
            quick,
            fast,
            report,
            key,
            repair,
        } => {
            let _auth_token = check_fido2_auth().await?;

            if key.is_some() {
                warn_secret_arg("--key");
            }
            let key = config::resolve_encryption_key(
                key,
                config.as_deref().and_then(profile_name).as_deref(),
                None,
            );

            // Determine verification mode: config or backup path
            let (cfg, profile_path) = if let Some(config_path) = config {
                // Verify via profile
                let mut cfg = config::load_config(&config_path)?;
                if let Some(k) = key {
                    cfg.encryption_key = Some(k);
                }
                (cfg, config_path.to_string_lossy().to_string())
            } else if let Some(backup_path) = backup {
                // Verify via direct backup path using portable restore function
                if !backup_path.exists() {
                    eprintln!("❌ Error: Backup path not found: {}", backup_path.display());
                    return Err(format!("Backup not found: {}", backup_path.display()).into());
                }
                // Create config using portable restore function
                let cfg = rsb_sdk::config_from_backup_path(&backup_path, key.clone())
                    .map_err(|e| format!("Failed to create config from backup: {}", e))?;
                (cfg, format!("Backup: {}", backup_path.display()))
            } else {
                return Err(
                    "❌ Error: Either --config <FILE> or --backup <PATH> is required".into(),
                );
            };

            // Use quick mode if requested (quick takes precedence over fast)
            let use_fast = quick || fast;
            if quick && fast {
                println!("Using quick verification (--quick takes precedence)");
            } else if quick {
                println!("Using quick verification mode");
            } else if fast {
                println!("Using fast verification mode (hash only, no decryption)");
            }

            if repair && use_fast {
                println!("--repair performs a full check; ignoring --quick/--fast");
            }
            let mut report_data = perform_verify(
                &cfg,
                VerifyOptions {
                    snapshot_id: snapshot.clone(),
                    quiet,
                    fast: use_fast,
                    repair,
                    ..Default::default()
                },
            )
            .await?;
            let failed = report_data.files_with_errors > 0 || !report_data.errors.is_empty();

            if report {
                report_data.profile_path = profile_path;
                let html = rsb_sdk::report::generate_html(&report_data);
                let filename = PathBuf::from(format!(
                    "rsb-report-verify-{}.html",
                    Local::now().format("%Y%m%d-%H%M%S")
                ));
                fs::write(&filename, html)?;
                println!("📄 Report generated at: {}", filename.display());
            }

            // Scripts/healthchecks dependem do exit code: problemas → falha.
            if failed {
                for e in &report_data.errors {
                    eprintln!("  ❌ {}", e);
                }
                return Err(format!(
                    "❌ Verification found problems ({} file(s) with errors)",
                    report_data.files_with_errors
                )
                .into());
            }
            println!("✅ Verification completed.");
        }
        Commands::Prune {
            config,
            backup,
            keep_last,
            retention,
            dry_run,
            confirm,
            quiet: _,
            healthcheck_url,
        } => {
            if !dry_run && !confirm {
                return Err(
                    "Refusing destructive prune without --confirm (or preview with --dry-run). Re-run with --confirm to proceed."
                        .into(),
                );
            }
            let _auth_token = check_fido2_auth().await?;
            send_healthcheck(&healthcheck_url, "/start").await;

            // Prune only needs the repository (+ key): --backup <DIR> works
            // on the repository directly, like restore/verify; otherwise the
            // profile (CONFIG or auto-discovered) gives storage and key.
            let (mut cfg, profile) = match backup {
                Some(repo) => {
                    let repo = check_repository_dir("prune", &repo)?;
                    let cfg = rsb_sdk::config_from_backup_path(&repo, None)
                        .map_err(|e| anyhow::anyhow!("Not a backup repository: {}", e))?;
                    (cfg, None)
                }
                None => {
                    let config_path = resolve_profile_path(config)?;
                    let cfg = config::load_config(&config_path)?;
                    (cfg, profile_name(&config_path))
                }
            };

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

            let result = run_prune(&mut cfg, profile.as_deref(), keep_count, dry_run).await;
            if let Err(e) = result {
                send_healthcheck(&healthcheck_url, "/fail").await;
                return Err(e);
            }
            send_healthcheck(&healthcheck_url, "").await;
            info!("Prune completed.");
        }
        Commands::Schedule {
            config,
            backup,
            format,
        } => {
            // The generated job runs `backup <profile>`: it must name a
            // profile that exists and loads, never the repository directory.
            let config_path = resolve_command_profile("schedule", config, backup)?;
            let cfg = config::load_config(&config_path).map_err(|e| {
                anyhow::anyhow!("Cannot use profile {}: {}", config_path.display(), e)
            })?;
            let abs_config = std::fs::canonicalize(&config_path)?;
            // Relative (portable) paths are resolved against the working
            // directory: the job must run from the current one.
            let workdir = if profile_uses_relative_paths(&cfg) {
                Some(std::env::current_dir()?)
            } else {
                None
            };
            let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rsb"));
            let env_file = rsb_sdk::schedule::default_env_file();
            print!(
                "{}",
                render_schedule(
                    &format,
                    &exe,
                    &abs_config,
                    &cfg,
                    &env_file,
                    workdir.as_deref()
                )?
            );
        }
        Commands::Watch {
            config,
            backup,
            sync_to,
            key,
            interval,
            healthcheck_url,
        } => {
            let _auth_token = check_fido2_auth().await?;

            let config_path = resolve_command_profile("watch", config, backup)?;

            let mut cfg = config::load_config(&config_path)?;
            // Watch sempre cifra: sem chave disponível → erro (nunca em claro).
            let profile = profile_name(&config_path);
            let key = obtain_encryption_key(key, profile.as_deref(), Some(&cfg), true)?
                .ok_or("Encryption key required")?;
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

            let source = rsb_sdk::utils::expand_path(&cfg.source_path);
            let sync_dst = sync_to;
            let backup_dst = rsb_sdk::utils::expand_path(&cfg.destination_path);

            println!("🟢 Real-Time Sync started");
            println!("📂 Source: {}", source.display());
            println!("📁 Syncing to: {}", sync_dst.display());
            println!("💾 Backup to: {}", backup_dst.display());
            println!("⏱️ Debounce: {}s", interval);
            println!("🔐 Encryption: ENABLED");
            println!("\nPress Ctrl+C to stop\n");

            let rt = Arc::new(build_watch_sync(&cfg, sync_dst.clone(), interval));
            rt.start(&source)
                .await
                .map_err(|e| format!("Failed to watch {}: {}", source.display(), e))?;
            // Back up once at startup so the repository reflects the current
            // state before we only react to changes.
            rt.queue_full_backup(&source).await;
            let loop_handle = rt.clone().spawn_processing_loop();

            tokio::signal::ctrl_c().await?;
            println!(
                "\n⏹️ Stopping... a backup in progress will be allowed to finish (Ctrl+C again to abort it)."
            );
            rt.stop().await;
            tokio::select! {
                _ = loop_handle => {}
                _ = tokio::signal::ctrl_c() => {
                    // The snapshot manifest is only written at the very end
                    // (atomically), so aborting leaves no partial snapshot.
                    eprintln!("⚠️ Aborted: the interrupted backup produced no snapshot.");
                    std::process::exit(130);
                }
            }

            let stats = rt.get_stats().await;
            println!(
                "📊 {} change(s) backed up, {} backup(s) succeeded, {} failed",
                stats.synced, stats.backups_completed, stats.backups_failed
            );
            if stats.pending_changes > 0 {
                eprintln!(
                    "⚠️ {} change(s) were NOT backed up yet (run `rsb backup` to capture them).",
                    stats.pending_changes
                );
            }
            if let Some(err) = stats.last_error {
                eprintln!("❌ Last backup error: {}", err);
            }
        }

        Commands::Server { port } => {
            // Dummy sender since we're running server standalone

            let (tx, _rx) = tokio::sync::oneshot::channel();
            server::routes::start_auth_server(port, tx).await?;
        }

        Commands::Login { user_id, recovery } => {
            if recovery {
                // Recovery code flow
                println!("\n🔐 Recovery Code Authentication");
                println!("================================\n");

                use rpassword::prompt_password;

                print!("Enter recovery code: ");
                use std::io::Write;
                std::io::stdout().flush()?;

                let recovery_code = prompt_password("")?;

                if recovery_code.is_empty() {
                    return Err("❌ Recovery code cannot be empty".into());
                }

                // Load Fido2Manager and verify recovery code
                let mut fido2_manager =
                    Fido2Manager::new("http://localhost:3000", "localhost", "RSB CLI")?;

                // Sem credenciais carregadas não há códigos para verificar;
                // falhar aqui evita mascarar erros de keyring/ficheiro.
                let cred_path = Fido2Manager::default_storage_path()
                    .map_err(|e| format!("❌ Cannot locate credential storage: {}", e))?;
                if !cred_path.exists() {
                    return Err("❌ No FIDO2 credentials found; recovery is not possible".into());
                }
                fido2_manager
                    .load_from_file(&cred_path)
                    .map_err(|e| format!("❌ Could not load credentials: {}", e))?;

                // Verify recovery code
                if fido2_manager.verify_backup_code(&user_id, &recovery_code) {
                    // Save the updated manager (recovery code was consumed).
                    // Se não for possível persistir o consumo, o código continuaria
                    // reutilizável → não emitir token.
                    fido2_manager.save_to_file(&cred_path).map_err(|e| {
                        format!(
                            "❌ Failed to persist consumed recovery code ({}); no token issued",
                            e
                        )
                    })?;

                    // Create and save token
                    let jwt_mgr = rsb_sdk::auth::JwtManager::from_env()?;
                    let scopes = vec!["backup".to_string(), "restore".to_string()];
                    let token = jwt_mgr.create_token(&user_id, scopes.clone(), 1800)?;
                    // check_fido2_auth só aceita tokens cujo jti esteja no registo
                    // de sessões persistido.
                    let claims = jwt_mgr.verify_token(&token)?;
                    rsb_sdk::auth::SessionRegistry::default_registry()?.record_claims(&claims)?;

                    println!("\n✅ Recovery code verified successfully!");
                    println!("🔑 User: {}", user_id);
                    println!("📋 Scopes: {:?}\n", scopes);

                    let auth_file = save_auth_token(&token)?;

                    info!("✅ Authentication successful with recovery code!");
                    info!("📍 Token saved to: {}", auth_file.display());
                    println!("✅ You are now authenticated!");
                    println!("📍 Token saved to: {}\n", auth_file.display());
                    println!("You can now use: rsb backup, rsb restore, rsb verify\n");
                    println!("⚠️ IMPORTANT: Generate new recovery codes soon!");
                    println!("   Use: rsb auth generate-codes\n");
                } else {
                    return Err("❌ Invalid or already used recovery code".into());
                }
            } else {
                // FIDO2 flow (original)
                let mut login_flow = LoginFlow::new();

                match login_flow.start(user_id.clone()).await {
                    Ok(token) => {
                        // Verify and save token
                        let jwt_mgr = rsb_sdk::auth::JwtManager::from_env()?;
                        match jwt_mgr.verify_token(&token) {
                            Ok(claims) => {
                                println!("🔑 User: {}", claims.sub);
                                println!("📋 Scopes: {:?}\n", claims.scopes);

                                let auth_file = save_auth_token(&token)?;

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
        }

        Commands::Fido2(cmd) => {
            let origin = "http://localhost:3000";
            let rp_id = "localhost";

            let manager = Fido2Manager::new(origin, rp_id, "RSB CLI")?;

            cmd.execute(Arc::new(Mutex::new(manager))).await?;
        }
        Commands::Key(cmd) => {
            cmd.execute().await?;
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
        Commands::Diagnose {
            backup,
            key,
            verbose,
            json: output_json,
            repair,
        } => {
            // --repair altera o backup: exige autenticação como os outros comandos mutáveis.
            if repair {
                let _auth_token = check_fido2_auth().await?;
            }
            if !backup.exists() {
                eprintln!("❌ Error: Backup path not found: {}", backup.display());
                return Err(format!("Backup not found: {}", backup.display()).into());
            }
            if key.is_some() {
                warn_secret_arg("--key");
            }
            let key = config::resolve_encryption_key(key, None, None);

            println!("🔍 Diagnosing backup: {}\n", backup.display());

            // Run diagnostics
            let diagnostics =
                run_backup_diagnostics(&backup, key.as_deref(), verbose, repair).await?;

            if output_json {
                // Output JSON format
                println!("{}", serde_json::to_string_pretty(&diagnostics)?);
            } else {
                // Output human-readable format
                print_diagnostics(&diagnostics);
            }
        }

        Commands::Config(cmd) => {
            cmd.execute().await?;
        }
    }

    Ok(())
}

/// Prunes the repository of `cfg` down to the newest `keep` snapshots
/// (with `dry_run`, only lists what would be deleted). The key comes from
/// RSB_ENCRYPTION_KEY, the profile's keyring entry or a prompt; an
/// encrypted repository (keys/repo.key) without a key is refused before
/// anything is deleted, since its manifests could not be read for the
/// garbage collection.
async fn run_prune(
    cfg: &mut config::Config,
    profile: Option<&str>,
    keep: usize,
    dry_run: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = core::storage_backend::get_storage(cfg).await?;
    if dry_run {
        let doomed = core::plan_prune_by_count_with_storage(&*storage, keep).await?;
        if doomed.is_empty() {
            println!("Nothing to prune.");
        }
        for snap in doomed {
            println!("  would delete {}", snap);
        }
        return Ok(());
    }
    let encrypted =
        rsb_sdk::crypto::keyfile::repo_key_exists(&*storage).await? || cfg.encryption_required();
    let mut key = config::resolve_encryption_key(None, profile, Some(cfg));
    if key.is_none() && encrypted {
        if !std::io::stdin().is_terminal() {
            return Err(anyhow::anyhow!(
                "The repository is encrypted and no key is available: set {} (or run interactively)",
                config::ENCRYPTION_KEY_ENV
            )
            .into());
        }
        let k = rpassword::prompt_password("Enter encryption key: ")?;
        if k.is_empty() {
            return Err("Encryption key cannot be empty".into());
        }
        key = Some(k);
    }
    cfg.encryption_key = key;
    core::perform_prune_with_storage(storage, cfg.encryption_key.as_deref(), keep).await?;
    Ok(())
}

/// Grava o token de sessão em ~/.rs-shield/auth_token (dir 0700, ficheiro 0600
/// criado já com essas permissões).
fn save_auth_token(token: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("❌ Home directory not found")?;
    let rsb_dir = home.join(".rs-shield");
    config::create_dir_secure(&rsb_dir)?;
    let auth_file = rsb_dir.join("auth_token");
    config::write_file_secure(&auth_file, token.as_bytes())?;
    Ok(auth_file)
}

/// Builds the realtime synchronizer used by `rsb watch` (shared with the
/// desktop app, see [`rsb_sdk::realtime::mirror_and_backup_sync`]): each
/// debounced batch mirrors new/modified files to `sync_dst`, then runs an
/// encrypted incremental backup of the profile. The key comes from
/// `cfg.encryption_key` (already resolved by the caller) and is never
/// printed.
fn build_watch_sync(
    cfg: &config::Config,
    sync_dst: PathBuf,
    interval: u64,
) -> rsb_sdk::realtime::RealtimeSync {
    use rsb_sdk::realtime::{SyncStrategy, WatchEvent, mirror_and_backup_sync};
    let strategy = SyncStrategy {
        batch_interval: interval,
        ..SyncStrategy::default()
    };
    mirror_and_backup_sync(
        cfg,
        sync_dst,
        strategy,
        Some(Arc::new(|event| match event {
            WatchEvent::Mirrored { files } => {
                println!("✅ Sync: {} new/modified files synchronized.", files)
            }
            WatchEvent::BackedUp { number, summary } => {
                println!("💾 Backup #{}: {}", number, summary)
            }
        })),
    )
}
