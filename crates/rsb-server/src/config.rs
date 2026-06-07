use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub database_url: String,
    pub host: String,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            database_url: "sqlite://rs_shield.db".to_string(),
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}
