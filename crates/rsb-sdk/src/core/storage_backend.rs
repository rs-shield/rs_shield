use crate::config::Config;
use crate::credentials::CredentialsManager;
use crate::storage::{LocalStorage, S3Storage, Storage};
use std::sync::Arc;
use tracing::info;

/// Decide and create the appropriate storage backend (Local or S3)
/// based on the provided configuration.
///
/// - If there is valid S3 configuration → uses S3Storage with secure credentials
/// - Otherwise → uses LocalStorage
pub async fn get_storage(config: &Config) -> Arc<dyn Storage> {
    // Try to get S3 config from nested structure (preferred) or flat (backward compatibility)
    let s3_config = config.s3.as_ref();

    let bucket = s3_config
        .and_then(|s| s.bucket.clone())
        .or(config.s3_bucket.clone());

    let region = s3_config
        .and_then(|s| s.region.clone())
        .or(config.s3_region.clone());

    let endpoint = s3_config
        .and_then(|s| s.endpoint.clone())
        .or(config.s3_endpoint.clone());

    match bucket {
        Some(bucket) if !bucket.trim().is_empty() => {
            // SECURE credential loading
            // Priority:
            // 1. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
            // 2. Encrypted file ~/.rs-shield/s3_credentials.enc
            // 3. Prompt user interactively
            // 4. Config values (not recommended)

            let load_result = load_s3_credentials_securely(config).await;

            match load_result {
                Ok(credentials) => {
                    info!(
                        "✅ S3 storage backend → bucket: {}, region: {:?}, endpoint: {:?}",
                        bucket, region, endpoint
                    );
                    let s3 = S3Storage::new(&bucket, region, endpoint, credentials).await;
                    let storage: Arc<dyn Storage> = Arc::new(s3);
                    storage
                }
                Err(e) => {
                    // S3 is configured but credentials failed - this is a HARD ERROR
                    // Do not silently fallback to local storage!
                    eprintln!(
                        "\n❌ FATAL ERROR: S3 is configured but credentials could not be loaded"
                    );
                    eprintln!("   Error: {}", e);
                    eprintln!("\n📋 To fix:");
                    eprintln!("   1. Provide AWS credentials via environment variables:");
                    eprintln!("      export AWS_ACCESS_KEY_ID=\"AKIA...\"");
                    eprintln!("      export AWS_SECRET_ACCESS_KEY=\"secret...\"");
                    eprintln!("   2. Or configure encrypted credentials file:");
                    eprintln!("      rsb backup <config>");
                    eprintln!("      (you will be prompted for credentials)\n");
                    panic!("S3 credentials required but not found");
                }
            }
        }
        _ => {
            info!(
                "Using Local storage backend → path: {}",
                config.destination_path
            );
            Arc::new(LocalStorage::new(&config.destination_path))
        }
    }
}

/// Load S3 credentials securely
/// Priority:
/// 1. Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
/// 2. Encrypted file ~/.rs-shield/s3_credentials.enc
/// 3. Prompt user interactively if none found
/// 4. Config file (deprecated, not recommended)
///
/// Returns AWS Credentials without modifying environment variables
async fn load_s3_credentials_securely(
    config: &Config,
) -> Result<aws_credential_types::Credentials, String> {
    use std::env;

    // Option 1: Use environment variables (better for CI/CD)
    if let (Ok(access_key), Ok(secret_key)) = (
        env::var("AWS_ACCESS_KEY_ID"),
        env::var("AWS_SECRET_ACCESS_KEY"),
    ) {
        if !access_key.is_empty() && !secret_key.is_empty() {
            let session_token = env::var("AWS_SESSION_TOKEN").ok();
            info!("✅ S3 credentials loaded from environment variables (secure)");
            return Ok(aws_credential_types::Credentials::new(
                access_key,
                secret_key,
                session_token,
                None,
                "rsb-env",
            ));
        }
    }

    // Option 2: Use encrypted file
    let home = env::var("HOME").ok();
    if let Some(home_path) = home {
        let cred_file = format!("{}/.rs-shield/s3_credentials.enc", home_path);
        let cred_path = std::path::Path::new(&cred_file);

        if cred_path.exists() {
            match CredentialsManager::load_encrypted(&cred_file) {
                Ok(credentials) => {
                    // Create AWS Credentials from loaded S3Credentials
                    info!("✅ S3 credentials loaded from encrypted file");
                    return Ok(aws_credential_types::Credentials::new(
                        credentials.access_key.as_str(),
                        credentials.secret_key.as_str(),
                        credentials.session_token.as_ref().map(|s| s.to_string()),
                        None,
                        "rsb-encrypted-file",
                    ));
                }
                Err(e) => {
                    tracing::warn!("Failed to load encrypted credentials: {}", e);
                }
            }
        }
    }

    // Option 3: Prompt user interactively if no credentials found
    match CredentialsManager::prompt_for_credentials() {
        Ok(credentials) => {
            info!("✅ S3 credentials configured and loaded");
            return Ok(aws_credential_types::Credentials::new(
                credentials.access_key.as_str(),
                credentials.secret_key.as_str(),
                credentials.session_token.as_ref().map(|s| s.to_string()),
                None,
                "rsb-interactive",
            ));
        }
        Err(e) => {
            tracing::warn!("Failed to get credentials interactively: {}", e);
        }
    }

    // Option 4: Try to use config (deprecated, backward compatibility only)
    if let Some(s3) = config.s3.as_ref() {
        if let (Some(ak), Some(sk)) = (&s3.access_key, &s3.secret_key) {
            if !ak.is_empty() && !sk.is_empty() {
                tracing::warn!("⚠️  Credentials loaded from config file (not recommended!)");
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

    Err("No valid method to load S3 credentials found".to_string())
}
