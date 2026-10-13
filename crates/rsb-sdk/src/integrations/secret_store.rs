//! Secure storage for notification-integration secrets (SMTP password,
//! webhook URLs, bot tokens).
//!
//! Backends, in order of preference:
//! 1. the OS keyring (service `rsb-shield`, the same service as the S3 master
//!    password and the profile encryption keys; override it with
//!    [`KEYRING_SERVICE_ENV`], which tests use to stay off the real keychain);
//! 2. an encrypted file (`~/.rs-shield/integrations_secrets.enc`) that reuses
//!    the AES-256-GCM + PBKDF2 format of `s3_credentials.enc`. Without a
//!    keyring there is nowhere safe to keep its key, so the passphrase comes
//!    from [`PASSPHRASE_ENV`]; without it this backend is unavailable.
//!
//! Every error message is built from fixed text plus backend/platform error
//! descriptions: secret values never appear in errors, logs or `Debug`.

use crate::credentials::SecureString;
use crate::credentials::encryption::{
    EncryptedCredentials, decrypt_credentials, encrypt_credentials,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use zeroize::Zeroizing;

/// Overrides the keyring service name (default [`DEFAULT_KEYRING_SERVICE`]).
pub const KEYRING_SERVICE_ENV: &str = "RSB_INTEGRATIONS_KEYRING_SERVICE";
/// Keyring service shared with the other rs-shield secrets.
pub const DEFAULT_KEYRING_SERVICE: &str = "rsb-shield";
/// Passphrase of the encrypted secrets file (headless fallback).
pub const PASSPHRASE_ENV: &str = "RSB_INTEGRATIONS_PASSPHRASE";
/// `auto` (default: keyring, then encrypted file), `keyring` or `file`.
pub const SECRET_STORE_ENV: &str = "RSB_INTEGRATIONS_SECRET_STORE";
/// File name of the encrypted fallback store, next to `integrations.json`.
pub const SECRETS_FILE: &str = "integrations_secrets.enc";

/// Where a secret is persisted. This is the only thing `integrations.json`
/// records about a secret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecretLocation {
    Keyring,
    EncryptedFile,
}

impl fmt::Display for SecretLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            SecretLocation::Keyring => "OS keyring",
            SecretLocation::EncryptedFile => "encrypted file",
        })
    }
}

/// Secret-store failure. Never carries a secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretStoreError {
    /// The backend cannot be used on this machine (no keyring service,
    /// locked keyring, no passphrase for the encrypted file, ...).
    Unavailable(String),
    /// The backend is usable but the operation failed.
    Failed(String),
}

impl fmt::Display for SecretStoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretStoreError::Unavailable(m) => write!(f, "unavailable: {}", m),
            SecretStoreError::Failed(m) => write!(f, "failed: {}", m),
        }
    }
}

impl std::error::Error for SecretStoreError {}

/// A place where integration secrets can be kept.
pub trait SecretBackend: Send + Sync {
    /// What `integrations.json` records for secrets kept here.
    fn location(&self) -> SecretLocation;
    /// `Ok(None)` when there is no secret under `key`.
    fn get(&self, key: &str) -> Result<Option<SecureString>, SecretStoreError>;
    fn set(&self, key: &str, value: &str) -> Result<(), SecretStoreError>;
    /// Deleting a missing secret is not an error.
    fn delete(&self, key: &str) -> Result<(), SecretStoreError>;
}

// ---------------------------------------------------------------- keyring

/// OS keyring backend (macOS Keychain, Windows Credential Manager, Linux
/// Secret Service).
pub struct KeyringBackend {
    service: String,
}

impl KeyringBackend {
    pub fn new(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// Service from [`KEYRING_SERVICE_ENV`], else [`DEFAULT_KEYRING_SERVICE`].
    pub fn from_env() -> Self {
        Self::new(
            std::env::var(KEYRING_SERVICE_ENV)
                .ok()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| DEFAULT_KEYRING_SERVICE.to_string()),
        )
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry, SecretStoreError> {
        keyring::Entry::new(&self.service, key).map_err(|e| keyring_error(&e))
    }
}

/// Maps keyring errors to messages that cannot contain the secret
/// (`BadEncoding` carries the stored bytes, `Ambiguous` the credentials).
fn keyring_error(e: &keyring::Error) -> SecretStoreError {
    match e {
        keyring::Error::PlatformFailure(inner) => {
            SecretStoreError::Unavailable(format!("keyring platform failure: {}", inner))
        }
        keyring::Error::NoStorageAccess(inner) => {
            SecretStoreError::Unavailable(format!("keyring not accessible: {}", inner))
        }
        keyring::Error::NoEntry => SecretStoreError::Failed("no keyring entry".into()),
        keyring::Error::BadEncoding(_) => {
            SecretStoreError::Failed("keyring entry is not valid UTF-8".into())
        }
        keyring::Error::TooLong(attr, _) => {
            SecretStoreError::Failed(format!("keyring attribute '{}' is too long", attr))
        }
        keyring::Error::Invalid(attr, reason) => {
            SecretStoreError::Failed(format!("invalid keyring attribute '{}': {}", attr, reason))
        }
        keyring::Error::Ambiguous(_) => SecretStoreError::Failed("ambiguous keyring entry".into()),
        _ => SecretStoreError::Failed("keyring error".into()),
    }
}

impl SecretBackend for KeyringBackend {
    fn location(&self) -> SecretLocation {
        SecretLocation::Keyring
    }

