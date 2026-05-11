use crate::config::Config;
use crate::core::types::FileMetadata;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use tracing::info;

pub async fn perform_prune(
    config: &Config,
    keep_last: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    let storage = super::storage_backend::get_storage(config).await;
    let mut snapshots = storage.list("snapshots").await?;
    snapshots.retain(|s| s.ends_with(".toml"));

    if snapshots.len() <= keep_last {
        info!("Nothing to prune.");
        return Ok(());
    }

    snapshots.sort();
    let to_delete = snapshots.len() - keep_last;
    let delete_snapshots: Vec<String> = snapshots.drain(0..to_delete).collect();
    let keep_snapshots = snapshots;

    let mut keep_hashes = HashSet::new();

    for snap in keep_snapshots {
        let content =
            super::manifest::read_manifest(&*storage, &snap, config.encryption_key.as_deref())
                .await?;
        let manifest: HashMap<PathBuf, FileMetadata> = toml::from_str(&content)?;

        for metadata in manifest.values() {
            keep_hashes.insert(metadata.hash.clone());
            if let Some(chunks) = &metadata.chunks {
                for chunk in chunks {
                    keep_hashes.insert(chunk.hash.clone());
                }
            }
        }
    }

    let all_clear = storage.list("data/clear").await.unwrap_or_default();
    let all_enc = storage.list("data/enc").await.unwrap_or_default();
    let all_data = all_clear.into_iter().chain(all_enc);

    let mut deleted_count = 0;

    for data_file in all_data {
        let fname = PathBuf::from(&data_file)
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        if !keep_hashes.contains(&fname) {
            info!("Pruning data: {}", data_file);
            storage.delete(&data_file).await?;
            deleted_count += 1;
        }
    }

    for snap in delete_snapshots {
        info!("Pruning snapshot: {}", snap);
        storage.delete(&snap).await?;
        deleted_count += 1;
    }

    info!("Prune completed. Deleted {} items.", deleted_count);

    Ok(())
}
