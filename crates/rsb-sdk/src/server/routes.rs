use crate::auth::{self, SessionStore};
    
use crate::server::handlers::{
    AuthHandlerState, auth_finish, auth_localhost_callback, auth_logout, auth_refresh, auth_start,
    auth_verify, device_flow_lookup_code, device_flow_page, device_flow_start,
    device_flow_start_api, device_flow_token, device_flow_verify_page, home,
};
use crate::credentials;
use axum::{
    Router,
    routing::{get, post},
};
use std::fs;
use std::sync::Arc;
use tokio::sync::Mutex;

pub fn create_auth_router<S: SessionStore + Clone + 'static>(state: AuthHandlerState<S>) -> Router {
    Router::new()
        .route("/", get(home::<S>))
        .route("/auth/device/start", get(device_flow_page::<S>))
        .route("/api/auth/start", post(auth_start::<S>))
        .route("/api/auth/finish", post(auth_finish::<S>))
        .route("/api/auth/refresh", post(auth_refresh::<S>))
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

/// Start auth server for integrated login flow
pub async fn start_auth_server(
    port: u16,
    ready_tx: tokio::sync::oneshot::Sender<()>,
) -> Result<(), Box<dyn std::error::Error>> {
    use auth::{InMemorySessionStore, JwtManager};
    use credentials::Fido2Manager;
    use std::collections::HashMap;
    use tracing::info;

    let origin = format!("http://localhost:{}", port);
    let mut fido2_mgr = Fido2Manager::new(&origin, "localhost", "RSB Shield")?;

    if let Ok(storage_path) = Fido2Manager::default_storage_path() {
        if storage_path.exists() {
            let _ = fido2_mgr.load_from_file(&storage_path);
        }
    }

    let jwt_mgr = JwtManager::new("rsb-shield-secret-key-256bit")?;
    let session_store = Arc::new(InMemorySessionStore::new());
    let rate_limiter = Arc::new(auth::RateLimiter::new(5, 60));
    let audit_logger = Arc::new(auth::AuditLogger::new(Some(
        "audit.log".to_string(),
    )));

    let auth_state = AuthHandlerState {
        jwt_manager: Arc::new(jwt_mgr),
        fido2_manager: Arc::new(Mutex::new(fido2_mgr)),
        session_store,
        device_flows: Arc::new(Mutex::new(HashMap::new())),
        rate_limiter,
        audit_logger,
    };

    let app = create_auth_router(auth_state);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;

    info!("🚀 Auth server running on http://localhost:{}", port);

    // Signal that server is ready
    let _ = ready_tx.send(());

    axum::serve(listener, app).await?;

    Ok(())
}


pub async fn check_fido2_auth() -> Result<String, Box<dyn std::error::Error>> {
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

        // 1. Validação Local (JWT Signature & Expiration)
        let jwt_mgr = auth::JwtManager::new("rsb-shield-secret-key-256bit")?;
        jwt_mgr
            .verify_token(trimmed_token)
            .map_err(|e| format!("❌ Session expired or invalid: {}", e))?;

        // 2. Validação Remota (Consulta ao servidor para verificar revogação/JTI)
        let client = reqwest::Client::new();
        let res = client
            .get("http://localhost:3000/api/auth/verify")
            .header("Authorization", format!("Bearer {}", trimmed_token))
            .send()
            .await;

        match res {
            Ok(resp) if resp.status().is_success() => Ok(trimmed_token.to_string()),
            Ok(_) => Err("❌ Session revoked or expired on server. Please login again.".into()),
            Err(_) => {
                println!("⚠️ Auth server unreachable. Proceeding with local validation only.");
                Ok(trimmed_token.to_string())
            }
        }
    }
