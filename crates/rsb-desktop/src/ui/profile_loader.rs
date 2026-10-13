//! Profiles and encryption keys, resolved exactly like the CLI.
//!
//! A profile is read with the SDK's [`config::load_config`] (same serde
//! defaults, `~`/`$VAR` expansion and legacy-secret warnings as
//! `rsb <command> profile.toml`), and the encryption key is looked up in
//! the same order as the CLI: the key typed on screen → `RSB_ENCRYPTION_KEY`
//! → the profile's OS keyring entry (written by `rsb create-profile
//! --encrypt` and the desktop "Create profile" screen) → the legacy
//! `encryption_key` in the TOML.

use rsb_sdk::config::{self, Config};
use std::path::{Path, PathBuf};

/// A profile loaded from disk: where it came from and its configuration.
#[derive(Clone, Debug)]
pub struct LoadedProfile {
    pub path: PathBuf,
    pub config: Config,
}

impl PartialEq for LoadedProfile {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path
    }
}

impl LoadedProfile {
    /// Keyring account name of the profile (its file stem, as in the CLI).
    pub fn name(&self) -> Option<String> {
        profile_name(&self.path)
    }
}

/// Loads a profile with the SDK loader used by the CLI.
pub fn load_profile(path: &Path) -> Result<LoadedProfile, String> {
    let config = config::load_config(path)
        .map_err(|e| format!("Cannot load profile {}: {}", path.display(), e))?;
    if let Some(level) = config.compression_level {
        config::validate_compression_level(level).map_err(|e| e.to_string())?;
    }
    Ok(LoadedProfile {
        path: path.to_path_buf(),
        config,
    })
}

/// Name of a profile file (`docs.toml` → `docs`), the CLI's keyring account.
pub fn profile_name(path: &Path) -> Option<String> {
    path.file_stem()
        .and_then(|s| s.to_str())
        .map(|s| s.to_string())
}

/// S3 bucket configured by a profile, if any.
pub fn s3_bucket(cfg: &Config) -> Option<String> {
    cfg.s3
        .as_ref()
        .and_then(|s| s.bucket.clone())
        .or_else(|| cfg.s3_bucket.clone())
        .filter(|b| !b.trim().is_empty())
}

/// Where the backups of `cfg` live: `s3://bucket` or the local directory.
pub fn repository_label(cfg: &Config) -> String {
    match s3_bucket(cfg) {
        Some(b) => format!("s3://{}", b),
        None => cfg.destination_path.clone(),
    }
}

/// `None` for an empty (or blank) text field.
pub fn non_empty(s: &str) -> Option<String> {
    if s.trim().is_empty() {
        None
    } else {
        Some(s.to_string())
    }
}

/// Resolves the encryption key like the CLI (see the module docs).
///
/// May query the OS keyring, which can block (or show a prompt): call it
/// off the UI thread, e.g. with [`resolve_key_blocking`].
pub fn resolve_key(typed: &str, profile: Option<&LoadedProfile>) -> Option<String> {
    let name = profile.and_then(LoadedProfile::name);
    config::resolve_encryption_key(
        non_empty(typed),
        name.as_deref(),
        profile.map(|p| &p.config),
    )
}

/// [`resolve_key`] on a blocking thread.
pub async fn resolve_key_blocking(typed: String, profile: Option<LoadedProfile>) -> Option<String> {
    tokio::task::spawn_blocking(move || resolve_key(&typed, profile.as_ref()))
        .await
        .ok()
        .flatten()
}

/// Message shown when a profile requires encryption but no key was found
/// (same rule as `rsb backup`: never back up such a profile unencrypted).
pub const MISSING_KEY_MESSAGE: &str = "This profile has encryption enabled but no key is available. \
     Type the key, set RSB_ENCRYPTION_KEY, or store it in the OS keyring \
     (Create profile with encryption, or `rsb create-profile --encrypt`). \
     Refusing to back up unencrypted.";

