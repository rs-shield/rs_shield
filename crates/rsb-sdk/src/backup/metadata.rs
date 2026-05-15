use std::collections::HashMap;
use std::path::PathBuf;
use crate::core::types::FileMetadata;
use crate::storage::Storage;
use tracing::debug;

/// Loads metadata from previous snapshots and indexes by file hash for deduplication
/// The hash is the content hash of the original file, used for fast deduplication
pub async fn load_previous_metadata(
    storage: &dyn Storage,
    key: Option<&str>,
) -> Result<HashMap<String, FileMetadata>, Box<dyn std::error::Error>> {
    let mut metadata_by_hash = HashMap::new();
    
    if let Ok((_, content)) = crate::core::manifest::find_latest_snapshot(storage, None, key).await {
        if let Ok(prev) = toml::from_str::<HashMap<PathBuf, FileMetadata>>(&content) {
            metadata_by_hash.reserve(prev.len());
            // Index by hash for fast O(1) deduplication lookups during backup
            for (_, meta) in prev {
                metadata_by_hash.insert(meta.hash.clone(), meta);
            }
            debug!("✅ Loaded {} file hashes from previous snapshot for deduplication", metadata_by_hash.len());
        }
    }
    
    Ok(metadata_by_hash)
}

/// Structure to store encryption key in cache
/// Optimization: derives the key ONCE instead of per-file
pub struct CachedEncryptionKey {
    key: Option<std::sync::Arc<crate::crypto::EncryptionKey>>,
}

impl CachedEncryptionKey {
    /// Creates a cached encryption key from a password
    pub fn new(password: Option<&str>) -> Result<Self, Box<dyn std::error::Error>> {
        let key = password.and_then(|pwd| {
            match crate::crypto::EncryptionKey::new(pwd.as_bytes()) {
                Ok(k) => {
                    debug!(
                        "✅ Pre-derived encryption key (saved ~{:?} PBKDF2 iterations)",
                        600_000
                    );
                    Some(std::sync::Arc::new(k))
                }
                Err(e) => {
                    tracing::error!("Failed to pre-derive encryption key: {}", e);
                    None
                }
            }
        });

        Ok(Self { key })
    }

    pub fn as_ref(&self) -> Option<std::sync::Arc<crate::crypto::EncryptionKey>> {
        self.key.clone()
    }

    pub fn is_some(&self) -> bool {
        self.key.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cached_encryption_key_none() {
        let key = CachedEncryptionKey::new(None).unwrap();
        assert!(!key.is_some());
    }
}
