// src/crypto/mod.rs

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, AES_256_GCM, NONCE_LEN};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use std::io::{self, ErrorKind};
use std::num::NonZeroU32;
use tracing::{debug, error};

/// Calculates Blake3 hash of a file for integrity verification
pub fn hash_file(path: &std::path::Path) -> io::Result<String> {
    let content = std::fs::read(path)?;
    Ok(blake3::hash(&content).to_hex().to_string())
}

/// Calculates Blake3 hash of content in memory
pub fn hash_file_content(content: &[u8]) -> io::Result<String> {
    Ok(blake3::hash(content).to_hex().to_string())
}

/// Encrypts data with AES-256-GCM + PBKDF2 (prior compression already done)
/// Output format: salt (16 bytes) + nonce (12 bytes) + ciphertext + tag (16 bytes)
pub fn encrypt_data(data: &[u8], password: &[u8]) -> io::Result<Vec<u8>> {
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

    debug!(
        "Derived key generated successfully (size: {} bytes)",
        derived_key.len()
    );

    let unbound_key = UnboundKey::new(&AES_256_GCM, &derived_key).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "Failed to create unbound key (invalid length)",
        )
    })?;

    let key = LessSafeKey::new(unbound_key);

    let mut nonce_bytes = [0u8; NONCE_LEN];
    rand.fill(&mut nonce_bytes)
        .map_err(|_| io::Error::other("Failed to generate random nonce"))?;

    let nonce = Nonce::assume_unique_for_key(nonce_bytes);

    let mut in_out = data.to_vec();

    let tag = key
        .seal_in_place_separate_tag(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| io::Error::other(format!("Failed to encrypt/seal: {:?}", e)))?;

    debug!(
        "Encryption completed (final buffer size: {} bytes)",
        in_out.len()
    );

    let mut result =
        Vec::with_capacity(salt.len() + nonce_bytes.len() + in_out.len() + tag.as_ref().len());
    result.extend_from_slice(&salt);
    result.extend_from_slice(&nonce_bytes);
    result.extend_from_slice(&in_out);
    result.extend_from_slice(tag.as_ref());

    Ok(result)
}

/// Decrypts data in format: salt (16) + nonce (12) + ciphertext + tag (16)
pub fn decrypt_data(encrypted: &[u8], password: &[u8]) -> io::Result<Vec<u8>> {
    const SALT_LEN: usize = 16;
    const TAG_LEN: usize = 16;
    let min_len = SALT_LEN + NONCE_LEN + TAG_LEN;

    if encrypted.len() < min_len {
        return Err(io::Error::new(
            ErrorKind::InvalidData,
            format!(
                "Invalid encrypted data: size {} bytes (minimum required: {})",
                encrypted.len(),
                min_len
            ),
        ));
    }

    let salt = &encrypted[0..SALT_LEN];
    let nonce_bytes = &encrypted[SALT_LEN..SALT_LEN + NONCE_LEN];
    let data_with_tag = &encrypted[SALT_LEN + NONCE_LEN..];

    debug!(
        "Decrypting data (total size: {} bytes, ciphertext+tag: {} bytes)",
        encrypted.len(),
        data_with_tag.len()
    );

    let mut derived_key = [0u8; 32];
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        NonZeroU32::new(600_000).unwrap(),
        salt,
        password,
        &mut derived_key,
    );

    let unbound_key = UnboundKey::new(&AES_256_GCM, &derived_key).map_err(|_| {
        io::Error::new(
            ErrorKind::InvalidInput,
            "Failed to create unbound key during decryption",
        )
    })?;

    let key = LessSafeKey::new(unbound_key);

    let nonce_arr: [u8; NONCE_LEN] = nonce_bytes.try_into().map_err(|_| {
        io::Error::new(ErrorKind::InvalidData, "Invalid nonce (incorrect size)")
    })?;

    let nonce = Nonce::assume_unique_for_key(nonce_arr);

    let mut in_out = data_with_tag.to_vec();

    let plaintext_slice = key
        .open_in_place(nonce, Aad::empty(), &mut in_out)
        .map_err(|e| {
            error!("Failed to verify GCM tag: {:?}", e);
            io::Error::new(
                ErrorKind::InvalidData,
                format!("Failed to decrypt: tag verification failed (wrong password, corrupted data or invalid format): {:?}", e),
            )
        })?;

    debug!(
        "Decryption completed (plaintext size: {} bytes)",
        plaintext_slice.len()
    );

    Ok(plaintext_slice.to_vec())
}
