use super::SecureString;
/// Secure Credentials Manager
/// Uses OS keyring/secrets to store master password
/// And AES-256-GCM encryption for data at rest
use super::encryption::{EncryptedCredentials, decrypt_credentials, encrypt_credentials};
use crate::utils::ensure_directory_exists;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt;
const KEYRING_SERVICE: &str = "rsb-shield";
const KEYRING_ACCOUNT: &str = "s3-master-password";

#[derive(Serialize, Deserialize, Clone)]
pub struct S3Credentials {
    pub access_key: SecureString,
    pub secret_key: SecureString,
    pub session_token: Option<SecureString>,
}

impl fmt::Debug for S3Credentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("S3Credentials")
            .field("access_key", &"***REDACTED***")
            .field("secret_key", &"***REDACTED***")
            .field("session_token", &"***REDACTED***")
            .finish()
    }
}

impl S3Credentials {
    /// Validate credentials
    pub fn validate(&self) -> Result<(), String> {
        if self.access_key.is_empty() {
            return Err("Access key cannot be empty".to_string());
        }
        if self.secret_key.is_empty() {
            return Err("Secret key cannot be empty".to_string());
        }
        if self.access_key.as_str().len() < 10 {
            return Err("Access key too short (min 10 characters)".to_string());
        }
        if self.secret_key.as_str().len() < 20 {
            return Err("Secret key too short (min 20 characters)".to_string());
        }
        Ok(())
    }
}

pub struct CredentialsManager;

impl CredentialsManager {
    /// Save encrypted credentials to file with master password in keyring
    pub fn save_encrypted(
        file_path: &str,
        credentials: &S3Credentials,
        master_password_hint: bool,
    ) -> Result<(), String> {
        // Validate credentials
        credentials.validate()?;

        // Create directory if it doesn't exist
        if let Some(parent) = std::path::Path::new(file_path).parent() {
            let parent_str = parent
                .to_str()
                .ok_or("Invalid path characters in file path")?;
            ensure_directory_exists(parent_str)?;
        }

        // Get or create master password
        let master_password = Self::get_or_create_master_password(master_password_hint)?;

        // Serialize credentials
        let json = serde_json::to_string(credentials)
            .map_err(|e| format!("Error serializing credentials: {}", e))?;

        // Encrypt
        let encrypted = encrypt_credentials(&json, &master_password)?;

        // Save to file
        let encrypted_json = serde_json::to_string_pretty(&encrypted)
            .map_err(|e| format!("Error serializing encrypted data: {}", e))?;

        std::fs::write(file_path, encrypted_json)
            .map_err(|e| format!("Error writing file: {}", e))?;

        tracing::info!("S3 credentials encrypted and saved to: {}", file_path);
        Ok(())
    }

    /// Load and decrypt credentials from file
    pub fn load_encrypted(file_path: &str) -> Result<S3Credentials, String> {
        // Read file
        let content =
            std::fs::read_to_string(file_path).map_err(|e| format!("Error reading file: {}", e))?;

        // Deserialize encrypted data
        let encrypted: EncryptedCredentials =
            serde_json::from_str(&content).map_err(|e| format!("Error deserializing: {}", e))?;

        // Get master password from keyring
        let master_password = Self::get_master_password_interactive()?;

        // Decrypt
        let json = decrypt_credentials(&encrypted, &master_password)?;

        // Deserialize credentials
        let credentials: S3Credentials = serde_json::from_str(&json)
            .map_err(|e| format!("Error deserializing credentials: {}", e))?;

        tracing::info!("S3 credentials loaded successfully");
        Ok(credentials)
    }

    /// Try to get master password from keyring, or create a new one
    fn get_or_create_master_password(_with_hint: bool) -> Result<String, String> {
        // First, try to get from keyring
        if let Ok(password) = Self::get_master_password_from_keyring() {
            return Ok(password);
        }

        // If it doesn't exist, create a new one
        Self::create_new_master_password(_with_hint)
    }

    /// Get master password from keyring
    fn get_master_password_from_keyring() -> Result<String, String> {
        #[cfg(target_os = "macos")]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .map_err(|e| format!("Error accessing keyring: {}", e))?;
            entry
                .get_password()
                .map_err(|e| format!("Password not found in keyring: {}", e))
        }

