use crate::fido2::{error::Fido2Error, models::StoredPasskey};
use chrono::Utc;
use serde_json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use webauthn_rs::prelude::*;

pub struct Fido2Manager {
    webauthn: Webauthn,
    credentials: HashMap<String, StoredPasskey>,

    reg_state: Option<(PasskeyRegistration, String, String, String)>,
    auth_state: Option<(PasskeyAuthentication, String)>,

    storage_path: Option<PathBuf>,
}

impl Fido2Manager {
    pub fn new(origin: &str, rp_id: &str, rp_name: &str) -> Result<Self, Fido2Error> {
        let webauthn = WebauthnBuilder::new(rp_id, origin)
            .map_err(|e| Fido2Error::Registration(e.to_string()))?
            .rp_name(rp_name)
            .build()
            .map_err(|e| Fido2Error::Registration(e.to_string()))?;

        Ok(Self {
            webauthn,
            credentials: HashMap::new(),
            reg_state: None,
            auth_state: None,
            storage_path: None,
        })
    }

    pub fn with_storage(mut self, path: impl AsRef<Path>) -> Result<Self, Fido2Error> {
        let path = path.as_ref().to_path_buf();
        self.storage_path = Some(path.clone());
        self.load(&path)?;
        Ok(self)
    }

    // =========================
    // STORAGE
    // =========================

    pub fn load(&mut self, path: &Path) -> Result<(), Fido2Error> {
        if !path.exists() {
            // If file doesn't exist, that's OK on first run
            return Ok(());
        }

        match std::fs::read_to_string(path) {
            Ok(data) => match serde_json::from_str(&data) {
                Ok(map) => {
                    self.credentials = map;
                    tracing::info!(
                        "Loaded {} FIDO2 credentials from {:?}",
                        self.credentials.len(),
                        path
                    );
                    Ok(())
                }
                Err(e) => {
                    tracing::warn!("Failed to parse FIDO2 storage: {}", e);
                    Err(Fido2Error::Registration(format!(
                        "Invalid credentials file: {}",
                        e
                    )))
                }
            },
            Err(e) => {
                tracing::warn!("Failed to read FIDO2 storage: {}", e);
                Err(Fido2Error::Registration(format!(
                    "Failed to read credentials file: {}",
                    e
                )))
            }
        }
    }

    pub fn save(&self) -> Result<(), Fido2Error> {
        if let Some(path) = &self.storage_path {
            self.save_to(path)
        } else {
            Ok(())
        }
    }

    pub fn save_to(&self, path: &Path) -> Result<(), Fido2Error> {
        match serde_json::to_string_pretty(&self.credentials) {
            Ok(json) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                std::fs::write(path, json).map_err(|e| {
                    Fido2Error::Registration(format!("Failed to save credentials: {}", e))
                })?;
                tracing::info!("Saved FIDO2 credentials to {:?}", path);
                Ok(())
            }
            Err(e) => Err(Fido2Error::Registration(format!(
                "Failed to serialize credentials: {}",
                e
            ))),
        }
    }

    // =========================
    // REGISTER
    // =========================

    pub fn start_registration(
        &mut self,
        user_id: &str,
        username: &str,
        display: &str,
    ) -> Result<CreationChallengeResponse, Fido2Error> {
        // Validate inputs
        if user_id.is_empty() || user_id.len() > 255 {
            return Err(Fido2Error::Registration("Invalid user_id length".into()));
        }
        if username.is_empty() || username.len() > 255 {
            return Err(Fido2Error::Registration("Invalid username length".into()));
        }
        if display.is_empty() || display.len() > 255 {
            return Err(Fido2Error::Registration(
                "Invalid display_name length".into(),
            ));
        }

        if self.credentials.contains_key(user_id) {
            return Err(Fido2Error::AlreadyExists);
        }

        let user_uuid = uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_DNS, user_id.as_bytes());

        let (challenge, state) = self
            .webauthn
            .start_passkey_registration(user_uuid, username, display, None)
            .map_err(|e| Fido2Error::Registration(e.to_string()))?;

        self.reg_state = Some((state, user_id.into(), username.into(), display.into()));

        Ok(challenge)
    }

    pub fn finish_registration(
        &mut self,
        response: RegisterPublicKeyCredential,
    ) -> Result<(), Fido2Error> {
        let (state, user_id, username, display) =
            self.reg_state.take().ok_or(Fido2Error::InvalidState)?;

        let passkey = self
            .webauthn
            .finish_passkey_registration(&response, &state)
            .map_err(|e| Fido2Error::Registration(e.to_string()))?;

        let stored = StoredPasskey {
            user_id: user_id.clone(),
            username,
            display_name: display,
            passkey: serde_json::to_vec(&passkey).unwrap(),
            counter: 0,
            created_at: Utc::now().to_rfc3339(),
            last_used: None,
        };

        self.credentials.insert(user_id.clone(), stored);

        // Auto-save after registration
        self.save()?;

        tracing::info!("Registered FIDO2 credential for user: {}", user_id);
        Ok(())
    }

    // =========================
    // AUTH
    // =========================

    pub fn start_authentication(
        &mut self,
        user_id: &str,
    ) -> Result<RequestChallengeResponse, Fido2Error> {
        if user_id.is_empty() {
            return Err(Fido2Error::Registration("Invalid user_id".into()));
        }

        let cred = self.credentials.get(user_id).ok_or(Fido2Error::NotFound)?;

        let passkey: Passkey = serde_json::from_slice(&cred.passkey)
            .map_err(|_| Fido2Error::Authentication("Corrupted key".into()))?;

        let (challenge, state) = self
            .webauthn
            .start_passkey_authentication(&[passkey])
            .map_err(|e| Fido2Error::Authentication(e.to_string()))?;

        self.auth_state = Some((state, user_id.into()));

        Ok(challenge)
    }

    pub fn finish_authentication(
        &mut self,
        response: PublicKeyCredential,
    ) -> Result<String, Fido2Error> {
        let (state, user_id) = self.auth_state.take().ok_or(Fido2Error::InvalidState)?;

        let result = self
            .webauthn
            .finish_passkey_authentication(&response, &state)
            .map_err(|e| Fido2Error::Authentication(e.to_string()))?;

        if let Some(c) = self.credentials.get_mut(&user_id) {
            c.counter = result.counter();
            c.last_used = Some(Utc::now().to_rfc3339());

            // Auto-save after authentication
            let _ = self.save();
        }

        tracing::info!("Authenticated FIDO2 credential for user: {}", user_id);
        Ok(user_id)
    }

    pub fn list_credentials(&self) -> Vec<StoredPasskey> {
        self.credentials.values().cloned().collect()
    }

    pub fn delete_credential(&mut self, user_id: &str) -> Result<(), Fido2Error> {
        self.credentials
            .remove(user_id)
            .ok_or(Fido2Error::NotFound)?;

        // Auto-save after deletion
        self.save()?;

        tracing::info!("Deleted FIDO2 credential for user: {}", user_id);
        Ok(())
    }

    pub fn get_credential(&self, user_id: &str) -> Result<&StoredPasskey, Fido2Error> {
        self.credentials.get(user_id).ok_or(Fido2Error::NotFound)
    }
}
