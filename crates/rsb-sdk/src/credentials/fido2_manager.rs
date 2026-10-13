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
use std::time::Instant;
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

#[derive(Clone, Serialize, Deserialize)]
pub struct Fido2Credential {
    pub user_id: String,
    pub user_name: String,
    pub display_name: String,
    pub credential_data: Vec<u8>,
    pub created_at: String,
    pub last_used: Option<String>,
    pub counter: u32,
}

impl std::fmt::Debug for Fido2Credential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fido2Credential")
            .field("user_id", &self.user_id)
            .field("user_name", &self.user_name)
            .field("display_name", &self.display_name)
            .field("credential_data", &"***REDACTED***")
            .field("created_at", &self.created_at)
            .field("last_used", &self.last_used)
            .field("counter", &self.counter)
            .finish()
    }
}

/// Iterações PBKDF2 usadas pelos hashes de recovery codes antigos (sem versão).
pub const LEGACY_RECOVERY_CODE_ITERATIONS: u32 = 100_000;
/// Iterações PBKDF2 para novos hashes de recovery codes.
pub const RECOVERY_CODE_ITERATIONS: u32 = 600_000;

fn legacy_recovery_iterations() -> u32 {
    LEGACY_RECOVERY_CODE_ITERATIONS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserCredentials {
    pub credentials: Vec<Fido2Credential>,
    pub recovery_code_hashes: Vec<String>,
    pub salt: Vec<u8>, // Salt específico do usuário para os recovery codes
    /// Iterações PBKDF2 com que `recovery_code_hashes` foram gerados.
    /// Dados antigos (sem o campo) usavam 100k.
    #[serde(default = "legacy_recovery_iterations")]
    pub recovery_code_iterations: u32,
}

impl Default for UserCredentials {
    fn default() -> Self {
        Self {
            credentials: Vec::new(),
            recovery_code_hashes: Vec::new(),
            salt: Vec::new(),
            recovery_code_iterations: RECOVERY_CODE_ITERATIONS,
        }
    }
}

/// Material necessário para verificar um recovery code fora do lock do
/// manager (o PBKDF2 é caro e não deve bloquear o runtime nem o mutex).
#[derive(Debug, Clone)]
pub struct RecoveryMaterial {
    pub salt: Vec<u8>,
    pub iterations: u32,
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
struct PendingAuthentication {
    auth_state: PasskeyAuthentication,
    user_id: String,
}

/// Tempo de vida de um desafio pendente (registo ou autenticação).
const PENDING_TTL: std::time::Duration = std::time::Duration::from_secs(300);
/// Número máximo de desafios pendentes em simultâneo.
const MAX_PENDING: usize = 64;

#[derive(Clone)]
pub struct Fido2Manager {
    webauthn: Webauthn,
    users: HashMap<String, UserCredentials>,
    // Desafios pendentes indexados pelo próprio challenge (bytes), com TTL.
    registration_states: HashMap<Vec<u8>, (PendingRegistration, Instant)>,
    authentication_states: HashMap<Vec<u8>, (PendingAuthentication, Instant)>,
    rp_id: String,
    /// Caminho de onde o estado foi carregado/guardado com sucesso. Impede
    /// sobrescrever um ficheiro existente que este manager nunca leu.
    loaded_path: Option<PathBuf>,
}

/// Extrai o challenge (bytes) do clientDataJSON enviado pelo browser.
fn challenge_from_client_data(client_data_json: &[u8]) -> Option<Vec<u8>> {
    #[derive(Deserialize)]
    struct ClientData {
        challenge: String,
    }
    let cd: ClientData = serde_json::from_slice(client_data_json).ok()?;
    general_purpose::URL_SAFE_NO_PAD
        .decode(cd.challenge.trim_end_matches('='))
        .ok()
}

fn prune_pending<T>(map: &mut HashMap<Vec<u8>, (T, Instant)>) {
    map.retain(|_, (_, created)| created.elapsed() < PENDING_TTL);
    while map.len() >= MAX_PENDING {
        let oldest = map
            .iter()
            .min_by_key(|(_, (_, created))| *created)
            .map(|(k, _)| k.clone());
        match oldest {
            Some(k) => {
                map.remove(&k);
            }
            None => break,
        }
    }
}

/// Comparação em tempo constante.
fn ct_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

// Keyring service name for the Data Encryption Key (DEK)
const KEYRING_SERVICE_NAME: &str = "rsb-fido2-dek";
const KEYRING_USERNAME: &str = "default"; // A generic username for the DEK entry

/// Serviço do keyring onde vive o DEK. `RSB_FIDO2_KEYRING_SERVICE` permite aos
/// testes usarem uma entrada isolada em vez do DEK real do utilizador.
fn keyring_service_name() -> String {
    std::env::var("RSB_FIDO2_KEYRING_SERVICE")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| KEYRING_SERVICE_NAME.to_string())
}

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
            registration_states: HashMap::new(),
            authentication_states: HashMap::new(),
            rp_id: rp_id.to_string(),
            loaded_path: None,
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
        let entry = Entry::new(&keyring_service_name(), KEYRING_USERNAME)
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

    /// Gets the DEK from the Keyring WITHOUT creating it. Used when an encrypted
    /// credential file already exists: generating a new DEK there would make the
    /// file permanently unreadable (and a later save would overwrite it).
    fn get_existing_dek() -> Result<Vec<u8>, Fido2Error> {
        let entry = Entry::new(&keyring_service_name(), KEYRING_USERNAME)
            .map_err(|e| Fido2Error::KeyringError(format!("Keyring error: {}", e)))?;

        match entry.get_password() {
            Ok(password_base64) => general_purpose::STANDARD
                .decode(password_base64)
                .map_err(|e| Fido2Error::EncryptionError(format!("Failed to decode DEK: {}", e))),
            Err(keyring::Error::NoEntry) => Err(Fido2Error::KeyringError(
                "Credential file exists but its encryption key (DEK) is missing from the keyring; \
                 refusing to continue to avoid data loss"
                    .to_string(),
            )),
            Err(e) => Err(Fido2Error::KeyringError(format!(
                "Error accessing keyring: {}",
                e
            ))),
        }
    }

    /// True if this manager has loaded (or written) the given path.
    pub fn is_loaded_from(&self, path: &Path) -> bool {
        self.loaded_path.as_deref() == Some(path)
    }

    /// Save credentials of the form encrypted using AES-256-GCM and PBKDF2.
    /// Structure of the file: [16B Salt][12B Nonce][Encrypted Payload]
    pub fn save_to_file(&mut self, path: &Path) -> Result<(), Fido2Error> {
        // Proteção contra perda de dados: nunca sobrescrever um ficheiro de
        // credenciais existente (não vazio) que este manager não carregou com
        // sucesso (ex.: load falhou e o erro foi ignorado pelo chamador).
        let file_has_data = fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false);
        if file_has_data && !self.is_loaded_from(path) {
            return Err(Fido2Error::EncryptionError(format!(
                "Refusing to overwrite {:?}: it was not loaded successfully by this process",
                path
            )));
        }

        let plaintext = serde_json::to_vec(&self.users)
            .map_err(|e| Fido2Error::EncryptionError(format!("Serialize error: {}", e)))?;
        let mut rng = rand::rng();

        // 1. Generate random Salt (16 bytes)
        let mut salt = [0u8; 16];
        rng.fill_bytes(&mut salt);

        // 2. Derive 32-byte key (256 bits) using PBKDF2-HMAC-SHA256
        // Se o ficheiro já existe, a DEK tem de existir (nunca criar uma nova).
        let dek = if file_has_data {
            Self::get_existing_dek()?
        } else {
            Self::get_or_create_dek()?
        };
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

        // Escrita atómica (temp + rename) com modo 0600.
        crate::auth::secure_fs::write_private_atomic(path, &final_payload)
            .map_err(|e| Fido2Error::EncryptionError(format!("Failed to write file: {}", e)))?;
        self.loaded_path = Some(path.to_path_buf());
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

            // Derive the same key (get-only: nunca gerar uma DEK nova aqui)
            let dek = Self::get_existing_dek()?;

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
        self.loaded_path = Some(path.to_path_buf());
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

        prune_pending(&mut self.registration_states);
        self.registration_states.insert(
            challenge.public_key.challenge.to_vec(),
            (
                PendingRegistration {
                    reg_state,
                    user_id: user_id.to_string(),
                    user_name: user_name.to_string(),
                    display_name: display_name.to_string(),
                },
                Instant::now(),
            ),
        );

        Ok(challenge)
    }

    pub fn finish_registration(
        &mut self,
        reg_response: RegisterPublicKeyCredential,
    ) -> Result<Fido2Credential, Fido2Error> {
        prune_pending(&mut self.registration_states);
        let key = challenge_from_client_data(&reg_response.response.client_data_json)
            .ok_or(Fido2Error::NoRegistrationInProgress)?;
        let (pending, _) = self
            .registration_states
            .remove(&key)
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

        let user_entry = self.users.entry(pending.user_id.clone()).or_default();
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

        prune_pending(&mut self.authentication_states);
        self.authentication_states.insert(
            challenge.public_key.challenge.to_vec(),
            (
                PendingAuthentication {
                    auth_state,
                    user_id: user_id.to_string(),
                },
                Instant::now(),
            ),
        );

        Ok(challenge)
    }

    pub fn finish_authentication(
        &mut self,
        auth_response: PublicKeyCredential,
    ) -> Result<String, Fido2Error> {
        debug!("Finishing authentication");

        prune_pending(&mut self.authentication_states);
        let key = challenge_from_client_data(&auth_response.response.client_data_json)
            .ok_or(Fido2Error::NoAuthenticationInProgress)?;
        let (pending, _) = self
            .authentication_states
            .remove(&key)
            .ok_or(Fido2Error::NoAuthenticationInProgress)?;
        let PendingAuthentication {
            auth_state,
            user_id,
        } = pending;

        let result = self
            .webauthn
            .finish_passkey_authentication(&auth_response, &auth_state)
            .map_err(|e| {
                error!("Authentication failed: {:?}", e);
                Fido2Error::AuthenticationFailed(format!("{:?}", e))
            })?;

        let cred_id = result.cred_id().clone();

        // Find and update the specific credential
        let credential = self
            .get_credential_mut(&user_id, &cred_id)
            .ok_or(Fido2Error::CredentialNotFound)?;

        // Atualiza o Passkey armazenado via API do webauthn-rs (contador, backup state)
        let mut passkey: Passkey = serde_json::from_slice(&credential.credential_data)
            .map_err(|e| Fido2Error::AuthenticationFailed(format!("Corrupted data: {}", e)))?;
        if passkey.update_credential(&result).is_none() {
            return Err(Fido2Error::AuthenticationFailed(
                "Credential ID mismatch while updating passkey".to_string(),
            ));
        }
        credential.credential_data = serde_json::to_vec(&passkey)
            .map_err(|e| Fido2Error::AuthenticationFailed(format!("Serialization error: {}", e)))?;
        credential.counter = result.counter();
        credential.last_used = Some(chrono::Local::now().to_rfc3339());

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

    /// True se o utilizador existe (tem entrada registada).
    pub fn user_exists(&self, user_id: &str) -> bool {
        self.users.contains_key(user_id)
    }

    /// Gera novos códigos de recuperação para um usuário existente.
    /// Retorna os códigos em texto claro (deve ser exibido apenas uma vez).
    pub fn generate_backup_codes(&mut self, user_id: &str) -> Result<Vec<String>, Fido2Error> {
        // Não criar utilizadores implicitamente.
        if !self.user_exists(user_id) {
            return Err(Fido2Error::CredentialNotFound);
        }

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

            hashed_codes.push(Self::compute_recovery_hash(
                &code,
                &salt,
                RECOVERY_CODE_ITERATIONS,
            ));
            plain_codes.push(code);
        }

        let user_entry = self
            .users
            .get_mut(user_id)
            .ok_or(Fido2Error::CredentialNotFound)?;
        user_entry.recovery_code_hashes = hashed_codes;
        user_entry.salt = salt.to_vec();
        user_entry.recovery_code_iterations = RECOVERY_CODE_ITERATIONS;

        Ok(plain_codes)
    }

    /// Hash PBKDF2 de um recovery code (base64). Operação cara: em contexto
    /// async, executar via `spawn_blocking`.
    pub fn compute_recovery_hash(code: &str, salt: &[u8], iterations: u32) -> String {
        let mut hash = [0u8; 32];
        pbkdf2::pbkdf2_hmac::<Sha256>(code.as_bytes(), salt, iterations, &mut hash);
        general_purpose::STANDARD.encode(hash)
    }

    /// Salt e iterações para verificar um recovery code deste utilizador,
    /// ou `None` se não existir / não tiver códigos.
    pub fn recovery_material(&self, user_id: &str) -> Option<RecoveryMaterial> {
        let user = self.users.get(user_id)?;
        if user.recovery_code_hashes.is_empty() {
            return None;
        }
        let iterations = if user.recovery_code_iterations == 0 {
            LEGACY_RECOVERY_CODE_ITERATIONS
        } else {
            user.recovery_code_iterations
        };
        Some(RecoveryMaterial {
            salt: user.salt.clone(),
            iterations,
        })
    }

    /// Consome (remove) o hash indicado se existir. Comparação em tempo constante.
    pub fn consume_recovery_hash(&mut self, user_id: &str, input_hash_encoded: &str) -> bool {
        let user_data = match self.users.get_mut(user_id) {
            Some(u) => u,
            None => return false,
        };
        let mut found = None;
        for (i, h) in user_data.recovery_code_hashes.iter().enumerate() {
            if ct_eq(h.as_bytes(), input_hash_encoded.as_bytes()) && found.is_none() {
                found = Some(i);
            }
        }
        if let Some(pos) = found {
            user_data.recovery_code_hashes.remove(pos);
            info!("Recovery code used and invalidated for user: {}", user_id);
            return true;
        }
        false
    }

    /// Valida e consome um código de recuperação (síncrono).
    /// O chamador DEVE persistir com `save_to_file` e tratar a falha.
    pub fn verify_backup_code(&mut self, user_id: &str, code: &str) -> bool {
        let material = match self.recovery_material(user_id) {
            Some(m) => m,
            None => return false,
        };
        let input_hash = Self::compute_recovery_hash(code, &material.salt, material.iterations);
        self.consume_recovery_hash(user_id, &input_hash)
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

    /// Deletes every key of `user_id` from the credentials file at `path`
    /// (`rsb auth delete`): reloads the file first, so nothing newer is
    /// lost and a file that cannot be read is never overwritten with an
    /// empty state; refuses unknown users; then saves. The caller must
    /// have authenticated as `user_id`.
    pub fn revoke_user_persisted(&mut self, path: &Path, user_id: &str) -> Result<(), Fido2Error> {
        self.load_from_file(path)?;
        if !self.has_credential(user_id) {
            return Err(Fido2Error::CredentialNotFound);
        }
        self.revoke_user(user_id)?;
        self.save_to_file(path)
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
    #[test]
    fn test_revoke_user_persisted_refuses_unknown_user_without_writing() {
        // No credentials file: loading it touches neither the disk nor the
        // keyring, and an unknown user is refused before anything is saved.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fido2_credentials.json");
        let mut mgr = create_manager();
        assert!(matches!(
            mgr.revoke_user_persisted(&path, "ghost"),
            Err(Fido2Error::CredentialNotFound)
        ));
        assert!(!path.exists());

        // An unreadable file is never replaced.
        std::fs::write(&path, b"short").unwrap();
        assert!(mgr.revoke_user_persisted(&path, "ghost").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"short");
    }

    #[test]
    fn test_generate_codes_refuses_unknown_user() {
        let mut mgr = create_manager();
        assert!(!mgr.user_exists("ghost"));
        assert!(matches!(
            mgr.generate_backup_codes("ghost"),
            Err(Fido2Error::CredentialNotFound)
        ));
        // Não pode ter criado o utilizador implicitamente.
        assert!(!mgr.user_exists("ghost"));
    }

    #[test]
    fn test_legacy_recovery_hash_100k_still_verifies() {
        // Regressão: hashes antigos (100k, sem campo de iterações) deixaram de
        // validar quando o default passou para 600k.
        let salt = vec![7u8; 16];
        let legacy_hash = Fido2Manager::compute_recovery_hash(
            "OLDCODE12345",
            &salt,
            LEGACY_RECOVERY_CODE_ITERATIONS,
        );
        let json = serde_json::json!({
            "credentials": [],
            "recovery_code_hashes": [legacy_hash],
            "salt": salt,
        });
        let user: UserCredentials = serde_json::from_value(json).unwrap();
        assert_eq!(
            user.recovery_code_iterations,
            LEGACY_RECOVERY_CODE_ITERATIONS
        );

        let mut mgr = create_manager();
        mgr.users.insert("alice".to_string(), user);

        assert!(!mgr.verify_backup_code("alice", "WRONGCODE000"));
        assert!(mgr.verify_backup_code("alice", "OLDCODE12345"));
        // Uso único.
        assert!(!mgr.verify_backup_code("alice", "OLDCODE12345"));
    }

    #[test]
    fn test_new_recovery_codes_use_600k_and_are_single_use() {
        let mut mgr = create_manager();
        mgr.users
            .insert("bob".to_string(), UserCredentials::default());
        let codes = mgr.generate_backup_codes("bob").unwrap();
        assert_eq!(codes.len(), 10);
        assert_eq!(
            mgr.users["bob"].recovery_code_iterations,
            RECOVERY_CODE_ITERATIONS
        );
        // Serialização preserva a versão.
        let roundtrip: UserCredentials =
            serde_json::from_slice(&serde_json::to_vec(&mgr.users["bob"]).unwrap()).unwrap();
        assert_eq!(roundtrip.recovery_code_iterations, RECOVERY_CODE_ITERATIONS);

        assert!(mgr.verify_backup_code("bob", &codes[3]));
        assert!(!mgr.verify_backup_code("bob", &codes[3]));
        assert_eq!(mgr.users["bob"].recovery_code_hashes.len(), 9);
    }

    #[test]
    fn test_save_refuses_to_overwrite_unloaded_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fido2_credentials.json");
        std::fs::write(&path, b"existing-encrypted-data-not-loaded").unwrap();
        let mut mgr = create_manager();
        assert!(mgr.save_to_file(&path).is_err());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"existing-encrypted-data-not-loaded"
        );
    }

    #[test]
    fn test_challenge_from_client_data() {
        let challenge = vec![1u8, 2, 3, 250, 251];
        let encoded = general_purpose::URL_SAFE_NO_PAD.encode(&challenge);
        let cd = format!(
            r#"{{"type":"webauthn.get","challenge":"{}","origin":"x"}}"#,
            encoded
        );
        assert_eq!(challenge_from_client_data(cd.as_bytes()), Some(challenge));
        assert_eq!(challenge_from_client_data(b"not json"), None);
    }
}
