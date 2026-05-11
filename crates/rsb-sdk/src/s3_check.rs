use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::{config::Region, Client};

/// Verifies the S3 connection by trying to access the bucket's metadata (HeadBucket).
/// Returns Ok(message) on success, or Err(error_message) on failure.
pub async fn verify_s3_connection(
    bucket: &str,
    region: &str,
    endpoint: &str,
    access_key: &str,
    secret_key: &str,
) -> Result<String, String> {
    if bucket.is_empty() || region.is_empty() || access_key.is_empty() || secret_key.is_empty() {
        return Err("Required fields (Bucket, Region, Keys) not filled.".to_string());
    }

    let credentials = Credentials::new(access_key, secret_key, None, None, "manual_test");

    let region_provider = Region::new(region.to_string());

    let mut config_loader = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .region(region_provider);

    // If an endpoint is provided (e.g., MinIO, R2), use it. Otherwise, use the AWS default.
    if !endpoint.trim().is_empty() {
        config_loader = config_loader.endpoint_url(endpoint);
    }

    let sdk_config = config_loader.load().await;

    // S3-specific configuration
    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        // Force Path Style (e.g., domain.com/bucket) if we use a custom endpoint.
        // Essential for MinIO, Localstack, and some S3-compatible providers.
        .force_path_style(!endpoint.trim().is_empty())
        .build();

    let client = Client::from_conf(s3_config);

    // Tries to get bucket metadata to validate access and existence
    match client.head_bucket().bucket(bucket).send().await {
        Ok(_) => Ok(format!(
            "✅ Connection established! Bucket '{}' accessible.",
            bucket
        )),
        Err(e) => {
            let err_msg = e.to_string();
            // Simplifies the error message for the user
            Err(format!("❌ Connection failed: {}", err_msg))
        }
    }
}
