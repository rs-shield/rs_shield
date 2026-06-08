use axum::{routing::get, Router};
use std::net::SocketAddr;
use tracing_subscriber;

mod config;
mod db;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    let config = config::ServerConfig::default();
    let pool = db::init_db(&config).await.expect("Failed to connect to Postgres");

    let app = Router::new()
        .route("/health", get(|| async { "OK - RS Shield Server with Postgres" }));

    let addr = SocketAddr::from(([0, 0, 0, 0], config.port));
    println!("Server running on http://{}", addr);
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}
