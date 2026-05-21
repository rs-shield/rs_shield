use ax_auth::{Router, routing::{get, post}, extract::State, Json};
use crate::credentials::Fido2Manager;
use std::sync::Arc;
use tokio::sync::Mutex;

pub type SharedFido2Manager = Arc<Mutex<Fido2Manager>>;

/// Cria o Router Axum para a cerimônia FIDO2.
/// Agora qualquer interface (CLI ou Desktop) pode hospedar o login.
pub fn create_fido2_router(manager: SharedFido2Manager) -> Router {
    use crate::credentials::fido2_handlers::*; // Handlers movidos do CLI para cá
    
    Router::new()
        .route("/register/start", post(register_start))
        .route("/register/finish", post(register_finish))
        .route("/auth/start", post(auth_start))
        .route("/auth/finish", post(auth_finish))
        .with_state(manager)
}

pub async fn start_fido2_ceremony(manager: SharedFido2Manager, port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let app = create_fido2_router(manager);
    let listener = tokio::net::TcpListener::bind(format!("127.0.0.1:{}", port)).await?;
    axum::serve(listener, app).await?;
    Ok(())
}