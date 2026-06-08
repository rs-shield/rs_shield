# RS Shield Developer Guide

Technical documentation for developers working on RS Shield.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Structure](#project-structure)
3. [Development Setup](#development-setup)
4. [Key Modules](#key-modules)
5. [FIDO2/WebAuthn Integration](#fido2webauthn-integration)
6. [Chat Integrations](#chat-integrations-telegram-slack-discord)
7. [Building & Testing](#building--testing)
8. [Contributing](#contributing)
9. [API Reference](#api-reference)

---

## Architecture Overview

RS Shield follows a **modular, layered architecture**:

```
┌─────────────────────────────────────────────┐
│         Desktop UI (Dioxus + Tailwind)      │  rsb-desktop/
├─────────────────────────────────────────────┤
│         CLI Interface (clap)                 │  rsb-cli/
├─────────────────────────────────────────────┤
│  Core Engine (Backup/Restore/Crypto/S3)    │  rsb-sdk/
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
├── crates/
│   ├── rsb-sdk/                     # Core library (reusable SDK)
│   │   ├── src/
│   │   │   ├── lib.rs               # Library root
│   │   │   ├── auth/                # Authentication & JWT
│   │   │   ├── backup/              # Backup engine
│   │   │   │   ├── mod.rs
│   │   │   │   ├── diagnostic.rs    # Backup diagnostics
│   │   │   │   ├── discovery.rs     # File discovery
│   │   │   │   ├── metadata.rs      # Backup metadata
│   │   │   │   ├── processing.rs    # File processing
│   │   │   │   ├── progress.rs      # Progress tracking
│   │   │   │   ├── stats.rs         # Backup statistics
│   │   │   │   └── threading.rs     # Multi-threading
│   │   │   ├── config/              # Configuration management
│   │   │   ├── core/                # Core operations
│   │   │   │   ├── mod.rs
│   │   │   │   ├── cancellation.rs  # Cancellation support
│   │   │   │   ├── manifest.rs      # Manifest management
│   │   │   │   ├── prune.rs         # Cleanup/retention
│   │   │   │   ├── restore.rs       # Restore engine
│   │   │   │   ├── storage_backend.rs # Storage abstraction
│   │   │   │   ├── storage_ops.rs   # Storage operations
│   │   │   │   ├── file_processor.rs # File handling
│   │   │   │   ├── resource_monitor.rs # CPU/Battery monitoring
│   │   │   │   ├── types.rs         # Core type definitions
│   │   │   │   ├── notification_logger.rs
│   │   │   │   ├── notification_history.rs
│   │   │   │   ├── email_notifications.rs
│   │   │   │   └── chat_integrations.rs
│   │   │   ├── credentials/         # Credential management
│   │   │   ├── crypto/              # Encryption/Decryption (AES-256-GCM)
│   │   │   ├── fido2/               # FIDO2/WebAuthn
│   │   │   ├── integrity/           # Verification & integrity checks
│   │   │   ├── metrics/             # Metrics & monitoring
│   │   │   ├── operation/           # Operation definitions
│   │   │   ├── repository/          # Repository pattern
│   │   │   ├── s3_check.rs          # S3 connectivity checks
│   │   │   ├── server/              # Authentication server
│   │   │   ├── snapshot/            # Snapshot management
│   │   │   ├── storage/             # Storage backends (local, S3)
│   │   │   ├── utils/               # Utility functions
│   │   │   ├── realtime.rs          # Real-time sync
│   │   │   ├── report.rs            # Backup reports
│   │   │   └── portable_restore.rs  # Portable restore support
│   │   ├── tests/                   # Integration tests
│   │   └── Cargo.toml
│   │
│   ├── rsb-cli/                     # Command-line interface
│   │   ├── src/
│   │   │   ├── main.rs              # CLI entry point
│   │   │   ├── command/
│   │   │   │   ├── main_cmd.rs      # Command definitions
│   │   │   │   ├── config_cmd.rs    # Config subcommands
│   │   │   │   ├── fido2_cmd.rs     # FIDO2 subcommands
│   │   │   │   ├── snapshot_cmd.rs  # Snapshot subcommands
│   │   │   │   └── list_profiles_cmd.rs
│   │   │   └── assets/
│   │   ├── Cargo.toml
│   │   ├── CLI_GUIDE.md
│   │   └── README.md
│   │
│   └── rsb-desktop/                 # Desktop GUI (Dioxus + Tailwind)
│       ├── src/
│       │   ├── main.rs              # App entry point
│       │   └── ui/
│       │       ├── mod.rs
│       │       ├── backup_screen.rs
│       │       ├── restore_screen.rs
│       │       ├── verify_screen.rs
│       │       ├── prune_screen.rs
│       │       ├── realtime_sync_screen.rs
│       │       └── ...
│       ├── Dioxus.toml
│       ├── tailwind.config.js
│       ├── postcss.config.js
│       ├── DESIGN_GUIDE.md
│       └── Cargo.toml
│
├── tests/                           # Workspace-level integration tests
├── docs/                            # Documentation
│   ├── CLI.md
│   ├── USER_GUIDE.md
│   ├── DEVELOPER_GUIDE.md
│   ├── TROUBLESHOOTING_PORTABILITY.md
│   └── ...
├── Cargo.toml                       # Workspace manifest
├── Cargo.lock
├── deny.toml                        # Security audit config
├── README.md
└── LICENSE
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

Core backup engine - incremental, encrypted backups with multi-threading support.

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
         │ Fido2Manager       │  (rsb-sdk)
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

**Location:** `crates/rsb-sdk/src/fido2/mod.rs` and `crates/rsb-sdk/src/credentials/`

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

**Unit Tests:** `crates/rsb-sdk/src/fido2/mod.rs#[cfg(test)]`

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
cargo test fido2

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

## Chat Integrations (Telegram, Slack, Discord)

RS Shield supports sending notifications to multiple chat platforms to alert users about backup status, errors, and warnings.

### Architecture Overview

```
┌──────────────────────────────────────┐
│  Desktop UI (Dioxus)                 │
│  - IntegrationScreen                 │
│  - TelegramValidator component       │
└────────────────┬─────────────────────┘
               │
┌──────────────────────────────────────┐
│  Integration Config Management       │
│  - IntegrationConfig (JSON storage)  │
│  - TelegramIntegrationConfig         │
└────────────────┬─────────────────────┘
               │
┌──────────────────────────────────────┐
│  Core SDK (rsb-sdk)                  │
│  - chat_integrations.rs              │
│  - telegram_validator.rs             │
└────────────────┬─────────────────────┘
               │
         ┌─────────┴──────────────────────┐
         │ External APIs                  │
         ├─────────────────────────────────┤
         │ • Telegram Bot API              │
         │ • Slack Webhooks                │
         │ • Discord Webhooks              │
         └────────────────────────────────┘
```

### Telegram Integration

#### Components

**1. TelegramValidator** - Desktop UI Component

**Location:** `crates/rsb-desktop/src/ui/telegram_validator.rs`

Provides real-time token and chat ID validation:

```rust
pub fn TelegramValidator(
    bot_token: String,
) -> Element {
    // Validates bot token by calling Telegram API
    // Retrieves available chat IDs from /getUpdates
    // Displays validation results to user
}
```

**Features:**
- ✅ Validates bot token format
- ✅ Checks bot accessibility
- ✅ Retrieves available chat IDs
- ✅ Distinguishes between private chats and groups
- ✅ Provides user-friendly error messages

**2. Telegram Validator** - Backend Module

**Location:** `crates/rsb-sdk/src/core/telegram_validator.rs`

Core validation functions:

```rust
/// Validate bot token by calling getMe API
pub async fn validate_telegram_token(bot_token: &str) 
    -> Result<TelegramBot, String>

/// Get available chat IDs by polling getUpdates
pub async fn get_telegram_chat_id(bot_token: &str) 
    -> Result<Vec<(i64, String)>, String>
```

**Data Structures:**

```rust
#[derive(Debug, Deserialize)]
pub struct TelegramBot {
    pub id: u64,
    pub is_bot: bool,
    pub first_name: String,
    pub username: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramUpdatesResponse {
    pub ok: bool,
    pub result: Option<Vec<TelegramUpdate>>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct TelegramMessage {
    pub message_id: u64,
    pub chat: TelegramChat,
}

#[derive(Debug, Deserialize)]
pub struct TelegramChat {
    pub id: i64,
    pub r#type: String,  // "private", "group", "supergroup", "channel"
}
```

**3. Chat Notification Sender**

**Location:** `crates/rsb-sdk/src/core/chat_integrations.rs`

Sends notifications to Telegram:

```rust
pub async fn send_chat_notification(
    integration: &ChatIntegration,
    title: &str,
    message: &str,
    notification_type: &str,  // "success", "error", "warning", "info"
) -> Result<(), Box<dyn std::error::Error>>

async fn send_telegram_notification(
    bot_token: &str,
    chat_id: &str,
    title: &str,
    message: &str,
    notification_type: &str,
) -> Result<(), Box<dyn std::error::Error>>
```

**Notification Format:**

```
✅ Backup Complete
Backed up 1,250 files in 45 minutes
Compressed: 2.3 GB → 1.1 GB
```

#### Configuration

**1. Desktop Configuration** (`integrations_screen.rs`)

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramIntegrationConfig {
    pub enabled: bool,
    pub bot_token: String,      // From @BotFather
    pub chat_id: String,        // From /getUpdates API
}
```

**2. File Storage**

Configurations saved to `integrations.json` in profile directory:

```json
{
  "telegram": {
    "enabled": true,
    "bot_token": "123456789:ABCdefGHIjklmnoPQRstuvWXYZ",
    "chat_id": "-1001234567890"
  }
}
```

#### Setup Workflow

##### Step 1: Create Bot via @BotFather

1. Open Telegram app
2. Search for **@BotFather**
3. Send `/newbot` command
4. Follow instructions:
   - Enter bot name (e.g., "RS Shield Bot")
   - Enter username (e.g., "rs_shield_bot_username")
5. Copy the token: `123456789:ABCdefGHIjklmnoPQRstuvWXYZ`

##### Step 2: Validate Token in RS Shield

1. Open RS Shield Desktop
2. Go to **Integrations → Telegram**
3. Paste bot token in "Token do Bot" field
4. Click **"Validate Token"** button
5. Validator calls:
   - `https://api.telegram.org/bot{TOKEN}/getMe` → Confirms bot exists
   - `https://api.telegram.org/bot{TOKEN}/getUpdates` → Lists available chats

**Possible Results:**
- ✅ **Success:** Bot valid, displays bot name and ID
- ❌ **Invalid Token:** Token format incorrect or bot doesn't exist
- ⚠️ **No Messages Found:** User hasn't sent `/start` to bot yet

##### Step 3: Send /start to Bot

1. Search for your bot in Telegram (e.g., @rs_shield_bot_username)
2. Send `/start` command
3. This creates an update so validator can find the chat ID

##### Step 4: Get Chat ID

1. In RS Shield, click **"Validate Token"** again
2. Select the chat ID from the list
3. Chat ID is automatically populated
4. Save configuration

#### Validation Logic

**TelegramValidator Component Flow:**

```
┌─────────────────────────────────────┐
│ User enters bot token               │
│ Click "Validate Token"              │
└─────────────┬───────────────────────┘
              │
              ▼
┌─────────────────────────────────────┐
│ 1. validate_telegram_token()        │
│    - Calls /getMe API               │
│    - Verifies token format          │
│    - Confirms bot exists            │
└─────────────┬───────────────────────┘
              │
         ✅ Success? ──❌─→ Show Error Message
              │
              ▼
┌─────────────────────────────────────┐
│ 2. get_telegram_chat_id()           │
│    - Calls /getUpdates API          │
│    - Retrieves available chats      │
│    - Extracts chat IDs and types    │
└─────────────┬───────────────────────┘
              │
         ✅ Chats found? ──❌─→ "No messages found.
              │                  Send /start to bot"
              ▼
┌─────────────────────────────────────┐
│ 3. Display Results                  │
│    - Show bot info                  │
│    - List available chats           │
│    - Allow user to select chat      │
└─────────────────────────────────────┘
```

#### Error Handling

**HTTP Status Codes:**

| Status | Meaning | Solution |
|--------|---------|----------|
| 200 | Success | Notification sent ✅ |
| 400 | Bad Request | Check token/chat ID format |
| 401 | Unauthorized | Token invalid or expired |
| 403 | Forbidden | Bot can't send to this chat |
| 404 | Not Found | Chat doesn't exist |

**Common Errors:**

**Error 403: "Bot can't send messages to the bot"**
- **Cause:** Using bot's own ID as chat_id
- **Solution:** Send `/start` to bot, then use your chat ID

**Error 403: "Bot hasn't been started"**
- **Cause:** User hasn't sent `/start` to bot
- **Solution:** Open Telegram, find bot, send `/start`

**Error: "No messages found"**
- **Cause:** No chat activity with bot
- **Solution:** Send `/start` to bot first, then validate again

**Error: "Invalid token"**
- **Cause:** Token copied incorrectly or bot deleted
- **Solution:** Get new token from @BotFather

#### Testing Notifications

**Manual Test in Desktop UI:**

1. Configure Telegram integration
2. Go to **Integrations** tab
3. Click **"Test Notification"** button
4. Notification should appear in Telegram within 1-2 seconds

**Test via CLI:**

```bash
# Test via configuration
cargo run -p rsb-cli -- test-integrations --profile my-backup
```

**Test Programmatically:**

```rust
use rsb_sdk::core::chat_integrations::{ChatIntegration, send_chat_notification};

let integration = ChatIntegration::Telegram {
    bot_token: "123456789:ABCdefGHIjklmnoPQRstuvWXYZ".to_string(),
    chat_id: "-1001234567890".to_string(),
};

send_chat_notification(
    &integration,
    "Test Title",
    "This is a test message",
    "info",
).await?;
```

#### Use Cases

**1. Backup Completion Notification**

```
✅ Backup Completed
Profile: Daily Backup
Files: 5,432 backed up
Duration: 1h 23m
Size: 45.2 GB → 18.7 GB
Status: All files verified ✓
```

**2. Error Alert**

```
❌ Backup Failed
Profile: Weekly Backup
Error: S3 connection timeout
Retry: Scheduled for 8 PM
Action: Check your S3 credentials
```

**3. Warning Notification**

```
⚠️ High Resource Usage
CPU: 87% (pause threshold: 90%)
Battery: 12% (pause threshold: 20%)
Backup paused to protect system
Will resume when resources available
```

#### Configuration in Code

**Desktop Integration Screen:**

```rust
pub struct TelegramIntegrationConfig {
    pub enabled: bool,
    pub bot_token: String,
    pub chat_id: String,
}

// Load from file
let config = IntegrationConfig::load(profile_path);

// Convert to SDK ChatIntegration
let chat_integration = ChatIntegration::Telegram {
    bot_token: config.telegram.bot_token,
    chat_id: config.telegram.chat_id,
};
```

**In Backup Operations:**

```rust
// Send notification when backup completes
let notification = Notification {
    title: "Backup Complete".to_string(),
    message: format!("Backed up {} files", stats.total_files),
    notification_type: "success".to_string(),
};

for integration in config.chat_integrations {
    send_chat_notification(
        &integration,
        &notification.title,
        &notification.message,
        &notification.notification_type,
    ).await.ok();
}
```

#### Testing

**Unit Tests:**

```rust
#[test]
fn test_telegram_response_parsing() {
    // Test parsing bot response
}

#[test]
fn test_invalid_token() {
    // Test error handling
}

#[test]
fn test_chat_id_extraction() {
    // Test extracting chat IDs from updates
}
```

**Integration Tests:**

```bash
cargo test telegram
cargo test chat_integrations
```

---

## Building & Testing

### Prerequisites

- **Rust 1.70+** - Install from [rustup.rs](https://rustup.rs)
- **OpenSSL** - Required for cryptography
- **C compiler** - For FIDO2 dependencies

**macOS:**
```bash
brew install openssl
export LDFLAGS="-L/opt/homebrew/opt/openssl@3/lib"
export CPPFLAGS="-I/opt/homebrew/opt/openssl@3/include"
```

**Linux (Ubuntu/Debian):**
```bash
sudo apt-get install libssl-dev pkg-config build-essential
```

### Build Commands

```bash
# Development build (all crates)
cargo build

# Release build (optimized, all crates)
cargo build --release

# Specific crate
cargo build -p rsb-sdk --release    # Core library
cargo build -p rsb-cli --release    # CLI tool
cargo build -p rsb-desktop --release # Desktop GUI
```

### Running the CLI

```bash
# Show available commands
cargo run -p rsb-cli -- --help

# Create profile
cargo run -p rsb-cli -- create-profile --name my-backup --source /home/user/docs

# List profiles
cargo run -p rsb-cli -- list-profiles

# Perform backup
cargo run -p rsb-cli -- backup --profile my-backup

# Verify backup integrity
cargo run -p rsb-cli -- verify --backup /path/to/backup

# Diagnose backup issues
cargo run -p rsb-cli -- diagnose --backup /path/to/backup

# Restore from backup
cargo run -p rsb-cli -- restore --backup /path/to/backup --destination /tmp/restored
```

### Testing

```bash
# Run all tests
cargo test --workspace

# Run tests for specific crate
cargo test -p rsb-sdk
cargo test -p rsb-cli

# Run specific test file
cargo test --test integration_tests

# Run specific test function
cargo test backup_incremental

# Run with output
cargo test -- --nocapture

# Run with debug logging
RUST_LOG=debug cargo test -- --nocapture

# Performance tests (may take longer)
cargo test --release -- --ignored
```

### Test Organization

**Unit Tests:**
- Located within each module (inline `#[cfg(test)]`)
- Test specific functions/components
- Examples: `crates/rsb-sdk/src/crypto/mod.rs#[cfg(test)]`

**Integration Tests:**
- Located in `crates/rsb-sdk/tests/` directory
- Test complete workflows (backup → restore)
- Examples: `backup_integration_tests.rs`, `restore_integration_tests.rs`

**Workspace Tests:**
- Located in `tests/` directory (root level)
- Test cross-crate functionality
- Run with: `cargo test --test integration_tests`

### Test Coverage

Generate coverage report (requires `tarpaulin`):

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate HTML coverage report
cargo tarpaulin --workspace \
    --out Html \
    --output-dir coverage \
    --timeout 300 \
    --skip-clean
```

### Linting & Formatting

```bash
# Format all code
cargo fmt --all

# Check formatting (without modifying)
cargo fmt --all -- --check

# Lint with clippy
cargo clippy --all-targets --all-features

# Fix clippy warnings automatically
cargo clippy --fix --allow-dirty --all-targets --all-features

# Security audit (deny.toml)
cargo deny check

# Check for outdated dependencies
cargo outdated
```

### Continuous Integration

Before submitting a pull request, run:

```bash
# Full CI-like checks
cargo fmt --all -- --check
cargo clippy --all-targets --all-features
cargo test --all
cargo deny check
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

2. **Implement in rsb-sdk first** (core library)
   - Core logic independent of UI/CLI
   - Add tests in `crates/rsb-sdk/tests/` folder
   - Update module documentation

3. **Update CLI** (rsb-cli) if needed
   - Add command/subcommand in `src/command/`
   - Add help text via clap derive macros
   - Test with: `cargo run -p rsb-cli -- --help`

4. **Update GUI** (rsb-desktop) if needed
   - Add new screen or modify existing in `src/ui/`
   - Test on all platforms (Windows, macOS, Linux)
   - Ensure Tailwind CSS styling is consistent

5. **Update documentation:**
   - Add rustdoc comments to public API
   - Update [docs/CLI.md](docs/CLI.md) if adding commands
   - Update [docs/USER_GUIDE.md](docs/USER_GUIDE.md) for user-facing changes
   - Update this DEVELOPER_GUIDE.md if architecture changes

6. **Run full test suite:**
   ```bash
   cargo fmt --all
   cargo clippy --all-targets --all-features
   cargo test --all
   cargo deny check
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
RUST_LOG=rsb_sdk=debug,rsb_cli=info cargo run -p rsb-cli
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
