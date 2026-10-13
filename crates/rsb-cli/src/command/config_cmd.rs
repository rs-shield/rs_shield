use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use std::fs;
use std::path::PathBuf;

#[derive(Subcommand)]
pub enum ConfigCommand {
    /// Set master password
    SetPassword,

    /// Configure S3 credentials
    S3Credentials {
        /// S3 bucket name
        #[arg(long)]
        bucket: String,

        /// S3 region
        #[arg(long)]
        region: String,

        /// S3 endpoint URL (optional)
        #[arg(long)]
        endpoint: Option<String>,

        /// AWS access key ID
        #[arg(long)]
        access_key: Option<String>,

        /// AWS secret access key (DEPRECATED: visible in `ps`/history; omit to be prompted)
        #[arg(long)]
        secret_key: Option<String>,
    },

    /// List current configuration
    List,

    /// Reset all settings (WARNING: destructive)
    Reset {
        /// Confirm reset
        #[arg(long)]
        confirm: bool,
    },
}

impl ConfigCommand {
    pub async fn execute(&self) -> Result<()> {
        match self {
            ConfigCommand::SetPassword => {
                self.set_password().await?;
            }

            ConfigCommand::S3Credentials {
                bucket,
                region,
                endpoint,
                access_key,
                secret_key,
            } => {
                self.configure_s3(bucket, region, endpoint, access_key, secret_key)
                    .await?;
            }

            ConfigCommand::List => {
                self.list_config().await?;
            }

            ConfigCommand::Reset { confirm } => {
                self.reset_config(*confirm).await?;
            }
        }

        Ok(())
    }

    async fn set_password(&self) -> Result<()> {
        println!("🔐 Setting Master Password\n");

        let password = rpassword::prompt_password("Enter master password: ")
            .context("Failed to read password")?;

        let confirm = rpassword::prompt_password("Confirm password: ")
            .context("Failed to read confirmation")?;

        if password != confirm {
            return Err(anyhow!("❌ Passwords do not match"));
        }
        if password.len() < 16 {
            return Err(anyhow!("❌ Password too short (minimum 16 characters)"));
        }
        let mut confirm = confirm;
        {
            use zeroize::Zeroize;
            confirm.zeroize();
        }

        // Guarda a password no OS keyring; em disco fica SÓ um verificador
        // PBKDF2-HMAC-SHA256 600k + salt 16B (nunca reversível, nunca `hashed:`).
        use base64::Engine as _;
        use rand::Rng;
        let mut salt = [0u8; 16];
        rand::rng().fill_bytes(&mut salt);
        let mut verifier = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<sha2::Sha256>(password.as_bytes(), &salt, 600_000, &mut verifier);
        use zeroize::Zeroize;
        let record = format!(
            "v1$pbkdf2-sha256$600000${}${}",
            base64::engine::general_purpose::STANDARD.encode(salt),
            base64::engine::general_purpose::STANDARD.encode(verifier)
        );

        // Guarda a password no keyring para uso pelo SDK (antes de zeroizar).
        let keyring_result = keyring::Entry::new("rsb-shield", "s3-master-password")
            .and_then(|entry| entry.set_password(&password));
        let mut pwd_bytes = password.into_bytes();
        pwd_bytes.zeroize();
        match keyring_result {
            Ok(()) => println!("🔐 Master password stored in OS keyring."),
            Err(e) => eprintln!(
                "⚠️ Failed to store master password in OS keyring: {} (the verifier is still written)",
                e
            ),
        }

        let config_dir = self.get_config_dir()?;
        let password_file = config_dir.join(".password.verifier");
        // Remove legado reversível se existir.
        let legacy = config_dir.join(".password");
        let _ = fs::remove_file(&legacy);
        rsb_sdk::config::write_file_secure(&password_file, format!("{}\n", record).as_bytes())?;

        println!("✅ Master password verifier stored (PBKDF2 600k, 0600).");
        println!(
            "   Keep the password in your password manager — it cannot be recovered from disk."
        );
        Ok(())
    }

