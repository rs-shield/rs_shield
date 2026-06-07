use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct ServerConfig {
    pub database_url: String,
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            database_url: std::env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://user:pass@localhost/rs_shield".to_string()),
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}
