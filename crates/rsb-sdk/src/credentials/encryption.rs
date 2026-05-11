/// Encryption module for credentials at rest
/// Uses AES-256-GCM for authenticated encryption
use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use pbkdf2::pbkdf2_hmac;
use rand::Rng;
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::fmt;

#[derive(Serialize, Deserialize, Clone)]
pub struct EncryptedCredentials {
    /// Ciphertext encoded in base64
    pub cipher: String,
    /// Nonce (IV) encoded in base64 - must be unique for each encryption
    pub nonce: String,
    /// Salt for key derivation
    pub salt: String,
    /// Encryption algorithm version for future compatibility
    pub version: u32,
}

impl fmt::Debug for EncryptedCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EncryptedCredentials")
            .field("cipher", &"***REDACTED***")
            .field("nonce", &"***REDACTED***")
            .field("salt", &"***REDACTED***")
            .field("version", &self.version)
            .finish()
    }
}

/// Derive key from master password using PBKDF2
fn derive_key_from_password(password: &str, salt: &[u8]) -> [u8; 32] {
    let mut key = [0u8; 32];
    pbkdf2_hmac::<Sha256>(password.as_bytes(), salt, 100_000, &mut key);
    key
}

/// Encrypt credentials JSON
pub fn encrypt_credentials(
    json: &str,
    master_password: &str,
) -> Result<EncryptedCredentials, String> {
    let mut rng = rand::rng();

    // Generate salt (16 bytes)
    let mut salt = [0u8; 16];
    rng.fill_bytes(&mut salt);

    // Derive key from master password
    let key_bytes = derive_key_from_password(master_password, &salt);
    let key = Key::<Aes256Gcm>::from(key_bytes);

    // Generate unique nonce (12 bytes for GCM)
    let mut nonce_bytes = [0u8; 12];
    rng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    // Encrypt
    let cipher = Aes256Gcm::new(&key);
    let ciphertext = cipher
        .encrypt(nonce, Payload::from(json.as_bytes()))
        .map_err(|e| format!("Encryption failed: {}", e))?;

    // Encode in base64 for storage
    let cipher_b64 = BASE64.encode(&ciphertext);
    let nonce_b64 = BASE64.encode(nonce_bytes);
    let salt_b64 = BASE64.encode(salt);

    Ok(EncryptedCredentials {
        cipher: cipher_b64,
        nonce: nonce_b64,
        salt: salt_b64,
        version: 1,
    })
}

/// Decrypt credentials JSON
pub fn decrypt_credentials(
    encrypted: &EncryptedCredentials,
    master_password: &str,
) -> Result<String, String> {
    // Decode from base64
    let ciphertext = BASE64
        .decode(&encrypted.cipher)
        .map_err(|e| format!("Failed to decode cipher: {}", e))?;
    let nonce_bytes = BASE64
        .decode(&encrypted.nonce)
        .map_err(|e| format!("Failed to decode nonce: {}", e))?;
    let salt = BASE64
        .decode(&encrypted.salt)
        .map_err(|e| format!("Failed to decode salt: {}", e))?;

    // Validate sizes
    if nonce_bytes.len() != 12 {
        return Err("Invalid nonce".to_string());
    }
    if salt.len() != 16 {
        return Err("Invalid salt".to_string());
    }

    // Derive key
    let key_bytes = derive_key_from_password(master_password, &salt);
    let key = Key::<Aes256Gcm>::from(key_bytes);

    // Decrypt
    let nonce = Nonce::from_slice(&nonce_bytes);
    let cipher = Aes256Gcm::new(&key);
    let plaintext = cipher
        .decrypt(nonce, Payload::from(ciphertext.as_ref()))
        .map_err(|e| format!("Decryption failed (incorrect password?): {}", e))?;

    // Convert to string
    String::from_utf8(plaintext).map_err(|e| format!("Invalid JSON: {}", e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let original = r#"{"access_key":"AKIA123","secret_key":"secret123"}"#;
        let password = "my-master-password-1234";

        let encrypted = encrypt_credentials(original, password).unwrap();
        let decrypted = decrypt_credentials(&encrypted, password).unwrap();

        assert_eq!(original, &decrypted);
    }

    #[test]
    fn test_wrong_password_fails() {
        let original = r#"{"access_key":"AKIA123","secret_key":"secret123"}"#;
        let password = "correct-password";
        let wrong_password = "wrong-password";

        let encrypted = encrypt_credentials(original, password).unwrap();
        let result = decrypt_credentials(&encrypted, wrong_password);

        assert!(result.is_err());
    }

    #[test]
    fn test_different_encryptions_produce_different_ciphers() {
        let original = r#"{"access_key":"AKIA123"}"#;
        let password = "password";

        let enc1 = encrypt_credentials(original, password).unwrap();
        let enc2 = encrypt_credentials(original, password).unwrap();

        // Same data but different nonces and salts produce different ciphers
        assert_ne!(enc1.cipher, enc2.cipher);
        assert_ne!(enc1.nonce, enc2.nonce);
    }
}
