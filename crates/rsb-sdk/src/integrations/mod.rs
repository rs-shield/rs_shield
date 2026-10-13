//! Notification integrations (email, Slack, Telegram, Discord) shared by the
//! desktop app and the CLI.
//!
//! `~/.rs-shield/integrations.json` (mode 0600) holds only non-secret fields.
//! Each secret (SMTP password, Slack/Discord webhook URL, Telegram bot token)
//! is kept in a [`SecretStores`] backend and the JSON records only where:
//!
//! ```json
//! "sender_password": { "stored": "keyring" }
//! ```
//!
//! Files written by older versions hold the secrets in plaintext. They are
//! migrated on the first load: each secret is written to the secure store and
//! read back, and only then is the file rewritten (atomically, 0600) without
//! them. If no secure store is available the file is left untouched (the
//! secrets keep working) and a warning is logged.
//!
//! At run time each secret can be overridden from the environment (headless
//! servers, cron): [`ENV_SMTP_PASSWORD`], [`ENV_SLACK_WEBHOOK_URL`],
//! [`ENV_TELEGRAM_BOT_TOKEN`], [`ENV_DISCORD_WEBHOOK_URL`].

pub mod secret_store;

pub use secret_store::{
    DEFAULT_KEYRING_SERVICE, EncryptedFileBackend, KEYRING_SERVICE_ENV, KeyringBackend,
    MemoryBackend, PASSPHRASE_ENV, SECRET_STORE_ENV, SECRETS_FILE, SecretBackend, SecretLocation,
    SecretStoreError, SecretStores,
};

use crate::core::{ChatIntegration, EmailConfig};
use crate::credentials::SecureString;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Once;
use zeroize::Zeroizing;

/// File name of the integrations configuration.
pub const INTEGRATIONS_FILE: &str = "integrations.json";

/// Overrides the stored SMTP password.
pub const ENV_SMTP_PASSWORD: &str = "RSB_SMTP_PASSWORD";
/// Overrides the stored Slack webhook URL.
pub const ENV_SLACK_WEBHOOK_URL: &str = "RSB_SLACK_WEBHOOK_URL";
/// Overrides the stored Telegram bot token.
pub const ENV_TELEGRAM_BOT_TOKEN: &str = "RSB_TELEGRAM_BOT_TOKEN";
/// Overrides the stored Discord webhook URL.
pub const ENV_DISCORD_WEBHOOK_URL: &str = "RSB_DISCORD_WEBHOOK_URL";

const KEY_EMAIL: &str = "integration:email:smtp-password";
const KEY_SLACK: &str = "integration:slack:webhook-url";
const KEY_TELEGRAM: &str = "integration:telegram:bot-token";
const KEY_DISCORD: &str = "integration:discord:webhook-url";

/// Every secret slot: (integration, store key, env override).
const SLOTS: [(&str, &str, &str); 4] = [
    ("email", KEY_EMAIL, ENV_SMTP_PASSWORD),
    ("slack", KEY_SLACK, ENV_SLACK_WEBHOOK_URL),
    ("telegram", KEY_TELEGRAM, ENV_TELEGRAM_BOT_TOKEN),
    ("discord", KEY_DISCORD, ENV_DISCORD_WEBHOOK_URL),
];

static MIGRATION_NOTICE: Once = Once::new();
static MIGRATION_FAILURE_NOTICE: Once = Once::new();

// ------------------------------------------------------------ StoredSecret

/// A secret field of an integration. Serializes **only** its location
/// (`{"stored": "keyring"}`), never its value; still deserializes the legacy
/// plaintext string so old files can be migrated. `Debug` is redacted.
#[derive(Clone, Default)]
pub struct StoredSecret {
    value: SecureString,
    location: Option<SecretLocation>,
    /// The value must be written to (or removed from) the secure store.
    dirty: bool,
}

impl StoredSecret {
    /// A new secret value (to be stored on the next save).
    pub fn new(value: impl Into<String>) -> Self {
        let value = SecureString::new(value.into());
        let dirty = !value.is_empty();
        Self {
            value,
            location: None,
            dirty,
        }
    }

