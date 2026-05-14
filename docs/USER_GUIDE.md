# RS Shield User Guide

A complete guide for end users to backup, restore, and manage their data with RS Shield.

**Made with ❤️ by [Zebedeu](https://github.com/zebedeu)**

## Table of Contents

1. [Installation](#installation)
2. [Desktop App](#desktop-app)
3. [Security KeySecurity Keys](#fido2-security-keys)
4. [Configuration](#configuration)
5. [Backup Operations](#backup-operations)
6. [Restore Operations](#restore-operations)
7. [S3 Storage Setup](#s3-storage-setup)
8. [Troubleshooting](#troubleshooting)
9. [FAQ](#faq)

---

## Installation

### macOS

```bash
# Using Homebrew (when available)
brew install rs-shield

# Or download from releases
curl -L https://github.com/yourusername/rs-shield/releases/download/latest/rs-shield-mac.dmg
# Double-click the DMG and drag RS Shield to Applications
```

---

## Security KeySecurity Keys

RS Shield supports FIDO2/WebAuthn standard for hardware-based authentication using security keys. This provides phishing-resistant, cryptographically secure authentication.

### Supported Devices

- **YubiKey** (5 Series, Security Key ) - USB, NFC, Lightning
- **Windows Hello** - Facial recognition, fingerprint, PIN
- **Touch ID / Face ID** - macOS and iOS
- **Android Biometric** - Fingerprint, face recognition
- **Titan Security Key ** - USB, NFC
- **Nitrokey** - Open-source security key

### Why Use FIDO2?

✅ **Phishing-Resistant** - Keys only work with legitimate sites/apps
✅ **Hardware-Based** - Private keys never leave the device, offering superior protection.
✅ **No Passwords** - Replaces vulnerable password authentication with cryptographic proofs.
✅ **Short-Lived Sessions** - Generates temporary, short-lived session tokens (15-30 minutes) for enhanced security.
✅ **Instant Revocation** - Utilizes unique session IDs (JTI) for immediate session revocation if needed.
✅ **Two-Factor Compatible** - Works alongside existing 2FA methods.
✅ **Industry Standard** - Supported by major providers (Google, Microsoft, Apple).

### Setting Up Security KeyAuthentication

#### Step 1: Requirements

RS Shield uses the **FIDO2/WebAuthn** standard. You can use:
- **Hardware Keys:** YubiKey, Google Titan, Nitrokey.
- **Platform Authenticators:** Touch ID (macOS), Windows Hello, or Android/iOS biometrics.

#### Step 2: Register Your Key
Before your first login, you must register your security device with a unique User ID.

```bash
# Register the security key
rsb auth register --user-id "your_name" --name "Main YubiKey"

# Follow the prompt to touch/interact with your device
```

#### Step 3: Verify Registration

```bash
# List all registered keys
rsb auth list

# Output shows your registered credentials
```

#### Step 4: Authenticate

```bash
# Use your key for authentication
rsb auth authenticate --user-id user@example.com

# Touch your security key when prompted
# Authentication succeeds without entering a password
```

### Managing Multiple Keys

You can register multiple security keys for redundancy:

```bash
# Register primary key
rsb auth register --user-id user@example.com --name "YubiKey Primary"

# Register backup key
rsb auth register --user-id user@example.com --name "YubiKey Backup"

# Both keys will work for authentication
rsb auth authenticate --user-id user@example.com
```

### Removing a Key

If your key is lost or compromised:

```bash
# List all keys
rsb auth list

# Remove the compromised key
rsb auth revoke --user-id user@example.com

# Re-register if needed
rsb auth register --user-id user@example.com --name "Replacement Key"
```

---

### Linux

```bash
# Ubuntu/Debian
sudo apt install rs-shield

# Or via package manager
cargo install --locked rs-shield-cli

# Desktop variant (if available)
cargo install --locked rs-shield-desktop
```

### Windows

```bash
# Download installer from releases
# Run: rs-shield-installer.exe

# Or via scoop
scoop install rs-shield
```

---

## Desktop App

### Starting the Application

```bash
# From installation
rs-shield

# Or build from source
./run-desktop.sh
```

### Main UI Elements

1. **Top Navigation Bar**
   - Profile selector
   - Settings
   - Help

2. **Left Sidebar**
   - Create Backup
   - Restore
   - Verify
   - Prune (cleanup)
   - Real-Time Sync
   - Dashboard

3. **Main Content Area**
   - Tab-based interface
   - Progress bars
   - Status indicators

---

## Configuration

### Creating a Backup Profile

1. **Open RS Shield Desktop App**
2. **Click "Create Backup" or "Settings" → "New Profile"**
3. **Enter Profile Details:**
   - Name: `my-documents-backup`
   - Source: `/home/user/Documents`
   - Destination: `/mnt/external-drive/backups`

4. **Configure Options:**
   - Backup Mode: `Incremental` (recommended)
   - Compression Level: `3` (balanced)
   - Encryption: ✅ Enable
   - Password: Set a strong password

5. **S3 Configuration (Optional)**
   - Click "Configure S3"
   - Select provider (AWS, MinIO, etc.)
   - Enter credentials
   - Test connection

6. **Save Profile** → Profile is now ready to use

### Profile File Location

Profile files are stored as TOML in:
- **macOS/Linux:** `~/.config/rs-shield/profiles/`
- **Windows:** `%APPDATA%\rs-shield\profiles\`

Example profile:
```toml
source_path = "/home/user/documents"
destination_path = "/mnt/backup"
backup_mode = "incremental"
compression_level = 3
encryption_key = "your-password"

[s3]
bucket = "my-backups"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"
```

---

## Backup Operations

### Starting a Backup

1. **Select Profile** from dropdown
2. **Review Settings** (click gear icon to change)
3. **Click "Start Backup"**
4. **Monitor Progress:**
   - Real-time file count
   - Data size transferred
   - Estimated time remaining
   - Speed (MB/s)

### Backup Options

| Option | Purpose |
|--------|---------|
| **Incremental** | Only backup changed files (faster, recommended) |
| **Full** | Backup all files (slower, but complete) |
| **Dry Run** | Preview changes without actually backing up |
| **Encryption** | Encrypt backup with AES-256 |
| **Compression** | Reduce backup size with Zstd |

### Real-Time Sync

Monitor a folder continuously:

1. **Open "Real-Time Sync" Tab**
2. **Select Source Folder** to monitor
3. **Select Destination** for synced files
4. **Optional:** Enable encrypted backups
5. **Click "Start"** to begin monitoring
6. **View Dashboard** for live statistics
7. **Click "Stop"** to end monitoring

Changes are detected every 2 seconds and automatically synced.

---

## Restore Operations

### Restoring from a Backup

1. **Click "Restore" Tab**
2. **Select Backup Source:**
   - Local backup folder
   - S3 bucket
3. **Verify Backup** (Recommended)
   - Checks integrity
   - Shows file count
4. **Select Restore Location** (destination to restore to)
5. **Enter Encryption Password** (if encrypted)
6. **Click "Start Restore"**
7. **Monitor Progress** until complete

### Restoring Specific Files

Use the file browser to select:
- Use search to find files
- Check/uncheck individual files
- Restore only what you need

---

## S3 Storage Setup

### AWS S3

1. **Create AWS Account** at [aws.amazon.com](https://aws.amazon.com)
2. **Create S3 Bucket:**
   - Name: `my-backups-2026`
   - Region: Select based on location
   - Block public access: ✅
   - Versioning: Optional (recommended)

3. **Create IAM User:**
   - Attach policy: `AmazonS3FullAccess` (or custom minimal policy)
   - Create access key
   - Save Access Key ID and Secret Key

4. **In RS Shield:**
   - Settings → Add S3 Connection
   - Provider: AWS S3
   - Access Key + Secret Key
   - Region: `us-east-1` (or selected region)
   - Click "Test Connection"

### MinIO (Self-Hosted)

1. **Install MinIO:**
   ```bash
   docker run -p 9000:9000 -p 9001:9001 minio/minio server /data
   ```

2. **Access MinIO Console:**
   - URL: `http://localhost:9001`
   - Default: `minioadmin:minioadmin`

3. **Create Bucket:**
   - Name: `backups`
   - Default settings

4. **Create User:**
   - Username: `backupuser`
   - Password: Generate strong password
   - Assign `readwrite` policy to bucket

5. **In RS Shield:**
   - Settings → Add S3 Connection
   - Endpoint: `http://localhost:9000`
   - Access Key + Secret Key
   - Bucket: `backups`
   - Click "Test Connection"

### DigitalOcean Spaces

Similar to AWS S3 but:
- Endpoint: `https://nyc3.digitaloceanspaces.com` (varies by region)
- Bucket name constraints are stricter
- Region format is different

---

## Troubleshooting

### Backup Fails with "Permission Denied"

**Solutions:**
1. Check source folder permissions: `ls -ld /source/path`
2. Ensure destination has write permissions
3. On Linux: Try `chmod 755 /destination/path`
4. Run as user who owns the files

### Encryption Password Issues

**"Incorrect Password"**
- Password is case-sensitive
- Check caps lock
- Try the correct password for that backup

**"Master Password Required"**
- First time setup: Enter your desired master password
- Create a password hint for recovery

### S3 Connection Failures

**"Connection Refused"**
- Check MinIO/server is running
- Verify endpoint URL (includes `http://` or `https://`)
- Check firewall rules

**"InvalidAccessKeyId"**
- Access Key is incorrect or expired
- Verify credentials in provider console
- Regenerate if necessary

**"AccessDenied"**
- Bucket name doesn't match
- User lacks permissions
- Region/endpoint mismatch

### Desktop App Won't Start

**macOS:**
```bash
# Check if integrity violation
sudo spctl --assess --verbose /Applications/rs-shield.app
# Allow in System Preferences → Security & Privacy
```

**Linux:**
```bash
# Missing dependencies
sudo apt install libgtk-3-0 libwebkit2gtk-4.0-37
```

### High CPU/Memory Usage

1. **Reduce thread count:** Settings → Performance → Threads
2. **Enable resource limits:** Settings → Efficiency → CPU Limit = 80%
3. **Enable battery saving:** Settings → Power → Suspend on Low Battery
4. **Restart application** and try again

---

## FAQ

### Q: Is my data safe?
**A:** Yes. Data is encrypted with AES-256-GCM before storing. Passwords are never saved in plain text.

### Q: Can I access backups outside RS Shield?
**A:** Local backups are just encrypted TAR files. S3 backups require decryption. You can extract manually if needed.

### Q: How much space do I need?
**A:** Depends on compression (40-70% reduction) and encryption overhead (~20 bytes per file).

### Q: Can I backup to multiple destinations?
**A:** Yes. Create multiple profiles for different destinations, or use "Advanced Settings" for multi-destination backup.

### Q: What if I forget my password?
**A:** Backups cannot be recovered without the original password. Consider writing it down securely.

### Q: Can I pause and resume a backup?
**A:** Yes. Click "Pause" and resume later from the same backup window.

### Q: Does incremental backup work with S3?
**A:** Yes. RS Shield tracks which files have changed and only uploads modified files.

### Q: How often should I backup?
**A:** 3-2-1 rule recommended:
- 3 copies: Original + 2 backups
- 2 different media: Local + Cloud
- 1 offsite: At least one copy away from primary

### Q: Can I use with cloud storage syncs (Dropbox, Drive)?
**A:** Not recommended. Backup to S3 or external drive instead.

### Q: Does RS Shield delete old backups automatically?
**A:** Use "Prune" feature to remove old incremental backups. Sets retention policy.

---

## Support

- 📖 [Documentation](https://rsbshield.co.ao)
- 🐛 [Report Issues](https://github.com/zebedeu/rs-shield/issues)
- 💬 [Discussions](https://github.com/zebedeu/rs-shield/discussions)
- 📧 Email: marciozebedeu@rsbshield.co.ao

---

*Last Updated: February 2026*
