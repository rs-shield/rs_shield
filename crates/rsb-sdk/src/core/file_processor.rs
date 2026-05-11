use super::types::{
    ChunkMetadata, FileMetadata, FileStatus, ProgressCallback, CHUNK_SIZE, MULTIPART_THRESHOLD,
};
use crate::crypto::{encrypt_data, hash_file_content};
use crate::storage::Storage;
use crate::utils::mmap_file;
use glob::Pattern;
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{atomic::Ordering, Arc, Mutex};
use tokio::runtime::Handle;
use tracing::info;
use zstd::stream::write::Encoder as ZstdEncoder;

#[allow(clippy::too_many_arguments)]
pub fn process_file(
    src: &Path,
    storage: &Arc<dyn Storage>,
    rel_path: &Path,
    key: Option<&str>,
    snapshot_manifest: &Arc<Mutex<HashMap<PathBuf, FileMetadata>>>,
    dry_run: bool,
    previous_metadata_cache: &HashMap<String, FileMetadata>,
    rt_handle: &Handle,
    encrypt_patterns: &Option<Vec<String>>,
    on_progress: &Option<ProgressCallback>,
    compression_level: u8,
) -> io::Result<FileStatus> {
    let mapped = mmap_file(src)?;
    let current_hash = hash_file_content(&mapped)?;

    // Call progress callback if provided
    if let Some(callback) = on_progress {
        callback(0, mapped.len(), "Processing file".to_string());
    }

    if let Some(cached) = previous_metadata_cache.get(&current_hash) {
        let mut m = snapshot_manifest.lock().unwrap();
        m.insert(rel_path.to_path_buf(), cached.clone());
        return Ok(FileStatus::Skipped);
    }

    if mapped.len() as u64 > MULTIPART_THRESHOLD {
        return process_file_multipart(
            storage,
            rel_path,
            &mapped,
            current_hash,
            key,
            snapshot_manifest,
            dry_run,
            rt_handle,
            encrypt_patterns,
            compression_level,
        );
    }

    let mut should_encrypt = key.is_some();
    if let Some(patterns) = encrypt_patterns {
        if !patterns.is_empty() {
            should_encrypt = patterns
                .iter()
                .any(|p| Pattern::new(p).is_ok_and(|pat| pat.matches_path(rel_path)));
        }
    }
    let final_should_encrypt = should_encrypt && key.is_some();

    let data_path = format!(
        "data/{}/{}",
        if final_should_encrypt { "enc" } else { "clear" },
        current_hash
    );

    let mut metadata = FileMetadata {
        hash: current_hash.clone(),
        encrypted: final_should_encrypt,
        stored_hash: None,
        stored_size: None,
        chunks: None,
        compressed: compression_level > 0,
    };

    if rt_handle.block_on(storage.exists(&data_path))? {
        if let Some(cached) = previous_metadata_cache.get(&current_hash) {
            metadata.stored_hash = cached.stored_hash.clone();
            metadata.stored_size = cached.stored_size;
        }
        let mut m = snapshot_manifest.lock().unwrap();
        m.insert(rel_path.to_path_buf(), metadata);
        return Ok(FileStatus::Skipped);
    }

    if dry_run {
        let mut m = snapshot_manifest.lock().unwrap();
        m.insert(rel_path.to_path_buf(), metadata);
        return Ok(FileStatus::Processed);
    }

    let data_to_encrypt = if compression_level > 0 {
        let mut compressed = Vec::new();
        let mut encoder = ZstdEncoder::new(&mut compressed, compression_level as i32)?;
        encoder.write_all(&mapped)?;
        encoder.finish()?;
        compressed
    } else {
        mapped.to_vec()
    };

    let final_data = if final_should_encrypt {
        encrypt_data(&data_to_encrypt, key.unwrap().as_bytes())?
    } else {
        data_to_encrypt
    };

    let stored_hash = hash_file_content(&final_data)?;
    metadata.stored_hash = Some(stored_hash);
    metadata.stored_size = Some(final_data.len() as u64);

    rt_handle.block_on(storage.write(&data_path, &final_data))?;

    {
        let mut m = snapshot_manifest.lock().unwrap();
        m.insert(rel_path.to_path_buf(), metadata);
    }

    Ok(FileStatus::Processed)
}

