use crate::auth::{self, SessionStatus, SessionStore};

use crate::credentials;
use crate::server::handlers::{
    AuthHandlerState, auth_finish, auth_localhost_callback, auth_logout, auth_recovery,
    auth_refresh, auth_start, auth_verify, device_flow_lookup_code, device_flow_page,
    device_flow_start, device_flow_start_api, device_flow_token, device_flow_verify_page, home,
};
use axum::{
    Router,
    routing::{get, post},
};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Porta por omissão do servidor de autenticação local.
pub const DEFAULT_SERVER_PORT: u16 = 3000;

/// Porta do servidor de autenticação: `RSB_SERVER_PORT` ou 3000.
pub fn server_port_from_env() -> u16 {
    std::env::var("RSB_SERVER_PORT")
        .ok()
        .and_then(|v| v.trim().parse::<u16>().ok())
        .filter(|p| *p != 0)
        .unwrap_or(DEFAULT_SERVER_PORT)
}

pub fn create_auth_router<S: SessionStore + Clone + 'static>(state: AuthHandlerState<S>) -> Router {
    Router::new()
        .route("/", get(home::<S>))
        .route("/auth/device/start", get(device_flow_page::<S>))
        .route("/api/auth/start", post(auth_start::<S>))
        .route("/api/auth/finish", post(auth_finish::<S>))
        .route("/api/auth/refresh", post(auth_refresh::<S>))
        .route("/api/auth/recovery", post(auth_recovery::<S>))
        .route("/api/auth/logout", post(auth_logout::<S>))
        .route(
            "/api/auth/localhost-callback",
            post(auth_localhost_callback::<S>),
        )
        .route("/api/auth/verify", get(auth_verify::<S>))
        .route("/auth/device/start", post(device_flow_start::<S>))
        .route("/auth/device/start-api", post(device_flow_start_api::<S>))
        .route("/auth/device/verify", get(device_flow_verify_page::<S>))
        .route(
            "/api/auth/device/lookup-code",
            post(device_flow_lookup_code::<S>),
        )
        .route("/auth/device/token", post(device_flow_token::<S>))
        .with_state(state)
}

/// Absolute audit log path outside the repo (fail-closed, no CWD dependence).
pub fn default_audit_log_path() -> String {
    if let Some(home) = dirs::home_dir() {
        return home
            .join(".rs-shield")
            .join("audit.log")
            .to_string_lossy()
            .into_owned();
    }
    "audit.log".to_string()
}

/// Start auth server for integrated login flow
pub async fn start_auth_server(
    port: u16,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    use auth::{JwtManager, PersistentSessionStore, SessionRegistry};
    use credentials::Fido2Manager;
    use std::collections::HashMap;
    use tracing::info;

    let origin = format!("http://localhost:{}", port);
    let mut fido2_mgr = Fido2Manager::new(&origin, "localhost", "RSB Shield")?;

    // Falhar se o ficheiro existir mas não puder ser carregado (ex.: DEK em
    // falta): continuar com estado vazio arriscaria sobrescrever credenciais.
    let storage_path = Fido2Manager::default_storage_path()?;
    fido2_mgr.load_from_file(&storage_path)?;

    let jwt_mgr = JwtManager::from_env()?;
    // Sessões persistidas para que a CLI valide o JTI sem este servidor ativo.
    let session_store = Arc::new(PersistentSessionStore::new(
        SessionRegistry::default_registry()?,
    ));
    let rate_limiter = Arc::new(auth::RateLimiter::new(5, 60));
    let audit_logger = Arc::new(auth::AuditLogger::new(Some(default_audit_log_path())));
    let device_flows = Arc::new(Mutex::new(HashMap::new()));

    // Periodic cleanup of expired sessions, rate-limit windows and device flows.
    {
        let store = session_store.clone();
        let limiter = rate_limiter.clone();
        let flows = device_flows.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let _ = store.cleanup_expired().await;
                limiter.cleanup_expired().await;
                let now = chrono::Utc::now();
                flows.lock().await.retain(
                    |_, f: &mut crate::server::handlers::DeviceFlowStatus| f.expires_at > now,
                );
            }
        });
    }

    let auth_state = AuthHandlerState {
        jwt_manager: Arc::new(jwt_mgr),
        fido2_manager: Arc::new(Mutex::new(fido2_mgr)),
        session_store,
        device_flows,
        rate_limiter,
        audit_logger,
        fido2_storage_path: storage_path,
        public_base_url: origin.clone(),
    };

    let app = create_auth_router(auth_state).layer(axum::middleware::from_fn_with_state(
        port,
        crate::server::middleware::local_host_guard_middleware,
    ));
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    info!("🚀 Auth server running on {}", origin);

    // Signal that server is ready
    let _ = ready_tx.send(());

    axum::serve(listener, app).await?;

    Ok(())
}

