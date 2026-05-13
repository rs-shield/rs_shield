// file_processor.rs - Versão performática com EncryptionKey
use super::types::{
    CHUNK_SIZE, ChunkMetadata, FileMetadata, FileStatus, MULTIPART_THRESHOLD,
};
use crate::crypto::{EncryptionKey, hash_file_content};
use crate::storage::Storage;
use crate::utils::mmap_file;
use glob::Pattern;
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::Path;
use std::sync::Arc;
use tokio::runtime::Handle;
use zstd::stream::write::Encoder as ZstdEncoder;

#[allow(clippy::too_many_arguments)]
pub fn process_file(
    src: &Path,
    storage: &Arc<dyn Storage>,
    rel_path: &Path,
    encryption_key: Option<Arc<EncryptionKey>>,  // ⚡ Pre-derived EncryptionKey instead of password
    previous_metadata_cache: &HashMap<String, FileMetadata>,
    dry_run: bool,
    rt_handle: &Handle,
    encrypt_patterns: &Option<Vec<String>>,
    compression_level: u8,
) -> io::Result<(FileStatus, FileMetadata)> {
    let mapped = mmap_file(src)?;
    let current_hash = hash_file_content(&mapped)?;

    // Fast deduplication
    if let Some(cached) = previous_metadata_cache.get(&current_hash) {
        return Ok((FileStatus::Skipped, cached.clone()));
    }

    let should_encrypt = should_encrypt_file(rel_path, encryption_key.as_deref(), encrypt_patterns);

    // =============== FICHEIROS GRANDES (Multipart) ===============
    if mapped.len() as u64 > MULTIPART_THRESHOLD {
        let metadata = process_file_multipart_optimized(
            storage,
            rel_path,
            &mapped,
            current_hash.clone(),
            should_encrypt,
            encryption_key.clone(),
            dry_run,
            rt_handle,
            compression_level,
        )?;
        return Ok((FileStatus::Processed, metadata));
    }

    // =============== FICHEIROS NORMAIS ===============
    let data_path = format!(
        "data/{}/{}",
        if should_encrypt { "enc" } else { "clear" },
        current_hash
    );

    if rt_handle.block_on(storage.exists(&data_path))? {
        let metadata = build_file_metadata(current_hash, should_encrypt, compression_level, None, None);
        return Ok((FileStatus::Skipped, metadata));
    }

    if dry_run {
        let metadata = build_file_metadata(current_hash, should_encrypt, compression_level, None, None);
        return Ok((FileStatus::Processed, metadata));
    }

    // Compress + Encrypt (otimizado)
    let final_data = compress_and_encrypt(
        &mapped,
        compression_level,
        should_encrypt,
        encryption_key.as_deref(),  // ⚡ Dereference Arc to get Option<&EncryptionKey>
    )?;

    let stored_size = final_data.len() as u64;
    let stored_hash = hash_file_content(&final_data)?;

    rt_handle.block_on(storage.write(&data_path, &final_data))?;

    let metadata = build_file_metadata(
        current_hash,
        should_encrypt,
        compression_level,
        Some(stored_hash),
        Some(stored_size),
    );

    Ok((FileStatus::Processed, metadata))
}

