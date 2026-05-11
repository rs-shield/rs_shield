use std::collections::HashMap;
use std::path::PathBuf;

use crate::core::types::FileMetadata;

pub fn parse_manifest(
    content: &str,
) -> Result<HashMap<PathBuf, FileMetadata>, Box<dyn std::error::Error>> {
    Ok(toml::from_str(content)?)
}
