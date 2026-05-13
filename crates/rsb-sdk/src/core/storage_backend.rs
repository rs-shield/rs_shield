// storage_backend.rs - Versão performance + robustez
use crate::config::Config;
use crate::credentials::CredentialsManager;
use crate::storage::{LocalStorage, S3Storage, Storage};
use std::sync::Arc;
use tracing::{error, info, warn};

/// Retorna o backend de storage adequado (Local ou S3)
pub async fn get_storage(config: &Config) -> Arc<dyn Storage> {
    // Prioridade: configuração S3 aninhada ou plana (backward compatibility)
    let s3_config = config.s3.as_ref();

    let bucket = s3_config
        .and_then(|s| s.bucket.clone())
        .or_else(|| config.s3_bucket.clone());

    let region = s3_config
        .and_then(|s| s.region.clone())
        .or_else(|| config.s3_region.clone());

    let endpoint = s3_config
        .and_then(|s| s.endpoint.clone())
        .or_else(|| config.s3_endpoint.clone());

    match bucket {
        Some(bucket) if !bucket.trim().is_empty() => {
            match load_s3_credentials_securely(config).await {
                Ok(credentials) => {
                    info!(
                        "☁️  Using S3 storage → bucket: {} | region: {:?} | endpoint: {:?}",
                        bucket, region, endpoint
                    );
                    let s3 = S3Storage::new(&bucket, region, endpoint, credentials).await;
                    Arc::new(s3) as Arc<dyn Storage>
                }
                Err(e) => {
                    error!("❌ Failed to load S3 credentials: {}", e);
                    eprintln!("\n❌ FATAL: S3 configured but credentials unavailable.");
                    eprintln!("   Please set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY");
                    panic!("S3 credentials required");
                }
            }
        }
        _ => {
            info!("📁 Using Local storage → path: {}", config.destination_path);
            Arc::new(LocalStorage::new(&config.destination_path))
        }
    }
}

/// Carregamento seguro e priorizado de credenciais S3
async fn load_s3_credentials_securely(
    config: &Config,
) -> Result<aws_credential_types::Credentials, String> {
    use std::env;

    // 1. Environment variables (mais rápido e recomendado para produção/CI)
    if let (Ok(access_key), Ok(secret_key)) = (
        env::var("AWS_ACCESS_KEY_ID"),
        env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        if !access_key.is_empty() && !secret_key.is_empty() {
            let session_token = env::var("AWS_SESSION_TOKEN").ok();
            info!("🔑 S3 credentials loaded from environment variables");
            return Ok(aws_credential_types::Credentials::new(
                access_key,
                secret_key,
                session_token,
                None,
                "rsb-env",
            ));
        }
    }

    // 2. Ficheiro encriptado (persistente)
    if let Some(home) = env::var("HOME").ok() {
        let cred_path = format!("{}/.rs-shield/s3_credentials.enc", home);
        if std::path::Path::new(&cred_path).exists() {
            match CredentialsManager::load_encrypted(&cred_path) {
                Ok(creds) => {
                    info!("🔑 S3 credentials loaded from encrypted file");
                    return Ok(aws_credential_types::Credentials::new(
                        creds.access_key.as_str(),
                        creds.secret_key.as_str(),
                        creds.session_token.as_ref().map(|s| s.to_string()),
                        None,
                        "rsb-encrypted",
                    ));
                }
                Err(e) => warn!("Failed to load encrypted credentials: {}", e),
            }
        }
    }

    // 3. Prompt interativo (último recurso)
    match CredentialsManager::prompt_for_credentials() {
        Ok(creds) => {
            info!("🔑 S3 credentials configured interactively");
            return Ok(aws_credential_types::Credentials::new(
                creds.access_key.as_str(),
                creds.secret_key.as_str(),
                creds.session_token.as_ref().map(|s| s.to_string()),
                None,
                "rsb-interactive",
            ));
        }
        Err(e) => warn!("Interactive credential prompt failed: {}", e),
    }

    // 4. Config file (deprecated)
    if let Some(s3) = config.s3.as_ref() {
        if let (Some(ak), Some(sk)) = (&s3.access_key, &s3.secret_key) {
            if !ak.is_empty() && !sk.is_empty() {
                warn!("⚠️ Using credentials from config file (not recommended for security)");
                return Ok(aws_credential_types::Credentials::new(
                    ak.clone(),
                    sk.clone(),
                    None,
                    None,
                    "rsb-config",
                ));
            }
        }
    }

    Err("No S3 credentials found. Please provide them via env vars or encrypted file.".to_string())
}
