/// Session Store - In-memory store with optional Redis backend
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub user_id: String,
    pub jti: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub revoked: bool,
    pub ip_address: Option<String>,
    pub user_agent: Option<String>,
    pub fido2_counter: u32,
    pub scopes: Vec<String>,
}

#[async_trait::async_trait]
pub trait SessionStore: Send + Sync {
    async fn save(&self, session: Session) -> Result<(), String>;
    async fn get(&self, jti: &str) -> Result<Option<Session>, String>;
    async fn revoke(&self, jti: &str) -> Result<(), String>;
    async fn cleanup_expired(&self) -> Result<usize, String>;
}

/// In-memory session store
#[derive(Clone)]
pub struct InMemorySessionStore {
    sessions: Arc<RwLock<HashMap<String, Session>>>,
}

impl InMemorySessionStore {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
        }
    }
}

#[async_trait::async_trait]
impl SessionStore for InMemorySessionStore {
    async fn save(&self, session: Session) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;
        sessions.insert(session.jti.clone(), session);
        Ok(())
    }

    async fn get(&self, jti: &str) -> Result<Option<Session>, String> {
        let sessions = self.sessions.read().await;

        if let Some(session) = sessions.get(jti) {
            // Check if expired
            if session.expires_at < Utc::now() {
                return Ok(None);
            }

            if session.revoked {
                return Ok(None);
            }

            return Ok(Some(session.clone()));
        }

        Ok(None)
    }

    async fn revoke(&self, jti: &str) -> Result<(), String> {
        let mut sessions = self.sessions.write().await;

        if let Some(session) = sessions.get_mut(jti) {
            session.revoked = true;
        }

        Ok(())
    }

    async fn cleanup_expired(&self) -> Result<usize, String> {
        let mut sessions = self.sessions.write().await;
        let now = Utc::now();
        let initial_count = sessions.len();

        sessions.retain(|_, session| session.expires_at > now && !session.revoked);

        let cleaned = initial_count - sessions.len();
        Ok(cleaned)
    }
}

impl Default for InMemorySessionStore {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_save_and_get() {
        let store = InMemorySessionStore::new();
        let session = Session {
            user_id: "user1".to_string(),
            jti: "jti1".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(15),
            revoked: false,
            ip_address: None,
            user_agent: None,
            fido2_counter: 0,
            scopes: vec!["backup".to_string()],
        };

        store.save(session.clone()).await.unwrap();
        let retrieved = store.get("jti1").await.unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().user_id, "user1");
    }

    #[tokio::test]
    async fn test_revoke() {
        let store = InMemorySessionStore::new();
        let session = Session {
            user_id: "user1".to_string(),
            jti: "jti1".to_string(),
            created_at: Utc::now(),
            expires_at: Utc::now() + chrono::Duration::minutes(15),
            revoked: false,
            ip_address: None,
            user_agent: None,
            fido2_counter: 0,
            scopes: vec!["backup".to_string()],
        };

        store.save(session).await.unwrap();
        store.revoke("jti1").await.unwrap();
        let retrieved = store.get("jti1").await.unwrap();

        assert!(retrieved.is_none());
    }
}
