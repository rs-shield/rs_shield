// src/utils/mod.rs

use ignore::{Walk, WalkBuilder};
use memmap2::Mmap;
use std::path::{Path, PathBuf};
use tracing::warn;

pub fn expand_path(path: &str) -> PathBuf {
    // First, expand ~ if path starts with it
    let path_str = if path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            path.replacen("~", &home.to_string_lossy(), 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    // Then, expand environment variables ($VAR and ${VAR})
    let expanded = expand_env_vars(&path_str);

    PathBuf::from(expanded)
}

fn expand_env_vars(path: &str) -> String {
    let mut result = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            match chars.peek() {
                Some('{') => {
                    chars.next(); // consume {
                    let mut var_name = String::new();

                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next(); // consume }
                            break;
                        }
                        var_name.push(c);
                        chars.next();
                    }

                    // Tentar expandir com fallbacks inteligentes
                    if let Some(expanded) = expand_env_var_with_fallback(&var_name) {
                        result.push_str(&expanded);
                    } else {
                        // Se a variável não existir, manter como está
                        result.push('$');
                        result.push('{');
                        result.push_str(&var_name);
                        result.push('}');
                    }
                }
                Some(c) if c.is_alphanumeric() || *c == '_' => {
                    let mut var_name = String::new();

                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            var_name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    // Try to expand with smart fallbacks
                    if let Some(expanded) = expand_env_var_with_fallback(&var_name) {
                        result.push_str(&expanded);
                    } else {
                        // If variable doesn't exist, keep as is
                        result.push('$');
                        result.push_str(&var_name);
                    }
                }
                _ => {
                    result.push('$');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

/// Try to expand an environment variable with smart cross-platform fallbacks
fn expand_env_var_with_fallback(var_name: &str) -> Option<String> {
    // First, try the literal variable
    if let Ok(val) = std::env::var(var_name) {
        return Some(val);
    }

    // Smart fallbacks for HOME / USERPROFILE
    match var_name {
        "HOME" => {
            // On Windows, try USERPROFILE if HOME doesn't exist
            std::env::var("USERPROFILE")
                .ok()
                .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().into_owned()))
        }
        "USERPROFILE" => {
            // On Linux/macOS, try HOME if USERPROFILE doesn't exist
            std::env::var("HOME")
                .ok()
                .or_else(|| dirs::home_dir().map(|p| p.to_string_lossy().into_owned()))
        }
        _ => None,
    }
}

pub fn matches_exclude_pattern(path: &Path, pattern: &str) -> bool {
    let path_str = path.to_string_lossy();
    let path_display = path.display().to_string();

    if pattern.is_empty() {
        return false;
    }

    let pattern_normalized = pattern.trim_end_matches('/');

    // Case 1: Wildcard pattern → *.ext
    // ============================================================================
    if pattern_normalized.starts_with("*.") {
        // Example: *.tmp, *.js, *.log
        let ext = &pattern_normalized[1..]; // Remove * → .tmp, .js, .log
        return path_str.ends_with(ext);
    }

    // Case 2: Pattern starting with dot → .git, .gitignore, .DS_Store
    // ============================================================================
    if pattern_normalized.starts_with('.') {
        // Check path components that match this pattern
        // Example: .git, .DS_Store
        for component in path.iter() {
            if let Some(comp_str) = component.to_str() {
                if comp_str == pattern_normalized || comp_str.starts_with(pattern_normalized) {
                    return true;
                }
            }
        }
    }

    // Case 3: File or directory
    // ============================================================================

    // 3a: Check if last path component matches exactly
    // Example: pattern="index.js" → /path/to/index.js → EXCLUDE
    //          pattern="app.ts" → /path/to/app.ts → EXCLUDE
    if let Some(file_name) = path.file_name() {
        if let Some(file_name_str) = file_name.to_str() {
            if file_name_str == pattern_normalized {
                return true;
            }
        }
    }

    // 3b: Check if it's a complete directory (middle path component)
    // Example: pattern="node_modules" → /src/node_modules/lib.js → EXCLUDE
    //          pattern="build" → /src/build/out.js → EXCLUDE
    for component in path.iter() {
        if let Some(comp_str) = component.to_str() {
            if comp_str == pattern_normalized {
                return true;
            }
        }
    }

    // 3c: Check as substring for relative paths
    // Example: pattern="node_modules" → containing /node_modules/ or node_modules/
    if path_display.contains(&format!("/{}/", pattern_normalized))
        || path_display.contains(&format!("{}/", pattern_normalized))
        || path_display.starts_with(&format!("{}/", pattern_normalized))
    {
        return true;
    }

    false
}

/// Builds a WalkBuilder configured with exclusion filters
pub fn build_walker(root: &Path, _custom_globs: &[String], respect_gitignore: bool) -> WalkBuilder {
    let mut builder = WalkBuilder::new(root);

    builder
        .git_ignore(respect_gitignore)
        .ignore(respect_gitignore)
        .hidden(false)
        .follow_links(false)
        .max_depth(None)
        .threads(0);

    builder
}