#[allow(clippy::too_many_arguments)]
pub fn process_file_multipart(
    storage: &Arc<dyn Storage>,
    rel_path: &Path,
    mapped: &[u8],
    current_hash: String,
    key: Option<&str>,
    snapshot_manifest: &Arc<Mutex<HashMap<PathBuf, FileMetadata>>>,
    dry_run: bool,
    rt_handle: &Handle,
    encrypt_patterns: &Option<Vec<String>>,
    compression_level: u8,
) -> io::Result<FileStatus> {
    // Criar progress bar para multipart processing
    let pb = ProgressBar::new(mapped.len() as u64);
    pb.set_style(
        ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes}",
            )
            .unwrap()
            .progress_chars("#>-"),
    );

    let mut should_encrypt = key.is_some();
    if let Some(patterns) = encrypt_patterns {
        if !patterns.is_empty() {
            should_encrypt = patterns
                .iter()
                .any(|p| Pattern::new(p).is_ok_and(|pat| pat.matches_path(rel_path)));
        }
    }
    let final_should_encrypt = should_encrypt && key.is_some();

    let mut chunks_meta = Vec::new();
    let mut total_stored_size = 0u64;
    let chunk_counter = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    for chunk_data in mapped.chunks(CHUNK_SIZE) {
        // Use Ordering to update counter
        let chunk_num = chunk_counter.fetch_add(1, Ordering::SeqCst);
        pb.inc(chunk_data.len() as u64);
        info!(
            "📦 Processing chunk {} - {} bytes",
            chunk_num,
            chunk_data.len()
        );
        let chunk_hash = hash_file_content(chunk_data)?;
        let data_path = format!(
            "data/{}/{}",
            if final_should_encrypt { "enc" } else { "clear" },
            chunk_hash
        );

        let exists = rt_handle.block_on(storage.exists(&data_path))?;
        let (stored_size, stored_hash) = if exists {
            let data = rt_handle.block_on(storage.read(&data_path))?;
            (data.len() as u64, Some(hash_file_content(&data)?))
        } else if dry_run {
            (0, None)
        } else {
            let mut compressed = Vec::new();
            {
                let mut encoder = ZstdEncoder::new(&mut compressed, compression_level as i32)?;
                encoder.write_all(chunk_data)?;
                encoder.finish()?;
            }
            let final_data = if final_should_encrypt {
                encrypt_data(&compressed, key.unwrap().as_bytes())?
            } else {
                compressed
            };
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

    let metadata = FileMetadata {
        hash: current_hash,
        encrypted: final_should_encrypt,
        stored_hash: None,
        stored_size: Some(total_stored_size),
        chunks: Some(chunks_meta),
        compressed: compression_level > 0,
    };

    let mut m = snapshot_manifest.lock().unwrap();
    m.insert(rel_path.to_path_buf(), metadata);

    pb.finish_and_clear();
    Ok(FileStatus::Processed)
}

pub fn get_file_priority(path: &Path) -> u8 {
    match path.extension().and_then(|s| s.to_str()) {
        Some(ext) => {
            let ext_lower = ext.to_lowercase();
            match ext_lower.as_str() {
                // Documentos e texto - prioridade máxima
                "txt" | "md" | "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "odt"
                | "ods" | "rtf" | "csv" | "json" | "toml" | "xml" | "yaml" | "yml" => 0,

                // Código fonte e scripts
                "rs" | "py" | "js" | "ts" | "c" | "cpp" | "h" | "java" | "go" | "html" | "css"
                | "sql" | "sh" | "bat" | "ps1" => 1,

                // Imagens
                "jpg" | "jpeg" | "png" | "gif" | "bmp" | "svg" | "webp" | "tiff" | "heic" => 2,

                // Áudio
                "mp3" | "wav" | "aac" | "flac" | "ogg" | "m4a" | "wma" => 3,

                // Vídeo
                "mp4" | "mkv" | "avi" | "mov" | "wmv" | "flv" | "webm" | "mpeg" | "mpg" => 4,

                // Arquivos compactados e binários
                "zip" | "tar" | "gz" | "7z" | "rar" | "iso" | "bin" | "exe" | "dll" | "so"
                | "dmg" | "apk" | "deb" | "rpm" => 5,

                _ => 10, // Outros ficheiros (baixa prioridade)
            }
        }
        None => 10,
    }
}
