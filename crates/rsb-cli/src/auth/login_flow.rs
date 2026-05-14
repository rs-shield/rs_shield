use anyhow::{Context, Result, anyhow};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::time::timeout;
use tracing::info;
/// Manages the integrated login flow: server + browser + auth + shutdown
pub struct LoginFlow {
    server_handle: Option<tokio::task::JoinHandle<()>>,
    client: reqwest::Client,
}

impl Default for LoginFlow {
    fn default() -> Self {
        Self::new()
    }
}
impl LoginFlow {
    pub fn new() -> Self {
        Self {
            server_handle: None,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(10))
                .pool_idle_timeout(Duration::from_secs(30))
                .build()
                .expect("Failed to create HTTP client"),
        }
    }

    /// Start the integrated login flow
    pub async fn start(&mut self, user_id: String) -> Result<String> {
        info!("Starting integrated login flow for user: {}", user_id);

        // 1. Create channel for server readiness
        let (tx, rx) = oneshot::channel();

        // 2. Start the server in background
        self.server_handle = Some(tokio::spawn(async move {
            if let Err(e) = crate::auth::routes::start_auth_server(3000, tx).await {
                eprintln!("Server error: {}", e);
            }
        }));

        // 3. Wait for server to signal readiness (with timeout)
        println!("⏳ Starting authentication server...");

        // Wait for readiness signal with timeout, but also have a fallback
        match timeout(Duration::from_secs(10), rx).await {
            Ok(Ok(_)) => {
                println!("✅ Server ready!");
            }
            Ok(Err(_)) => {
                println!("⚠️  Server channel closed, proceeding anyway...");
            }
            Err(_) => {
                println!("⚠️  Server startup timeout, attempting manual connection...");
                // Fallback: try to connect manually with retries
                self.wait_for_server_ready().await?;
            }
        }

        // 4. Initiate device flow and get user code and device code
        let (device_code, user_code) = self.initiate_device_flow(&user_id).await?;

        // 5. Build auth URL without user_code
        let auth_url = "http://localhost:3000/auth/device/verify".to_string();

        println!("\n🔐 Authentication Required");
        println!("════════════════════════════════════════════");
        println!("📌 User Code: {}", user_code);
        println!("🔗 Opening browser: {}", auth_url);
        println!("⏳ Waiting for authentication...\n");

        // 6. Open browser automatically
        if let Err(e) = open::that(&auth_url) {
            eprintln!("⚠️  Could not open browser: {}", e);
            println!("Please open the browser manually and navigate to:");
            println!("   {}", auth_url);
        }

        // 7. Wait for token with timeout (5 minutes)
        let token = timeout(Duration::from_secs(300), self.poll_for_token(&device_code))
            .await
            .context("Authentication timeout (5 minutes exceeded)")??;

        println!("✅ Authentication successful!");
        Ok(token)
    }

    /// Wait for server to be ready with manual connection retry
    async fn wait_for_server_ready(&self) -> Result<()> {
        println!("⏳ Starting authentication server...");

        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );

        pb.set_message("Waiting for authentication server to start...");

        let start = std::time::Instant::now();

        for attempt in 0..120 {
            match self.client.get("http://localhost:3000/").send().await {
                Ok(_) => {
                    pb.finish_with_message("✅ Server is responding!");
                    println!("   Server ready in {:.1}s", start.elapsed().as_secs_f32());
                    return Ok(());
                }
                Err(_) => {
                    // Atualiza a mensagem apenas a cada 8 tentativas para não poluir
                    if attempt % 8 == 0 && attempt > 0 {
                        pb.set_message(format!(
                            "Waiting for server... ({:.0}s)",
                            start.elapsed().as_secs_f32()
                        ));
                    }

                    tokio::time::sleep(Duration::from_millis(450)).await;
                }
            }
        }

        pb.finish_with_message("❌ Server startup timeout");
        Err(anyhow::anyhow!(
            "Server failed to start after {} seconds",
            start.elapsed().as_secs()
        ))
    }

    /// Initiate device flow with server
    async fn initiate_device_flow(&self, user_id: &str) -> Result<(String, String)> {
        let response = self
            .client
            .post("http://127.0.0.1:3000/auth/device/start")
            .timeout(Duration::from_secs(10))
            .json(&serde_json::json!({
                "user_id": user_id
            }))
            .send()
            .await
            .context("Failed to connect to authentication server")?;

        if !response.status().is_success() {
            let status = response.status();

            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown server error".to_string());

            return Err(anyhow!(
                "Authentication server returned {}: {}",
                status,
                body
            ));
        }

        let data: serde_json::Value = response
            .json()
            .await
            .context("Failed to parse device flow response")?;

        let device_code = data
            .get("device_code")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Missing device_code in response"))?;

        let user_code = data
            .get("user_code")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Missing user_code in response"))?;

        if device_code.is_empty() || user_code.is_empty() {
            return Err(anyhow!("Invalid device flow response: empty codes"));
        }

        Ok((device_code, user_code))
    }
    /// Poll server for authentication token
    async fn poll_for_token(&self, device_code: &str) -> Result<String> {
        loop {
            match self
                .client
                .post("http://localhost:3000/auth/device/token")
                .json(&serde_json::json!({ "device_code": device_code }))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => {
                    if let Ok(data) = resp.json::<serde_json::Value>().await {
                        if let Some(token) = data.get("access_token").and_then(|v| v.as_str()) {
                            if !token.is_empty() {
                                return Ok(token.to_string());
                            }
                        }
                    }
                }
                Ok(_) => {
                    // Still waiting
                }
                Err(_) => {
                    // Server temporarily unavailable
                }
            }

            // Wait before next poll (1 second)
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    /// Stop server gracefully
    pub async fn shutdown(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }

        // Additional grace period
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

impl Drop for LoginFlow {
    fn drop(&mut self) {
        if let Some(handle) = self.server_handle.take() {
            handle.abort();
        }
    }
}
