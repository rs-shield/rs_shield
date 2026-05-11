use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// JWT Claims - Stateless (sem server-side store)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionClaims {
    pub sub: String,         // user_id
    pub scopes: Vec<String>, // ["backup", "restore"]
    pub jti: String,         // JWT ID (unique session identifier)
    pub iat: i64,            // issued at (unix timestamp)
    pub exp: i64,            // expiration
    pub aud: String,         // "rsb-shield"
}

/// Credenciais para FIDO2 unlock
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthRequest {
    pub user_id: String,
    pub fido2_response: Option<String>,
    pub device_code: Option<String>,
}

/// Resposta de login bem-sucedido
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse {
    pub access_token: String, // JWT
    pub expires_in: i64,      // segundos (1800 = 30 min)
    pub token_type: String,   // "Bearer"
    pub user_id: String,
}

/// Localhost callback para integração CLI
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalhostCallbackRequest {
    pub token: String,
    pub state: Option<String>,
}

/// Auditlog entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub timestamp: DateTime<Utc>,
    pub user_id: String,
    pub event_type: String, // "auth_success", "auth_failure", "backup_start"
    pub status: String,     // "success", "failure"
    pub ip_address: String,
    pub user_agent: String,
    pub details: serde_json::Value,
}

/// Rate limit counter
#[derive(Debug, Clone)]
pub struct RateLimitCounter {
    pub user_id: String,
    pub counter: u32,
    pub reset_at: DateTime<Utc>,
}

/// Contexto de credenciais desbloqueadas
#[derive(Debug, Clone)]
pub struct UnlockedCredentialsContext {
    pub user_id: String,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}
