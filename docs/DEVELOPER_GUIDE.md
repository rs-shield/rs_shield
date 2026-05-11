# RS Shield Developer Guide

Technical documentation for developers working on RS Shield.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Structure](#project-structure)
3. [Development Setup](#development-setup)
4. [Key Modules](#key-modules)
5. [FIDO2/WebAuthn Integration](#fido2webauthn-integration)
6. [Building & Testing](#building--testing)
7. [Contributing](#contributing)
8. [API Reference](#api-reference)

---

## Architecture Overview

RS Shield follows a **modular, layered architecture**:

```
┌─────────────────────────────────────────────┐
│         Desktop UI (Dioxus + Tailwind)      │  rsb-desktop/
├─────────────────────────────────────────────┤
│         CLI Interface                        │  rsb-cli/
├─────────────────────────────────────────────┤
│  Core Engine (Backup/Restore/S3/Crypto)    │  rsb-core/
├─────────────────────────────────────────────┤
│    OS APIs (Tokio, System, Filesystem)      │
└─────────────────────────────────────────────┘
```

### Design Principles

1. **Security First** - Encryption by default, no plaintext storage
2. **Performance** - Async/await, multi-threading, smart chunking
3. **Modularity** - Core logic independent from UI/CLI
4. **Testing** - Comprehensive test coverage (unit + integration)
5. **Maintainability** - Clean code, good documentation, type safety

---

## Project Structure

```
rs-shield/
├── rsb-core/                    # Core library (reusable)
│   ├── src/
│   │   ├── lib.rs             # Library root
│   │   ├── main.rs            # Standalone binary (testing)
│   │   ├── config/
│   │   │   └── mod.rs         # Configuration management
│   │   ├── core/
│   │   │   ├── mod.rs         # Main module
│   │   │   ├── backup.rs      # Backup engine
│   │   │   ├── restore.rs     # Restore engine
│   │   │   ├── manifest.rs    # Backup manifest/metadata
│   │   │   ├── prune.rs       # Cleanup/retention
│   │   │   ├── storage_backend.rs  # Abstract storage
│   │   │   ├── file_processor.rs   # File handling
│   │   │   ├── email_notifications.rs
│   │   │   ├── notification_logger.rs
│   │   │   └── resource_monitor.rs  # CPU/Battery monitoring
│   │   ├── crypto/
│   │   │   ├── mod.rs          # Encryption/Decryption
│   │   │   └── ...
│   │   ├── storage/
│   │   │   ├── local.rs        # Filesystem storage
│   │   │   ├── s3.rs           # S3 backend
│   │   │   └── mod.rs
│   │   ├── credentials/
│   │   │   └── credentials_manager.rs  # Keyring management
│   │   ├── utils/
│   │   │   └── mod.rs          # Helper functions
│   │   ├── realtime.rs         # Real-time sync
│   │   └── report.rs           # Backup reports
│   ├── tests/
│   │   └── *.rs               # Integration tests
│   └── Cargo.toml
│
├── rsb-cli/                    # Command-line interface
│   ├── src/
│   │   └── main.rs            # CLI entry point
│   └── Cargo.toml
│
├── rsb-desktop/                # Desktop GUI
│   ├── src/
│   │   ├── main.rs            # App entry
│   │   ├── ui/
│   │   │   ├── backup_screen.rs
│   │   │   ├── restore_screen.rs
│   │   │   ├── verify_screen.rs
│   │   │   ├── prune_screen.rs
│   │   │   ├── realtime_sync_screen.rs
│   │   │   └── ...
│   │   └── i18n/              # Internationalization
│   ├── tailwind.config.js     # Tailwind CSS config
│   └── Cargo.toml
│
├── tests/                     # Workspace-level tests
├── Cargo.toml                 # Workspace manifest
└── README.md
```

---

## Development Setup

### Prerequisites

```bash
# Minimum Rust version
rustc --version  # Should be 1.70+

# Update Rust
rustup update stable
```

### Clone & Setup

```bash
git clone https://github.com/yourusername/rs-shield.git
cd rs-shield

# Install dependencies (macOS)
brew install llvm

# Install dependencies (Ubuntu)
sudo apt install build-essential libssl-dev libpq-dev

# Create .env for development
cp example-secure-config.toml .env.development
```

### IDE Setup

**VSCode:**
```json
{
  "rust-analyzer.cargo.features": "all",
  "rust-analyzer.checkOnSave.command": "clippy",
  "[rust]": {
    "editor.formatOnSave": true,
    "editor.defaultFormatter": "rust-lang.rust-analyzer"
  }
}
```

**Environment Variables:**
```bash
export RUST_LOG=debug
export RUST_BACKTRACE=1
```

---

## Key Modules

### rsb_sdk::config

Handles configuration file parsing and management.

```rust
pub struct Config {
    pub source_path: String,
    pub destination_path: String,
    pub backup_mode: String,  // "full" | "incremental"
    pub encryption_key: Option<String>,
    pub s3: Option<S3Config>,
    pub s3_buckets: Option<Vec<S3BucketConfig>>,
    pub compression_level: Option<u8>,
    pub pause_on_high_cpu: Option<u8>,
    pub pause_on_low_battery: Option<u8>,
}

// Load from TOML file
let config = load_config(Path::new("profile.toml"))?;

// Create new profile
create_profile("my-backup", 
    Path::new("/home/user/docs"),
    Path::new("/mnt/backup"))?;
```

### rsb_sdk::core::backup

Core backup engine - incremental, encrypted backups.

```rust
pub async fn perform_backup(
    config: &Config,
    mode: &str,  // "full" or "incremental"
    password: Option<&str>,
    dry_run: bool,
    verify: bool,
    progress_callback: Option<Box<dyn Fn(BackupProgress)>>,
) -> Result<BackupReport, BackupError>

// Example usage
let report = perform_backup(
    &config,
    "incremental",
    Some("my-password"),
    false,  // not dry run
    true,   // verify
    None,   // no progress callback
).await?;

println!("Backedup {} files", report.files_processed);
```

### rsb_sdk::crypto

Encryption/decryption using AES-256-GCM.

```rust
pub async fn encrypt_file(
    input_path: &Path,
    output_path: &Path,
    password: &str,
    compression_level: u8,
) -> Result<(), CryptoError>

pub async fn decrypt_file(
    input_path: &Path,
    output_path: &Path,
    password: &str,
) -> Result<(), CryptoError>

// Key derivation from password
pub fn derive_key(password: &str, salt: &[u8; 16]) 
    -> Result<[u8; 32], CryptoError>
```

### rsb_sdk::storage

Abstract storage backend - supports local filesystem and S3.

```rust
#[async_trait]
pub trait StorageBackend: Send + Sync {
    async fn upload(&self, local_path: &Path, remote_path: &str) 
        -> Result<(), StorageError>;
    async fn download(&self, remote_path: &str, local_path: &Path) 
        -> Result<(), StorageError>;
    async fn list(&self, prefix: &str) 
        -> Result<Vec<String>, StorageError>;
    async fn delete(&self, remote_path: &str) 
        -> Result<(), StorageError>;
}

// Implementations
pub struct LocalStorage { /* ... */ }
pub struct S3Storage { /* ... */ }
```

### rsb_sdk::realtime

Real-time file synchronization.

```rust
pub struct RealtimeSync {
    source: PathBuf,
    destination: PathBuf,
    ignore_patterns: Vec<String>,
    stop_signal: Arc<AtomicBool>,
}

impl RealtimeSync {
    pub async fn start(&mut self) -> Result<SyncStats, SyncError>
    
    pub async fn stop(&mut self) -> Result<(), SyncError>
    
    pub async fn sync_all_files(&mut self) 
        -> Result<SyncStats, SyncError>
}
```

---

## FIDO2/WebAuthn Integration

RS Shield implements W3C WebAuthn standard authentication using the `webauthn-rs` library (v0.5.4).

### Architecture

```
┌────────────────────────────────────────────────────────┐
│  CLI Interface (rsb-cli)             │
│  fido2_cmd.rs: register, authenticate │
│                list, revoke          │
└────────────────┬───────────────────────────────────────┘
               │
         ┌─────────┴──────────┐
         │ Fido2Manager       │  (rsb-core)
         └─────────┬──────────┘
               │
         ┌─────────┴──────────────────────┐
         │ webauthn-rs        │
         │ - Webauthn struct  │
         │ - Passkey mgmt     │
         │ - Challenge/State  │
         └─────────┬──────────────────────┘
               │
         ┌─────────┴──────────────────────┐
         │ FIDO2 Device       │
         │ (YubiKey, etc)     │
         └────────────────────────────────┘
```

### Core Components

#### Fido2Manager

**Location:** `rsb-core/src/credentials/fido2.rs`

Main orchestrator for FIDO2 operations:

```rust
pub struct Fido2Manager {
    webauthn: Webauthn,  // W3C WebAuthn instance
    credentials: HashMap<String, (Passkey, Fido2Credential)>,
    registration_state: Option<(PasskeyRegistration, String)>,
    authentication_state: Option<(PasskeyAuthentication, String)>,
    rp_id: String,  // Relying Party ID
}
```

#### Registration Flow

```rust
// 1. Start registration challenge
let challenge = manager.start_registration(
    "user@example.com",
    "user@example.com",
    "User Name"
)?;
// Returns CreationChallengeResponse for client

// 2. Client interacts with device, gets response
// 3. Server completes registration
let credential = manager.finish_registration(registration_response)?;
// Stores Passkey + metadata
```

**Key Points:**
- Creates unique credential ID per device
- Stores public key (private key stays on device)
- Records creation timestamp and counter (for anti-cloning)
- All data serialized/deserialized via serde

#### Authentication Flow

```rust
// 1. Start authentication challenge
let challenge = manager.start_authentication("user@example.com")?;
// Returns RequestChallengeResponse

// 2. Device signs challenge with its private key
// 3. Server verifies signature
manager.finish_authentication(auth_response)?;
// Updates counter and last_used timestamp
```

**Security Features:**
- Counter validation prevents device cloning
- Each authentication increments counter
- Signature cryptographically verifies device ownership
- Challenge prevents replay attacks

#### Data Structures

```rust
#[derive(Clone, Serialize, Deserialize)]
pub struct Fido2Credential {
    pub user_id: String,
    pub user_name: String,
    pub display_name: String,
    pub credential_data: Vec<u8>,  // Serialized Passkey
    pub created_at: String,        // RFC3339 timestamp
    pub last_used: Option<String>, // RFC3339 timestamp
    pub counter: u32,              // Anti-cloning counter
}

pub enum Fido2Error {
    CredentialNotFound,
    InvalidUserId,
    RegistrationFailed(String),
    AuthenticationFailed(String),
    NoRegistrationInProgress,
    NoAuthenticationInProgress,
}
```

### Dependencies

```toml
# root Cargo.toml [workspace.dependencies]
webauthn-rs = "0.5.4"  # W3C WebAuthn standard
authenticator-rs = "*" # Hardware key communication
ctap-hid-fido2 = "*"   # USB HID protocol for FIDO2
uuid = { version = "*", features = ["v4", "serde"] }
serde = { version = "*", features = ["derive"] }
serde_json = "*"
chrono = { version = "*", features = ["serde"] }
```

### Testing

**Unit Tests:** `rsb-core/src/credentials/fido2.rs#[cfg(test)]`

```rust
#[test]
fn test_creation() {/* Manager initialization */}

#[test]
fn test_has_credential() {/* Credential existence checks */}

#[test]
fn test_list_empty() {/* List operations on empty store */}
```

**Running Tests:**

```bash
# All tests
cargo test --release

# FIDO2 module only
cargo test --lib credentials::fido2

# With output
cargo test --release -- --nocapture
```

### CLI Integration

**Location:** `rsb-cli/src/fido2_cmd.rs`

Provides user-friendly commands:

```bash
rsb fido2 register --user-id user@example.com
rsb fido2 authenticate --user-id user@example.com
rsb fido2 list
rsb fido2 revoke --user-id user@example.com
```

---

## Building & Testing

### Build Commands

```bash
# Development build
cargo build

# Release build (optimized)
cargo build --release

# Specific package
cargo build -p rsb-core
cargo build -p rsb-cli
cargo build -p rsb-desktop
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for specific package
cargo test -p rsb-core

# Run specific test
cargo test backup_incremental

# With output
cargo test -- --nocapture

# Performance tests (may take longer)
cargo test --release -- --ignored
```

### Test Coverage

Generate coverage report (requires `tarpaulin`):

```bash
cargo install cargo-tarpaulin

cargo tarpaulin --workspace \
    --out Html \
    --output-dir coverage \
    --timeout 300
```

### Linting & Formatting

```bash
# Format code
cargo fmt

# Check formatting
cargo fmt -- --check

# Lint with clippy
cargo clippy --all-targets --all-features

# Fix clippy warnings
cargo clippy --fix --all-targets --all-features
```

---

## Contributing

### Code Style

- Follow Rust API Guidelines
- Use `cargo fmt` for formatting
- Run `cargo clippy` before committing
- Add tests for new functionality
- Document public APIs with rustdoc

### Adding a Feature

1. **Create branch:**
   ```bash
   git checkout -b feature/my-feature
   ```

2. **Implement in rsb-core first** (library)
   - Core logic independent of UI
   - Add tests in `tests/` folder

3. **Update CLI** (rsb-cli) if needed
   - Add command/subcommand
   - Add help text

4. **Update GUI** (rsb-desktop) if needed
   - Add new screen or modify existing
   - Test on all platforms

5. **Update documentation:**
   - Code comments
   - README.md
   - docs/DEVELOPER_GUIDE.md

6. **Run full test suite:**
   ```bash
   cargo test --workspace
   cargo clippy --all-targets
   cargo fmt
   ```

7. **Submit pull request**

### Git Workflow

```bash
# Create feature branch
git checkout -b feature/add-feature-name

# Make commits
git add .
git commit -m "feat: add feature description"

# Push to fork
git push origin feature/add-feature-name

# Create PR on GitHub
# Wait for CI checks to pass
# Get code review
# Merge when approved
```

### Commit Message Format

Use conventional commits:
```
feat: add incremental backup support
fix: resolve S3 upload timeout
docs: update API documentation
test: add tests for encryption
chore: update dependencies
refactor: simplify backup logic
```

---

## API Reference

### Core Backup API

```rust
// Perform backup operation
pub async fn rsb_sdk::perform_backup(
    config: &Config,
    mode: &str,
    password: Option<&str>,
    dry_run: bool,
    verify: bool,
    progress_callback: Option<Box<dyn Fn(BackupProgress)>>,
) -> Result<BackupReport, BackupError>
```

### Core Restore API

```rust
// Perform restore operation
pub async fn rsb_sdk::restore::perform_restore(
    config: &Config,
    password: Option<&str>,
    progress_callback: Option<Box<dyn Fn(RestoreProgress)>>,
) -> Result<RestoreReport, RestoreError>
```

### Real-Time Sync API

```rust
// Start real-time monitoring
pub async fn rsb_sdk::realtime::sync_all_files(
    source: &Path,
    destination: &Path,
    ignore_patterns: &[String],
    encryption_password: Option<&str>,
) -> Result<SyncStats, SyncError>
```

---

## Performance Tips

1. **Batch Operations** - Process multiple files together
2. **Async I/O** - Use Tokio for parallel operations
3. **Compression** - Use level 3-5 for balance
4. **Chunking** - 512MB chunks for S3 uploads
5. **Caching** - Cache file metadata during incremental backups

---

## Debugging

### Enable Logging

```rust
// In main.rs
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

tracing_subscriber::registry()
    .with(tracing_subscriber::EnvFilter::from_default_env())
    .with(tracing_subscriber::fmt::layer())
    .init();
```

```bash
# Run with debug logging
RUST_LOG=debug cargo run
RUST_LOG=rsb_core=debug,rsb_cli=info cargo run
```

### Debugging in VSCode

Create `.vscode/launch.json`:
```json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug rsb-cli",
            "cargo": {
                "args": [
                    "build",
                    "--bin=rsb-cli",
                    "--package=rsb-cli"
                ]
            },
            "args": ["--help"]
        }
    ]
}
```

---

## Resources

- [Tokio Documentation](https://tokio.rs/)
- [Dioxus Guide](https://dioxuslabs.com/learn/0.5)
- [AWS SDK Rust](https://github.com/awslabs/aws-sdk-rust)
- [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)

## Support

- 📧 Email: marciozebedeu@rsbshield.co.ao
- 🔗 Website: [rsbshield.co.ao](https://rsbshield.co.ao)
- GitHub: [@zebedeu](https://github.com/zebedeu)

---

*Last Updated: February 2026*
