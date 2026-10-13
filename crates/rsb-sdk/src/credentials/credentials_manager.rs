use super::SecureString;
/// Secure Credentials Manager
/// Uses OS keyring/secrets to store master password
/// And AES-256-GCM encryption for data at rest
use super::encryption::{
    CURRENT_VERSION, EncryptedCredentials, decrypt_credentials, encrypt_credentials,
};
#[cfg(not(unix))]
use crate::utils::ensure_directory_exists;
use serde::{Deserialize, Serialize};
use std::fmt;
use zeroize::Zeroizing;
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
    /// Credenciais a partir das chaves digitadas (espaços nas pontas
    /// removidos; sem session token).
    pub fn from_keys(access_key: &str, secret_key: &str) -> Self {
        S3Credentials {
            access_key: SecureString::new(access_key.trim().to_string()),
            secret_key: SecureString::new(secret_key.trim().to_string()),
            session_token: None,
        }
    }

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

/// Diretório (relativo a `$HOME`) do ficheiro de credenciais cifrado.
pub const CREDENTIALS_DIR: &str = ".rs-shield";
/// Nome do ficheiro de credenciais S3 cifrado.
pub const CREDENTIALS_FILE: &str = "s3_credentials.enc";

impl CredentialsManager {
    /// `<home>/.rs-shield/s3_credentials.enc`.
    pub fn credentials_path_in(home: &std::path::Path) -> std::path::PathBuf {
        home.join(CREDENTIALS_DIR).join(CREDENTIALS_FILE)
    }

    /// Caminho por omissão do ficheiro de credenciais S3 cifrado
    /// (`$HOME/.rs-shield/s3_credentials.enc`) — o mesmo que
    /// `storage_backend` lê. `None` se não houver diretório home.
    pub fn default_credentials_path() -> Option<std::path::PathBuf> {
        std::env::var_os("HOME")
            .filter(|h| !h.is_empty())
            .map(std::path::PathBuf::from)
            .or_else(dirs::home_dir)
            .map(|home| Self::credentials_path_in(&home))
    }

    /// Guarda as chaves S3 no ficheiro cifrado por omissão (password mestra
    /// no keyring do SO). Nunca escreve as chaves em claro nem em logs.
    /// Devolve o caminho do ficheiro escrito.
    pub fn save_default(credentials: &S3Credentials) -> Result<std::path::PathBuf, String> {
        let path = Self::default_credentials_path()
            .ok_or("Home directory not found: cannot store S3 credentials")?;
        let path_str = path
            .to_str()
            .ok_or("Invalid characters in credentials path")?;
        Self::save_encrypted(path_str, credentials, false)?;
        Ok(path)
    }

    /// Save encrypted credentials to file with master password in keyring
    pub fn save_encrypted(
        file_path: &str,
        credentials: &S3Credentials,
        master_password_hint: bool,
    ) -> Result<(), String> {
        // Validate credentials
        credentials.validate()?;

        // Create directory if it doesn't exist (0700 on Unix)
        Self::ensure_private_parent_dir(file_path)?;

        // Get or create master password (wiped on drop)
        let master_password = Self::get_or_create_master_password(master_password_hint)?;

        // Serialize credentials (plaintext JSON is wiped on drop)
        let json = Zeroizing::new(
            serde_json::to_string(credentials)
                .map_err(|e| format!("Error serializing credentials: {}", e))?,
        );

        // Encrypt
        let encrypted = encrypt_credentials(&json, &master_password)?;

        // Save to file (atomic, 0600)
        Self::write_encrypted_file(file_path, &encrypted)?;

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

        // Get master password from keyring (wiped on drop)
        let master_password = Self::get_master_password_interactive()?;

        // Decrypt (plaintext JSON is wiped on drop)
        let json = decrypt_credentials(&encrypted, &master_password)?;

        // Deserialize credentials
        let credentials: S3Credentials = serde_json::from_str(&json)
            .map_err(|e| format!("Error deserializing credentials: {}", e))?;

        // Transparent upgrade: re-encrypt legacy files with the current
        // version / iteration count. A failure here must not block the load.
        if encrypted.needs_upgrade() {
            match encrypt_credentials(&json, &master_password)
                .and_then(|upgraded| Self::write_encrypted_file(file_path, &upgraded))
            {
                Ok(()) => tracing::info!(
                    "Upgraded credentials file to version {}: {}",
                    CURRENT_VERSION,
                    file_path
                ),
                Err(e) => tracing::warn!(
                    "Could not upgrade credentials file {} (kept old version): {}",
                    file_path,
                    e
                ),
            }
        }

        tracing::info!("S3 credentials loaded successfully");
        Ok(credentials)
    }