pub fn walk_filtered(root: &Path, custom_globs: &[String], respect_gitignore: bool) -> Walk {
    build_walker(root, custom_globs, respect_gitignore).build()
}

/// Memory-map de ficheiro
pub fn mmap_file(path: &Path) -> std::io::Result<Mmap> {
    let file = std::fs::File::open(path)?;
    unsafe { Mmap::map(&file) }
}

pub fn ensure_directory_exists(path: &str) -> std::result::Result<PathBuf, String> {
    // Expand path (supports ~, $VAR, ${VAR})
    let expanded_path = expand_path(path);

    if expanded_path.exists() {
        if expanded_path.is_dir() {
            return Ok(expanded_path);
        } else {
            return Err(format!(
                "Path exists but is not a directory: {}",
                expanded_path.display()
            ));
        }
    }

    // Create directory with appropriate permissions
    #[cfg(unix)]
    {
        use std::fs::DirBuilder;
        use std::os::unix::fs::DirBuilderExt;

        let mut builder = DirBuilder::new();
        builder.mode(0o700); // rwx------
        builder.recursive(true);

        builder.create(&expanded_path).map_err(|e| {
            format!(
                "Error creating directory '{}': {}",
                expanded_path.display(),
                e
            )
        })?;
    }

    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(&expanded_path).map_err(|e| {
            format!(
                "Error creating directory '{}': {}",
                expanded_path.display(),
                e
            )
        })?;
    }

    Ok(expanded_path)
}

pub async fn ensure_directory_exists_async(path: &str) -> std::result::Result<PathBuf, String> {
    use tokio::fs;

    // Expand path (supports ~, $VAR, ${VAR})
    let expanded_path = expand_path(path);

    // Check if already exists
    match fs::metadata(&expanded_path).await {
        Ok(meta) => {
            if meta.is_dir() {
                return Ok(expanded_path);
            } else {
                return Err(format!(
                    "Path exists but is not a directory: {}",
                    expanded_path.display()
                ));
            }
        }
        Err(_) => {
            // Path doesn't exist yet, will create
        }
    }

    // Create directory
    fs::create_dir_all(&expanded_path).await.map_err(|e| {
        format!(
            "Error creating directory '{}': {}",
            expanded_path.display(),
            e
        )
    })?;

    Ok(expanded_path)
}

/// Checks if a directory exists, returns expanded PathBuf or error
pub fn verify_directory_exists(path: &str) -> std::result::Result<PathBuf, String> {
    let expanded_path = expand_path(path);

    if !expanded_path.exists() {
        return Err(format!(
            "Directory not found: {} (expanded from: {})",
            expanded_path.display(),
            path
        ));
    }

    if !expanded_path.is_dir() {
        return Err(format!(
            "Path exists but is not a directory: {}",
            expanded_path.display()
        ));
    }

    Ok(expanded_path)
}

/// Calculate the number of backups to keep based on retention policy
/// Supports: Xd (days), Xm (months), Xy (years), or direct numbers
/// Assumes 1 backup per day as the typical schedule
pub fn calculate_retention_backups(policy: &str) -> usize {
    match policy {
        // Common presets with optimal backup counts
        "7d" => 7,
        "14d" => 14,
        "30d" => 30,
        "60d" => 60,
        "90d" => 90,

        // Monthly presets (weekly backups)
        "6m" => 26,  // ~6 months
        "12m" => 52, // ~1 year

        // Yearly presets (weekly backups)
        "1y" => 52,
        "2y" => 104,
        "3y" => 156,
        "5y" => 260,

        _ => {
            // Parse custom daily retention
            if let Some(days_str) = policy.strip_suffix('d') {
                if let Ok(days) = days_str.parse::<usize>() {
                    return days;
                }
            }

            // Parse custom monthly retention
            if let Some(months_str) = policy.strip_suffix('m') {
                if let Ok(months) = months_str.parse::<usize>() {
                    let weeks = (months * 30) / 7;
                    return weeks.max(1);
                }
            }

            // Parse custom yearly retention
            if let Some(years_str) = policy.strip_suffix('y') {
                if let Ok(years) = years_str.parse::<usize>() {
                    return years * 52;
                }
            }

            warn!(
                "Unknown retention policy: '{}'. Using default: 10 backups",
                policy
            );

            10
        }
    }
}
pub async fn send_healthcheck(url: &Option<String>, suffix: &str) {
    if let Some(base_url) = url {
        let target = format!("{}{}", base_url, suffix);
        let client = reqwest::Client::new();
        if let Err(e) = client
            .get(&target)
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
        {
            warn!("Failed to send healthcheck to {}: {}", target, e);
        }
    }
}
