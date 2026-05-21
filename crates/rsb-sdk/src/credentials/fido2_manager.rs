use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use base64::engine::general_purpose;
use keyring::Entry;
use pbkdf2::pbkdf2_hmac;
use rand::prelude::*;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::{debug, error, info};
use webauthn_rs::prelude::{
    CreationChallengeResponse, Passkey, PasskeyAuthentication, PasskeyRegistration,
    PublicKeyCredential, RegisterPublicKeyCredential, RequestChallengeResponse, Webauthn,
    WebauthnBuilder,
};
use zeroize::Zeroize;
/// ==============================
/// CREDENTIAL STRUCT
/// ==============================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fido2Credential {
    pub user_id: String,
    pub user_name: String,
    pub display_name: String,
    pub credential_data: Vec<u8>,
    pub created_at: String,
    pub last_used: Option<String>,
    pub counter: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UserCredentials {
    pub credentials: Vec<Fido2Credential>,
    pub recovery_code_hashes: Vec<String>,
    pub salt: Vec<u8>, // Salt específico do usuário para os recovery codes
}

/// ==============================
/// INTERNAL STATE
/// ==============================

#[derive(Clone)]
struct PendingRegistration {
    reg_state: PasskeyRegistration,
    user_id: String,
    user_name: String,
    display_name: String,
}

/// ==============================
/// MANAGER
/// ==============================

#[derive(Clone)]
pub struct Fido2Manager {
    webauthn: Webauthn,
    users: HashMap<String, UserCredentials>,
    registration_state: Option<PendingRegistration>,
    authentication_state: Option<(PasskeyAuthentication, String)>,
    rp_id: String,
}

// Keyring service name for the Data Encryption Key (DEK)
const KEYRING_SERVICE_NAME: &str = "rsb-fido2-dek";
const KEYRING_USERNAME: &str = "default"; // A generic username for the DEK entry

impl Fido2Manager {
    /// Create manager
    pub fn new(
        origin: &str,
        rp_id: &str,
        rp_name: &str,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let rp = Url::parse(origin)?;

        let webauthn = WebauthnBuilder::new(rp_id, &rp)?.rp_name(rp_name).build()?;

        Ok(Self {
            webauthn,
            users: HashMap::new(),
            registration_state: None,
            authentication_state: None,
            rp_id: rp_id.to_string(),
        })
    }

    /// Default storage path
    pub fn default_storage_path() -> Result<PathBuf, String> {
        dirs::home_dir()
            .map(|h| h.join(".rs-shield").join("fido2_credentials.json"))
            .ok_or_else(|| "Could not determine HOME directory".to_string())
    }

    /// Gets the DEK (Data Encryption Key) from the Keyring or generates a new one if it doesn't exist.
    fn get_or_create_dek() -> Result<Vec<u8>, Fido2Error> {
        let entry = Entry::new(KEYRING_SERVICE_NAME, KEYRING_USERNAME)
            .map_err(|e| Fido2Error::EncryptionError(format!("Keyring error: {}", e)))?;

        match entry.get_password() {
            Ok(password_base64) => general_purpose::STANDARD
                .decode(password_base64)
                .map_err(|e| Fido2Error::EncryptionError(format!("Failed to decode DEK: {}", e))),
            Err(keyring::Error::NoEntry) => {
                info!("No DEK found in keyring, generating a new one");
                let mut dek_bytes = [0u8; 32];
                rand::rng().fill_bytes(&mut dek_bytes);

                let dek_base64 = general_purpose::STANDARD.encode(dek_bytes);
                entry.set_password(&dek_base64).map_err(|e| {
                    Fido2Error::EncryptionError(format!("Failed to save DEK to keyring: {}", e))
                })?;

                Ok(dek_bytes.to_vec())
            }
            Err(e) => Err(Fido2Error::EncryptionError(format!(
                "Error accessing keyring: {}",
                e
            ))),
        }
    }

    /// Save credentials of the form encrypted using AES-256-GCM and PBKDF2.
    /// Structure of the file: [16B Salt][12B Nonce][Encrypted Payload]
    pub fn save_to_file(&self, path: &Path) -> Result<(), Fido2Error> {
        let plaintext = serde_json::to_vec(&self.users)
            .map_err(|e| Fido2Error::EncryptionError(format!("Serialize error: {}", e)))?;
        let mut rng = rand::rng();

        // 1. Generate random Salt (16 bytes)
        let mut salt = [0u8; 16];
        rng.fill_bytes(&mut salt);

        // 2. Derive 32-byte key (256 bits) using PBKDF2-HMAC-SHA256
        let dek = Self::get_or_create_dek()?;
        let mut key = [0u8; 32];
        let iterations = 600_000; // Recommended iterations for PBKDF2
        pbkdf2_hmac::<Sha256>(&dek, &salt, iterations, &mut key);

        // Enterprise Security: Clear DEK from memory after derivation
        let mut zeroized_dek = dek;
        zeroized_dek.zeroize();

        // 3. Initialize AES-256-GCM
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|e| Fido2Error::EncryptionError(e.to_string()))?;

