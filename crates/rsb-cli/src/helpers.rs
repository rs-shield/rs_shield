use anyhow::{Result, anyhow};
use std::fs;
use std::path::PathBuf;

/// Resolve config path from either --config, --backup, or auto-discover
/// Returns: (config_path, is_portable_mode)
pub fn resolve_config_or_backup(
    config: &Option<PathBuf>,
    backup: &Option<PathBuf>,
) -> Result<(PathBuf, bool)> {
    // If --backup provided, use it directly (portable mode)
    if let Some(backup_path) = backup {
        if backup_path.exists() {
            return Ok((backup_path.clone(), true));
        } else {
            return Err(anyhow!(
                "❌ Error: Backup path not found: {}",
                backup_path.display()
            ));
        }
    }

    // If --config provided, use it
    if let Some(config_path) = config {
        if config_path.exists() {
            return Ok((config_path.clone(), false));
        } else {
            return Err(anyhow!(
                "❌ Error: Configuration file not found: {}",
                config_path.display()
            ));
        }
    }

    // Try to auto-discover *.toml in current directory
    if let Ok(entries) = fs::read_dir(".") {
        let mut toml_files: Vec<PathBuf> = entries
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                if path.is_file() {
                    if let Some(name) = path.file_name() {
                        if let Some(name_str) = name.to_str() {
                            if name_str.ends_with(".toml") {
                                return Some(path);
                            }
                        }
                    }
                }
                None
            })
            .collect();

        if toml_files.len() == 1 {
            return Ok((toml_files.remove(0), false));
        } else if toml_files.len() > 1 {
            return Err(anyhow!(
                "❌ Error: Multiple .toml files found in current directory. Please specify --config"
            ));
        }
    }

    Err(anyhow!(
        "❌ Error: Either --config or --backup is required, or no config file found in current directory\n\
         \n\
         Examples:\n\
         • rsb backup my-backup.toml\n\
         • rsb backup --backup /path/to/backup\n\
         • rsb backup --config my-backup.toml"
    ))
}

/// Extract destination path from config.toml
pub fn extract_destination_from_config(config_path: &PathBuf) -> Result<PathBuf> {
    let config_content = fs::read_to_string(config_path)
        .map_err(|e| anyhow!("Failed to read config: {:?} - {}", config_path, e))?;

    let config_table: toml::Table = toml::from_str(&config_content)
        .map_err(|e| anyhow!("Failed to parse config.toml: {}", e))?;

    let dest_val = config_table
        .get("destination_path")
        .or_else(|| config_table.get("destination"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow!("'destination_path' not found in config.toml"))?;

    Ok(PathBuf::from(dest_val))
}