/// Verifica a autenticação FIDO2 da CLI.
///
/// Validação local: assinatura/exp/aud/iss do JWT + registo persistido de
/// sessões (`~/.rs-shield/sessions.json`, escrito pelo servidor ao emitir e
/// revogar). Não depende do servidor de login estar ativo.
///
/// A verificação remota é opcional: ativa com `RSB_AUTH_REMOTE_VERIFY=1`,
/// usando a porta `RSB_SERVER_PORT` (por omissão 3000).
pub async fn check_fido2_auth() -> Result<String, Box<dyn std::error::Error>> {
    let remote = std::env::var("RSB_AUTH_REMOTE_VERIFY")
        .map(|v| matches!(v.trim(), "1" | "true" | "yes"))
        .unwrap_or(false);
    let port = if remote {
        Some(server_port_from_env())
    } else {
        None
    };
    check_fido2_auth_with(port).await
}

/// Como `check_fido2_auth`, com verificação remota opcional na porta dada.
pub async fn check_fido2_auth_with(
    remote_port: Option<u16>,
) -> Result<String, Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("Home directory not found")?;
    let rsb_dir = home.join(".rs-shield");
    let auth_file = rsb_dir.join("auth_token");

    if !auth_file.exists() {
        return Err("❌ Not authenticated. Please run: rsb login <user_id>".into());
    }

    let token =
        fs::read_to_string(&auth_file).map_err(|_| "❌ Failed to read authentication token")?;

    let trimmed_token = token.trim();
    if trimmed_token.is_empty() {
        return Err("❌ Invalid authentication token".into());
    }

    let registry = auth::SessionRegistry::default_registry()?;
    verify_token_locally(trimmed_token, &auth::JwtManager::from_env()?, &registry)?;

    // Verificação remota opcional (ex.: servidor de longa duração).
    if let Some(port) = remote_port {
        let client = reqwest::Client::new();
        let res = client
            .get(format!("http://localhost:{}/api/auth/verify", port))
            .header("Authorization", format!("Bearer {}", trimmed_token))
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => {}
            Ok(_) => {
                return Err("❌ Session revoked or expired on server. Please login again: rsb login <USER_ID>".into());
            }
            Err(e) => {
                return Err(format!(
                    "❌ Auth server unreachable for remote verification (fail-closed). ({})",
                    e
                )
                .into());
            }
        }
    }

    Ok(trimmed_token.to_string())
}

/// Validação local (sem rede): JWT (assinatura, exp, aud, iss) + JTI ativo no
/// registo persistido de sessões e pertencente ao mesmo utilizador.
pub fn verify_token_locally(
    token: &str,
    jwt_mgr: &auth::JwtManager,
    registry: &auth::SessionRegistry,
) -> Result<auth::SessionClaims, Box<dyn std::error::Error>> {
    let claims = jwt_mgr.verify_token(token).map_err(|e| {
        format!(
            "❌ Session expired or invalid ({}). Please login again: rsb login <USER_ID>",
            e
        )
    })?;

    match registry.status(&claims.jti, &claims.sub)? {
        SessionStatus::Active => Ok(claims),
        SessionStatus::Revoked => Err("❌ Session revoked. Please login again: rsb login <USER_ID>".into()),
        SessionStatus::Expired => Err("❌ Session expired. Please login again: rsb login <USER_ID>".into()),
        SessionStatus::Unknown => Err(
            "❌ Session not recognized (not issued by this installation). Please login again: rsb login <USER_ID>"
                .into(),
        ),
    }
}