    /// The secret value ("" when none, or when it could not be read).
    pub fn expose(&self) -> &str {
        self.value.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Where the secret is persisted (`None`: not persisted securely).
    pub fn location(&self) -> Option<SecretLocation> {
        self.location
    }

    /// True for a value read in plaintext from `integrations.json`, or set
    /// in memory, that is not in the secure store yet.
    pub fn needs_persist(&self) -> bool {
        self.dirty
    }

    /// This secret with the value typed by the user. An unchanged value
    /// keeps the current state, so a secret that could not be read (locked
    /// keyring: shown empty) is not deleted by saving an untouched form.
    pub fn updated(&self, new_value: &str) -> Self {
        if new_value == self.value.as_str() {
            return self.clone();
        }
        Self {
            value: SecureString::new(new_value.to_string()),
            location: self.location,
            dirty: true,
        }
    }

    fn has_no_reference(&self) -> bool {
        self.location.is_none()
    }
}

impl fmt::Debug for StoredSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("StoredSecret")
            .field(
                "value",
                &if self.value.is_empty() {
                    "<empty>"
                } else {
                    "***REDACTED***"
                },
            )
            .field("location", &self.location)
            .finish()
    }
}

#[derive(Serialize, Deserialize)]
struct SecretRef {
    stored: SecretLocation,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum SecretRepr {
    Ref(SecretRef),
    Plaintext(SecureString),
}

impl Serialize for StoredSecret {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self.location {
            Some(stored) => SecretRef { stored }.serialize(s),
            None => s.serialize_none(),
        }
    }
}

impl<'de> Deserialize<'de> for StoredSecret {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(match Option::<SecretRepr>::deserialize(d)? {
            None => Self::default(),
            Some(SecretRepr::Ref(r)) => Self {
                value: SecureString::default(),
                location: Some(r.stored),
                dirty: false,
            },
            Some(SecretRepr::Plaintext(v)) => {
                let dirty = !v.is_empty();
                Self {
                    value: v,
                    location: None,
                    dirty,
                }
            }
        })
    }
}

// ------------------------------------------------------------ config types

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct IntegrationConfig {
    #[serde(default)]
    pub email: Option<EmailIntegrationConfig>,
    #[serde(default)]
    pub slack: Option<SlackIntegrationConfig>,
    #[serde(default)]
    pub telegram: Option<TelegramIntegrationConfig>,
    #[serde(default)]
    pub discord: Option<DiscordIntegrationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailIntegrationConfig {
    #[serde(default)]
    pub enabled: bool,
    pub smtp_server: String,
    pub smtp_port: u16,
    pub sender_email: String,
    #[serde(default, skip_serializing_if = "StoredSecret::has_no_reference")]
    pub sender_password: StoredSecret,
    pub recipient_email: String,
    pub use_tls: bool,
}

impl Default for EmailIntegrationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            smtp_server: "smtp.gmail.com".to_string(),
            smtp_port: 587,
            sender_email: String::new(),
            sender_password: StoredSecret::default(),
            recipient_email: String::new(),
            use_tls: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SlackIntegrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "StoredSecret::has_no_reference")]
    pub webhook_url: StoredSecret,
    #[serde(default)]
    pub mention_user: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TelegramIntegrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "StoredSecret::has_no_reference")]
    pub bot_token: StoredSecret,
    pub chat_id: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiscordIntegrationConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "StoredSecret::has_no_reference")]
    pub webhook_url: StoredSecret,
}

/// Error of a save/migration. Never carries a secret value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationsError(pub String);

impl fmt::Display for IntegrationsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for IntegrationsError {}

/// Where the secret of an integration currently comes from (for status
/// output; never the value).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretSource {
    /// Not configured.
    None,
    /// Environment override.
    Env(&'static str),
    /// Secure store.
    Stored(SecretLocation),
    /// Stored, but it could not be read now (locked keyring, no passphrase).
    Unreadable(SecretLocation),
    /// Still in plaintext in `integrations.json` (migration not possible).
    LegacyPlaintext,
}

impl fmt::Display for SecretSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretSource::None => f.write_str("not set"),
            SecretSource::Env(var) => write!(f, "environment ({})", var),
            SecretSource::Stored(l) => write!(f, "{}", l),
            SecretSource::Unreadable(l) => write!(f, "{} (cannot be read now)", l),
            SecretSource::LegacyPlaintext => {
                f.write_str("PLAINTEXT in integrations.json (could not be migrated)")
            }
        }
    }
}

/// One line of [`IntegrationConfig::status`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrationStatus {
    pub name: &'static str,
    pub enabled: bool,
    pub secret: SecretSource,
}

fn env_lookup(name: &str) -> Option<String> {
    std::env::var(name).ok()
}

