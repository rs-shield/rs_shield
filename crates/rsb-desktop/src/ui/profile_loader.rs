//! Profiles and encryption keys, resolved exactly like the CLI.
//!
//! A profile is read with the SDK's [`config::load_config`] (same serde
//! defaults, `~`/`$VAR` expansion and legacy-secret warnings as
//! `rsb <command> profile.toml`), and the encryption key is looked up in
//! the same order as the CLI: the key typed on screen → `RSB_ENCRYPTION_KEY`
//! → the profile's OS keyring entry (written by `rsb create-profile
//! --encrypt` and the desktop "Create profile" screen) → the legacy
//! `encryption_key` in the TOML.

use crate::ui::i18n::{Translations, tr};
use crate::ui::s3_settings::S3Settings;
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
pub fn load_profile(path: &Path, t: &Translations) -> Result<LoadedProfile, String> {
    let config = config::load_config(path)
        .map_err(|e| tr(t.pl_cannot_load_profile, &[&path.display(), &e]))?;
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

/// The repository an operation (restore, verify, prune) works on, chosen
/// like the CLI's `<profile>` / `--backup <repository dir>`, or the S3
/// bucket configured in Settings when neither is given.
pub fn repository_config(
    profile: Option<&LoadedProfile>,
    repo_dir: &Path,
    s3: &S3Settings,
    t: &Translations,
) -> Result<Config, String> {
    if let Some(p) = profile {
        return Ok(p.config.clone());
    }
    if !repo_dir.as_os_str().is_empty() {
        if repo_dir.is_file() {
            return Err(tr(t.pl_path_is_file, &[&repo_dir.display()]));
        }
        return rsb_sdk::config_from_backup_path(repo_dir, None)
            .map_err(|e| tr(t.pl_not_a_repository, &[&e]));
    }
    if let Some(bucket) = &s3.bucket {
        return rsb_sdk::portable_restore::config_for_s3_restore(
            bucket,
            s3.region.clone(),
            s3.endpoint.clone(),
        )
        .map_err(|e| e.to_string());
    }
    Err(t.pl_select_repository.to_string())
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

/// Checks the key for a backup of `cfg`: a profile with `encrypt_patterns`
/// must not be backed up without a key, since the files would silently be
/// stored unencrypted (same rule as `rsb backup`; the refusal message is
/// `pl_missing_key`).
pub fn check_backup_key(
    cfg: &Config,
    key: Option<String>,
    t: &Translations,
) -> Result<Option<String>, String> {
    match key {
        Some(k) => Ok(Some(k)),
        None if cfg.encryption_required() => Err(t.pl_missing_key.to_string()),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::i18n::{Language, get_texts};

    fn t() -> &'static Translations {
        get_texts(Language::En)
    }

    fn load_profile(path: &Path) -> Result<LoadedProfile, String> {
        super::load_profile(path, t())
    }

    fn repository_config(
        profile: Option<&LoadedProfile>,
        repo_dir: &Path,
        s3: &S3Settings,
    ) -> Result<Config, String> {
        super::repository_config(profile, repo_dir, s3, t())
    }

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
        assert_eq!(check_backup_key(&cfg, None, t()), Ok(None));
        cfg.encrypt_patterns = Some(vec!["*".into()]);
        assert_eq!(
            check_backup_key(&cfg, None, t()),
            Err(t().pl_missing_key.into())
        );
        assert_eq!(
            check_backup_key(&cfg, Some("k".into()), t()),
            Ok(Some("k".to_string()))
        );
    }

    #[test]
    fn repository_from_profile_folder_or_s3() {
        let dir = tempfile::tempdir().unwrap();
        let none = S3Settings::default();
        assert!(repository_config(None, Path::new(""), &none).is_err());

        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join("snapshots")).unwrap();
        std::fs::create_dir_all(repo.join("data")).unwrap();
        let cfg = repository_config(None, &repo, &none).unwrap();
        assert_eq!(cfg.destination_path, repo.to_string_lossy());
        assert!(cfg.s3_bucket.is_none());

        let not_repo = dir.path().join("empty");
        std::fs::create_dir_all(&not_repo).unwrap();
        assert!(repository_config(None, &not_repo, &none).is_err());
        let file = write(
            dir.path(),
            "p.toml",
            "source_path = \"/s\"\ndestination_path = \"/d\"\n",
        );
        assert!(
            repository_config(None, &file, &none)
                .unwrap_err()
                .contains("is a file")
        );

        let s3 = S3Settings::from_values("bkt".into(), "eu-west-1".into(), String::new());
        let cfg = repository_config(None, Path::new(""), &s3).unwrap();
        assert_eq!(cfg.s3_bucket.as_deref(), Some("bkt"));

        // A loaded profile wins over the other fields.
        let prof = load_profile(&file).unwrap();
        let cfg = repository_config(Some(&prof), &repo, &s3).unwrap();
        assert_eq!(cfg.destination_path, "/d");
    }

    #[test]
    fn blank_fields_are_none() {
        assert_eq!(non_empty("  "), None);
        assert_eq!(non_empty("x").as_deref(), Some("x"));
    }
}