        // Clear derived key after initializing cipher
        key.zeroize();

        // 4. Generate random Nonce (IV) of 12 bytes
        let mut nonce_bytes = [0u8; 12];
        rng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        // 5. Encrypt
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_slice())
            .map_err(|e| Fido2Error::EncryptionError(format!("Encryption failed: {}", e)))?;

        // 6. Concatenate Salt + Nonce + Ciphertext
        let mut final_payload =
            Vec::with_capacity(salt.len() + nonce_bytes.len() + ciphertext.len());
        final_payload.extend_from_slice(&salt);
        final_payload.extend_from_slice(&nonce_bytes);
        final_payload.extend_from_slice(&ciphertext);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                Fido2Error::EncryptionError(format!(
                    "Failed to create storage directory {:?}: {}",
                    parent, e
                ))
            })?;
        }

        fs::write(path, final_payload)
            .map_err(|e| Fido2Error::EncryptionError(format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    /// Loads and decrypts credentials from disk.
    pub fn load_from_file(&mut self, path: &Path) -> Result<(), Fido2Error> {
        if path.exists() {
            let encrypted_data = fs::read(path)
                .map_err(|e| Fido2Error::EncryptionError(format!("Failed to read file: {}", e)))?;

            if encrypted_data.len() < 28 {
                // 16 (salt) + 12 (nonce)
                return Err(Fido2Error::EncryptionError(
                    "Credential file corrupted or too short".into(),
                ));
            }

            // Extract metadata
            let (salt, rest) = encrypted_data.split_at(16);
            let (nonce_bytes, ciphertext) = rest.split_at(12);

            // Derive the same key
            let dek = Self::get_or_create_dek()?;

            let mut key = [0u8; 32];
            let iterations = 600_000; // Must match the saving iterations
            pbkdf2_hmac::<Sha256>(&dek, salt, iterations, &mut key);

            let mut zeroized_dek = dek;
            zeroized_dek.zeroize();

            let cipher = Aes256Gcm::new_from_slice(&key)
                .map_err(|e| Fido2Error::EncryptionError(e.to_string()))?;

            key.zeroize();

            let nonce = Nonce::from_slice(nonce_bytes);

            // Decrypt and validate integrity (GCM fails if data was altered)
            let plaintext = cipher.decrypt(nonce, ciphertext).map_err(|_| {
                Fido2Error::EncryptionError(
                    "Decryption failed: incorrect password or tampered data".into(),
                )
            })?;

            let users: HashMap<String, UserCredentials> = serde_json::from_slice(&plaintext)
                .map_err(|e| {
                    Fido2Error::EncryptionError(format!("Deserialization failure: {}", e))
                })?;
            self.users = users;

            info!("Securely loaded {} credentials", self.users.len());
        }
        Ok(())
    }

    pub fn start_registration(
        &mut self,
        user_id: &str,
        user_name: &str,
        display_name: &str,
    ) -> Result<CreationChallengeResponse, Fido2Error> {
        debug!("Starting registration for {}", user_id);

        let ns = uuid::Uuid::NAMESPACE_DNS;
        let user_uuid = uuid::Uuid::new_v5(&ns, user_id.as_bytes());

        // Se o usuário já existe, carregamos as chaves atuais para evitar duplicatas (exclude_credentials)
        let exclude_credentials = self.users.get(user_id).map(|u| {
            u.credentials
                .iter()
                .filter_map(|c| {
                    let pk: Result<Passkey, _> = serde_json::from_slice(&c.credential_data);
                    pk.ok().map(|p| p.cred_id().clone())
                })
                .collect::<Vec<_>>()
        });

        let (challenge, reg_state) = self
            .webauthn
            .start_passkey_registration(user_uuid, user_name, display_name, exclude_credentials)
            .map_err(|e| {
                error!("Registration challenge failed: {:?}", e);
                Fido2Error::RegistrationFailed(format!("{:?}", e))
            })?;

        self.registration_state = Some(PendingRegistration {
            reg_state,
            user_id: user_id.to_string(),
            user_name: user_name.to_string(),
            display_name: display_name.to_string(),
        });

        Ok(challenge)
    }

    pub fn finish_registration(
        &mut self,
        reg_response: RegisterPublicKeyCredential,
    ) -> Result<Fido2Credential, Fido2Error> {
        let pending = self
            .registration_state
            .take()
            .ok_or(Fido2Error::NoRegistrationInProgress)?;

        let passkey = self
            .webauthn
            .finish_passkey_registration(&reg_response, &pending.reg_state)
            .map_err(|e| Fido2Error::RegistrationFailed(format!("{:?}", e)))?;

        let credential = Fido2Credential {
            user_id: pending.user_id.clone(),
            user_name: pending.user_name.clone(),
            display_name: pending.display_name.clone(),
            credential_data: serde_json::to_vec(&passkey)
                .map_err(|e| Fido2Error::RegistrationFailed(e.to_string()))?,
            created_at: chrono::Local::now().to_rfc3339(),
            last_used: None,
            counter: 0,
        };

        let user_entry = self
            .users
            .entry(pending.user_id.clone())
            .or_insert_with(UserCredentials::default);
        user_entry.credentials.push(credential.clone());

        Ok(credential)
    }

    pub fn start_authentication(
        &mut self,
        user_id: &str,
    ) -> Result<RequestChallengeResponse, Fido2Error> {
        debug!("Starting authentication for {}", user_id);

        let user_data = self
            .users
            .get(user_id)
            .ok_or(Fido2Error::CredentialNotFound)?;

        let passkeys: Vec<Passkey> = user_data
            .credentials
            .iter()
            .filter_map(|c| serde_json::from_slice(&c.credential_data).ok())
            .collect();

        if passkeys.is_empty() {
            return Err(Fido2Error::CredentialNotFound);
        }

        let (challenge, auth_state) = self
            .webauthn
            .start_passkey_authentication(&passkeys)
            .map_err(|e| {
                error!("Authentication challenge failed: {:?}", e);
                Fido2Error::AuthenticationFailed(format!("{:?}", e))
            })?;

        self.authentication_state = Some((auth_state, user_id.to_string()));

        Ok(challenge)
    }

    pub fn finish_authentication(
        &mut self,
        auth_response: PublicKeyCredential,
    ) -> Result<String, Fido2Error> {
        debug!("Finishing authentication");

        let (auth_state, user_id) = self
            .authentication_state
            .take()
            .ok_or(Fido2Error::NoAuthenticationInProgress)?;

        let result = self
            .webauthn
            .finish_passkey_authentication(&auth_response, &auth_state)
            .map_err(|e| {
                error!("Authentication failed: {:?}", e);
                Fido2Error::AuthenticationFailed(format!("{:?}", e))
            })?;

        let new_counter = result.counter();
        let cred_id = result.cred_id();

        // Find and update the specific credential
        let credential = self
            .get_credential_mut(&user_id, cred_id)
            .ok_or(Fido2Error::CredentialNotFound)?;

        credential.counter = new_counter;
        credential.last_used = Some(chrono::Local::now().to_rfc3339());

        // Update counter within stored Passkey JSON
        // This is necessary because the `credential_data` stores the Passkey,
        // and the Passkey's internal counter needs to be updated for subsequent authentications.
        let mut passkey_json: serde_json::Value =
            serde_json::from_slice(&credential.credential_data)
                .map_err(|e| Fido2Error::AuthenticationFailed(format!("Corrupted data: {}", e)))?;

        if let Some(cred_obj) = passkey_json.get_mut("cred") {
            if let Some(counter_val) = cred_obj.get_mut("counter") {
                *counter_val = serde_json::Value::from(new_counter);
            } else {
                // If 'counter' field is missing, add it. This might happen with older formats.
                cred_obj["counter"] = serde_json::Value::from(new_counter);
            }
        } else {
            // If 'cred' field is missing, this indicates a serious data corruption.
            return Err(Fido2Error::AuthenticationFailed(
                "Corrupted Passkey data: 'cred' field missing".to_string(),
            ));
        }
        credential.credential_data = serde_json::to_vec(&passkey_json)
            .map_err(|e| Fido2Error::AuthenticationFailed(format!("Serialization error: {}", e)))?;

        info!("Authentication successful for {}", user_id);

        Ok(user_id)
    }

    /// Helper to get a mutable reference to a specific Fido2Credential.
    pub fn get_credential_mut(
        &mut self,
        user_id: &str,
        cred_id: &webauthn_rs::prelude::CredentialID,
    ) -> Option<&mut Fido2Credential> {
        self.users.get_mut(user_id).and_then(|user_data| {
            user_data.credentials.iter_mut().find(|c| {
                let pk: Result<Passkey, _> = serde_json::from_slice(&c.credential_data);
                pk.map(|p| p.cred_id() == cred_id).unwrap_or(false)
            })
        })
    }

    /// Gera novos códigos de recuperação para um usuário.
    /// Retorna os códigos em texto claro (deve ser exibido apenas uma vez).
    pub fn generate_backup_codes(&mut self, user_id: &str) -> Result<Vec<String>, Fido2Error> {
        let mut rng = rand::rng();
        let mut plain_codes = Vec::new();
        let mut hashed_codes = Vec::new();

        // Gerar 16 bytes de salt para o usuário
        let mut salt = [0u8; 16];
        rng.fill_bytes(&mut salt);

        for _ in 0..10 {
            // Gerar código legível: 12 caracteres alfanuméricos
            let code: String = (0..12)
                .map(|_| rng.sample(rand::distr::Alphanumeric) as char)
                .collect();

            let mut hash = [0u8; 32];
            pbkdf2::pbkdf2_hmac::<Sha256>(code.as_bytes(), &salt, 100_000, &mut hash);

            plain_codes.push(code);
            hashed_codes.push(general_purpose::STANDARD.encode(hash));
        }

        let user_entry = self.users.entry(user_id.to_string()).or_default();
        user_entry.recovery_code_hashes = hashed_codes;
        user_entry.salt = salt.to_vec();

        Ok(plain_codes)
    }

    /// Valida e consome um código de recuperação.
    pub fn verify_backup_code(&mut self, user_id: &str, code: &str) -> bool {
        let user_data = match self.users.get_mut(user_id) {
            Some(u) => u,
            None => return false,
        };

        if user_data.recovery_code_hashes.is_empty() {
            return false;
        }

        let mut input_hash = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(code.as_bytes(), &user_data.salt, 100_000, &mut input_hash);
        let input_hash_encoded = general_purpose::STANDARD.encode(input_hash);

        if let Some(pos) = user_data
            .recovery_code_hashes
            .iter()
            .position(|h| h == &input_hash_encoded)
        {
            user_data.recovery_code_hashes.remove(pos);
            info!("Recovery code used and invalidated for user: {}", user_id);
            return true;
        }

        false
    }

    pub fn list_credentials(&self) -> Vec<Fido2Credential> {
        self.users
            .values()
            .flat_map(|u| u.credentials.clone())
            .collect()
    }

    pub fn list_user_credentials(&self, user_id: &str) -> Vec<Fido2Credential> {
        self.users
            .get(user_id)
            .map(|u| u.credentials.clone())
            .unwrap_or_default()
    }

    pub fn revoke_user(&mut self, user_id: &str) -> Result<(), Fido2Error> {
        self.users
            .remove(user_id)
            .ok_or(Fido2Error::CredentialNotFound)?;
        Ok(())
    }

    pub fn revoke_credential(
        &mut self,
        user_id: &str,
        cred_id_hex: &str,
    ) -> Result<(), Fido2Error> {
        if let Some(user_data) = self.users.get_mut(user_id) {
            user_data.credentials.retain(|c| {
                let pk: Result<Passkey, _> = serde_json::from_slice(&c.credential_data);
                match pk {
                    Ok(p) => hex::encode(p.cred_id()) != cred_id_hex,
                    Err(_) => true,
                }
            });
            Ok(())
        } else {
            Err(Fido2Error::CredentialNotFound)
        }
    }

    pub fn has_credential(&self, user_id: &str) -> bool {
        self.users
            .get(user_id)
            .map(|u| !u.credentials.is_empty())
            .unwrap_or(false)
    }

    pub fn rp_id(&self) -> String {
        self.rp_id.clone()
    }
}

#[derive(Debug, Clone)]
pub enum Fido2Error {
    CredentialNotFound,
    RegistrationFailed(String),
    AuthenticationFailed(String),
    NoRegistrationInProgress,
    NoAuthenticationInProgress,
    EncryptionError(String),
    KeyringError(String),
}

impl std::fmt::Display for Fido2Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::CredentialNotFound => write!(f, "Credential not found"),
            Self::RegistrationFailed(e) => write!(f, "Registration failed: {}", e),
            Self::AuthenticationFailed(e) => write!(f, "Authentication failed: {}", e),
            Self::NoRegistrationInProgress => write!(f, "No registration in progress"),
            Self::NoAuthenticationInProgress => write!(f, "No authentication in progress"),
            Self::EncryptionError(e) => write!(f, "Encryption error: {}", e),
            Self::KeyringError(e) => write!(f, "Keyring error: {}", e),
        }
    }
}

impl std::error::Error for Fido2Error {}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_manager() -> Fido2Manager {
        Fido2Manager::new("https://example.com", "example.com", "Test")
            .expect("Failed to create manager")
    }

    #[test]
    fn test_creation() {
        let _mgr = create_manager(); // No direct access to mgr.credentials
    }

    #[test]
    fn test_has_credential() {
        let mgr = create_manager();
        assert!(!mgr.has_credential("user1"));
    }

    #[test]
    fn test_list_empty() {
        let mgr = create_manager();
        assert_eq!(mgr.list_credentials().len(), 0);
    }
}
