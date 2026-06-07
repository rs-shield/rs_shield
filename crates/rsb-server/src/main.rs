use axum::{routing::get, Router, extract::State};
use std::net::SocketAddr;
use tracing_subscriber;
use rsb_server::{ServerConfig, db::init_db};

#[derive(Clone)]
struct AppState {
    db: sqlx::SqlitePool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = ServerConfig::default();
    let db = init_db(&config.database_url).await.expect("Failed to initialize database");

    let state = AppState { db };

    let app = Router::new()
        .route("/health", get(health_handler))
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    println!("🚀 RS Shield Server running on http://{}/", addr);
    println!("📁 SQLite DB: {}", config.database_url);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn health_handler() -> &'static str {
    "✅ RS Shield Server is healthy! SQLite initialized. 🛡️"
}
