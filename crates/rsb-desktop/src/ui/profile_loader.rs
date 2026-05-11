use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Expande paths especiais como ~ (home directory) e variáveis de ambiente ($VAR, ${VAR})
fn expand_path(path: &str) -> PathBuf {
    // Primeiro, expandir ~ se o path começar com ele
    let path_str = if path.starts_with("~") {
        if let Some(home) = dirs::home_dir() {
            path.replacen("~", &home.to_string_lossy(), 1)
        } else {
            path.to_string()
        }
    } else {
        path.to_string()
    };

    // Depois, expandir variáveis de ambiente ($VAR e ${VAR})
    let expanded = expand_env_vars(&path_str);

    PathBuf::from(expanded)
}

/// Expande variáveis de ambiente no formato $VAR ou ${VAR}
fn expand_env_vars(path: &str) -> String {
    let mut result = String::new();
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '$' {
            match chars.peek() {
                Some('{') => {
                    // ${VAR} format
                    chars.next(); // consume {
                    let mut var_name = String::new();

                    while let Some(&c) = chars.peek() {
                        if c == '}' {
                            chars.next(); // consume }
                            break;
                        }
                        var_name.push(c);
                        chars.next();
                    }

                    if let Ok(val) = std::env::var(&var_name) {
                        result.push_str(&val);
                    } else {
                        result.push('$');
                        result.push('{');
                        result.push_str(&var_name);
                        result.push('}');
                    }
                }
                Some(c) if c.is_alphanumeric() || *c == '_' => {
                    // $VAR format
                    let mut var_name = String::new();

                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            var_name.push(c);
                            chars.next();
                        } else {
                            break;
                        }
                    }

                    if let Ok(val) = std::env::var(&var_name) {
                        result.push_str(&val);
                    } else {
                        result.push('$');
                        result.push_str(&var_name);
                    }
                }
                _ => {
                    result.push('$');
                }
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub source_path: String,
    pub destination_path: String,
    pub exclude_patterns: Vec<String>,
    pub encryption_key: Option<String>,
    pub backup_mode: String,
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3: Option<S3Config>,
    pub encrypt_patterns: Option<Vec<String>>,
    pub pause_on_low_battery: Option<u8>,
    pub pause_on_high_cpu: Option<u8>,
    pub compression_level: Option<u8>,
}

pub fn load_profile(path: &Path) -> Result<Config, String> {
    let content =
        std::fs::read_to_string(path).map_err(|e| format!("Erro ao ler arquivo: {}", e))?;

    let mut config: Config =
        toml::from_str(&content).map_err(|e| format!("Erro ao parsear TOML: {}", e))?;

    // Expandir paths especiais como ~ e variáveis de ambiente
    config.source_path = expand_path(&config.source_path)
        .to_string_lossy()
        .into_owned();
    config.destination_path = expand_path(&config.destination_path)
        .to_string_lossy()
        .into_owned();

    Ok(config)
}

pub struct ProfileData {
    pub source_path: String,
    pub destination_path: String,
    pub exclude_patterns: String,
    pub encryption_key: String,
    pub backup_mode: String,
    pub s3_bucket: String,
    pub s3_region: String,
    pub s3_endpoint: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub encrypt_patterns: String,
    pub pause_on_low_battery: String,
    pub pause_on_high_cpu: String,
    pub compression_level: String,
}

impl From<Config> for ProfileData {
    fn from(cfg: Config) -> Self {
        let s3_bucket = cfg
            .s3
            .as_ref()
            .and_then(|s| s.bucket.clone())
            .or(cfg.s3_bucket)
            .unwrap_or_default();
        let s3_region = cfg
            .s3
            .as_ref()
            .and_then(|s| s.region.clone())
            .or(cfg.s3_region)
            .unwrap_or_default();
        let s3_endpoint = cfg
            .s3
            .as_ref()
            .and_then(|s| s.endpoint.clone())
            .or(cfg.s3_endpoint)
            .unwrap_or_default();
        let s3_access_key = cfg
            .s3
            .as_ref()
            .and_then(|s| s.access_key.clone())
            .unwrap_or_default();
        let s3_secret_key = cfg
            .s3
            .as_ref()
            .and_then(|s| s.secret_key.clone())
            .unwrap_or_default();

        ProfileData {
            source_path: cfg.source_path,
            destination_path: cfg.destination_path,
            exclude_patterns: cfg.exclude_patterns.join("\n"),
            encryption_key: cfg.encryption_key.unwrap_or_default(),
            backup_mode: cfg.backup_mode,
            s3_bucket,
            s3_region,
            s3_endpoint,
            s3_access_key,
            s3_secret_key,
            encrypt_patterns: cfg
                .encrypt_patterns
                .map(|v| v.join("\n"))
                .unwrap_or_default(),
            pause_on_low_battery: cfg
                .pause_on_low_battery
                .map(|v| v.to_string())
                .unwrap_or_default(),
            pause_on_high_cpu: cfg
                .pause_on_high_cpu
                .map(|v| v.to_string())
                .unwrap_or_default(),
            compression_level: cfg
                .compression_level
                .map(|v| v.to_string())
                .unwrap_or_default(),
        }
    }
}
