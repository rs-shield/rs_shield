use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::get,
    Router,
};
use serde_json::json;
use std::net::SocketAddr;
use tracing_subscriber;

#[tokio::main]
async fn main() {
    // Inicializa logging
    tracing_subscriber::fmt::init();

    tracing::info!("🚀 Iniciando RS Shield Server...");

    // Router básico
    let app = Router::new()
        .route("/health", get(health_handler))
        .route("/api/files", get(list_files_handler));

    let addr = SocketAddr::from(([0, 0, 0, 0], 8080));
    tracing::info!("Servidor rodando em http://{}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "version": "0.1.0-alpha.4",
        "service": "rsb-server"
    }))
}

async fn list_files_handler(State(_state): State<()>) -> Json<serde_json::Value> {
    // TODO: Integrar com rsb-sdk para listar arquivos reais
    Json(json!({
        "files": [],
        "message": "Integração com rsb-sdk em progresso"
    }))
}
