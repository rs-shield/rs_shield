use std::path::{Path, PathBuf};
use rayon::prelude::*;
use tracing::{debug, info};
use crate::utils::{matches_exclude_pattern, walk_filtered};
use crate::core::file_processor;

pub fn discover_files(
    source: &Path,
    exclude_patterns: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>, Box<dyn std::error::Error>> {
    if !source.exists() {
        return Err(format!("Source path does not exist: {}", source.display()).into());
    }

    if !source.is_dir() {
        return Err(format!("Source path is not a directory: {}", source.display()).into());
    }

    let walker = walk_filtered(source, exclude_patterns, true);

    let mut files: Vec<(PathBuf, PathBuf)> = walker
        .filter_map(|e| {
            match e {
                Ok(entry) => {
                    if entry.path().is_file() {
                        let full = entry.path().to_path_buf();
                        if let Ok(rel) = full.strip_prefix(source) {
                            let rel_path = rel.to_path_buf();
                            // Check exclusion patterns
                            for pattern in exclude_patterns {
                                if matches_exclude_pattern(&rel_path, pattern) {
                                    debug!("Excluding: {}", rel_path.display());
                                    return None;
                                }
                            }
                            return Some((full, rel_path));
                        }
                    }
                    None
                }
                Err(e) => {
                    debug!("Walker error: {}", e);
                    None
                }
            }
        })
        .collect();

    // Sort files by priority for more efficient processing
    files.par_sort_by_key(|(full, _)| file_processor::get_file_priority(full));
    
    info!("📊 Found {} files to process from: {}", files.len(), source.display());

    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discover_empty_directory() {
        // TODO: Add test for empty directory
    }
}
