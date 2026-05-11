use anyhow::{anyhow, Context, Result};
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

        /// AWS secret access key
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

        // In production, this would be securely stored using keyring or similar
        let config_dir = self.get_config_dir()?;
        let password_file = config_dir.join(".password");

        // Hash the password in real implementation - this is just a placeholder
        let hashed = format!("hashed:{}", password); // Should use proper hashing!
        fs::write(&password_file, hashed)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            fs::set_permissions(&password_file, perms)?;
        }

        println!("✅ Master password set successfully!");
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

        if let Some(key) = access_key {
            s3_config.insert("access_key".to_string(), toml::Value::String(key.clone()));
        }

        if let Some(secret) = secret_key {
            s3_config.insert(
                "secret_key".to_string(),
                toml::Value::String(secret.clone()),
            );
        }

        let config_str = toml::to_string_pretty(&s3_config)?;
        fs::write(&s3_config_file, config_str)?;

        println!("✅ S3 configuration saved!");
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

        // Check for password
        let password_file = config_dir.join(".password");
        if password_file.exists() {
            println!("🔐 Master Password: ✅ Set");
        } else {
            println!("🔐 Master Password: ❌ Not set");
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

        println!();
        Ok(())
    }

    async fn reset_config(&self, confirm: bool) -> Result<()> {
        if !confirm {
            println!("⚠️  WARNING: This will delete ALL RS Shield configuration and credentials!");
            println!("   Use --confirm flag to proceed");
            return Ok(());
        }

        let config_dir = self.get_config_dir()?;

        println!("🗑️  Resetting RS Shield configuration...");

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

        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }

        Ok(config_dir)
    }
}
