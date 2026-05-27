# RS Shield User Guide

A complete guide for end users to backup, restore, and manage their data with RS Shield.

**Made with ❤️ by [Zebedeu](https://github.com/zebedeu)**

## Table of Contents

1. [Installation](#installation)
2. [Desktop App](#desktop-app)
3. [FIDO2 Security Keys & Authentication](#fido2-security-keys--authentication)
4. [Configuration](#configuration)
5. [Backup Operations](#backup-operations)
6. [Restore Operations](#restore-operations)
7. [Advanced Features](#advanced-features)
   - [Snapshots & Versioning](#snapshots--versioning)
   - [Scheduled Backups](#scheduled-backups)
   - [Real-Time Sync](#real-time-sync)
   - [Backup Diagnostics](#backup-diagnostics)
   - [Portable Mode](#portable-mode-new) (NEW!)
   - [Automatic Deduplication](#automatic-deduplication) (NEW!)
8. [S3 Storage Setup](#s3-storage-setup)
9. [Troubleshooting](#troubleshooting)
10. [FAQ](#faq)

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

## FIDO2 Security Keys & Authentication

RS Shield supports FIDO2/WebAuthn standard for hardware-based authentication using security keys. This provides phishing-resistant, cryptographically secure authentication.

### Supported Devices

- **YubiKey** (5 Series, Security Key) - USB, NFC, Lightning
- **Windows Hello** - Facial recognition, fingerprint, PIN
- **Touch ID / Face ID** - macOS and iOS
- **Android Biometric** - Fingerprint, face recognition
- **Titan Security Key** - USB, NFC
- **Nitrokey** - Open-source security key

### Why Use FIDO2?

✅ **Phishing-Resistant** - Keys only work with legitimate sites/apps
✅ **Hardware-Based** - Private keys never leave the device, offering superior protection.
✅ **No Passwords** - Replaces vulnerable password authentication with cryptographic proofs.
✅ **Short-Lived Sessions** - Generates temporary, short-lived session tokens (15-30 minutes) for enhanced security.
✅ **Instant Revocation** - Utilizes unique session IDs (JTI) for immediate session revocation if needed.
✅ **Two-Factor Compatible** - Works alongside existing 2FA methods.
✅ **Industry Standard** - Supported by major providers (Google, Microsoft, Apple).

### Setting Up FIDO2 Authentication

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

### Authentication Methods

RS Shield supports two main authentication methods:

#### 1. FIDO2 Security Key (Recommended)

Provides the strongest security with hardware-based authentication:

```bash
rsb login --user-id user@example.com
```

Touch your security key when prompted. No password needed.

**Advantages:**
- ✅ Phishing-resistant
- ✅ No password to forget
- ✅ Works offline
- ✅ Fast authentication

#### 2. Recovery Codes

Emergency backup authentication method:

```bash
rsb login --user-id user@example.com --recovery
# Enter recovery code when prompted
```

**Important:**
- Use recovery codes only if your security key is unavailable
- Each recovery code can be used only once
- Keep backup copies in a secure location (password manager)

#### Generating Recovery Codes

```bash
rsb auth generate-codes
# Save these codes somewhere secure!
```

Output:
```
🔐 Recovery Codes (keep safe):

1. XXXX-XXXX-XXXX-XXXX-1
2. XXXX-XXXX-XXXX-XXXX-2
3. XXXX-XXXX-XXXX-XXXX-3
...
```

### Best Practices

1. **Register Multiple Keys**
   - Primary: YubiKey in a safe
   - Backup: YubiKey in wallet
   - Tertiary: Biometric (Touch ID/Windows Hello)

2. **Store Recovery Codes**
   - Use a password manager (Bitwarden, 1Password, etc.)
   - Print and store in safe
   - Never email or cloud sync unencrypted

3. **Regular Updates**
   - Register new keys as you acquire them
   - Revoke compromised keys immediately
   - Update recovery codes quarterly

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

## Advanced Features

### Snapshots & Versioning

RS Shield automatically creates snapshots of each backup state, allowing you to access multiple versions of your data.

#### Viewing Snapshots

**Desktop App:**
1. **Select Backup** from the sidebar
2. **Click "Snapshots" Tab**
3. **View Timeline:**
   - Date and time of each snapshot
   - File count at that time
   - Data size for that snapshot

**Command Line:**
```bash
# List all snapshots
rsb snapshots list --config mybackup.toml

# Or using portable mode
rsb snapshots list --backup /backup/docs

# Show snapshot details
rsb snapshots show 2026-05-25T10:30:00 --config mybackup.toml
```

#### Comparing Snapshots (NEW!)

Compare two snapshots to see what changed:

**Desktop App:**
1. **Click Snapshots Tab**
2. **Select two snapshots**
3. **Click "Compare"**
4. **View changes:** Added, Removed, Modified files

**Command Line:**
```bash
# Compare two snapshots
rsb snapshots diff 2026-05-20T10:00:00 2026-05-25T10:00:00 --config mybackup.toml

# Or using portable backup path
rsb snapshots diff snap1 snap2 --backup /backup/docs

# Shows:
# ✅ Added files
# ❌ Removed files
# 🔄 Modified files (with size changes)
```

#### Accessing Historical Data

Restore from a specific snapshot:

**Desktop App:**
1. **Click Snapshots Tab**
2. **Select desired snapshot date**
3. **Click "Restore from this snapshot"**
4. **Choose restore location**
5. **Confirm restoration**

**Command Line:**
```bash
# Restore from specific snapshot
rsb restore --config mybackup.toml --snapshot 2026-05-25T10:30:00 --output ~/restored

# Or using portable mode
rsb restore --backup /backup/docs --snapshot snap-id --output ~/restored
```

### Snapshot Management

#### Deleting Snapshots

Remove specific snapshots to free space:

```bash
# Delete a snapshot
rsb snapshots delete 2026-05-20T10:00:00 --config mybackup.toml

# Or using backup path
rsb snapshots delete snap-id --backup /backup/docs
```

### Scheduled Backups

Automate your backups to run at specific times.

#### Using Cron (Linux/macOS)

Generate cron schedule:

```bash
rsb schedule --config docs.toml --format cron
```

Add to crontab:

```bash
crontab -e
# Paste the output from schedule command
```

Common patterns:
```bash
# Daily at 2 AM
0 2 * * * /usr/local/bin/rsb backup docs.toml

# Every 6 hours
0 */6 * * * /usr/local/bin/rsb backup docs.toml

# Every Monday at 1 AM
0 1 * * 1 /usr/local/bin/rsb backup docs.toml
```

#### Using Systemd (Linux)

Generate systemd service:

```bash
rsb schedule --config docs.toml --format systemd
```

Create service file `/etc/systemd/system/rsb-backup.service`:

```ini
[Unit]
Description=RS Shield Backup
After=network.target

[Service]
Type=oneshot
ExecStart=/usr/local/bin/rsb backup /path/to/docs.toml

[Install]
WantedBy=multi-user.target
```

Create timer `/etc/systemd/system/rsb-backup.timer`:

```ini
[Unit]
Description=RS Shield Backup Timer
Requires=rsb-backup.service

[Timer]
OnBootSec=10min
OnUnitActiveSec=6h

[Install]
WantedBy=timers.target
```

Enable and start:

```bash
sudo systemctl enable rsb-backup.timer
sudo systemctl start rsb-backup.timer
```

### Real-Time Sync

Monitor a folder and automatically backup changes as they happen.

#### CLI Usage

```bash
rsb watch --config docs.toml --sync-to /tmp/sync --key mypassword
```

Monitor output:
```
✅ Sync: 5 new/modified files synchronized.
💾 Backup #1: 1050 files total, 5 processed
```

#### With Health Checks

Monitor backup health with external service:

```bash
rsb watch --config docs.toml \
  --sync-to /tmp/sync \
  --key mypassword \
  --healthcheck-url https://healthchecks.io/ping/your-uuid
```

### Backup Diagnostics

Identify and repair backup issues.

#### Running Diagnostics

```bash
# Simple check
rsb diagnose --backup /backup/docs

# Detailed check
rsb diagnose --backup /backup/docs --verbose

# JSON output (for automation)
rsb diagnose --backup /backup/docs --json
```

#### What Gets Checked

- File integrity (hashes)
- Encryption validity
- Compression status
- Metadata consistency
- Missing or corrupt snapshots

#### Automatic Repair

Attempt to fix detected issues:

```bash
rsb diagnose --backup /backup/docs --repair
```

**Caution:** Creates backup before attempting repairs.

#### Advanced Verification

Verify without decryption (fast check):

```bash
# Via profile
rsb verify --config docs.toml

# Quick verify via backup path (hashes only)
rsb verify --backup /backup/docs --quick

# Full verify (with decryption)
rsb verify --backup /backup/docs

# Generate HTML report
rsb verify --backup /backup/docs --report
```

---

## Portable Mode (NEW!)

Portable mode allows you to create and restore backups **without needing a config file**. This is perfect for:
- External drives and USB backups
- Backing up to cloud storage
- Restoring on different computers
- Cross-platform backup workflows

### How It Works

Instead of creating a config file, use the `--backup` flag to specify your backup destination directly:

```bash
# Create portable backup
rsb backup --backup /media/external/my-backup

# Verify the backup
rsb verify --backup /media/external/my-backup

# Restore on any computer
rsb restore --backup /media/external/my-backup --output ~/restored

# List snapshots anywhere
rsb snapshots list --backup /media/external/my-backup

# Compare snapshots across computers
rsb snapshots diff snap1 snap2 --backup /media/external/my-backup
```

### Portable Backup Workflow Example

**Computer A (Backup):**
```bash
# 1. Insert external drive
# 2. Create portable backup (no config needed!)
rsb backup --backup /mnt/external-drive/backup

# 3. Verify backup
rsb verify --backup /mnt/external-drive/backup
```

**Computer B (Restore):**
```bash
# 1. Connect same external drive
# 2. List available snapshots
rsb snapshots list --backup /mnt/external-drive/backup

# 3. Restore files
rsb restore --backup /mnt/external-drive/backup --output ~/restored
```

### Config vs Portable Mode Comparison

| Feature | Config Mode | Portable Mode |
|---------|------------|---------------|
| **Setup** | Create config file | Use --backup flag |
| **Works across computers** | Limited | ✅ Yes |
| **External drives** | Limited | ✅ Perfect |
| **Use case** | Regular scheduled backups | One-time, portable backups |
| **Best for** | Desktop automation | USB drives, cloud sync |

### Auto-Discovery

All commands automatically look for `.toml` files in the current directory. You can omit the config file if it's nearby:

```bash
# If mybackup.toml is in current directory:
rsb backup          # Auto-discovers mybackup.toml
rsb verify          # Auto-discovers mybackup.toml
rsb snapshots list  # Auto-discovers mybackup.toml
```

---

## Automatic Deduplication

RS Shield automatically deduplicates your files to **save storage space and backup time**.

### How It Works

1. **Content-based hashing**: Each file is hashed based on its content
2. **Duplicate detection**: If a file with the same hash already exists in the backup, it's skipped
3. **Transparent optimization**: Happens automatically, no configuration needed

### Storage Savings Example

```
First backup:
  ├─ Documents: 100 files (500 MB)
  ├─ Photos: 50 files (2 GB)
  └─ Code projects: 30 files (50 MB)
  Total: 180 files, 2.55 GB stored

Second backup (added 20 new files):
  ├─ New documents: 10 files (50 MB) [NEW]
  ├─ New photos: 5 files (100 MB) [NEW]
  ├─ Existing files (not changed): 165 files [SKIPPED - deduplicated ✓]
  └─ Modified code: 5 files (15 MB) [NEW]
  
Result:
  - Only 20 new files processed
  - 165 duplicate files skipped
  - Backup time reduced by ~90%
  - Storage increase: 165 MB (vs 2.55 GB without dedup)
```

### Benefits

- **Reduced storage**: Identical files don't consume extra space
- **Faster backups**: Duplicate files skip compression and encryption
- **Cross-snapshot dedup**: Works across multiple backup snapshots
- **Transparent**: No manual configuration needed

### Monitoring Deduplication

When you backup, RS Shield shows:
```
💾 Backup Progress
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Files:  165 / 180
Skipped (duplicates): 15 ✓
Size: 2.45 GB / 2.55 GB
Progress: 92%
```

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

### Q: Can I access previous versions of my files?
**A:** Yes! Use Snapshots to access any previous backup state. Each snapshot represents a complete backup at a specific time.

### Q: How do I schedule automated backups?
**A:** Use `rsb schedule` command to generate cron/systemd instructions, or set up through the Desktop app's scheduler.

### Q: What if my security key is lost?
**A:** Use recovery codes to authenticate and create new security key registrations. Keep recovery codes in a safe location.

### Q: Can I repair a corrupted backup?
**A:** Use `rsb diagnose --repair` to automatically attempt repairs. Always test restoration afterward.

### Q: How do snapshots work?
**A:** Every backup creates a snapshot. You can browse snapshots, compare changes, and restore from any snapshot point in time.

---

## Support

- 📖 [Documentation](https://rsbshield.co.ao)
- 🐛 [Report Issues](https://github.com/zebedeu/rs-shield/issues)
- 💬 [Discussions](https://github.com/zebedeu/rs-shield/discussions)
- 📧 Email: marciozebedeu@rsbshield.co.ao

---

*Last Updated: February 2026*
