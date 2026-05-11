use crate::{core::types::ChunkMetadata, storage::Storage};

pub async fn verify_chunk(
    storage: &dyn Storage,
    chunk: &ChunkMetadata,
    encrypted: bool,
) -> Result<String, String> {
    let path = format!(
        "data/{}/{}",
        if encrypted { "enc" } else { "clear" },
        chunk.hash
    );

    if !storage.exists(&path).await.map_err(|e| e.to_string())? {
        return Err(format!("Chunk missing: {}", chunk.hash));
    }

    Ok(path)
}
