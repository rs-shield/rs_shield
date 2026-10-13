use anyhow::Result;
use axum::response::Html;
use clap::Subcommand;
use rsb_sdk::credentials::Fido2Manager;
use rsb_sdk::fido2::fido2_web;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

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

    /// Generate recovery codes for a user
    GenerateCodes {
        /// User ID to generate codes for
        user_id: String,

        /// Path to store Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,

        /// Output file to save the codes (optional)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Show codes on screen (default: true; use `--display false` or `--no-display`)
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        display: bool,

        /// Do not show codes on screen (requires --output)
        #[arg(long)]
        no_display: bool,
    },

    /// List recovery codes for a user (shows count only, not codes)
    ListCodes {
        /// User ID
        user_id: String,

        /// Path to stored Auth credentials (default: ~/.rsb/auth.json)
        #[arg(short, long)]
        storage: Option<PathBuf>,
    },

    /// Export recovery codes to a file
    ExportCodes {
        /// User ID
        user_id: String,

        /// Output file path
        #[arg(short, long)]
        output: PathBuf,

        /// Format: text or json
        #[arg(short, long, default_value = "text")]
        format: String,

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
                fido2_web::run_server_with_mode(
                    manager,
                    Html(include_str!("../assets/fido2_auth.html")),
                    fido2_web::ServerMode::Register,
                )
                .await
                .map_err(|e| anyhow::anyhow!("{}", e))?;
            }

            Self::Authenticate { .. } => {
                info!("🔐 Starting Auth authentication flow...");
                fido2_web::run_server(manager, Html(include_str!("../assets/fido2_auth.html")))
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
                // Operação destrutiva: exige sessão autenticada do próprio utilizador.
                require_auth_as(&user_id).await?;
                info!("🗑️  Deleting Auth credential for user: {}", user_id);

                let mut mg = manager.lock().await;
                // Reload, refuse unknown users, revoke and save (shared with
                // the desktop app).
                mg.revoke_user_persisted(&path, &user_id)
                    .map_err(|e| match e {
                        rsb_sdk::credentials::fido2_manager::Fido2Error::CredentialNotFound => {
                            anyhow::anyhow!("❌ Unknown user: {}", user_id)
                        }
                        e => anyhow::anyhow!("❌ Error deleting credential: {}", e),
                    })?;
                println!("✅ Credential deleted successfully");
            }

            Self::GenerateCodes {
                user_id,
                output,
                display,
                no_display,
                ..
            } => {
                let display = display && !no_display;
                if !display && output.is_none() {
                    return Err(anyhow::anyhow!(
                        "❌ Codes would be neither displayed nor saved. Use --output <file> with --no-display."
                    ));
                }

                // Gerar códigos substitui os existentes: exige sessão autenticada
                // do próprio utilizador (senão qualquer pessoa podia sequestrar a conta).
                require_auth_as(&user_id).await?;
                info!("🔑 Generating recovery codes for user: {}", user_id);

                let mut mg = manager.lock().await;

                // Load existing credentials (falha = abortar)
                mg.load_from_file(&path)
                    .map_err(|e| anyhow::anyhow!("❌ Failed to load credentials: {}", e))?;

                // Recusar utilizadores desconhecidos (sem credencial FIDO2 registada).
                if !mg.has_credential(&user_id) {
                    return Err(anyhow::anyhow!("❌ Unknown user: {}", user_id));
                }

                match mg.generate_backup_codes(&user_id) {
                    Ok(codes) => {
                        println!("\n✅ Recovery codes generated successfully!\n");
                        println!("🔐 IMPORTANT SECURITY NOTICE:");
                        println!("════════════════════════════════════════");
                        println!("These codes are displayed ONLY ONCE.");
                        println!("Save them immediately in a secure location!");
                        println!("════════════════════════════════════════\n");

                        if display {
                            println!("📋 Your Recovery Codes:\n");
                            for (i, code) in codes.iter().enumerate() {
                                println!("{:2}. {}", i + 1, code);
                            }
                            println!();
                        }

                        // Save to file if requested
                        if let Some(output_path) = output {
                            let content = format!(
                                "RS Shield - Recovery Codes\n\
                                User: {}\n\
                                Generated: {}\n\
                                ════════════════════════════════════════\n\n\
                                {}",
                                user_id,
                                chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
                                codes.join("\n")
                            );
                            // 0600: os códigos dão acesso à conta.
                            rsb_sdk::config::write_file_secure(&output_path, content.as_bytes())?;
                            println!("💾 Codes saved to: {}", output_path.display());
                        }

                        // Save updated manager to storage
                        mg.save_to_file(&path).map_err(|e| {
                            anyhow::anyhow!("❌ Failed to save recovery codes: {}", e)
                        })?;
                        println!("\n⚠️  REMINDER: Generate new codes after using some!");
                    }
                    Err(e) => return Err(anyhow::anyhow!("❌ Error generating codes: {}", e)),
                }
            }

            Self::ListCodes { user_id, .. } => {
                info!("📋 Listing recovery codes count for user: {}", user_id);

                let mut mg = manager.lock().await;

                // Load credentials
                if let Err(e) = mg.load_from_file(&path) {
                    println!("⚠️ Failed to load credentials: {}", e);
                    return Ok(());
                }

                // Count recovery codes (we can't display them, only show how many are available)
                // This would need to be exposed via the Fido2Manager API
                println!("📁 Checking recovery codes for user: {}\n", user_id);
                println!("ℹ️  Use `rsb auth generate-codes <user_id>` to see the actual codes");
                println!("⚠️  Recovery codes cannot be displayed again after generation");
            }

            Self::ExportCodes {
                user_id,
                output: _output,
                format,
                ..
            } => {
                info!("📤 Exporting recovery codes for user: {}", user_id);

                if format != "text" && format != "json" {
                    return Err(anyhow::anyhow!("❌ Invalid format. Use 'text' or 'json'"));
                }

                println!("❌ Recovery codes export is for educational purposes only.");
                println!("⚠️  Note: Recovery codes can only be viewed once at generation time.");
                println!("📄 If you need to save them again, generate new codes with:");
                println!("   rsb auth generate-codes {} --output codes.txt", user_id);
            }
        }
        Ok(())
    }

    /// Auxiliar para exibir as credenciais no terminal
    fn print_credentials(
        &self,
        credentials: &[rsb_sdk::credentials::fido2_manager::Fido2Credential],
    ) {
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
            Self::GenerateCodes { storage, .. } => storage,
            Self::ListCodes { storage, .. } => storage,
            Self::ExportCodes { storage, .. } => storage,
        };

        storage_opt.clone().unwrap_or_else(|| {
            // Unifica o caminho com o padrão definido no Fido2Manager do SDK
            Fido2Manager::default_storage_path()
                .unwrap_or_else(|_| PathBuf::from(".rs-shield/fido2_credentials.json"))
        })
    }
}

/// Exige uma sessão válida (check_fido2_auth) cujo `sub` seja `user_id`.
async fn require_auth_as(user_id: &str) -> Result<()> {
    let token = rsb_sdk::server::routes::check_fido2_auth()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let claims = rsb_sdk::auth::JwtManager::from_env()
        .map_err(|e| anyhow::anyhow!("{}", e))?
        .verify_token(&token)
        .map_err(|e| anyhow::anyhow!("❌ Invalid session: {}", e))?;
    if claims.sub != user_id {
        return Err(anyhow::anyhow!(
            "❌ Authenticated as '{}', cannot manage credentials of '{}'",
            claims.sub,
            user_id
        ));
    }
    Ok(())
}