/// Env override first, then the stored value.
fn effective(
    secret: &StoredSecret,
    env_name: &str,
    env: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    env(env_name)
        .filter(|v| !v.is_empty())
        .or_else(|| (!secret.is_empty()).then(|| secret.expose().to_string()))
}

impl IntegrationConfig {
    /// `~/.rs-shield` (the directory of every rs-shield user file).
    pub fn default_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".rs-shield")
    }

    /// The one file the desktop Integrations screen saves and every
    /// operation reads: `~/.rs-shield/integrations.json`.
    pub fn default_path() -> PathBuf {
        Self::default_dir().join(INTEGRATIONS_FILE)
    }

    /// `~/.rs-shield/integrations_secrets.enc` (encrypted fallback store).
    pub fn default_secrets_path() -> PathBuf {
        Self::default_dir().join(SECRETS_FILE)
    }

    /// The secret stores chosen by the environment (see [`SecretStores::from_env`]).
    pub fn default_stores() -> SecretStores {
        SecretStores::from_env(&Self::default_secrets_path())
    }

    /// Loads [`Self::default_path`] with [`Self::default_stores`],
    /// migrating plaintext secrets.
    pub fn load_default() -> Self {
        Self::load(&Self::default_path(), &Self::default_stores())
    }

    /// Loads `path` (empty configuration if missing/invalid), migrates any
    /// plaintext secret into `stores` and resolves the stored secrets.
    pub fn load(path: &Path, stores: &SecretStores) -> Self {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => Zeroizing::new(c),
            Err(_) => return Self::default(),
        };
        let mut cfg: Self = match serde_json::from_str(&content) {
            Ok(c) => c,
            Err(e) => {
                // Only position/category: serde messages may quote values.
                tracing::warn!(
                    "Ignoring invalid {} ({:?} error at line {}, column {})",
                    path.display(),
                    e.classify(),
                    e.line(),
                    e.column()
                );
                return Self::default();
            }
        };

        if cfg.has_plaintext_secrets() {
            cfg.migrate(path, stores);
        }
        cfg.resolve_secrets(stores);
        cfg
    }

    /// Moves plaintext secrets into `stores` and rewrites `path` without
    /// them. On any failure the file is left exactly as it was.
    fn migrate(&mut self, path: &Path, stores: &SecretStores) {
        let mut migrated = self.clone();
        let result = migrated
            .persist_secrets(stores)
            .and_then(|()| migrated.write_file(path));
        match result {
            Ok(()) => {
                let mut places: Vec<String> = migrated
                    .secrets()
                    .into_iter()
                    .filter_map(|(_, _, _, s)| s.location().map(|l| l.to_string()))
                    .collect();
                places.dedup();
                MIGRATION_NOTICE.call_once(|| {
                    tracing::warn!(
                        "🔐 Moved plaintext notification secrets out of {} into the {}; the file now holds only references.",
                        path.display(),
                        places.join(" / ")
                    );
                });
                *self = migrated;
            }
            Err(e) => {
                MIGRATION_FAILURE_NOTICE.call_once(|| {
                    tracing::warn!(
                        "⚠️ {} holds notification secrets in plaintext and they could not be moved to a secure store ({}). The file was left unchanged. Make the OS keyring available, set {} for the encrypted-file fallback, or remove them and use {}/{}/{}/{}.",
                        path.display(),
                        e,
                        PASSPHRASE_ENV,
                        ENV_SMTP_PASSWORD,
                        ENV_SLACK_WEBHOOK_URL,
                        ENV_TELEGRAM_BOT_TOKEN,
                        ENV_DISCORD_WEBHOOK_URL
                    );
                });
            }
        }
    }

    /// Saves to [`Self::default_path`] with [`Self::default_stores`].
    pub fn save_default(&mut self) -> Result<(), IntegrationsError> {
        self.save(&Self::default_path(), &Self::default_stores())
    }

    /// Stores changed secrets in `stores` (verified by read-back), then
    /// writes `path` atomically (0600) with non-secret fields and secret
    /// locations only. Fails, writing nothing, if a secret cannot be stored.
    pub fn save(&mut self, path: &Path, stores: &SecretStores) -> Result<(), IntegrationsError> {
        self.persist_secrets(stores)?;
        self.write_file(path)
    }

    fn write_file(&self, path: &Path) -> Result<(), IntegrationsError> {
        // Serialization never emits secret values (see `StoredSecret`).
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| IntegrationsError(format!("cannot serialize integrations: {}", e)))?;
        if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
            crate::config::create_dir_secure(dir).map_err(|e| {
                IntegrationsError(format!("cannot create {}: {}", dir.display(), e))
            })?;
        }
        crate::config::write_file_atomic_secure(path, json.as_bytes())
            .map_err(|e| IntegrationsError(format!("cannot write {}: {}", path.display(), e)))
    }

    /// (integration, store key, env override, secret) for each configured
    /// integration.
    fn secrets(&self) -> Vec<(&'static str, &'static str, &'static str, &StoredSecret)> {
        let fields = [
            self.email.as_ref().map(|e| &e.sender_password),
            self.slack.as_ref().map(|s| &s.webhook_url),
            self.telegram.as_ref().map(|t| &t.bot_token),
            self.discord.as_ref().map(|d| &d.webhook_url),
        ];
        SLOTS
            .iter()
            .zip(fields)
            .filter_map(|(&(n, k, e), f)| f.map(|f| (n, k, e, f)))
            .collect()
    }

    fn secrets_mut(&mut self) -> Vec<(&'static str, &'static str, &mut StoredSecret)> {
        let fields = [
            self.email.as_mut().map(|e| &mut e.sender_password),
            self.slack.as_mut().map(|s| &mut s.webhook_url),
            self.telegram.as_mut().map(|t| &mut t.bot_token),
            self.discord.as_mut().map(|d| &mut d.webhook_url),
        ];
        SLOTS
            .iter()
            .zip(fields)
            .filter_map(|(&(n, k, _), f)| f.map(|f| (n, k, f)))
            .collect()
    }

    /// True if a secret is (still) held in plaintext / not yet stored.
    pub fn has_plaintext_secrets(&self) -> bool {
        self.secrets().iter().any(|(_, _, _, s)| s.needs_persist())
    }

    fn persist_secrets(&mut self, stores: &SecretStores) -> Result<(), IntegrationsError> {
        for (name, key, secret) in self.secrets_mut() {
            if !secret.dirty {
                continue;
            }
            if secret.value.is_empty() {
                stores.delete(key).map_err(|e| {
                    IntegrationsError(format!("cannot remove the {} secret: {}", name, e))
                })?;
                secret.location = None;
            } else {
                let loc = stores.store(key, secret.value.as_str()).map_err(|e| {
                    IntegrationsError(format!("cannot store the {} secret securely: {}", name, e))
                })?;
                secret.location = Some(loc);
            }
            secret.dirty = false;
        }
        Ok(())
    }

    fn resolve_secrets(&mut self, stores: &SecretStores) {
        for (name, key, secret) in self.secrets_mut() {
            let Some(loc) = secret.location else { continue };
            if secret.dirty || !secret.value.is_empty() {
                continue;
            }
            match stores.load(loc, key) {
                Ok(Some(v)) => secret.value = v,
                Ok(None) => tracing::warn!(
                    "The {} notification secret is missing from the {}",
                    name,
                    loc
                ),
                Err(e) => tracing::warn!(
                    "Cannot read the {} notification secret from the {}: {}",
                    name,
                    loc,
                    e
                ),
            }
        }
    }

    /// Where each configured integration's secret comes from.
    pub fn status(&self) -> Vec<IntegrationStatus> {
        self.status_with(&env_lookup)
    }

    pub fn status_with(&self, env: &dyn Fn(&str) -> Option<String>) -> Vec<IntegrationStatus> {
        let enabled = [
            self.email.as_ref().map(|e| e.enabled),
            self.slack.as_ref().map(|s| s.enabled),
            self.telegram.as_ref().map(|t| t.enabled),
            self.discord.as_ref().map(|d| d.enabled),
        ];
        let flags: Vec<bool> = enabled.into_iter().flatten().collect();
        self.secrets()
            .into_iter()
            .zip(flags)
            .map(|((name, _, env_name, s), enabled)| {
                let secret = if env(env_name).is_some_and(|v| !v.is_empty()) {
                    SecretSource::Env(env_name)
                } else if s.needs_persist() && !s.is_empty() {
                    SecretSource::LegacyPlaintext
                } else {
                    match (s.location(), s.is_empty()) {
                        (Some(l), false) => SecretSource::Stored(l),
                        (Some(l), true) => SecretSource::Unreadable(l),
                        (None, _) => SecretSource::None,
                    }
                };
                IntegrationStatus {
                    name,
                    enabled,
                    secret,
                }
            })
            .collect()
    }

    /// Removes every integration secret from `stores` (for `config reset`).
    pub fn delete_all_secrets(stores: &SecretStores) -> Result<(), IntegrationsError> {
        for (name, key, _) in SLOTS {
            stores.delete(key).map_err(|e| {
                IntegrationsError(format!("cannot remove the {} secret: {}", name, e))
            })?;
        }
        Ok(())
    }

    /// SDK email configuration, if email is enabled.
    pub fn to_email_config(&self) -> Option<EmailConfig> {
        self.to_email_config_with(&env_lookup)
    }

    /// [`Self::to_email_config`] with an explicit environment lookup.
    pub fn to_email_config_with(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Option<EmailConfig> {
        self.email
            .as_ref()
            .filter(|e| e.enabled)
            .map(|e| EmailConfig {
                smtp_server: e.smtp_server.clone(),
                smtp_port: e.smtp_port,
                sender_email: e.sender_email.clone(),
                sender_password: effective(&e.sender_password, ENV_SMTP_PASSWORD, env)
                    .unwrap_or_default(),
                recipient_email: e.recipient_email.clone(),
                use_tls: e.use_tls,
            })
    }

    /// SDK chat integrations that are enabled and have their secret.
    pub fn to_chat_integrations(&self) -> Vec<ChatIntegration> {
        self.to_chat_integrations_with(&env_lookup)
    }

    /// [`Self::to_chat_integrations`] with an explicit environment lookup.
    pub fn to_chat_integrations_with(
        &self,
        env: &dyn Fn(&str) -> Option<String>,
    ) -> Vec<ChatIntegration> {
        let mut integrations = Vec::new();
        let missing = |name: &str, var: &str| {
            tracing::warn!(
                "{} notifications are enabled but their secret is unavailable (stored secret unreadable and {} unset); skipping",
                name,
                var
            );
        };

        if let Some(slack) = self.slack.as_ref().filter(|s| s.enabled) {
            match effective(&slack.webhook_url, ENV_SLACK_WEBHOOK_URL, env) {
                Some(webhook_url) => integrations.push(ChatIntegration::Slack {
                    webhook_url,
                    mention_user: slack.mention_user.clone(),
                }),
                None => missing("Slack", ENV_SLACK_WEBHOOK_URL),
            }
        }

        if let Some(telegram) = self.telegram.as_ref().filter(|t| t.enabled) {
            match effective(&telegram.bot_token, ENV_TELEGRAM_BOT_TOKEN, env) {
                Some(bot_token) => integrations.push(ChatIntegration::Telegram {
                    bot_token,
                    chat_id: telegram.chat_id.clone(),
                }),
                None => missing("Telegram", ENV_TELEGRAM_BOT_TOKEN),
            }
        }

        if let Some(discord) = self.discord.as_ref().filter(|d| d.enabled) {
            match effective(&discord.webhook_url, ENV_DISCORD_WEBHOOK_URL, env) {
                Some(webhook_url) => integrations.push(ChatIntegration::Discord { webhook_url }),
                None => missing("Discord", ENV_DISCORD_WEBHOOK_URL),
            }
        }

        integrations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PASS: &str = "smtp-pass-0123456789";
    const SLACK: &str = "https://hooks.slack.com/services/T000/B000/SLACKSECRET";
    const TG: &str = "123456789:TELEGRAMSECRETTOKEN";
    const DISCORD: &str = "https://discord.com/api/webhooks/1/DISCORDSECRET";
    const ALL: [&str; 4] = [PASS, SLACK, TG, DISCORD];

    fn legacy_json() -> String {
        serde_json::json!({
            "email": {
                "enabled": true, "smtp_server": "smtp.example.com", "smtp_port": 587,
                "sender_email": "a@example.com", "sender_password": PASS,
                "recipient_email": "b@example.com", "use_tls": true
            },
            "slack": { "enabled": true, "webhook_url": SLACK, "mention_user": "U1" },
            "telegram": { "enabled": true, "bot_token": TG, "chat_id": "-100" },
            "discord": { "enabled": false, "webhook_url": DISCORD }
        })
        .to_string()
    }

    fn keyring_only() -> SecretStores {
        SecretStores::new(vec![Box::new(MemoryBackend::new(SecretLocation::Keyring))])
    }

    fn no_env(_: &str) -> Option<String> {
        None
    }

    fn assert_no_secrets(text: &str) {
        for s in ALL {
            assert!(!text.contains(s), "secret leaked into: {}", text);
        }
    }

    fn write_legacy(dir: &Path) -> PathBuf {
        let path = dir.join(".rs-shield").join(INTEGRATIONS_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        crate::config::write_file_secure(&path, legacy_json().as_bytes()).unwrap();
        path
    }

    #[test]
    fn plaintext_file_is_migrated_and_rewritten_without_secrets() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_legacy(dir.path());
        let stores = keyring_only();

        let cfg = IntegrationConfig::load(&path, &stores);

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_no_secrets(&raw);
        assert!(raw.contains("\"stored\": \"keyring\""), "{}", raw);
        assert!(raw.contains("smtp.example.com") && raw.contains("-100"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        // No temp files left next to it.
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );

        // The secrets are in the store and still usable.
        assert_eq!(
            stores
                .load(SecretLocation::Keyring, KEY_EMAIL)
                .unwrap()
                .unwrap()
                .as_str(),
            PASS
        );
        assert!(!cfg.has_plaintext_secrets());
        assert_eq!(
            cfg.to_email_config_with(&no_env).unwrap().sender_password,
            PASS
        );
        assert_eq!(cfg.to_chat_integrations_with(&no_env).len(), 2);

        // A second load reads the references back.
        let again = IntegrationConfig::load(&path, &stores);
        assert_eq!(
            again.discord.as_ref().unwrap().webhook_url.expose(),
            DISCORD
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), raw);
    }

    #[test]
    fn without_any_secure_store_the_plaintext_file_is_left_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_legacy(dir.path());
        let before = std::fs::read_to_string(&path).unwrap();
        let stores = SecretStores::new(vec![
            Box::new(MemoryBackend::unavailable(SecretLocation::Keyring)),
            Box::new(EncryptedFileBackend::new(
                dir.path().join(".rs-shield").join(SECRETS_FILE),
                None,
            )),
        ]);

        let cfg = IntegrationConfig::load(&path, &stores);

        // Never lose a secret: file unchanged, notifications keep working.
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        assert!(cfg.has_plaintext_secrets());
        assert_eq!(cfg.to_chat_integrations_with(&no_env).len(), 2);
        assert!(
            cfg.status_with(&no_env)
                .iter()
                .all(|s| s.secret == SecretSource::LegacyPlaintext)
        );
    }

    #[test]
    fn no_keyring_falls_back_to_the_encrypted_file() {
        let dir = tempfile::tempdir().unwrap();
        // One secret only: every encrypted-file write/read-back runs PBKDF2
        // 600k, which is slow in unoptimized test builds.
        let path = dir.path().join(".rs-shield").join(INTEGRATIONS_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let legacy = serde_json::json!({
            "telegram": { "enabled": true, "bot_token": TG, "chat_id": "-100" }
        });
        std::fs::write(&path, legacy.to_string()).unwrap();
        let enc = dir.path().join(".rs-shield").join(SECRETS_FILE);
        let stores = || {
            SecretStores::new(vec![
                Box::new(MemoryBackend::unavailable(SecretLocation::Keyring)),
                Box::new(EncryptedFileBackend::new(
                    &enc,
                    Some("test passphrase".into()),
                )),
            ])
        };

        IntegrationConfig::load(&path, &stores());

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_no_secrets(&raw);
        assert!(raw.contains("\"stored\": \"encrypted_file\""), "{}", raw);
        assert_no_secrets(&std::fs::read_to_string(&enc).unwrap());

        let cfg = IntegrationConfig::load(&path, &stores());
        assert_eq!(cfg.telegram.as_ref().unwrap().bot_token.expose(), TG);
        assert_eq!(cfg.to_chat_integrations_with(&no_env).len(), 1);
        assert_eq!(
            cfg.status_with(&no_env)[0].secret,
            SecretSource::Stored(SecretLocation::EncryptedFile)
        );
    }

    #[test]
    fn save_round_trip_update_and_clear() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join(INTEGRATIONS_FILE);
        let stores = keyring_only();

        let mut cfg = IntegrationConfig::load(&path, &stores);
        assert!(cfg.to_chat_integrations_with(&no_env).is_empty());
        cfg.discord = Some(DiscordIntegrationConfig {
            enabled: true,
            webhook_url: StoredSecret::new(DISCORD),
        });
        cfg.save(&path, &stores).unwrap();
        assert_no_secrets(&std::fs::read_to_string(&path).unwrap());

        let mut back = IntegrationConfig::load(&path, &stores);
        assert_eq!(back.to_chat_integrations_with(&no_env).len(), 1);

        // Unchanged form value: nothing is rewritten or deleted.
        let d = back.discord.as_mut().unwrap();
        d.webhook_url = d.webhook_url.updated(DISCORD);
        assert!(!d.webhook_url.needs_persist());

        // Clearing removes it from the store and the reference from the file.
        d.webhook_url = d.webhook_url.updated("");
        back.save(&path, &stores).unwrap();
        assert!(
            stores
                .load(SecretLocation::Keyring, KEY_DISCORD)
                .unwrap()
                .is_none()
        );
        assert!(!std::fs::read_to_string(&path).unwrap().contains("stored"));
    }

    #[test]
    fn unreadable_secret_is_not_deleted_by_an_untouched_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INTEGRATIONS_FILE);
        let mut cfg = IntegrationConfig {
            slack: Some(SlackIntegrationConfig {
                enabled: true,
                webhook_url: StoredSecret::new(SLACK),
                mention_user: None,
            }),
            ..Default::default()
        };
        cfg.save(&path, &keyring_only()).unwrap();

        // Keyring locked now: the secret reads back empty.
        let locked = SecretStores::new(vec![Box::new(MemoryBackend::unavailable(
            SecretLocation::Keyring,
        ))]);
        let mut loaded = IntegrationConfig::load(&path, &locked);
        let s = loaded.slack.as_mut().unwrap();
        assert!(s.webhook_url.is_empty());
        s.webhook_url = s.webhook_url.updated("");
        loaded.save(&path, &locked).unwrap();
        assert!(std::fs::read_to_string(&path).unwrap().contains("keyring"));
        assert_eq!(
            loaded.status_with(&no_env)[0].secret,
            SecretSource::Unreadable(SecretLocation::Keyring)
        );
        // Nothing to send without the secret.
        assert!(loaded.to_chat_integrations_with(&no_env).is_empty());
    }

    #[test]
    fn env_overrides_win() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_legacy(dir.path());
        let cfg = IntegrationConfig::load(&path, &keyring_only());
        let env = |name: &str| match name {
            ENV_SMTP_PASSWORD => Some("from-env-pass".to_string()),
            ENV_TELEGRAM_BOT_TOKEN => Some("1:from-env-token".to_string()),
            _ => None,
        };
        assert_eq!(
            cfg.to_email_config_with(&env).unwrap().sender_password,
            "from-env-pass"
        );
        let chats = cfg.to_chat_integrations_with(&env);
        assert!(chats.iter().any(|c| matches!(
            c,
            ChatIntegration::Telegram { bot_token, .. } if bot_token == "1:from-env-token"
        )));
        assert_eq!(
            cfg.status_with(&env)[0].secret,
            SecretSource::Env(ENV_SMTP_PASSWORD)
        );
    }

    #[test]
    fn debug_and_errors_are_redacted() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_legacy(dir.path());
        let unavailable = SecretStores::new(vec![Box::new(MemoryBackend::unavailable(
            SecretLocation::Keyring,
        ))]);
        let mut cfg = IntegrationConfig::load(&path, &unavailable);
        assert_no_secrets(&format!("{:?}", cfg));
        assert_no_secrets(&format!("{:?}", cfg.to_email_config_with(&no_env)));
        assert_no_secrets(&format!("{:?}", cfg.to_chat_integrations_with(&no_env)));
        let err = cfg.save(&path, &unavailable).unwrap_err();
        assert_no_secrets(&err.to_string());
        assert_no_secrets(&format!("{:?}", err));
        // Serializing an unsaved config never emits the plaintext either.
        assert_no_secrets(&serde_json::to_string(&cfg).unwrap());
    }

    #[test]
    fn invalid_file_loads_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(INTEGRATIONS_FILE);
        std::fs::write(&path, "{\"email\": {\"smtp_port\": \"hunter2\"}}").unwrap();
        let cfg = IntegrationConfig::load(&path, &keyring_only());
        assert!(cfg.email.is_none());
        assert!(IntegrationConfig::default_path().ends_with(".rs-shield/integrations.json"));
    }
}
