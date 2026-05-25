use anyhow::{Context, Result};
use clap::ValueEnum;
use std::fs;
use std::path::PathBuf;

#[derive(ValueEnum, Clone, Copy)]
pub enum OutputFormat {
    /// Plain text table
    Table,
    /// JSON format
    Json,
    /// CSV format
    Csv,
}

pub struct ListProfilesCmd;

impl ListProfilesCmd {
    pub async fn execute(directory: Option<PathBuf>, format: OutputFormat) -> Result<()> {
        let profiles_dir = Self::get_profiles_directory(directory)?;

        let mut profiles = Vec::new();

        if profiles_dir.exists() {
            for entry in fs::read_dir(&profiles_dir).context("Failed to read profiles directory")? {
                let entry = entry?;
                let path = entry.path();

                if path.is_file() && path.extension().is_some_and(|e| e == "toml") {
                    if let Ok(filename) = entry.file_name().into_string() {
                        let name = filename.trim_end_matches(".toml").to_string();
                        profiles.push((name, path));
                    }
                }
            }
        }

        profiles.sort_by(|a, b| a.0.cmp(&b.0));

        match format {
            OutputFormat::Table => Self::display_table(&profiles)?,
            OutputFormat::Json => Self::display_json(&profiles)?,
            OutputFormat::Csv => Self::display_csv(&profiles)?,
        }

        Ok(())
    }

    fn get_profiles_directory(custom_dir: Option<PathBuf>) -> Result<PathBuf> {
        if let Some(dir) = custom_dir {
            return Ok(dir);
        }

        let home = dirs::home_dir().context("Could not determine home directory")?;
        Ok(home.join(".config/rs-shield"))
    }

    fn display_table(profiles: &[(String, PathBuf)]) -> Result<()> {
        println!("\n📋 RS Shield Backup Profiles\n");

        if profiles.is_empty() {
            println!("   ℹ️  No profiles found");
            println!("\n   Create a profile with:");
            println!(
                "   rsb create-profile --name mybackup --source /path/to/source --dest /path/to/dest\n"
            );
            return Ok(());
        }

        println!("{:<30} {:<50}", "Profile Name", "Path");
        println!("{}", "-".repeat(80));

        for (name, path) in profiles {
            let path_str = path.to_string_lossy();
            let truncated = if path_str.len() > 48 {
                format!("{}…", &path_str[..45])
            } else {
                path_str.to_string()
            };
            println!("{:<30} {}", name, truncated);
        }

        println!();
        Ok(())
    }

    fn display_json(profiles: &[(String, PathBuf)]) -> Result<()> {
        let json_profiles: Vec<serde_json::Value> = profiles
            .iter()
            .map(|(name, path)| {
                serde_json::json!({
                    "name": name,
                    "path": path.to_string_lossy()
                })
            })
            .collect();

        let output = serde_json::json!({
            "profiles": json_profiles,
            "count": profiles.len()
        });

        println!("{}", serde_json::to_string_pretty(&output)?);
        Ok(())
    }

    fn display_csv(profiles: &[(String, PathBuf)]) -> Result<()> {
        println!("Name,Path");
        for (name, path) in profiles {
            println!("{},{}", name, path.to_string_lossy());
        }
        Ok(())
    }
}
