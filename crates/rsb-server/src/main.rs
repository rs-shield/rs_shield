use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use rsb_server::ServerConfig;
use serde_json::json;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber;

#[derive(Clone)]
struct AppState {
    config: Arc<ServerConfig>,
    // TODO: Adicionar pool de conexão com banco e sdk state
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    tracing::info!("🚀 Iniciando RS Shield Server (Drive-like)...");

    let config = Arc::new(ServerConfig::default());
    let state = AppState { config };

    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/files", get(list_files_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("✅ Servidor rodando em http://{}/health", addr);
    tracing::info!("📁 API de arquivos: http://{}/api/files", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "version": "0.1.0-alpha.4",
        "service": "rsb-server",
        "message": "Sistema de armazenamento tipo Google Drive em desenvolvimento"
    }))
}

async fn list_files_handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    // Placeholder - Integrar com rsb-sdk no futuro
    Json(json!({
        "files": [],
        "total": 0,
        "config": {
            "data_dir": state.config.data_dir,
            "port": state.config.port
        }
    }))
}
