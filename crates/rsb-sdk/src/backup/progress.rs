use indicatif::{ProgressBar, ProgressStyle};
use std::path::Path;
use crate::core::types::ProgressCallback;
use super::stats::Stats;

pub fn create_progress_bar(len: u64) -> ProgressBar {
    let pb = ProgressBar::new(len);
    pb.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
            .unwrap()
            .progress_chars("#>-"),
    );
    pb
}

/// Shortens the file path for progress bar display
fn shorten_path(path: &Path) -> String {
    let components: Vec<_> = path.components().collect();
    if components.len() > 2 {
        // Shows only the last 2 components
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("...");
        let file = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("?");
        format!("{}/{}", parent, file)
    } else {
        path.display().to_string()
    }
}

pub fn update_progress(
    pb: &ProgressBar,
    stats: &Stats,
    total: usize,
    rel_path: &Path,
    on_progress: &Option<ProgressCallback>,
) {
    // Shortens the path for a cleaner UI
    let display_path = shorten_path(rel_path);
    pb.set_message(format!("📄 {}", display_path));

    // Optional callback for UI integration
    if let Some(cb) = on_progress {
        let current = stats.get_processed() + stats.get_skipped() + stats.get_errors();
        cb(
            current,
            total,
            format!("📄 {}", display_path),
        );
    }
}

pub fn finish_progress_bar(pb: &ProgressBar, stats: &Stats) {
    let current = stats.get_processed() + stats.get_skipped() + stats.get_errors();
    let total = current; // We assume everything was processed
    
    pb.finish_with_message(format!(
        "✅ Processing completed ({}/{})",
        current, total
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_shorten_path_long() {
        let path = PathBuf::from("a/b/c/d/main.rs");
        let shortened = shorten_path(&path);
        assert_eq!(shortened, "d/main.rs");
    }

    #[test]
    fn test_shorten_path_short() {
        let path = PathBuf::from("main.rs");
        let shortened = shorten_path(&path);
        assert_eq!(shortened, "main.rs");
    }
}
