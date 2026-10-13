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
7. [Verify, Prune and the Repository Key (Desktop)](#verify-prune-and-the-repository-key-desktop)
8. [Advanced Features](#advanced-features)
   - [Snapshots & Versioning](#snapshots--versioning)
   - [Scheduled Backups](#scheduled-backups)
   - [Real-Time Sync](#real-time-sync)
   - [Backup Diagnostics](#backup-diagnostics)
   - [Portable Mode](#portable-mode-new) (NEW!)
   - [Automatic Deduplication](#automatic-deduplication) (NEW!)
9. [S3 Storage Setup](#s3-storage-setup)
10. [Troubleshooting](#troubleshooting)
11. [FAQ](#faq)

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
# Register the security key: opens a page on http://localhost:3000 in your
# browser, where you enter your User ID
rsb auth register

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
# Use your key for authentication (browser page, as for registration)
rsb auth authenticate

# Touch your security key when prompted
# Authentication succeeds without entering a password
```

### Managing Multiple Keys

You can register multiple security keys for redundancy:

```bash
# Register primary key (enter user@example.com in the browser page)
rsb auth register

# Register backup key (same User ID)
rsb auth register

# Both keys will work for authentication
rsb login user@example.com
```

### Removing a Key

If your key is lost or compromised:

```bash
# List all keys
rsb auth list

# Remove the compromised key (needs a session as that user, e.g. via a
# recovery code: rsb login user@example.com --recovery)
rsb auth delete user@example.com

# Re-register if needed
rsb auth register
```

### Authentication Methods

RS Shield supports two main authentication methods:

#### 1. FIDO2 Security Key (Recommended)

Provides the strongest security with hardware-based authentication:

```bash
rsb login user@example.com
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
rsb login user@example.com --recovery
# Enter recovery code when prompted
```

**Important:**
- Use recovery codes only if your security key is unavailable
- Each recovery code can be used only once
- Keep backup copies in a secure location (password manager)

#### Generating Recovery Codes

```bash
# Needs a session as that user (rsb login user@example.com)
rsb auth generate-codes user@example.com
# Save these codes somewhere secure!

# Or write them to a file without showing them
rsb auth generate-codes user@example.com --output ~/safe/codes.txt --no-display
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

# Or build from source (installs the `rsb-cli` binary)
git clone https://github.com/zebedeu/rs-shield.git
cd rs-shield
cargo install --locked --path crates/rsb-cli
```

The command-line examples in this guide call the CLI `rsb`; the installed
binary is `rsb-cli`, so add `alias rsb=rsb-cli` to your shell profile (or
type `rsb-cli`).

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

After signing in with your FIDO2 key (or a recovery code), the left
sidebar opens the screens. Each one does what the matching CLI command
does, with the same SDK code:

| Screen | CLI equivalent |
|--------|----------------|
| Create Profile / Manage Profiles | `rsb create-profile`, `rsb list-profiles` |
| Backup | `rsb backup <profile>` (`--dry-run`, `--verify`) |
| Restore | `rsb restore` (`--snapshot`, `--date`, `--files`, `--force`, `--versioned`, `--verify`) |
| Integrity / Diagnostics | `rsb diagnose --backup <dir>` |
| Verify | `rsb verify` (`--snapshot`, `--fast`, `--repair`) |
| Prune | `rsb prune` (`-l N` or age, `--dry-run`, `--confirm`) |
| Real-Time Sync | `rsb watch` |
| Snapshots | `rsb snapshots` |
| Schedule | `rsb schedule` |
| FIDO2 | `rsb fido2 list`, `rsb fido2 generate-codes` |
| Repository key | `rsb key export/import/change-password` |
| Settings | S3 settings and session credentials, app defaults |

The operation screens take either a **profile** (its source, repository —
local or S3 — and key are used exactly as by the CLI; editing a path
afterwards switches to working without the profile), a **repository
folder** (like `--backup <dir>`), or the **S3 bucket** configured in
Settings.

**Encryption key.** When the key field is left blank, the key is looked
up like in the CLI: `RSB_ENCRYPTION_KEY`, then the profile's entry in the
OS keyring (written by Create Profile or `rsb create-profile --encrypt`),
then a legacy key in the TOML. A profile with encryption enabled is never
backed up without a key.

HTML reports are written to `~/.rs-shield/reports/`. Notifications
configured in **Integrations** (`~/.rs-shield/integrations.json`, mode
0600) are sent after backup, restore and verify.

---

## Configuration

### Creating a Backup Profile

1. Open **Create Profile**.
2. Enter a name (letters, digits, `-`, `_`, `.`), the source folder and the
   destination folder. For an S3 profile, open **S3 Configuration** and
   enter the bucket (the destination can then stay empty: it becomes
   `s3://<bucket>`); access keys entered there are saved encrypted in
   `~/.rs-shield/s3_credentials.enc`, never in the profile.
3. Leave **Encrypt backups** on and type the password twice: it is stored
   in the OS keyring for this profile, never in the TOML file.
4. Click **Create Profile**. An existing profile with the same name is
   never overwritten.

### Profile File Location

Profiles created by the desktop app are stored as TOML files (mode 0600)
in `~/.rsb-desktop/` (`%USERPROFILE%\.rsb-desktop\` on Windows); the CLI
writes them in the current directory. Both read either.

Example profile:
```toml
source_path = "/home/user/documents"
destination_path = "/mnt/backup"
backup_mode = "incremental"
compression_level = 3
encrypt_patterns = ["*"]   # encryption on; the password is not stored here

[s3]
bucket = "my-backups"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"
```

---

## Backup Operations

### Starting a Backup

1. Open **Backup** and load a profile (📂), or type a source and a
   destination folder (the options of Settings are then used).
2. Optionally tick **Dry-Run** (nothing is written), **Verify the backup
   afterwards** or **Generate report**.
3. Click **Start Backup**. The status shows the files processed and lists
   the files that failed, if any.
4. **Cancel** stops after the files in progress; no snapshot is written
   for a cancelled backup.

### Backup Options

| Option | Purpose |
|--------|---------|
| **Incremental** | Only backup changed files (faster, recommended) |
| **Full** | Backup all files (slower, but complete) |
| **Dry Run** | Preview changes without actually backing up |
| **Encryption** | Encrypt backup with AES-256 |
| **Compression** | Reduce backup size with Zstd |

### Real-Time Sync

Monitor a folder continuously (same engine as `rsb watch`):

1. Open **Real-Time Sync**.
2. Load a profile, or select the folder to monitor and the backup
   (repository) folder.
3. Select the **synchronization folder**, which receives a copy of new and
   modified files.
4. Type the encryption password (optional with a profile whose key is in
   the keyring): these backups are always encrypted.
5. Click **Start**. An initial backup runs, then each burst of changes is
   mirrored and backed up (incremental) once the folder is quiet for 2
   seconds. Failed backups are retried; the ignore patterns are applied.
6. **Stop** lets a running backup finish and reports changes that were not
   backed up yet.

---

## Restore Operations

### Restoring from a Backup

1. Open **Restore**.
2. Choose the source: load a profile, pick the repository folder (or
   **Load backup directly**, which validates it first), or leave both empty
   to restore from the S3 bucket configured in Settings.
3. Select the restore target folder.
4. Optionally choose a snapshot ID or a date, and files to restore.
5. Existing files in the target are kept unless **Overwrite existing
   files** (`--force`) is ticked; **Restore into a new folder**
   (`--versioned`) restores into `restore_<timestamp>`; **Verify the
   snapshot afterwards** checks it (`--verify`).
6. Click **Start Restore**.

### Restoring Specific Files

In the Restore tab, type one pattern per line in **Files to restore**:
an exact path (`notes/todo.txt`), a folder (`photos/`) or a glob
(`**/*.pdf`), relative to the backup source. Leave it empty to restore
everything. If nothing matches, the restore fails and nothing is written.

From the command line:

```bash
rsb restore --backup /backup/docs --target ~/restored --files photos/ '**/*.pdf'
```

### Restoring From a Date

Fill **Date** in the Restore tab (leave the snapshot ID empty), or use
`--date`: RS Shield restores the latest snapshot taken at or before that
moment. `2026-10-01` means the end of that day and `2026-10-01 12:00` the end
of that minute, both in your computer's local time; RFC 3339
(`2026-10-01T12:00:00Z`) is an exact instant.

```bash
rsb restore --backup /backup/docs --target ~/restored --date 2026-10-01
```

See [docs/CLI.md](CLI.md#restore) for the full pattern and date rules.

## Verify, Prune and the Repository Key (Desktop)

- **Verify** checks the latest snapshot (or the one entered). A
  verification that finds problems is shown as a failure, with the
  problems listed. **Quick** checks presence, size and stored hash without
  decrypting; **Repair** deletes objects proven corrupt so the next backup
  uploads them again.
- **Prune** keeps the last N snapshots or the snapshots of the last N
  days (the newest is always kept). **Preview** lists what would be
  deleted; deleting requires ticking the confirmation box. An encrypted
  repository is never pruned without its key.
- **Repository key** exports `keys/repo.key` (JSON or printable paper
  form), imports an exported key file and changes the repository
  password. Keep an export: without `keys/repo.key` encrypted backups
  cannot be read, even with the password.
- **Schedule** shows the cron line or systemd service of `rsb schedule`
  for a profile; the key is read from `~/.rs-shield/rsb-backup.env`
  (mode 0600) and never written into the job. Nothing is installed
  automatically.

---

## Advanced Features

### Snapshots & Versioning

RS Shield automatically creates snapshots of each backup state, allowing you to access multiple versions of your data.

#### Viewing Snapshots

**Desktop App:**
1. **Click the "Snapshots" Tab**
2. **Choose the repository:** a profile (its local destination or S3
   bucket), a backup folder, or neither to use the S3 bucket configured in
   the app. Enter the encryption key for an encrypted backup (or leave it
   empty to use `RSB_ENCRYPTION_KEY` or the profile's saved key).
3. **View the list**, newest first:
   - Date and time (UTC) of each snapshot
   - File count and size
   - 🔒 for encrypted snapshots; without the key only their id and date
     are shown
4. **Click "View"** for the file list of a snapshot

The tab remembers the repository while the app runs and reloads it when you
come back, e.g. after a backup or a prune. Errors (wrong key, missing
credentials, not a backup folder) are shown above the list.

**Command Line:**
```bash
# List all snapshots
rsb snapshots list --config mybackup.toml

# Or using portable mode
rsb snapshots list --backup /backup/docs

# Show snapshot details
rsb snapshots show 2026-05-25T103000.000Z --config mybackup.toml
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
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --config mybackup.toml

# Or using portable backup path
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --backup /backup/docs

# Shows:
# ✅ Added files
# ❌ Removed files
# 🔄 Modified files (with size changes)
```

#### Accessing Historical Data

Restore from a specific snapshot:

**Desktop App:**
1. **Find the snapshot id** in the Snapshots tab
2. **Open the Restore tab** and enter it as the snapshot ID (or give a date)
3. **Choose restore location** and start the restore

**Command Line:**
```bash
# Restore from specific snapshot
rsb restore mybackup.toml --snapshot 2026-05-25T103000.000Z --target ~/restored

# Or using portable mode
rsb restore --backup /backup/docs --snapshot 2026-05-25T103000.000Z --target ~/restored
```

### Snapshot Management

#### Deleting Snapshots

Remove specific snapshots to free space. The data that only the deleted
snapshot used is deleted too (the same clean-up as prune); deleting the last
snapshot keeps its data. In the desktop app, use **Delete** in the
Snapshots tab and confirm.

```bash
# Preview what would be deleted
rsb snapshots delete 2026-05-20T100000.000Z --config mybackup.toml --dry-run

# Delete a snapshot
rsb snapshots delete 2026-05-20T100000.000Z --config mybackup.toml --confirm

# Or using backup path
rsb snapshots delete 2026-05-20T100000.000Z --backup /backup/docs --confirm
```

### Scheduled Backups

Automate your backups to run at specific times.

#### Using Cron (Linux/macOS)

Generate cron schedule:

```bash
rsb schedule docs.toml --format cron
```

It prints how to store the encryption key in `~/.rs-shield/rsb-backup.env`
(mode 0600, `RSB_ENCRYPTION_KEY=...`) and the crontab line, which loads that
file before running the backup. Add it to crontab:

```bash
crontab -e
# Paste the output from schedule command
```

Common patterns (change the schedule fields of the generated line):
```bash
# Daily at 2 AM
0 2 * * * set -a && . "$HOME/.rs-shield/rsb-backup.env" && set +a && /usr/local/bin/rsb-cli backup /home/user/docs.toml --storage local

# Every 6 hours
0 */6 * * * set -a && . "$HOME/.rs-shield/rsb-backup.env" && set +a && /usr/local/bin/rsb-cli backup /home/user/docs.toml --storage local

# Every Monday at 1 AM
0 1 * * 1 set -a && . "$HOME/.rs-shield/rsb-backup.env" && set +a && /usr/local/bin/rsb-cli backup /home/user/docs.toml --storage local
```

#### Using Systemd (Linux)

Generate systemd service:

```bash
rsb schedule docs.toml --format systemd
```

Create service file `/etc/systemd/system/rsb-backup.service`:

```ini
[Unit]
Description=RS Shield Backup
After=network.target

[Service]
Type=oneshot
EnvironmentFile=/home/user/.rs-shield/rsb-backup.env
ExecStart=/usr/local/bin/rsb-cli backup /path/to/docs.toml --storage local

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
# Prompts for the encryption password (or reads RSB_ENCRYPTION_KEY)
rsb watch docs.toml --sync-to /tmp/sync
```

Monitor output:
```
✅ Sync: 5 new/modified files synchronized.
💾 Backup #1: 1050 files total, 5 processed
```

#### With Health Checks

Monitor backup health with external service:

```bash
rsb watch docs.toml \
  --sync-to /tmp/sync \
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
rsb verify docs.toml

# Quick verify via backup path (hashes only)
rsb verify --backup /backup/docs --quick

# Full verify (with decryption)
rsb verify --backup /backup/docs

# Generate HTML report
rsb verify --backup /backup/docs --report

# Delete objects proven corrupt; the next backup uploads them again
rsb verify docs.toml --repair
```

---

## Portable Mode (NEW!)

Portable mode allows you to verify, inspect, prune and restore backups **without needing a config file**. This is perfect for:
- External drives and USB backups
- Restoring on different computers
- Cross-platform backup workflows

### How It Works

Backing up always uses a profile; `create-profile --portable` stores relative
paths so the profile can move with the drive (the profile is the `.toml` in
the current directory; no copy is kept in the backup directory). `restore`,
`verify`, `prune` and `snapshots` can point the `--backup` flag at the backup
directory directly. For `backup`, `schedule` and `watch`, `--backup <DIR>`
picks the profile (in DIR or the current directory) whose destination is
DIR, and fails with instructions if there is none or several:

```bash
# Create a portable profile and back up
rsb create-profile --name my-backup --source ~/Documents \
  --dest /media/external/my-backup --portable
rsb backup my-backup.toml
# ...or let rsb find the profile that backs up to the drive
rsb backup --backup /media/external/my-backup

# Verify the backup
rsb verify --backup /media/external/my-backup

# Restore on any computer
rsb restore --backup /media/external/my-backup --target ~/restored

# List snapshots anywhere
rsb snapshots list --backup /media/external/my-backup

# Compare snapshots across computers
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --backup /media/external/my-backup

# Keep only the 10 newest snapshots (key from RSB_ENCRYPTION_KEY or the prompt)
rsb prune --backup /media/external/my-backup --keep-last 10 --confirm
```

### Portable Backup Workflow Example

**Computer A (Backup):**
```bash
# 1. Insert external drive
# 2. Back up with a profile whose destination is on the drive
rsb backup my-backup.toml

# 3. Verify backup
rsb verify --backup /mnt/external-drive/backup
```

**Computer B (Restore):**
```bash
# 1. Connect same external drive
# 2. List available snapshots
rsb snapshots list --backup /mnt/external-drive/backup

# 3. Restore files (no config needed!)
rsb restore --backup /mnt/external-drive/backup --target ~/restored
```

### Config vs Portable Mode Comparison

| Feature | Config Mode | Portable Mode |
|---------|------------|---------------|
| **Setup** | Create config file | Use --backup flag (restore, verify, prune, snapshots) |
| **Works across computers** | Limited | ✅ Yes |
| **External drives** | Limited | ✅ Perfect |
| **Use case** | Regular scheduled backups | One-time, portable backups |
| **Best for** | Desktop automation | USB drives, cloud sync |

### Auto-Discovery

`backup`, `prune`, `schedule`, `watch` and the `snapshots` commands look for a profile `.toml` in the current directory. You can omit the config file if it is the only profile there (with several, the command lists them and asks you to pick one). `restore` and `verify` always need a profile or `--backup`:

```bash
# If mybackup.toml is the only profile in current directory:
rsb backup          # Auto-discovers mybackup.toml
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
**A:** Use `rsb verify --repair` (deletes objects proven corrupt so the next backup uploads them again) or `rsb diagnose --backup <DIR> --repair` to attempt repairs. Always test restoration afterward.

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
