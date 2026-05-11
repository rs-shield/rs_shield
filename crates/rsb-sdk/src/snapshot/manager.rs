use super::manifest::read_manifest;
use super::types::{
    Manifest,
    SnapshotDetails,
    SnapshotFileEntry,
    SnapshotInfo,
    SnapshotList,
};

use crate::storage::Storage;

use chrono::{DateTime, Utc};
use std::path::PathBuf;

pub struct SnapshotManager<'a> {
    storage: &'a dyn Storage,
}

impl<'a> SnapshotManager<'a> {
    pub fn new(storage: &'a dyn Storage) -> Self {
        Self { storage }
    }

    /// ===============================
    /// List snapshots
    /// ===============================

    pub async fn list_snapshots(
        &self,
    ) -> Result<SnapshotList, Box<dyn std::error::Error>> {
        let mut snapshots = self.storage.list("snapshots").await?;

        snapshots.retain(|s| s.ends_with(".toml"));

        snapshots.sort();
        snapshots.reverse();

        let mut result = Vec::new();

        for path in snapshots {
            let id = path
                .split('/')
                .last()
                .unwrap_or_default()
                .replace(".toml", "");

            let created_at = DateTime::parse_from_rfc3339(
                &format!(
                    "{}:{}:{}+00:00",
                    &id[0..13],
                    &id[13..15],
                    &id[15..17]
                )
            )
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(|_| Utc::now());

            result.push(SnapshotInfo {
                id,
                path,
                created_at,
                total_files: 0,
                total_chunks: 0,
                total_size: 0,
            });
        }

        Ok(SnapshotList {
            snapshots: result,
        })
    }

    /// ===============================
    /// Snapshot details
    /// ===============================

    pub async fn get_snapshot_details(
        &self,
        snapshot_path: &str,
        key: Option<&str>,
    ) -> Result<SnapshotDetails, Box<dyn std::error::Error>> {
        let manifest_str =
            read_manifest(self.storage, snapshot_path, key).await?;

        let manifest: Manifest = toml::from_str(&manifest_str)?;

        let mut total_size = 0;
        let mut total_chunks = 0;

        let mut files = Vec::new();

        for (path, meta) in &manifest {
            total_size += meta.size;
            total_chunks += meta.chunks.len();

            files.push(SnapshotFileEntry {
                path: PathBuf::from(path),
                size: meta.size,
                chunks: meta.chunks.len(),
            });
        }

        let id = snapshot_path
            .split('/')
            .last()
            .unwrap_or_default()
            .replace(".toml", "");

        Ok(SnapshotDetails {
            snapshot: SnapshotInfo {
                id,
                path: snapshot_path.to_string(),
                created_at: Utc::now(),
                total_files: files.len(),
                total_chunks,
                total_size,
            },
            files,
        })
    }

    /// ===============================
    /// Delete snapshot
    /// ===============================

    pub async fn delete_snapshot(
        &self,
        snapshot_path: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.storage.delete(snapshot_path).await?;

        Ok(())
    }
}