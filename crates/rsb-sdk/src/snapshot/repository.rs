use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;

use crate::snapshot::model::Snapshot;

pub struct SnapshotRepository {
    path: PathBuf,
}

impl SnapshotRepository {
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            path: base_path.join("snapshots"),
        }
    }

    pub fn list(&self) -> Result<Vec<Snapshot>> {
        if !self.path.exists() {
            return Ok(vec![]);
        }

        let mut snapshots = Vec::new();

        for entry in fs::read_dir(&self.path)? {
            let entry = entry?;
            let path = entry.path();

            if !path.is_file() {
                continue;
            }

            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }

            let content = fs::read_to_string(&path)?;
            let snapshot: Snapshot = serde_json::from_str(&content)?;

            snapshots.push(snapshot);
        }

        snapshots.sort_by(|a, b| b.created_at.cmp(&a.created_at));

        Ok(snapshots)
    }

    /// Buscar snapshot por ID
    pub fn get(&self, id: &str) -> Result<Snapshot> {
        let path = self.path.join(format!("{}.json", id));

        if !path.exists() {
            return Err(anyhow!("Snapshot not found"));
        }

        let content = fs::read_to_string(path)?;
        let snapshot: Snapshot = serde_json::from_str(&content)?;

        Ok(snapshot)
    }

    /// Remover snapshot
    pub fn delete(&self, id: &str) -> Result<()> {
        let path = self.path.join(format!("{}.json", id));

        if !path.exists() {
            return Err(anyhow!("Snapshot not found"));
        }

        fs::remove_file(path)?;

        Ok(())
    }

    /// Salvar snapshot
    pub fn save(&self, snapshot: &Snapshot) -> Result<()> {
        fs::create_dir_all(&self.path)?;

        let path = self.path.join(format!("{}.json", snapshot.id));

        let json = serde_json::to_string_pretty(snapshot)?;

        fs::write(path, json)?;

        Ok(())
    }
}
