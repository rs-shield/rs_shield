use zstd::stream::copy_decode;

use crate::{Config, core::types::FileMetadata, crypto::decrypt_data};

pub fn verify_content_hash(
    data: &[u8],
    metadata: &FileMetadata,
    config: &Config,
) -> Result<(), String> {
    // 1. Decriptação
    let decrypted = if metadata.encrypted {
        if let Some(k) = config.encryption_key.as_deref() {
            decrypt_data(data, k.as_bytes()).map_err(|e| format!("Decryption failed: {}", e))?
        } else {
            data.to_vec()
        }
    } else {
        data.to_vec()
    };

    let final_data = if metadata.compressed {
        let mut decompressed = Vec::new();
        copy_decode(&decrypted[..], &mut decompressed)
            .map_err(|_| "Decompression failed".to_string())?;
        decompressed
    } else {
        decrypted
    };

    let computed_hash = crate::crypto::hash_file_content(&final_data)
        .map_err(|e| format!("Hash calculation failed: {}", e))?;

    if computed_hash != metadata.hash {
        return Err(format!(
            "Hash mismatch: expected {}, got {}",
            metadata.hash, computed_hash
        ));
    }

    Ok(())
}