    /// Create the parent directory of `file_path` if needed. On Unix new
    /// directories get mode 0700, and our own `.rs-shield` dir is tightened
    /// to 0700 if it already existed.
    fn ensure_private_parent_dir(file_path: &str) -> Result<(), String> {
        let Some(parent) = std::path::Path::new(file_path).parent() else {
            return Ok(());
        };
        if parent.as_os_str().is_empty() {
            return Ok(());
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(parent)
                .map_err(|e| format!("Error creating directory {}: {}", parent.display(), e))?;
            if parent.file_name().is_some_and(|n| n == ".rs-shield") {
                std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700)).map_err(
                    |e| format!("Error setting permissions on {}: {}", parent.display(), e),
                )?;
            }
            Ok(())
        }

        #[cfg(not(unix))]
        {
            let parent_str = parent
                .to_str()
                .ok_or("Invalid path characters in file path")?;
            ensure_directory_exists(parent_str).map(|_| ())
        }
    }

    /// Atomically write the encrypted credentials: write to a temp file in the
    /// same directory, created with mode 0600 (no umask window), fsync, then
    /// rename over the destination. Errors are propagated, never ignored.
    pub(crate) fn write_encrypted_file(
        file_path: &str,
        encrypted: &EncryptedCredentials,
    ) -> Result<(), String> {
        let encrypted_json = serde_json::to_string_pretty(encrypted)
            .map_err(|e| format!("Error serializing encrypted data: {}", e))?;
        crate::config::write_file_atomic_secure(
            std::path::Path::new(file_path),
            encrypted_json.as_bytes(),
        )
        .map_err(|e| format!("Error writing file: {}", e))
    }

    /// Try to get master password from keyring, or create a new one.
    /// A new password is created ONLY when the keyring has no entry; any other
    /// keyring error is propagated so an existing password is never
    /// overwritten (which would make an existing .enc file unreadable).
    fn get_or_create_master_password(_with_hint: bool) -> Result<Zeroizing<String>, String> {
        match Self::get_master_password_from_keyring()? {
            Some(password) => Ok(password),
            None => Self::create_new_master_password(_with_hint),
        }
    }

    /// Get master password from keyring.
    /// Returns `Ok(None)` only when the keyring has no entry (`NoEntry`).
    fn get_master_password_from_keyring() -> Result<Option<Zeroizing<String>>, String> {
        #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
        {
            let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ACCOUNT)
                .map_err(|e| format!("Error accessing keyring: {}", e))?;
            match entry.get_password() {
                Ok(password) => Ok(Some(Zeroizing::new(password))),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(e) => Err(format!("Error reading master password from keyring: {}", e)),
            }
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
    fn get_master_password_interactive() -> Result<Zeroizing<String>, String> {
        // Try to get from keyring first
        match Self::get_master_password_from_keyring() {
            Ok(Some(password)) => return Ok(password),
            Ok(None) => {}
            // Prompting is harmless here (nothing is overwritten): fall through.
            Err(e) => tracing::warn!("{}", e),
        }

        // If in an interactive environment (CLI), ask for password
        #[cfg(feature = "cli")]
        {
            use rpassword::read_password;
            println!("Enter your S3 master password (will be stored in system keyring):");
            match read_password() {
                Ok(password) => {
                    let password = Zeroizing::new(password);
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
                let home = std::env::var("HOME").ok();
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
        // ANTES: formato previsível `RsShield_USER_dia_uuid8` (~32 bits efetivos).
        // AGORA: 32 bytes CSPRNG (256 bits) em Base64 — sem estrutura fingerprintável.
        use rand::Rng;
        let mut buf = [0u8; 32];
        rand::rng().fill_bytes(&mut buf);
        let password = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, buf);
        zeroize::Zeroize::zeroize(&mut buf);

        // Save to keyring
        Self::save_master_password_to_keyring(&password)?;
        tracing::info!(
            "🔐 Master Password automatically generated (256-bit CSPRNG) and saved to keyring"
        );

        Ok(password)
    }

    fn create_new_master_password(_with_hint: bool) -> Result<Zeroizing<String>, String> {
        println!("\n🔐 Configuring Master Password for S3 credential encryption");
        println!("   It will be securely stored in your operating system's keyring");

        #[cfg(feature = "cli")]
        {
            use rpassword::read_password;

            loop {
                println!("\nEnter a strong master password (minimum 16 characters):");
                let password1 =
                    Zeroizing::new(read_password().map_err(|e| format!("Error: {}", e))?);

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
                let password2 =
                    Zeroizing::new(read_password().map_err(|e| format!("Error: {}", e))?);

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
            Self::generate_automatic_master_password().map(Zeroizing::new)
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

    #[test]
    fn test_from_keys_trims_and_default_path() {
        let c =
            S3Credentials::from_keys("  AKIA1234567890AB ", "\tvery-long-secret-key-1234567890\n");
        assert_eq!(c.access_key.as_str(), "AKIA1234567890AB");
        assert_eq!(c.secret_key.as_str(), "very-long-secret-key-1234567890");
        assert!(c.session_token.is_none());
        assert!(c.validate().is_ok());
        assert!(!format!("{:?}", c).contains("AKIA"));

        let home = std::path::Path::new("/home/someone");
        assert_eq!(
            CredentialsManager::credentials_path_in(home),
            std::path::PathBuf::from("/home/someone/.rs-shield/s3_credentials.enc")
        );
        if let Some(p) = CredentialsManager::default_credentials_path() {
            assert!(
                p.ends_with(".rs-shield/s3_credentials.enc"),
                "{}",
                p.display()
            );
        }
    }

    #[test]
    fn test_write_encrypted_file_is_private_and_atomic() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join(".rs-shield");
        let path = sub.join("s3_credentials.enc");
        let path_str = path.to_str().unwrap();

        CredentialsManager::ensure_private_parent_dir(path_str).unwrap();
        let enc = encrypt_credentials("{}", "pw").unwrap();
        CredentialsManager::write_encrypted_file(path_str, &enc).unwrap();
        // Overwrite must also succeed (rename over existing file).
        CredentialsManager::write_encrypted_file(path_str, &enc).unwrap();

        let back: EncryptedCredentials =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(back.version, CURRENT_VERSION);
        // No temp files left behind.
        assert_eq!(std::fs::read_dir(&sub).unwrap().count(), 1);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            let dir_mode = std::fs::metadata(&sub).unwrap().permissions().mode() & 0o777;
            assert_eq!(file_mode, 0o600);
            assert_eq!(dir_mode, 0o700);
        }
    }
}