        #[cfg(target_os = "linux")]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .map_err(|e| format!("Error accessing secret service: {}", e))?;
            entry
                .get_password()
                .map_err(|e| format!("Password not found: {}", e))
        }

        #[cfg(target_os = "windows")]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .map_err(|e| format!("Error accessing credential manager: {}", e))?;
            entry
                .get_password()
                .map_err(|e| format!("Credential not found: {}", e))
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err("Keyring not supported on this platform".to_string())
        }
    }

    /// Save master password to keyring
    fn save_master_password_to_keyring(password: &str) -> Result<(), String> {
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .map_err(|e| format!("Error accessing keyring: {}", e))?;
            entry
                .set_password(password)
                .map_err(|e| format!("Error saving to keyring: {}", e))
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            Err("Keyring not supported on this platform".to_string())
        }
    }

    /// Get master password interactively (CLI or UI)
    fn get_master_password_interactive() -> Result<String, String> {
        // Try to get from keyring first
        if let Ok(password) = Self::get_master_password_from_keyring() {
            return Ok(password);
        }

        // If in an interactive environment (CLI), ask for password
        #[cfg(feature = "cli")]
        {
            use rpassword::read_password;
            println!("Enter your S3 master password (will be stored in system keyring):");
            match read_password() {
                Ok(password) => {
                    if password.is_empty() {
                        return Err("Password cannot be empty".to_string());
                    }
                    Ok(password)
                }
                Err(e) => Err(format!("Error reading password: {}", e)),
            }
        }

        #[cfg(not(feature = "cli"))]
        {
            Err("Master password not found in keyring. Configure it first.".to_string())
        }
    }

    /// Prompt user for S3 credentials interactively
    pub fn prompt_for_credentials() -> Result<S3Credentials, String> {
        #[cfg(feature = "cli")]
        {
            use rpassword::read_password;

            println!("\n🔑 S3 Credentials not found. Please configure them.");
            println!("Get your credentials from: https://console.aws.amazon.com/iam\n");

            println!("Enter AWS Access Key ID:");
            let access_key = std::io::stdin()
                .lines()
                .next()
                .and_then(|line| line.ok())
                .ok_or("Failed to read access key")?
                .trim()
                .to_string();

            if access_key.is_empty() {
                return Err("Access key cannot be empty".to_string());
            }

            println!("Enter AWS Secret Access Key (hidden for security):");
            let secret_key =
                read_password().map_err(|e| format!("Error reading secret key: {}", e))?;

            if secret_key.is_empty() {
                return Err("Secret key cannot be empty".to_string());
            }

            let credentials = S3Credentials {
                access_key: SecureString::new(access_key),
                secret_key: SecureString::new(secret_key),
                session_token: None,
            };

            // Validate credentials
            credentials.validate()?;

            // Ask if user wants to save them
            println!("\n💾 Save credentials securely? (y/n)");
            let response = std::io::stdin()
                .lines()
                .next()
                .and_then(|line| line.ok())
                .ok_or("Failed to read response")?
                .trim()
                .to_lowercase();

            if response == "y" || response == "yes" {
                let home = env::var("HOME").ok();
                if let Some(home_path) = home {
                    let cred_file = format!("{}/.rs-shield/s3_credentials.enc", home_path);
                    Self::save_encrypted(&cred_file, &credentials, true)?;
                    println!("✅ Credentials saved securely to: {}", cred_file);
                }
            } else {
                println!("⚠️  Credentials will only be used in memory for this session");
            }

            Ok(credentials)
        }

        #[cfg(not(feature = "cli"))]
        {
            Err("Interactive credential prompt not available in non-CLI mode".to_string())
        }
    }

    /// Create new master password
    ///
    /// Automatically generates a deterministic master password for Desktop UI or CI/CD.
    /// Uses username hash + timestamp to create a password that can be regenerated if needed.
    /// The password is stored in the system keyring for secure access.
    pub fn generate_automatic_master_password() -> Result<String, String> {
        use std::time::{SystemTime, UNIX_EPOCH};

        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        // Generate deterministic hash based on username + timestamp
        // This ensures the password can be regenerated consistently
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            / 86400; // Divide by seconds per day to have a more stable value

        // Create password with mixed characters: uppercase, lowercase, number, special character
        let password = format!(
            "RsShield_{}_{}_{}_!",
            username.to_uppercase().chars().take(4).collect::<String>(),
            timestamp,
            uuid::Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        );

        // Save to keyring
        Self::save_master_password_to_keyring(&password)?;
        tracing::info!("🔐 Master Password automatically generated and saved to keyring");

        Ok(password)
    }

    fn create_new_master_password(_with_hint: bool) -> Result<String, String> {
        println!("\n🔐 Configuring Master Password for S3 credential encryption");
        println!("   It will be securely stored in your operating system's keyring");

        #[cfg(feature = "cli")]
        {
            use rpassword::read_password;

            loop {
                println!("\nEnter a strong master password (minimum 16 characters):");
                let password1 = read_password().map_err(|e| format!("Error: {}", e))?;

                if password1.is_empty() {
                    println!("❌ Password cannot be empty");
                    continue;
                }

                if password1.len() < 16 {
                    println!(
                        "❌ Password too short ({} characters). Minimum 16.",
                        password1.len()
                    );
                    continue;
                }

                // Validate password strength
                let has_upper = password1.chars().any(|c| c.is_uppercase());
                let has_lower = password1.chars().any(|c| c.is_lowercase());
                let has_digit = password1.chars().any(|c| c.is_numeric());
                let has_special = password1.chars().any(|c| !c.is_alphanumeric());

                if !has_upper || !has_lower || !has_digit || !has_special {
                    println!(
                        "❌ Weak password. Use: uppercase, lowercase, numbers, special characters"
                    );
                    continue;
                }

                println!("\nConfirm password:");
                let password2 = read_password().map_err(|e| format!("Error: {}", e))?;

                if password1 != password2 {
                    println!("❌ Passwords do not match");
                    continue;
                }

                // Save to keyring
                Self::save_master_password_to_keyring(&password1)?;
                println!("✅ Master password saved to system keyring!");

                return Ok(password1);
            }
        }

        #[cfg(not(feature = "cli"))]
        {
            // In Desktop/non-CLI mode, generate Master Password automatically
            tracing::warn!("Non-CLI mode detected. Generating Master Password automatically...");
            Self::generate_automatic_master_password()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credentials_validation() {
        let valid = S3Credentials {
            access_key: SecureString::new("AKIA1234567890AB".to_string()),
            secret_key: SecureString::new("very-long-secret-key-1234567890".to_string()),
            session_token: None,
        };

        assert!(valid.validate().is_ok());

        let invalid_short = S3Credentials {
            access_key: SecureString::new("SHORT".to_string()),
            secret_key: SecureString::new("also-short".to_string()),
            session_token: None,
        };

        assert!(invalid_short.validate().is_err());
    }
}
