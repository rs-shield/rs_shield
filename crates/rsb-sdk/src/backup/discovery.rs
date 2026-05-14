use std::path::{Path, PathBuf};
use rayon::prelude::*;
use tracing::debug;
use crate::utils::{matches_exclude_pattern, walk_filtered};
use crate::core::file_processor;

pub fn discover_files(
    source: &Path,
    exclude_patterns: &[String],
) -> Result<Vec<(PathBuf, PathBuf)>, Box<dyn std::error::Error>> {
    let walker = walk_filtered(source, exclude_patterns, true);

    let mut files: Vec<(PathBuf, PathBuf)> = walker
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .filter_map(|e| {
            let full = e.path().to_path_buf();
            let rel = full.strip_prefix(source).ok()?.to_path_buf();

            for pattern in exclude_patterns {
                if matches_exclude_pattern(&rel, pattern) {
                    debug!("Excluding: {}", rel.display());
                    return None;
                }
            }
            Some((full, rel))
        })
        .collect();

    // Sort files by priority for more efficient processing
    files.par_sort_by_key(|(full, _)| file_processor::get_file_priority(full));
    
    debug!("📊 Found {} files to process", files.len());

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
