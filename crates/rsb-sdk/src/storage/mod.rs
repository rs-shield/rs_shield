use crate::utils::ensure_directory_exists_async;
use async_trait::async_trait;
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::Client;
use aws_sdk_s3::config::Region;
use std::io::{self, ErrorKind};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[async_trait]
pub trait Storage: Send + Sync {
    async fn write(&self, path: &str, data: &[u8]) -> io::Result<()>;
    async fn read(&self, path: &str) -> io::Result<Vec<u8>>;
    async fn exists(&self, path: &str) -> io::Result<bool>;
    async fn list(&self, prefix: &str) -> io::Result<Vec<String>>;
    async fn delete(&self, path: &str) -> io::Result<()>;
}

#[derive(Clone, Debug)]
pub struct LocalStorage {
    base_path: PathBuf,
}

impl LocalStorage {
    pub fn new(path: &str) -> Self {
        Self {
            base_path: PathBuf::from(path),
        }
    }
}

#[async_trait]
impl Storage for LocalStorage {
    async fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        let full_path = self.base_path.join(path);
        if let Some(parent) = full_path.parent() {
            let parent_str = parent.to_string_lossy().to_string();
            ensure_directory_exists_async(&parent_str)
                .await
                .map_err(io::Error::other)?;
        }
        let mut file = fs::File::create(full_path).await?;
        file.write_all(data).await?;
        Ok(())
    }

    async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let full_path = self.base_path.join(path);
        fs::read(full_path).await
    }

    async fn exists(&self, path: &str) -> io::Result<bool> {
        let full_path = self.base_path.join(path);
        Ok(full_path.exists())
    }

    async fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        let dir = self.base_path.join(prefix);
        let mut results = Vec::new();

        if !dir.exists() {
            return Ok(results);
        }

        let mut entries = fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            if let Ok(file_name) = entry.file_name().into_string() {
                // Retorna caminho relativo completo, ex: "snapshots/snapshot1.toml"
                let relative = format!("{}/{}", prefix, file_name);
                results.push(relative);
            }
        }
        Ok(results)
    }

    async fn delete(&self, path: &str) -> io::Result<()> {
        let full_path = self.base_path.join(path);
        fs::remove_file(full_path).await
    }
}

pub struct S3Storage {
    client: Client,
    bucket: String,
}

impl S3Storage {
    /// Create S3 storage with explicitly provided credentials
    /// This avoids any side effects from environment variables
    pub async fn new(
        bucket: &str,
        region: Option<String>,
        endpoint: Option<String>,
        credentials: Credentials,
    ) -> Self {
        let mut config_loader = aws_config::defaults(BehaviorVersion::latest());

        if let Some(r) = region {
            config_loader = config_loader.region(Region::new(r));
        }

        let force_path_style = endpoint.is_some();
        if let Some(e) = endpoint {
            config_loader = config_loader.endpoint_url(e);
        }

        let sdk_config = config_loader.credentials_provider(credentials).load().await;

        // Configuração específica para S3
        let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
            // Força Path Style (ex: domain.com/bucket) se usarmos um endpoint customizado.
            // Essencial para MinIO, Localstack e alguns provedores S3 compatíveis.
            .force_path_style(force_path_style)
            .build();

        let client = Client::from_conf(s3_config);

        Self {
            client,
            bucket: bucket.to_string(),
        }
    }
}

#[async_trait]
impl Storage for S3Storage {
    async fn write(&self, path: &str, data: &[u8]) -> io::Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(path)
            .body(data.to_vec().into())
            .send()
            .await
            .map_err(|e| io::Error::other(format!("S3 Upload Error: {}", e)))?;
        Ok(())
    }

    async fn read(&self, path: &str) -> io::Result<Vec<u8>> {
        let resp = self
            .client
            .get_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await
            .map_err(|e| io::Error::new(ErrorKind::NotFound, format!("S3 Read Error: {}", e)))?;

        let data = resp
            .body
            .collect()
            .await
            .map_err(|e| io::Error::other(format!("S3 Body Error: {}", e)))?;

        Ok(data.into_bytes().to_vec())
    }

    async fn exists(&self, path: &str) -> io::Result<bool> {
        match self
            .client
            .head_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(e) => {
                let service_error = e.into_service_error();
                if service_error.is_not_found() {
                    Ok(false)
                } else {
                    Err(io::Error::other(format!(
                        "S3 Head Error: {}",
                        service_error
                    )))
                }
            }
        }
    }

    async fn list(&self, prefix: &str) -> io::Result<Vec<String>> {
        let resp = self
            .client
            .list_objects_v2()
            .bucket(&self.bucket)
            .prefix(prefix)
            .send()
            .await
            .map_err(|e| io::Error::other(format!("S3 List Error: {}", e)))?;

        let files = resp
            .contents
            .unwrap_or_default()
            .into_iter()
            .filter_map(|obj| obj.key)
            .collect();
        Ok(files)
    }

    async fn delete(&self, path: &str) -> io::Result<()> {
        self.client
            .delete_object()
            .bucket(&self.bucket)
            .key(path)
            .send()
            .await
            .map_err(|e| io::Error::other(format!("S3 Delete Error: {}", e)))?;
        Ok(())
    }
}