// ====================== MULTIPART OTIMIZADO ======================
fn process_file_multipart_optimized(
    storage: &Arc<dyn Storage>,
    rel_path: &Path,
    mapped: &[u8],
    file_hash: String,
    should_encrypt: bool,
    encryption_key: Option<Arc<EncryptionKey>>,  // ⚡ Pre-derived key
    dry_run: bool,
    rt_handle: &Handle,
    compression_level: u8,
) -> io::Result<FileMetadata> {
    let mut chunks_meta = Vec::with_capacity((mapped.len() / CHUNK_SIZE) + 1);
    let mut total_stored_size = 0u64;

    for chunk_data in mapped.chunks(CHUNK_SIZE) {
        let chunk_hash = hash_file_content(chunk_data)?;
        let data_path = format!(
            "data/{}/{}",
            if should_encrypt { "enc" } else { "clear" },
            chunk_hash
        );

        let (stored_size, stored_hash) = if rt_handle.block_on(storage.exists(&data_path))? {
            let data = rt_handle.block_on(storage.read(&data_path))?;
            (data.len() as u64, Some(hash_file_content(&data)?))
        } else if dry_run {
            (0, None)
        } else {
            let final_data = compress_and_encrypt_chunk(
                chunk_data,
                compression_level,
                should_encrypt,
                encryption_key.as_deref(),
            )?;

            let len = final_data.len() as u64;
            let hash = hash_file_content(&final_data)?;

            rt_handle.block_on(storage.write(&data_path, &final_data))?;
            (len, Some(hash))
        };

        chunks_meta.push(ChunkMetadata {
            hash: chunk_hash,
            stored_size,
            stored_hash,
        });
        total_stored_size += stored_size;
    }

    Ok(FileMetadata::new_multipart(
        file_hash,
        should_encrypt,
        compression_level > 0,
        chunks_meta,
        total_stored_size,
    ))
}

// ====================== HELPERS ======================

fn should_encrypt_file(
    rel_path: &Path,
    key: Option<&EncryptionKey>,
    encrypt_patterns: &Option<Vec<String>>,
) -> bool {
    if key.is_none() {
        return false;
    }
    match encrypt_patterns {
        Some(patterns) if !patterns.is_empty() => {
            patterns.iter().any(|p| {
                Pattern::new(p).is_ok_and(|pat| pat.matches_path(rel_path))
            })
        }
        _ => true, // encripta tudo se tiver chave mas sem padrões específicos
    }
}

fn compress_and_encrypt(
    data: &[u8],
    compression_level: u8,
    should_encrypt: bool,
    encryption_key: Option<&EncryptionKey>,  // ⚡ Pre-derived key instead of password
) -> io::Result<Vec<u8>> {
    let compressed = if compression_level > 0 {
        let mut compressed = Vec::with_capacity(data.len() * 2 / 3);
        let mut encoder = ZstdEncoder::new(&mut compressed, compression_level as i32)?;
        encoder.write_all(data)?;
        encoder.finish()?;
        compressed
    } else {
        data.to_vec()
    };

    if should_encrypt {
        if let Some(key) = encryption_key {
            key.encrypt(&compressed)  // ✅ No PBKDF2 - key already derived!
        } else {
            Ok(compressed)
        }
    } else {
        Ok(compressed)
    }
}

fn compress_and_encrypt_chunk(
    data: &[u8],
    compression_level: u8,
    should_encrypt: bool,
    enc_key: Option<&EncryptionKey>,
) -> io::Result<Vec<u8>> {
    let compressed = if compression_level > 0 {
        let mut compressed = Vec::with_capacity(data.len() * 2 / 3);
        let mut encoder = ZstdEncoder::new(&mut compressed, compression_level as i32)?;
        encoder.write_all(data)?;
        encoder.finish()?;
        compressed
    } else {
        data.to_vec()
    };

    if let Some(key) = enc_key {
        key.encrypt(&compressed)
    } else {
        Ok(compressed)
    }
}

fn build_file_metadata(
    hash: String,
    encrypted: bool,
    compression_level: u8,
    stored_hash: Option<String>,
    stored_size: Option<u64>,
) -> FileMetadata {
    FileMetadata {
        hash,
        encrypted,
        stored_hash,
        stored_size,
        chunks: None,
        compressed: compression_level > 0,
    }
}

pub fn get_file_priority(path: &Path) -> u8 {
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => {
            let ext_lower = ext.to_lowercase();
            match ext_lower.as_str() {
                "txt" | "md" | "pdf" | "doc" | "docx" | "json" | "toml" => 0,
                "rs" | "py" | "js" | "ts" | "go" | "java" => 1,
                "jpg" | "jpeg" | "png" | "webp" => 2,
                "mp3" | "wav" | "flac" => 3,
                "mp4" | "mkv" => 4,
                _ => 10,
            }
        }
        None => 10,
    }
}