    fn get(&self, key: &str) -> Result<Option<SecureString>, SecretStoreError> {
        match self.entry(key)?.get_password() {
            Ok(v) => Ok(Some(SecureString::new(v))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(keyring_error(&e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretStoreError> {
        self.entry(key)?
            .set_password(value)
            .map_err(|e| keyring_error(&e))
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        match self.entry(key)?.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(keyring_error(&e)),
        }
    }
}

// --------------------------------------------------------- encrypted file

/// Encrypted map `{key: secret}` in the `s3_credentials.enc` format
/// (AES-256-GCM, PBKDF2-HMAC-SHA256 600k), written atomically with mode 0600.
pub struct EncryptedFileBackend {
    path: PathBuf,
    passphrase: Option<Zeroizing<String>>,
    cache: Mutex<Option<BTreeMap<String, SecureString>>>,
}

impl EncryptedFileBackend {
    pub fn new(path: impl Into<PathBuf>, passphrase: Option<String>) -> Self {
        Self {
            path: path.into(),
            passphrase: passphrase.filter(|p| !p.is_empty()).map(Zeroizing::new),
            cache: Mutex::new(None),
        }
    }

    /// Passphrase from [`PASSPHRASE_ENV`] (unavailable without it).
    pub fn from_env(path: impl Into<PathBuf>) -> Self {
        Self::new(path, std::env::var(PASSPHRASE_ENV).ok())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn passphrase(&self) -> Result<&str, SecretStoreError> {
        self.passphrase
            .as_deref()
            .map(|s| s.as_str())
            .ok_or_else(|| {
                SecretStoreError::Unavailable(format!(
                    "set {} to use the encrypted secrets file",
                    PASSPHRASE_ENV
                ))
            })
    }

    fn cache(&self) -> std::sync::MutexGuard<'_, Option<BTreeMap<String, SecureString>>> {
        self.cache.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn read_map(&self) -> Result<BTreeMap<String, SecureString>, SecretStoreError> {
        if let Some(map) = self.cache().as_ref() {
            return Ok(map.clone());
        }
        if !self.path.exists() {
            return Ok(BTreeMap::new());
        }
        let passphrase = self.passphrase()?;
        let content = std::fs::read_to_string(&self.path).map_err(|e| {
            SecretStoreError::Failed(format!("cannot read {}: {}", self.path.display(), e))
        })?;
        let encrypted: EncryptedCredentials = serde_json::from_str(&content).map_err(|_| {
            SecretStoreError::Failed(format!(
                "{} is not a valid secrets file",
                self.path.display()
            ))
        })?;
        let json = decrypt_credentials(&encrypted, passphrase).map_err(|_| {
            SecretStoreError::Failed(format!(
                "cannot decrypt {} (wrong {} or corrupted file)",
                self.path.display(),
                PASSPHRASE_ENV
            ))
        })?;
        let map: BTreeMap<String, SecureString> = serde_json::from_str(&json).map_err(|_| {
            SecretStoreError::Failed(format!("{} has an invalid layout", self.path.display()))
        })?;
        if encrypted.needs_upgrade()
            && let Err(e) = self.write_map(&map)
        {
            tracing::warn!("Could not upgrade {}: {}", self.path.display(), e);
        }
        *self.cache() = Some(map.clone());
        Ok(map)
    }

    fn write_map(&self, map: &BTreeMap<String, SecureString>) -> Result<(), SecretStoreError> {
        let passphrase = self.passphrase()?;
        let json = Zeroizing::new(
            serde_json::to_string(map)
                .map_err(|_| SecretStoreError::Failed("cannot serialize secrets".into()))?,
        );
        let encrypted = encrypt_credentials(&json, passphrase).map_err(SecretStoreError::Failed)?;
        let encrypted_json = serde_json::to_string_pretty(&encrypted)
            .map_err(|_| SecretStoreError::Failed("cannot serialize secrets file".into()))?;
        if let Some(dir) = self.path.parent().filter(|d| !d.as_os_str().is_empty()) {
            crate::config::create_dir_secure(dir).map_err(|e| {
                SecretStoreError::Failed(format!("cannot create {}: {}", dir.display(), e))
            })?;
        }
        // Drop the cache first: the next read goes back to disk, so a
        // read-back really checks what was written.
        *self.cache() = None;
        crate::config::write_file_atomic_secure(&self.path, encrypted_json.as_bytes()).map_err(
            |e| SecretStoreError::Failed(format!("cannot write {}: {}", self.path.display(), e)),
        )
    }
}

impl SecretBackend for EncryptedFileBackend {
    fn location(&self) -> SecretLocation {
        SecretLocation::EncryptedFile
    }

    fn get(&self, key: &str) -> Result<Option<SecureString>, SecretStoreError> {
        Ok(self.read_map()?.get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretStoreError> {
        // Check the passphrase before touching anything.
        self.passphrase()?;
        let mut map = self.read_map()?;
        map.insert(key.to_string(), SecureString::new(value.to_string()));
        self.write_map(&map)
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        if !self.path.exists() {
            return Ok(());
        }
        let mut map = self.read_map()?;
        if map.remove(key).is_some() {
            self.write_map(&map)?;
        }
        Ok(())
    }
}

// ----------------------------------------------------------------- memory

/// In-memory backend, for tests and embedders that manage secrets
/// themselves. [`MemoryBackend::unavailable`] simulates a machine without
/// that backend (e.g. a headless server without a keyring service).
pub struct MemoryBackend {
    location: SecretLocation,
    available: bool,
    map: Mutex<BTreeMap<String, SecureString>>,
}

impl MemoryBackend {
    pub fn new(location: SecretLocation) -> Self {
        Self {
            location,
            available: true,
            map: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn unavailable(location: SecretLocation) -> Self {
        Self {
            available: false,
            ..Self::new(location)
        }
    }

    fn check(&self) -> Result<(), SecretStoreError> {
        if self.available {
            Ok(())
        } else {
            Err(SecretStoreError::Unavailable(format!(
                "{} not available (simulated)",
                self.location
            )))
        }
    }

    fn map(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, SecureString>> {
        self.map.lock().unwrap_or_else(|p| p.into_inner())
    }
}

impl SecretBackend for MemoryBackend {
    fn location(&self) -> SecretLocation {
        self.location
    }

    fn get(&self, key: &str) -> Result<Option<SecureString>, SecretStoreError> {
        self.check()?;
        Ok(self.map().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<(), SecretStoreError> {
        self.check()?;
        self.map()
            .insert(key.to_string(), SecureString::new(value.to_string()));
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        self.check()?;
        self.map().remove(key);
        Ok(())
    }
}

// ----------------------------------------------------------------- stores

/// The backends in order of preference.
pub struct SecretStores {
    backends: Vec<Box<dyn SecretBackend>>,
}

impl SecretStores {
    pub fn new(backends: Vec<Box<dyn SecretBackend>>) -> Self {
        Self { backends }
    }

    /// Backends selected by [`SECRET_STORE_ENV`]: `keyring`, `file`, or
    /// (default) the keyring with the encrypted file as fallback.
    pub fn from_env(secrets_file: &Path) -> Self {
        let mode = std::env::var(SECRET_STORE_ENV)
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        let keyring = || Box::new(KeyringBackend::from_env()) as Box<dyn SecretBackend>;
        let file =
            || Box::new(EncryptedFileBackend::from_env(secrets_file)) as Box<dyn SecretBackend>;
        match mode.as_str() {
            "keyring" => Self::new(vec![keyring()]),
            "file" => Self::new(vec![file()]),
            "" | "auto" => Self::new(vec![keyring(), file()]),
            other => {
                tracing::warn!(
                    "Unknown {}='{}' (expected auto, keyring or file); using auto",
                    SECRET_STORE_ENV,
                    other
                );
                Self::new(vec![keyring(), file()])
            }
        }
    }

    /// Stores `value` in the first backend that accepts it **and** gives it
    /// back unchanged, then removes stale copies from the other backends.
    pub fn store(&self, key: &str, value: &str) -> Result<SecretLocation, SecretStoreError> {
        let mut errors = Vec::new();
        let mut all_unavailable = true;
        for (i, backend) in self.backends.iter().enumerate() {
            let outcome = backend
                .set(key, value)
                .and_then(|()| match backend.get(key)? {
                    Some(back) if back.as_str() == value => Ok(()),
                    _ => Err(SecretStoreError::Failed(
                        "read-back after write did not match".into(),
                    )),
                });
            match outcome {
                Ok(()) => {
                    for (j, other) in self.backends.iter().enumerate() {
                        if j != i
                            && let Err(SecretStoreError::Failed(e)) = other.delete(key)
                        {
                            tracing::warn!(
                                "Could not remove an old copy of '{}' from the {}: {}",
                                key,
                                other.location(),
                                e
                            );
                        }
                    }
                    return Ok(backend.location());
                }
                Err(e) => {
                    if matches!(e, SecretStoreError::Failed(_)) {
                        all_unavailable = false;
                    }
                    errors.push(format!("{}: {}", backend.location(), e));
                }
            }
        }
        let msg = if errors.is_empty() {
            "no secret store configured".to_string()
        } else {
            errors.join("; ")
        };
        Err(if all_unavailable {
            SecretStoreError::Unavailable(msg)
        } else {
            SecretStoreError::Failed(msg)
        })
    }

    /// Reads `key` from the backend for `location`.
    pub fn load(
        &self,
        location: SecretLocation,
        key: &str,
    ) -> Result<Option<SecureString>, SecretStoreError> {
        match self.backends.iter().find(|b| b.location() == location) {
            Some(b) => b.get(key),
            None => Err(SecretStoreError::Unavailable(format!(
                "the {} store is disabled ({})",
                location, SECRET_STORE_ENV
            ))),
        }
    }

    /// Removes `key` from every backend (unavailable backends are skipped).
    pub fn delete(&self, key: &str) -> Result<(), SecretStoreError> {
        let mut first_err = None;
        for b in &self.backends {
            match b.delete(key) {
                Ok(()) | Err(SecretStoreError::Unavailable(_)) => {}
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        first_err.map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn store_falls_back_when_first_backend_is_unavailable() {
        let stores = SecretStores::new(vec![
            Box::new(MemoryBackend::unavailable(SecretLocation::Keyring)),
            Box::new(MemoryBackend::new(SecretLocation::EncryptedFile)),
        ]);
        let loc = stores.store("k", "s3cr3t").unwrap();
        assert_eq!(loc, SecretLocation::EncryptedFile);
        assert_eq!(stores.load(loc, "k").unwrap().unwrap().as_str(), "s3cr3t");
        assert!(stores.load(SecretLocation::Keyring, "k").is_err());
    }

    #[test]
    fn store_fails_without_any_backend_and_error_has_no_secret() {
        let stores = SecretStores::new(vec![Box::new(MemoryBackend::unavailable(
            SecretLocation::Keyring,
        ))]);
        let err = stores.store("k", "hunter2-secret").unwrap_err();
        assert!(matches!(err, SecretStoreError::Unavailable(_)));
        assert!(!err.to_string().contains("hunter2"));
    }

    #[test]
    fn encrypted_file_round_trip_is_private_and_needs_the_passphrase() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".rs-shield").join(SECRETS_FILE);

        let no_pass = EncryptedFileBackend::new(&path, None);
        assert!(matches!(
            no_pass.set("k", "v"),
            Err(SecretStoreError::Unavailable(_))
        ));
        assert!(!path.exists());

        let backend = EncryptedFileBackend::new(&path, Some("correct horse".into()));
        backend.set("a", "token-aaa").unwrap();
        backend.set("b", "token-bbb").unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("token-aaa") && !raw.contains("token-bbb"));

        // A fresh instance (no cache) reads it back.
        let again = EncryptedFileBackend::new(&path, Some("correct horse".into()));
        assert_eq!(again.get("a").unwrap().unwrap().as_str(), "token-aaa");
        again.delete("a").unwrap();
        assert!(again.get("a").unwrap().is_none());
        assert_eq!(again.get("b").unwrap().unwrap().as_str(), "token-bbb");

        let wrong = EncryptedFileBackend::new(&path, Some("wrong".into()));
        let err = wrong.get("b").unwrap_err();
        assert!(!err.to_string().contains("token-bbb"));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
    }

    #[test]
    fn keyring_errors_never_include_stored_bytes() {
        let e = keyring_error(&keyring::Error::BadEncoding(b"hunter2".to_vec()));
        assert!(!format!("{} {:?}", e, e).contains("hunter2"));
    }
}
