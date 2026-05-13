// src/crypto/mod.rs

use ring::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::io::{self, ErrorKind};
use std::num::NonZeroU32;
use tracing::{debug, error, warn};

/// Cached encryption key - derived only once per backup session
#[derive(Clone)]
pub struct EncryptionKey {
    key: LessSafeKey,
    // Salt is stored if needed for future decryption compatibility
    #[allow(dead_code)]
    salt: [u8; 16],
}

impl EncryptionKey {
    /// Derive the encryption key **only once** from the password
    pub fn new(password: &[u8]) -> io::Result<Self> {
        let rand = SystemRandom::new();

        let mut salt = [0u8; 16];
        rand.fill(&mut salt)
            .map_err(|_| io::Error::other("Failed to generate salt"))?;

        let mut derived_key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            NonZeroU32::new(600_000).unwrap(),
            &salt,
            password,
            &mut derived_key,
        );

        let unbound_key = UnboundKey::new(&AES_256_GCM, &derived_key)
            .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "Failed to create encryption key"))?;

        debug!("🔑 Encryption key derived successfully (PBKDF2 executed only once)");

        Ok(EncryptionKey {
            key: LessSafeKey::new(unbound_key),
            salt,
        })
    }

    /// Encrypt data using the pre-derived key (very fast)
    pub fn encrypt(&self, data: &[u8]) -> io::Result<Vec<u8>> {
        let rand = SystemRandom::new();

        let mut nonce_bytes = [0u8; NONCE_LEN];
        rand.fill(&mut nonce_bytes)
            .map_err(|_| io::Error::other("Failed to generate nonce"))?;

        let nonce = Nonce::assume_unique_for_key(nonce_bytes);

        let mut in_out = data.to_vec();
        let tag = self.key
            .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
            .map_err(|e| io::Error::other(format!("Encryption failed: {:?}", e)))?;

        // Format: salt(16) + nonce(12) + ciphertext + tag(16)
        let mut result = Vec::with_capacity(16 + NONCE_LEN + in_out.len() + 16);
        result.extend_from_slice(&self.salt);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&in_out);
        result.extend_from_slice(tag.as_ref());

        Ok(result)
    }
}

/// Legacy function - kept for backward compatibility (slow)
pub fn encrypt_data(data: &[u8], password: &[u8]) -> io::Result<Vec<u8>> {
    warn!("Using legacy encrypt_data (per-file key derivation). Use EncryptionKey for better performance.");
    let key = EncryptionKey::new(password)?;
    key.encrypt(data)
}

/// Optimized: Encrypt data with pre-derived key (reuse for multiple files)
pub fn encrypt_data_with_key(data: &[u8], key: &EncryptionKey) -> io::Result<Vec<u8>> {
    key.encrypt(data)
}

/// Decrypt data (supports the new format)
pub fn decrypt_data(encrypted: &[u8], password: &[u8]) -> io::Result<Vec<u8>> {
    const SALT_LEN: usize = 16;
    const TAG_LEN: usize = 16;
    let min_len = SALT_LEN + NONCE_LEN + TAG_LEN;

    if encrypted.len() < min_len {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!("Invalid encrypted data size: {} bytes (minimum: {})", encrypted.len(), min_len),
        ));
    }

    let salt = &encrypted[0..SALT_LEN];
    let nonce_bytes = &encrypted[SALT_LEN..SALT_LEN + NONCE_LEN];
    let data_with_tag = &encrypted[SALT_LEN + NONCE_LEN..];

    let mut derived_key = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600_000).unwrap(),
        salt,
        password,
        &mut derived_key,
    );

    let unbound_key = UnboundKey::new(&AES_256_GCM, &derived_key)
        .map_err(|_| io::Error::new(ErrorKind::InvalidInput, "Failed to create decryption key"))?;

    let key = LessSafeKey::new(unbound_key);

    let nonce_arr: [u8; NONCE_LEN] = nonce_bytes.try_into()
        .map_err(|_| io::Error::new(ErrorKind::InvalidData, "Invalid nonce size"))?;

    let nonce = Nonce::assume_unique_for_key(nonce_arr);

    let mut in_out = data_with_tag.to_vec();

    let plaintext = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| {
            error!("GCM tag verification failed: {:?}", e);
            io::Error::new(
                ErrorKind::InvalidData,
                "Decryption failed: wrong password, corrupted data or invalid format",
            )
        })?;

    debug!("Decrypted {} bytes successfully", plaintext.len());
    Ok(plaintext.to_vec())
}

/// Fast Blake3 hash (used for deduplication)
pub fn hash_file_content(content: &[u8]) -> io::Result<String> {
    Ok(blake3::hash(content).to_hex().to_string())
}

/// Hash entire file (less used)
pub fn hash_file(path: &std::path::Path) -> io::Result<String> {
    let content = std::fs::read(path)?;
    hash_file_content(&content)
}