    async fn configure_s3(
        &self,
        bucket: &str,
        region: &str,
        endpoint: &Option<String>,
        access_key: &Option<String>,
        secret_key: &Option<String>,
    ) -> Result<()> {
        println!("📦 Configuring S3 Credentials\n");

        let config_dir = self.get_config_dir()?;
        let s3_config_file = config_dir.join("s3_config.toml");

        let mut s3_config = toml::Table::new();

        s3_config.insert(
            "bucket".to_string(),
            toml::Value::String(bucket.to_string()),
        );
        s3_config.insert(
            "region".to_string(),
            toml::Value::String(region.to_string()),
        );

        if let Some(ep) = endpoint {
            s3_config.insert("endpoint".to_string(), toml::Value::String(ep.clone()));
        }

        // NUNCA persistir segredos em TOML plaintext. Se o utilizador passar
        // --access-key/--secret-key, guardamos cifrado via CredentialsManager
        // (s3_credentials.enc + master password no keyring) e o TOML fica só
        // com bucket/region/endpoint (não sensível) com 0600.
        if secret_key.is_some() {
            crate::helpers::warn_secret_arg("--secret-key");
        }
        // Se só a access key for passada, pedir a secret key de forma interativa
        // (não fica em `ps`/histórico).
        let prompted_secret: Option<String> = match (access_key, secret_key) {
            (Some(_), None) => {
                let sk = rpassword::prompt_password("AWS secret access key: ")
                    .context("Failed to read secret key")?;
                if sk.is_empty() {
                    return Err(anyhow!("Secret key cannot be empty"));
                }
                Some(sk)
            }
            _ => None,
        };
        let secret_key = if prompted_secret.is_some() {
            &prompted_secret
        } else {
            secret_key
        };
        if access_key.is_some() || secret_key.is_some() {
            match (access_key, secret_key) {
                (Some(ak), Some(sk)) => {
                    use rsb_sdk::credentials::{CredentialsManager, S3Credentials, SecureString};
                    let creds = S3Credentials {
                        access_key: SecureString::new(ak.clone()),
                        secret_key: SecureString::new(sk.clone()),
                        session_token: None,
                    };
                    if let Err(e) = creds.validate() {
                        eprintln!("⚠️ Provided credentials look invalid: {}", e);
                    }
                    let home = dirs::home_dir().context("Home directory not found")?;
                    let enc_path = home.join(".rs-shield").join("s3_credentials.enc");
                    if let Some(parent) = enc_path.parent() {
                        rsb_sdk::config::create_dir_secure(parent)?;
                    }
                    CredentialsManager::save_encrypted(&enc_path.to_string_lossy(), &creds, true)
                        .map_err(|e| anyhow!("Failed to store S3 secrets encrypted: {}", e))?;
                    // chmod 0600 no artefacto cifrado (defesa em profundidade).
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        let _ =
                            fs::set_permissions(&enc_path, std::fs::Permissions::from_mode(0o600));
                    }
                    println!("🔐 S3 secrets stored encrypted (keyring + s3_credentials.enc).");
                }
                _ => {
                    return Err(anyhow!(
                        "Provide BOTH --access-key and --secret-key, or neither (preferred: env AWS_* / encrypted file)"
                    ));
                }
            }
        }

        let config_str = toml::to_string_pretty(&s3_config)?;
        rsb_sdk::config::write_file_secure(&s3_config_file, config_str.as_bytes())?;

        println!("✅ S3 configuration saved (non-secret fields only)!");
        println!("   Bucket: {}", bucket);
        println!("   Region: {}", region);
        if let Some(ep) = endpoint {
            println!("   Endpoint: {}", ep);
        }

