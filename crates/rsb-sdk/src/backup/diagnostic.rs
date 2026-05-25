use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::utils::ensure_directory_exists_async;


#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct BackupDiagnostics {
    backup_path: String,
    status: String,
    issues: Vec<String>,
    warnings: Vec<String>,
    suggestions: Vec<String>,
    details: DiagnosticDetails,
}

#[derive(serde::Serialize, serde::Deserialize, Debug)]
pub struct DiagnosticDetails {
    structure_valid: bool,
    snapshots_count: usize,
    data_files_count: usize,
    encrypted_files_count: usize,
    total_size_mb: f64,
}

pub async fn run_backup_diagnostics(
    backup_path: &Path,
    key: Option<&str>,
    verbose: bool,
    repair: bool,
) -> Result<BackupDiagnostics, Box<dyn std::error::Error>> {
    let mut issues = Vec::new();
    let mut warnings = Vec::new();
    let mut suggestions = Vec::new();
    let mut status = "✅ Healthy".to_string();

    // Check directory structure
    let snapshots_dir = backup_path.join("snapshots");
    let data_dir = backup_path.join("data");

    let structure_valid = snapshots_dir.exists() && data_dir.exists();

    if !structure_valid {
        issues.push("❌ Backup structure is incomplete".to_string());
        if !snapshots_dir.exists() {
            issues.push("   - Missing snapshots/ directory".to_string());
            if repair {
                println!("🔧 Attempting to create missing snapshots/ directory...");
                if let Err(e) = std::fs::create_dir_all(&snapshots_dir) {
                    warnings.push(format!("⚠️  Failed to create snapshots/: {}", e));
                } else {
                    println!("✅ Created snapshots/ directory");
                }
            } else {
                suggestions.push("Ensure the entire backup folder was copied".to_string());
            }
        }
        if !data_dir.exists() {
            issues.push("   - Missing data/ directory".to_string());
            if repair {
                println!("🔧 Attempting to create missing data/ directory...");
                if let Err(e) = std::fs::create_dir_all(&data_dir) {
                    warnings.push(format!("⚠️  Failed to create data/: {}", e));
                } else {
                    println!("✅ Created data/ directory");
                }
            } else {
                suggestions
                    .push("Re-copy the complete backup from the original computer".to_string());
            }
        }
        status = "❌ Failed".to_string();
    }

    // Count files
    let snapshots_count = if snapshots_dir.exists() {
        std::fs::read_dir(&snapshots_dir)
            .ok()
            .map(|e| e.count())
            .unwrap_or(0)
    } else {
        0
    };


    let mut data_files_count = 0;
    let mut encrypted_files_count = 0;
    let mut total_size = 0u64;

    for entry in WalkDir::new(&data_dir)
        .into_iter()
        .filter_map(Result::ok)
    {
        let path = entry.path();

        if let Ok(metadata) = entry.metadata() {
            if metadata.is_file() {
                total_size += metadata.len();

                if path.components().any(|c| c.as_os_str() == "enc") {
                    encrypted_files_count += 1;
                } else {
                    data_files_count += 1;
                }
            }
        }
    }

    if snapshots_count == 0 {
        issues.push("❌ No snapshots found".to_string());
        status = "❌ Failed".to_string();
        suggestions.push("Backup may be corrupted or incomplete".to_string());
    }

    if data_files_count == 0 && encrypted_files_count == 0 {
        warnings.push("⚠️  No data files found".to_string());
    }

    if encrypted_files_count > 0 && key.is_none() {
        warnings.push("⚠️  Backup appears to be encrypted but no key provided".to_string());
        suggestions.push("Use --key option to verify encrypted backup".to_string());
    }

    let total_size_mb = total_size as f64 / (1024.0 * 1024.0);

    if repair {
        println!("\n🔧 Repair Mode Summary:");
        if structure_valid {
            println!("✅ Backup structure is intact - no repairs needed");
        } else {
            println!("🔧 Attempted to repair missing directories");
        }
        if !suggestions.is_empty() {
            println!("\n💡 Additional Steps:");
            for suggestion in &suggestions {
                println!("   • {}", suggestion);
            }
        }
        println!();
    }

    if verbose && !repair {
        println!("📊 Detailed Diagnostics:");
        println!("   Snapshots: {}", snapshots_count);
        println!("   Data files (clear): {}", data_files_count);
        println!("   Data files (encrypted): {}", encrypted_files_count);
        println!("   Total size: {:.2} MB", total_size_mb);
    }

    Ok(BackupDiagnostics {
        backup_path: backup_path.to_string_lossy().to_string(),
        status,
        issues,
        warnings,
        suggestions,
        details: DiagnosticDetails {
            structure_valid,
            snapshots_count,
            data_files_count,
            encrypted_files_count,
            total_size_mb,
        },
    })
}

pub fn print_diagnostics(diagnostics: &BackupDiagnostics) {
    println!("📋 Backup Diagnostics Report");
    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
    println!("📁 Path: {}", diagnostics.backup_path);
    println!("Status: {}\n", diagnostics.status);

    if !diagnostics.issues.is_empty() {
        println!("🔴 Issues:");
        for issue in &diagnostics.issues {
            println!("   {}", issue);
        }
        println!();
    }

    if !diagnostics.warnings.is_empty() {
        println!("🟡 Warnings:");
        for warning in &diagnostics.warnings {
            println!("   {}", warning);
        }
        println!();
    }

    println!("📊 Details:");
    println!(
        "   Structure valid: {}",
        if diagnostics.details.structure_valid {
            "✅ Yes"
        } else {
            "❌ No"
        }
    );
    println!("   Snapshots: {}", diagnostics.details.snapshots_count);
    println!(
        "   Data files (unencrypted): {}",
        diagnostics.details.data_files_count
    );
    println!(
        "   Data files (encrypted): {}",
        diagnostics.details.encrypted_files_count
    );
    println!(
        "   Total size: {:.2} MB\n",
        diagnostics.details.total_size_mb
    );

    if !diagnostics.suggestions.is_empty() {
        println!("💡 Suggestions:");
        for suggestion in &diagnostics.suggestions {
            println!("   • {}", suggestion);
        }
        println!();
    }

    println!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━");
}

pub async fn sync_changed_files(src: &PathBuf, dst: &Path) -> Result<usize, String> {
    use std::time::SystemTime;
    use walkdir::WalkDir;

    if !src.exists() {
        return Err("Source folder does not exist".to_string());
    }

    let mut copied_count = 0;
    for entry in WalkDir::new(src).into_iter().filter_map(|e| e.ok()) {
        let src_path = entry.path();
        if src_path.is_file() {
            let relative_path = src_path.strip_prefix(src).map_err(|e| e.to_string())?;
            let dst_path = dst.join(relative_path);

            if let Some(parent) = dst_path.parent() {
                ensure_directory_exists_async(
                    parent
                        .to_str()
                        .ok_or("Invalid path characters in destination")?,
                ).await?;
            }

            let should_copy = if !dst_path.exists() {
                true
            } else {
                let src_meta = fs::metadata(src_path).map_err(|e| e.to_string())?;
                let dst_meta = fs::metadata(&dst_path).map_err(|e| e.to_string())?;
                src_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
                    > dst_meta.modified().unwrap_or(SystemTime::UNIX_EPOCH)
            };

            if should_copy {
                fs::copy(src_path, &dst_path).map_err(|e| format!("Error copying file: {}", e))?;
                copied_count += 1;
            }
        }
    }

    Ok(copied_count)
}
