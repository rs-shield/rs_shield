use rsb_sdk::crypto;
use std::io;

#[test]
fn test_hash_file_content() {
    let content = b"Hello, World!";
    let hash = crypto::hash_file_content(content).expect("Failed to hash content");
    
    // Blake3 produces a 64-character hex string
    assert_eq!(hash.len(), 64, "Hash should be 64 hex characters (Blake3)");
    
    // Same content should produce same hash
    let hash2 = crypto::hash_file_content(content).expect("Failed to hash content");
    assert_eq!(hash, hash2, "Same content should produce same hash");
}

#[test]
fn test_hash_different_content() {
    let content1 = b"Hello, World!";
    let content2 = b"Hello, World?";
    
    let hash1 = crypto::hash_file_content(content1).expect("Failed to hash content");
    let hash2 = crypto::hash_file_content(content2).expect("Failed to hash content");
    
    assert_ne!(hash1, hash2, "Different content should produce different hashes");
}

#[test]
fn test_encrypt_decrypt_roundtrip() {
    let data = b"This is a secret message";
    let password = b"my-secret-password";
    
    // Encrypt
    let encrypted = crypto::encrypt_data(data, password).expect("Failed to encrypt");
    
    // Encrypted data should be longer than original
    assert!(encrypted.len() > data.len(), "Encrypted data should be longer (includes salt, nonce, tag)");
    
    // Decrypt
    let decrypted = crypto::decrypt_data(&encrypted, password).expect("Failed to decrypt");
    
    // Should match original
    assert_eq!(decrypted, data, "Decrypted data should match original");
}

#[test]
fn test_encrypt_with_empty_data() {
    let data = b"";
    let password = b"password";
    
    let encrypted = crypto::encrypt_data(data, password).expect("Failed to encrypt empty data");
    let decrypted = crypto::decrypt_data(&encrypted, password).expect("Failed to decrypt");
    
    assert_eq!(decrypted.len(), 0, "Decrypted empty data should be empty");
}

#[test]
fn test_encrypt_with_large_data() {
    let data = vec![0u8; 1024 * 1024]; // 1 MB
    let password = b"password";
    
    let encrypted = crypto::encrypt_data(&data, password).expect("Failed to encrypt large data");
    let decrypted = crypto::decrypt_data(&encrypted, password).expect("Failed to decrypt");
    
    assert_eq!(decrypted, data, "Large data roundtrip should work");
}

#[test]
fn test_decrypt_with_wrong_password() {
    let data = b"Secret message";
    let password1 = b"password1";
    let password2 = b"password2";
    
    let encrypted = crypto::encrypt_data(data, password1).expect("Failed to encrypt");
    let result = crypto::decrypt_data(&encrypted, password2);
    
    // Should fail with wrong password
    assert!(result.is_err(), "Decryption with wrong password should fail");
}

#[test]
fn test_decrypt_invalid_data() {
    let invalid_data = b"this is not encrypted data";
    let password = b"password";
    
    let result = crypto::decrypt_data(invalid_data, password);
    
    // Should fail with invalid data
    assert!(result.is_err(), "Decryption with invalid data should fail");
}

#[test]
fn test_decrypt_truncated_data() {
    let data = b"Secret message";
    let password = b"password";
    
    let encrypted = crypto::encrypt_data(data, password).expect("Failed to encrypt");
    
    // Truncate to invalid size (less than salt + nonce + tag)
    let truncated = &encrypted[0..10];
    let result = crypto::decrypt_data(truncated, password);
    
    assert!(result.is_err(), "Decryption with truncated data should fail");
}

#[test]
fn test_encrypt_determinism() {
    let data = b"Message";
    let password = b"password";
    
    let encrypted1 = crypto::encrypt_data(data, password).expect("Failed to encrypt");
    let encrypted2 = crypto::encrypt_data(data, password).expect("Failed to encrypt");
    
    // Different encryptions (due to random salt/nonce) but both should decrypt to original
    assert_ne!(encrypted1, encrypted2, "Encryption should be non-deterministic (different salt/nonce)");
    
    let decrypted1 = crypto::decrypt_data(&encrypted1, password).expect("Failed to decrypt");
    let decrypted2 = crypto::decrypt_data(&encrypted2, password).expect("Failed to decrypt");
    
    assert_eq!(decrypted1, data, "First decryption should match original");
    assert_eq!(decrypted2, data, "Second decryption should match original");
}

#[test]
fn test_various_password_lengths() {
    let data = b"Test data";
    
    let passwords = vec![
        b"a".to_vec(),
        b"short".to_vec(),
        b"medium-length-password".to_vec(),
        vec![0u8; 256], // Very long password
    ];
    
    for password in passwords {
        let encrypted = crypto::encrypt_data(data, &password).expect("Failed to encrypt");
        let decrypted = crypto::decrypt_data(&encrypted, &password).expect("Failed to decrypt");
        assert_eq!(decrypted, data, "Roundtrip should work with various password lengths");
    }
}
