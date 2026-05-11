use crate::utils::expand_path;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct S3Config {
    pub bucket: Option<String>,
    pub region: Option<String>,
    pub endpoint: Option<String>,
    pub access_key: Option<String>,
    pub secret_key: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct S3BucketConfig {
    pub name: String,
    pub region: String,
    pub endpoint: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Config {
    pub source_path: String,
    pub destination_path: String,
    pub exclude_patterns: Vec<String>,
    pub encryption_key: Option<String>,
    pub backup_mode: String, // "full" or "incremental"
    pub s3_bucket: Option<String>,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub s3: Option<S3Config>,
    pub s3_buckets: Option<Vec<S3BucketConfig>>, // List of known S3 buckets
    pub encrypt_patterns: Option<Vec<String>>,
    pub pause_on_low_battery: Option<u8>,
    pub pause_on_high_cpu: Option<u8>,
    pub compression_level: Option<u8>,
}

pub fn create_profile(name: &str, source: &Path, dest: &Path) -> io::Result<()> {
    create_profile_with_options(
        name, source, dest, None, None, false, None, None, None, None, None,
    )
}

/// Create a profile with all configurable options
#[allow(clippy::too_many_arguments)]
pub fn create_profile_with_options(
    name: &str,
    source: &Path,
    dest: &Path,
    mode: Option<&str>,
    compression: Option<u8>,
    encrypt: bool,
    password: Option<&str>,
    exclude: Option<&str>,
    s3_bucket: Option<&str>,
    s3_region: Option<&str>,
    s3_endpoint: Option<&str>,
) -> io::Result<()> {
    // Parse exclude patterns
    let exclude_patterns = if let Some(patterns_str) = exclude {
        patterns_str
            .split(',')
            .map(|p| p.trim().to_string())
            .collect()
    } else {
        vec!["*.tmp".to_string(), "node_modules".to_string()]
    };

    let config = Config {
        source_path: source.to_string_lossy().into_owned(),
        destination_path: dest.to_string_lossy().into_owned(),
        exclude_patterns,
        encryption_key: password.map(|p| p.to_string()),
        backup_mode: mode.unwrap_or("incremental").to_string(),
        s3_bucket: s3_bucket.map(|s| s.to_string()),
        s3_region: s3_region.map(|s| s.to_string()),
        s3_endpoint: s3_endpoint.map(|s| s.to_string()),
        s3: if s3_bucket.is_some() {
            Some(S3Config {
                bucket: s3_bucket.map(|s| s.to_string()),
                region: s3_region.map(|s| s.to_string()),
                endpoint: s3_endpoint.map(|s| s.to_string()),
                access_key: None,
                secret_key: None,
            })
        } else {
            None
        },
        s3_buckets: None,
        encrypt_patterns: if encrypt {
            Some(vec!["*".to_string()])
        } else {
            None
        },
        pause_on_low_battery: Some(20),
        pause_on_high_cpu: Some(20),
        compression_level: compression.or(Some(3)),
    };

    let toml_str = toml::to_string(&config).map_err(io::Error::other)?;
    let filename = format!("{}.toml", name);
    fs::write(filename, toml_str)?;
    Ok(())
}

pub fn load_config(path: &Path) -> io::Result<Config> {
    let content = fs::read_to_string(path)?;
    let mut config: Config =
        toml::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    // Expand special paths like ~ and environment variables
    config.source_path = expand_path(&config.source_path)
        .to_string_lossy()
        .into_owned();
    config.destination_path = expand_path(&config.destination_path)
        .to_string_lossy()
        .into_owned();

    Ok(config)
}

/// Prompt user for S3 configuration and save to TOML file
/// Allows selecting an existing bucket or creating a new one
#[cfg(feature = "cli")]
pub fn prompt_for_s3_config(config_path: &Path) -> io::Result<()> {
    use std::io::Write;

    let content = fs::read_to_string(config_path)?;
    let mut config: Config =
        toml::from_str(&content).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut known_buckets = config.s3_buckets.clone().unwrap_or_default();

    println!("\n📦 S3 Storage Configuration");
    println!("   Configure or select S3/S3-compatible storage\n");

    if !known_buckets.is_empty() {
        println!("📋 Known S3 Buckets:");
        for (idx, bucket_cfg) in known_buckets.iter().enumerate() {
            println!(
                "   {}. {} ({})",
                idx + 1,
                bucket_cfg.name,
                bucket_cfg.endpoint
            );
        }
        println!();
    }

    println!("Options:");
    println!("  1. Select existing bucket");
    println!("  2. Add new bucket");
    print!("\nChoose option (1 or 2): ");
    io::stdout().flush()?;

    let mut choice = String::new();
    io::stdin().read_line(&mut choice)?;
    let choice = choice.trim();

    let bucket_config = match choice {
        "1" => {
            if known_buckets.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "No known buckets. Please add a new bucket first.",
                ));
            }

            print!("\nSelect bucket number (1-{}): ", known_buckets.len());
            io::stdout().flush()?;
            let mut idx_str = String::new();
            io::stdin().read_line(&mut idx_str)?;
            let idx: usize = idx_str.trim().parse().map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidInput, "Invalid bucket number")
            })?;

            if idx < 1 || idx > known_buckets.len() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Bucket number out of range",
                ));
            }

            known_buckets[idx - 1].clone()
        }
        "2" => {
            println!("\n🆕 Create New S3 Bucket Configuration");

            print!("Enter S3 bucket name: ");
            io::stdout().flush()?;
            let mut bucket = String::new();
            io::stdin().read_line(&mut bucket)?;
            let bucket = bucket.trim().to_string();

            if bucket.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Bucket name cannot be empty",
                ));
            }

            print!("Enter S3 region (e.g., us-east-1, eu-west-1): ");
            io::stdout().flush()?;
            let mut region = String::new();
            io::stdin().read_line(&mut region)?;
            let region = region.trim().to_string();

            if region.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Region cannot be empty",
                ));
            }

            // Get endpoint (REQUIRED for S3-compatible services)
            println!("\n🔗 S3 Endpoint URL Examples:");
            println!("   AWS S3:        https://s3.amazonaws.com");
            println!("   AWS S3 (v4):   https://s3.{region}.amazonaws.com");
            println!("   MinIO:         http://localhost:9000 (or your MinIO URL)");
            println!("   Wasabi:        https://s3.wasabisys.com");
            println!("   DigitalOcean:  https://nyc3.digitaloceanspaces.com");
            println!("   R2 (Cloudflare): https://<account-id>.r2.cloudflarestorage.com\n");

            print!("Enter S3 endpoint URL: ");
            io::stdout().flush()?;
            let mut endpoint = String::new();
            io::stdin().read_line(&mut endpoint)?;
            let endpoint = endpoint.trim().to_string();

            if endpoint.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Endpoint URL cannot be empty.\nExamples:\n  https://s3.amazonaws.com\n  http://localhost:9000 (for MinIO)",
                ));
            }

            let new_bucket = S3BucketConfig {
                name: bucket.clone(),
                region: region.clone(),
                endpoint: endpoint.clone(),
            };

            known_buckets.push(new_bucket.clone());

            println!("\n✅ Bucket added to known buckets list");
            new_bucket
        }
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Invalid choice. Please enter 1 or 2",
            ));
        }
    };

    // Update S3 config with selected bucket
    let s3_config = S3Config {
        bucket: Some(bucket_config.name.clone()),
        region: Some(bucket_config.region.clone()),
        endpoint: Some(bucket_config.endpoint.clone()),
        access_key: None, // Will be set by credentials manager
        secret_key: None, // Will be set by credentials manager
    };

    config.s3 = Some(s3_config);
    config.s3_buckets = Some(known_buckets);

    let toml_str = toml::to_string(&config).map_err(io::Error::other)?;
    fs::write(config_path, toml_str)?;

    println!("\n✅ S3 configuration updated:");
    println!("   Bucket:   {}", bucket_config.name);
    println!("   Region:   {}", bucket_config.region);
    println!("   Endpoint: {}", bucket_config.endpoint);
    println!("   File:     {}\n", config_path.display());
    Ok(())
}

#[cfg(not(feature = "cli"))]
pub fn prompt_for_s3_config(_config_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Other,
        "Interactive S3 configuration is only available in CLI mode",
    ))
}
