use std::{collections::HashMap, path::PathBuf};

use indicatif::{ProgressBar, ProgressStyle};

use crate::core::types::FileMetadata;

pub fn progress_bar(manifest: HashMap<PathBuf, FileMetadata>) -> ProgressBar {
    let pb = ProgressBar::new(manifest.len() as u64);
    pb.set_style(
                ProgressStyle::default_bar()
                    .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({eta}) {msg}")
                    .unwrap()
                    .progress_chars("#>-"),
            );
    pb
}
