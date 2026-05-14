use anyhow::Result;
use clap::Subcommand;
use rsb_sdk::credentials::Fido2Manager;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::fido2::fido2_web;
#[derive(Subcommand)]
pub enum Fido2Command {
    /// Start Auth registration flow in browser
    Register {
        /// Path to store Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,
    },

    /// Start Auth authentication flow in browser
    Authenticate {
        /// Path to stored Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,
    },

    /// List all registered Auth credentials
    List {
        /// Path to stored Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,
    },

    /// Delete a Auth credential
    Delete {
        /// User ID to delete
        user_id: String,

        /// Path to stored Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,
    },
}

impl Fido2Command {
    pub async fn execute(self, manager: Arc<Mutex<Fido2Manager>>) -> Result<()> {
        let path = self.get_storage_path();

        match self {
            Self::Register { .. } => {
                info!("🔐 Starting Auth registration flow...");
                fido2_web::run_server(manager)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }

            Self::Authenticate { .. } => {
                info!("🔐 Starting Auth authentication flow...");
                fido2_web::run_server(manager)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
            }

            Self::List { .. } => {
                info!("📋 Listing Auth credentials...");

                println!("📁 Using storage: {:?}", path);

                let mut mg = manager.lock().await;

                // 🔥 carregar do disco
                if let Err(e) = mg.load_from_file(&path) {
                    println!("⚠️ Failed to load credentials: {}", e);
                }

                let credentials = mg.list_credentials();

                if credentials.is_empty() {
                    println!("ℹ️  No Auth credentials registered");
                } else {
                    self.print_credentials(&credentials);
                }
            }
            Self::Delete { user_id, .. } => {
                info!("🗑️  Deleting Auth credential for user: {}", user_id);

                let mut mg = manager.lock().await;

                // 🔥 carregar antes
                let _ = mg.load_from_file(&path);

                match mg.revoke_credential(&user_id) {
                    Ok(_) => {
                        // 🔥 salvar depois
                        let _ = mg.save_to_file(&path);
                        println!("✅ Credential deleted successfully");
                    }
                    Err(e) => println!("❌ Error deleting credential: {}", e),
                }
            }
        }
        Ok(())
    }

    /// Auxiliar para exibir as credenciais no terminal
    fn print_credentials(&self, credentials: &[rsb_sdk::credentials::fido2::Fido2Credential]) {
        println!("\n📋 Registered Auth Credentials:\n");
        for (i, c) in credentials.iter().enumerate() {
            println!("{}. User: {} ({})", i + 1, c.user_name, c.user_id);
            println!("   Display Name: {}", c.display_name);
            println!("   Created: {}", c.created_at);
            if let Some(last_used) = &c.last_used {
                println!("   Last Used: {}", last_used);
            }
            println!("   Counter: {}\n", c.counter);
        }
    }

    pub fn get_storage_path(&self) -> PathBuf {
        let storage_opt = match self {
            Self::Register { storage, .. } => storage,
            Self::Authenticate { storage, .. } => storage,
            Self::List { storage, .. } => storage,
            Self::Delete { storage, .. } => storage,
        };

        storage_opt.clone().unwrap_or_else(|| {
            Fido2Manager::default_storage_path().unwrap_or_else(|_| {
                let mut path = dirs::home_dir().unwrap_or_default();
                path.push(".rs-shield");
                path.push("auth_credentials.json");
                path
            })
        })
    }
}
