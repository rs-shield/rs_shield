use anyhow::{Context, Result, anyhow};
use clap::Subcommand;
use rsb_sdk::core::types::FileMetadata;
use rsb_sdk::storage::{LocalStorage, Storage};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Subcommand)]
pub enum SnapshotCommand {
    /// List snapshots
    List {
        /// Path to config.toml
        #[arg(short, long)]
        config: PathBuf,
        /// Decryption key (if manifests are encrypted)
        #[arg(short, long)]
        key: Option<String>,
    },

    /// Show snapshot details
    Show {
        /// Path to config.toml
        #[arg(short, long)]
        config: PathBuf,
        /// Snapshot ID
        id: String,
        /// Decryption key (if manifest is encrypted)
        #[arg(short, long)]
        key: Option<String>,
    },

    /// Delete snapshot
    Delete {
        /// Path to config.toml
        #[arg(short, long)]
        config: PathBuf,
        /// Snapshot ID
        id: String,
    },
}

impl SnapshotCommand {
    pub async fn execute(&self) -> Result<()> {
        match self {
            SnapshotCommand::List { config, key } => {
                let dest_path = self.extract_destination_from_config(config)?;
                let storage = LocalStorage::new(dest_path.to_string_lossy().as_ref());
                self.list(&storage, key.as_deref()).await?
            }

            SnapshotCommand::Show { config, id, key } => {
                let dest_path = self.extract_destination_from_config(config)?;
                let storage = LocalStorage::new(dest_path.to_string_lossy().as_ref());
                let path = format!("snapshots/{}.toml", id);

                let manifest = self
                    .load_manifest(&storage, &path, key.as_deref())
                    .await?
                    .with_context(|| format!("Snapshot '{}' not found or inaccessible", id))?;

                let total_size: u64 = manifest.values().map(|m| m.stored_size.unwrap_or(0)).sum();
                let files_count = manifest.len();

                println!("📦 Snapshot");
                println!("ID:           {}", id);
                println!("Files:        {}", files_count);
                println!("Total Size:   {}", human_bytes(total_size));

                println!("\n📄 Files:");
                for (rel_path, meta) in manifest {
                    println!(
                        "  {:<10} {}",
                        human_bytes(meta.stored_size.unwrap_or(0)),
                        rel_path.display()
                    );
                }
            }

            SnapshotCommand::Delete { config, id } => {
                let dest_path = self.extract_destination_from_config(config)?;
                let snapshots_path = dest_path.join("snapshots");
                let path = snapshots_path.join(format!("{}.toml", id));

                if !path.exists() {
                    return Err(anyhow!(
                        "Snapshot '{}' not found at {:?}",
                        id,
                        snapshots_path
                    ));
                }

                fs::remove_file(path)?;
                println!("✅ Snapshot deleted: {}", id);
            }
        }

        Ok(())
    }

    /// Extract destination path from config.toml
    fn extract_destination_from_config(&self, config: &PathBuf) -> Result<PathBuf> {
        let config_content = fs::read_to_string(config)
            .with_context(|| format!("Failed to read config: {:?}", config))?;

        let config_table: toml::Table =
            toml::from_str(&config_content).context("Failed to parse config.toml")?;

        // Try different possible keys
        let dest_val = config_table
            .get("destination_path")
            .or_else(|| config_table.get("destination"))
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("'destination_path' not found in config.toml"))?;

        Ok(PathBuf::from(dest_val))
    }

    async fn list(&self, storage: &dyn Storage, key: Option<&str>) -> Result<()> {
        let mut snapshots = storage
            .list("snapshots")
            .await
            .map_err(|e| anyhow!("Failed to list snapshots: {}", e))?;

        snapshots.retain(|s| s.ends_with(".toml"));

        println!("{} Snapshots found", snapshots.len());
        if snapshots.is_empty() {
            println!("ℹ️  No snapshots found");
            return Ok(());
        }

        // Sort descending (most recent first)
        snapshots.sort_by(|a, b| b.cmp(a));

        println!("\n{:<25} {:<10}", "ID", "FILES");
        println!("{}", "-".repeat(80));

        for path_str in snapshots {
            let id = Path::new(&path_str)
                .file_stem()
                .map(|s| s.to_string_lossy())
                .unwrap_or_default();

            match self.load_manifest(storage, &path_str, key).await {
                Ok(Some(manifest)) => {
                    let files_count = manifest.len();
                    let total_size: u64 =
                        manifest.values().map(|m| m.stored_size.unwrap_or(0)).sum();
                    println!(
                        "{:<25} {:<10} {}",
                        truncate(&id, 24),
                        files_count,
                        human_bytes(total_size),
                    );
                }
                Ok(None) => {
                    println!("{:<25} [Empty Manifest]", truncate(&id, 24));
                }
                Err(_) => {
                    println!("{:<25} [Encrypted/Invalid]", truncate(&id, 24));
                }
            }
        }

        println!();

        Ok(())
    }

    /// Helper to read and parse a manifest file
    async fn load_manifest(
        &self,
        storage: &dyn Storage,
        path: &str,
        key: Option<&str>,
    ) -> Result<Option<HashMap<PathBuf, FileMetadata>>> {
        if !storage.exists(path).await.unwrap_or(false) {
            return Ok(None);
        }

        let content = rsb_sdk::core::manifest::read_manifest(storage, path, key)
            .await
            .map_err(|e| anyhow!("{}", e))?;

        let manifest: HashMap<PathBuf, FileMetadata> =
            toml::from_str(&content).context("Failed to parse manifest TOML")?;

        Ok(Some(manifest))
    }
}

fn truncate(value: &str, max: usize) -> String {
    if value.len() <= max {
        value.to_string()
    } else {
        format!("{}…", &value[..max])
    }
}

fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;

    let b = bytes as f64;

    if b >= TB {
        format!("{:.2} TB", b / TB)
    } else if b >= GB {
        format!("{:.2} GB", b / GB)
    } else if b >= MB {
        format!("{:.2} MB", b / MB)
    } else if b >= KB {
        format!("{:.2} KB", b / KB)
    } else {
        format!("{} B", bytes)
    }
}