/// Checks the key for a backup of `cfg`: a profile with `encrypt_patterns`
/// must not be backed up without a key, since the files would silently be
/// stored unencrypted.
pub fn check_backup_key(cfg: &Config, key: Option<String>) -> Result<Option<String>, String> {
    match key {
        Some(k) => Ok(Some(k)),
        None if cfg.encryption_required() => Err(MISSING_KEY_MESSAGE.to_string()),
        None => Ok(None),
    }
}

/// Old desktop-only loader, kept while the screens move to [`load_profile`].
#[allow(dead_code)]
pub mod legacy {
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};

    /// Expande paths especiais como ~ (home directory) e variáveis de ambiente ($VAR, ${VAR})
    fn expand_path(path: &str) -> PathBuf {
        // Primeiro, expandir ~ se o path começar com ele
        let path_str = if path.starts_with("~") {
            if let Some(home) = dirs::home_dir() {
                path.replacen("~", &home.to_string_lossy(), 1)
            } else {
                path.to_string()
            }
        } else {
            path.to_string()
        };

        // Depois, expandir variáveis de ambiente ($VAR e ${VAR})
        let expanded = expand_env_vars(&path_str);

        PathBuf::from(expanded)
    }

    /// Expande variáveis de ambiente no formato $VAR ou ${VAR}
    fn expand_env_vars(path: &str) -> String {
        let mut result = String::new();
        let mut chars = path.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '$' {
                match chars.peek() {
                    Some('{') => {
                        // ${VAR} format
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

                        if let Ok(val) = std::env::var(&var_name) {
                            result.push_str(&val);
                        } else {
                            result.push('$');
                            result.push('{');
                            result.push_str(&var_name);
                            result.push('}');
                        }
                    }
                    Some(c) if c.is_alphanumeric() || *c == '_' => {
                        // $VAR format
                        let mut var_name = String::new();

                        while let Some(&c) = chars.peek() {
                            if c.is_alphanumeric() || c == '_' {
                                var_name.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        if let Ok(val) = std::env::var(&var_name) {
                            result.push_str(&val);
                        } else {
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

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct S3Config {
        pub bucket: Option<String>,
        pub region: Option<String>,
        pub endpoint: Option<String>,
        pub access_key: Option<String>,
        pub secret_key: Option<String>,
    }

    #[derive(Debug, Serialize, Deserialize, Clone)]
    pub struct Config {
        pub source_path: String,
        pub destination_path: String,
        pub exclude_patterns: Vec<String>,
        pub encryption_key: Option<String>,
        pub backup_mode: String,
        pub s3_bucket: Option<String>,
        pub s3_region: Option<String>,
        pub s3_endpoint: Option<String>,
        pub s3: Option<S3Config>,
        pub encrypt_patterns: Option<Vec<String>>,
        pub pause_on_low_battery: Option<u8>,
        pub pause_on_high_cpu: Option<u8>,
        pub compression_level: Option<u8>,
    }

    pub fn load_profile(path: &Path) -> Result<Config, String> {
        let content =
            std::fs::read_to_string(path).map_err(|e| format!("Erro ao ler arquivo: {}", e))?;

        let mut config: Config =
            toml::from_str(&content).map_err(|e| format!("Erro ao parsear TOML: {}", e))?;

        // Expandir paths especiais como ~ e variáveis de ambiente
        config.source_path = expand_path(&config.source_path)
            .to_string_lossy()
            .into_owned();
        config.destination_path = expand_path(&config.destination_path)
            .to_string_lossy()
            .into_owned();

        Ok(config)
    }

    pub struct ProfileData {
        pub source_path: String,
        pub destination_path: String,
        pub exclude_patterns: String,
        pub encryption_key: String,
        pub backup_mode: String,
        pub s3_bucket: String,
        pub s3_region: String,
        pub s3_endpoint: String,
        pub s3_access_key: String,
        pub s3_secret_key: String,
        pub encrypt_patterns: String,
        pub pause_on_low_battery: String,
        pub pause_on_high_cpu: String,
        pub compression_level: String,
    }

    impl From<Config> for ProfileData {
        fn from(cfg: Config) -> Self {
            let s3_bucket = cfg
                .s3
                .as_ref()
                .and_then(|s| s.bucket.clone())
                .or(cfg.s3_bucket)
                .unwrap_or_default();
            let s3_region = cfg
                .s3
                .as_ref()
                .and_then(|s| s.region.clone())
                .or(cfg.s3_region)
                .unwrap_or_default();
            let s3_endpoint = cfg
                .s3
                .as_ref()
                .and_then(|s| s.endpoint.clone())
                .or(cfg.s3_endpoint)
                .unwrap_or_default();
            let s3_access_key = cfg
                .s3
                .as_ref()
                .and_then(|s| s.access_key.clone())
                .unwrap_or_default();
            let s3_secret_key = cfg
                .s3
                .as_ref()
                .and_then(|s| s.secret_key.clone())
                .unwrap_or_default();

            ProfileData {
                source_path: cfg.source_path,
                destination_path: cfg.destination_path,
                exclude_patterns: cfg.exclude_patterns.join("\n"),
                encryption_key: cfg.encryption_key.unwrap_or_default(),
                backup_mode: cfg.backup_mode,
                s3_bucket,
                s3_region,
                s3_endpoint,
                s3_access_key,
                s3_secret_key,
                encrypt_patterns: cfg
                    .encrypt_patterns
                    .map(|v| v.join("\n"))
                    .unwrap_or_default(),
                pause_on_low_battery: cfg
                    .pause_on_low_battery
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                pause_on_high_cpu: cfg
                    .pause_on_high_cpu
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                compression_level: cfg
                    .compression_level
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let p = dir.join(name);
        std::fs::write(&p, body).unwrap();
        p
    }

    #[test]
    fn minimal_profile_loads_with_sdk_defaults() {
        // Only the two required fields: the old desktop loader rejected
        // this (missing exclude_patterns/backup_mode) although the CLI
        // accepts it.
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "docs.toml",
            "source_path = \"/src\"\ndestination_path = \"/repo\"\n",
        );
        let prof = load_profile(&p).unwrap();
        assert_eq!(prof.name().as_deref(), Some("docs"));
        assert_eq!(prof.config.backup_mode, "incremental");
        assert!(!prof.config.exclude_patterns.is_empty());
        assert_eq!(repository_label(&prof.config), "/repo");
    }

    #[test]
    fn s3_profile_label_and_invalid_profiles() {
        let dir = tempfile::tempdir().unwrap();
        let p = write(
            dir.path(),
            "cloud.toml",
            "source_path = \"/src\"\ndestination_path = \"s3://b\"\nmax_threads = 3\n[s3]\nbucket = \"b\"\n",
        );
        let prof = load_profile(&p).unwrap();
        assert_eq!(repository_label(&prof.config), "s3://b");
        assert_eq!(prof.config.max_threads, Some(3));

        let bad = write(dir.path(), "bad.toml", "source_path = 1\n");
        assert!(load_profile(&bad).unwrap_err().contains("bad.toml"));
        let level = write(
            dir.path(),
            "lvl.toml",
            "source_path = \"/s\"\ndestination_path = \"/d\"\ncompression_level = 99\n",
        );
        assert!(load_profile(&level).is_err());
    }

    #[test]
    fn encrypted_profile_without_key_is_refused() {
        let mut cfg = Config::new("/s", "/d");
        assert_eq!(check_backup_key(&cfg, None), Ok(None));
        cfg.encrypt_patterns = Some(vec!["*".into()]);
        assert_eq!(
            check_backup_key(&cfg, None),
            Err(MISSING_KEY_MESSAGE.into())
        );
        assert_eq!(
            check_backup_key(&cfg, Some("k".into())),
            Ok(Some("k".to_string()))
        );
    }

    #[test]
    fn blank_fields_are_none() {
        assert_eq!(non_empty("  "), None);
        assert_eq!(non_empty("x").as_deref(), Some("x"));
    }
}