        Ok(())
    }

    async fn list_config(&self) -> Result<()> {
        println!("📋 RS Shield Configuration\n");

        let config_dir = self.get_config_dir()?;

        println!("📂 Configuration Directory: {}", config_dir.display());
        println!();

        // Check for password (novo verificador; legado `.password` = inseguro)
        let verifier_file = config_dir.join(".password.verifier");
        let legacy_file = config_dir.join(".password");
        if verifier_file.exists() {
            println!("🔐 Master Password: ✅ Set (PBKDF2 verifier)");
        } else {
            println!("🔐 Master Password: ❌ Not set");
        }
        if legacy_file.exists() {
            println!(
                "⚠️ Legacy .password file found (reversível `hashed:`) — remova com `rsb config reset --confirm` + `set-password`."
            );
        }

        // Check for S3 config
        let s3_config_file = config_dir.join("s3_config.toml");
        if s3_config_file.exists() {
            let content = fs::read_to_string(&s3_config_file)?;
            if let Ok(table) = content.parse::<toml::Table>() {
                println!("\n📦 S3 Configuration:");
                if let Some(bucket) = table.get("bucket").and_then(|v| v.as_str()) {
                    println!("   Bucket: {}", bucket);
                }
                if let Some(region) = table.get("region").and_then(|v| v.as_str()) {
                    println!("   Region: {}", region);
                }
                if let Some(endpoint) = table.get("endpoint").and_then(|v| v.as_str()) {
                    println!("   Endpoint: {}", endpoint);
                }
            }
        } else {
            println!("\n📦 S3 Configuration: ❌ Not configured");
        }

        self.list_integrations();

        println!();
        Ok(())
    }

    /// Shows the notification integrations and where each secret lives
    /// (never the value). Loading also migrates plaintext secrets out of
    /// `integrations.json`, exactly like the desktop app.
    fn list_integrations(&self) {
        use rsb_sdk::integrations::{IntegrationConfig, SecretSource};

        let path = IntegrationConfig::default_path();
        if !path.exists() {
            println!("\n🔔 Notification integrations: ❌ Not configured");
            return;
        }
        let cfg = IntegrationConfig::load_default();
        println!("\n🔔 Notification integrations ({}):", path.display());
        let status = cfg.status();
        if status.is_empty() {
            println!("   (none)");
        }
        for s in status {
            let mark = match s.secret {
                SecretSource::LegacyPlaintext | SecretSource::Unreadable(_) => "⚠️",
                _ => "  ",
            };
            println!(
                "{} {:<9} {:<9} secret: {}",
                mark,
                s.name,
                if s.enabled { "enabled" } else { "disabled" },
                s.secret
            );
        }
    }

    async fn reset_config(&self, confirm: bool) -> Result<()> {
        if !confirm {
            println!("⚠️  WARNING: This will delete ALL RS Shield configuration and credentials!");
            println!("   Use --confirm flag to proceed");
            return Err(anyhow!("Reset aborted: --confirm not given"));
        }

        let config_dir = self.get_config_dir()?;

        println!("🗑️  Resetting RS Shield configuration...");

        // Notification secrets live outside the directory (OS keyring).
        {
            use rsb_sdk::integrations::IntegrationConfig;
            if let Err(e) =
                IntegrationConfig::delete_all_secrets(&IntegrationConfig::default_stores())
            {
                eprintln!(
                    "⚠️ Could not remove notification secrets from the keyring: {}",
                    e
                );
            }
        }

        if config_dir.exists() {
            fs::remove_dir_all(&config_dir).context("Failed to remove configuration directory")?;
        }

        println!("✅ Configuration reset successfully!");
        println!("   Please reconfigure with: rsb config s3-credentials ...");

        Ok(())
    }

    fn get_config_dir(&self) -> Result<PathBuf> {
        let home = dirs::home_dir().context("Could not determine home directory")?;
        let config_dir = home.join(".rs-shield");

        rsb_sdk::config::create_dir_secure(&config_dir)?;

        Ok(config_dir)
    }
}
