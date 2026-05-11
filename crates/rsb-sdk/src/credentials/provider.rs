// AWS Credentials Provider Layer
// Centralizes credential handling for all storage backends
// Supports multiple credential sources without environment variable side effects

use aws_credential_types::Credentials;
use aws_credential_types::provider::{self, ProvideCredentials};
use std::sync::Arc;

/// Represents different credential sources
#[derive(Debug, Clone)]
pub enum CredentialSource {
    /// Static credentials provided directly
    Static {
        access_key: String,
        secret_key: String,
        session_token: Option<String>,
    },
    /// Credentials from environment variables (for backward compatibility)
    Environment,
}

/// Provider wrapper that handles credential resolution
pub struct CredentialsProvider {
    source: CredentialSource,
}

impl CredentialsProvider {
    /// Create a provider from static credentials
    pub fn from_static(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
        session_token: Option<String>,
    ) -> Self {
        Self {
            source: CredentialSource::Static {
                access_key: access_key.into(),
                secret_key: secret_key.into(),
                session_token,
            },
        }
    }

    /// Create a provider that reads from environment variables
    pub fn from_environment() -> Self {
        Self {
            source: CredentialSource::Environment,
        }
    }

    /// Resolve credentials to aws_credential_types::Credentials
    pub fn resolve(&self) -> Result<Credentials, String> {
        match &self.source {
            CredentialSource::Static {
                access_key,
                secret_key,
                session_token,
            } => {
                Ok(Credentials::new(
                    access_key.clone(),
                    secret_key.clone(),
                    session_token.clone(),
                    None,
                    "rsb-sdk", // provider name
                ))
            }
            CredentialSource::Environment => {
                let access_key = std::env::var("AWS_ACCESS_KEY_ID")
                    .map_err(|_| "AWS_ACCESS_KEY_ID not found in environment".to_string())?;
                let secret_key = std::env::var("AWS_SECRET_ACCESS_KEY")
                    .map_err(|_| "AWS_SECRET_ACCESS_KEY not found in environment".to_string())?;
                let session_token = std::env::var("AWS_SESSION_TOKEN").ok();

                Ok(Credentials::new(
                    access_key,
                    secret_key,
                    session_token,
                    None,
                    "rsb-sdk-env",
                ))
            }
        }
    }
}

/// Shared credentials provider for use across multiple backends
pub type SharedCredentialsProvider = Arc<CredentialsProvider>;

/// Create a shared static credentials provider
pub fn create_static_provider(
    access_key: impl Into<String>,
    secret_key: impl Into<String>,
    session_token: Option<String>,
) -> SharedCredentialsProvider {
    Arc::new(CredentialsProvider::from_static(
        access_key,
        secret_key,
        session_token,
    ))
}

/// Create a shared environment credentials provider
pub fn create_env_provider() -> SharedCredentialsProvider {
    Arc::new(CredentialsProvider::from_environment())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_static_credentials_provider() {
        let provider = CredentialsProvider::from_static("AKIA123", "secret456", None);
        let creds = provider.resolve().unwrap();

        assert_eq!(creds.access_key_id(), "AKIA123");
        assert_eq!(creds.secret_access_key(), "secret456");
        assert_eq!(creds.session_token(), None);
    }

    #[test]
    fn test_static_credentials_with_session_token() {
        let provider =
            CredentialsProvider::from_static("AKIA123", "secret456", Some("token789".to_string()));
        let creds = provider.resolve().unwrap();

        assert_eq!(creds.access_key_id(), "AKIA123");
        assert_eq!(creds.secret_access_key(), "secret456");
        assert_eq!(creds.session_token(), Some("token789"));
    }
}
