# RS Shield CLI Reference

Complete command-line interface documentation for RS Shield.

## Table of Contents

1. [Installation](#installation)
2. [Global Options](#global-options)
3. [Commands](#commands)
4. [Examples](#examples)
5. [Exit Codes](#exit-codes)

---

## Installation

### From Source

```bash
git clone https://github.com/yourusername/rs-shield.git
cd rs-shield
cargo install --path ./rsb-cli
```

### Via Cargo

```bash
cargo install rs-shield-cli
```

### Verify Installation

```bash
rsb --version
rsb --help
```

---

## Global Options

Available with all commands:

```
-h, --help              Print help information
-V, --version           Print version information
-v, --verbose           Enable verbose output
-q, --quiet             Suppress output messages
--log-level <LEVEL>     Set logging level (debug, info, warn, error)
```

### Environment Variables

```bash
RUST_LOG=debug          # Enable debug logging
RUST_BACKTRACE=1        # Show backtraces on error
RS_SHIELD_HOME=/path    # Configuration directory
```

---

## Commands

### Create Profile

Create a new backup profile configuration.

```bash
rsb create-profile [OPTIONS] --name <NAME> --source <SOURCE> --dest <DEST>

OPTIONS:
  -n, --name <NAME>              Profile name
  -s, --source <SOURCE>          Source directory to backup
  -d, --dest <DEST>              Destination directory
  -m, --mode <MODE>              Backup mode: "incremental" or "full" [default: incremental]
  -c, --compression <LEVEL>      Compression level 0-11 [default: 3]
  -e, --encrypt                  Enable encryption [default: false]
  -p, --password <PASSWORD>      Encryption password (prompted if not provided)
  -e, --exclude <PATTERNS>       Exclude patterns (comma-separated)
  --s3-bucket <BUCKET>           S3 bucket name
  --s3-region <REGION>           S3 region
  --s3-endpoint <ENDPOINT>       S3 endpoint URL
```

**Examples:**

```bash
# Basic profile
rsb create-profile --name docs --source ~/Documents --dest /backup/docs

# With encryption
rsb create-profile --name secure \
  --source ~/Documents \
  --dest /backup/docs \
  --encrypt \
  --compression 6

# With S3
rsb create-profile --name cloud \
  --source ~/Documents \
  --dest s3://my-backups \
  --s3-bucket my-backups \
  --s3-region us-east-1 \
  --encrypt
```

### Backup

Perform a backup operation.

```bash
rsb backup [OPTIONS] --profile <FILE>

OPTIONS:
  -p, --profile <FILE>           Profile configuration file
  -m, --mode <MODE>              Override backup mode (full/incremental)
  -c, --password <PASSWORD>      Encryption password
  -d, --dry-run                  Preview changes without backing up
  -v, --verify                   Verify backup after completion
  --no-compress                  Disable compression
  --threads <COUNT>              Number of parallel threads
  -q, --quiet                    Suppress progress output
```

**Examples:**

```bash
# Simple backup
rsb backup --profile docs.toml

# Dry run to preview
rsb backup --profile docs.toml --dry-run

# With verification
rsb backup --profile docs.toml --verify

# Full backup (override incremental)
rsb backup --profile docs.toml --mode full

# Custom thread count
rsb backup --profile docs.toml --threads 4
```

### Restore

Restore from a backup.

```bash
rsb restore [OPTIONS] --backup <PATH> --output <PATH>

OPTIONS:
  -b, --backup <PATH>            Backup source path
  -o, --output <PATH>            Restore destination
  -p, --password <PASSWORD>      Decryption password
  -f, --files <PATTERNS>         Restore specific files (pattern matching)
  -d, --date <DATE>              Restore from specific date (format: YYYY-MM-DD)
  --verify                       Verify backup before restoring
  -q, --quiet                    Suppress output
```

**Examples:**

```bash
# Full restore
rsb restore --backup /backup/docs --output ~/restored

# Specific files only
rsb restore --backup /backup/docs --output ~/restored --files "*.pdf"

# Restore specific date
rsb restore --backup /backup/docs --output ~/restored --date 2026-02-01

# From S3
rsb restore --backup s3://my-backups/docs --output ~/restored
```

### Verify

Verify backup integrity.

```bash
rsb verify [OPTIONS] --backup <PATH>

OPTIONS:
  -b, --backup <PATH>            Backup to verify
  -p, --password <PASSWORD>      Decryption password (if encrypted)
  --quick                        Quick check (skip full verification)
  -q, --quiet                    Suppress output
```

**Examples:**

```bash
# Verify backup
rsb verify --backup /backup/docs

# Quick check without decryption
rsb verify --backup /backup/docs --quick

# Verbose output
rsb verify --backup /backup/docs --verbose
```

### Prune

Remove old backup files and optimize storage.

```bash
rsb prune [OPTIONS] --retention <POLICY>

OPTIONS:
  -p, --backup <PATH>            Backup directory to prune
  -r, --retention <POLICY>       Retention policy (e.g., "30d", "6m", "1y")
  --dry-run                      Preview what would be deleted
  -q, --quiet                    Suppress output
```

**Examples:**

```bash
# Keep last 30 days
rsb prune --backup /backup/docs --retention 30d --dry-run

# Keep last 6 months
rsb prune --backup /backup/docs --retention 6m

# Keep last 1 year
rsb prune --backup /backup/docs --retention 1y
```

### List Profiles

List all available backup profiles.

```bash
rsb list-profiles [OPTIONS]

OPTIONS:
  -d, --directory <PATH>         Profile directory [default: ~/.config/rs-shield]
  -f, --format <FORMAT>          Output format: table, json, csv [default: table]
```

**Examples:**

```bash
# List profiles
rsb list-profiles

# JSON output
rsb list-profiles --format json

# Custom directory
rsb list-profiles --directory ~/.backup/profiles
```

### Configuration

Manage credentials and settings.

```bash
rsb config [OPTIONS] <COMMAND>

COMMANDS:
  set-password              Set master password
  s3-credentials           Configure S3 credentials
  list                     List current configuration
  reset                    Reset all settings [WARNING: destructive]
```

**Examples:**

```bash
# Set master password
rsb config set-password

# Configure S3
rsb config s3-credentials --bucket my-backups --region us-east-1

# List config
rsb config list

# Reset (danger!)
rsb config reset --confirm
```

### FIDO2 Authentication

Manage hardware security keys and FIDO2/WebAuthn credentials for phishing-resistant authentication.

```bash
rsb fido2 <COMMAND> [OPTIONS]

COMMANDS:
  register    Register a new FIDO2 security key
  authenticate   Authenticate using a registered FIDO2 key
  list        List all registered FIDO2 credentials
  revoke      Remove a FIDO2 credential
```

#### Register a Security Key

```bash
rsb fido2 register --user-id <USER_ID> --name <NAME>

OPTIONS:
  -u, --user-id <USER_ID>        User identifier (email or username)
  -n, --name <NAME>              Display name for the credential
```

**Examples:**

```bash
# Register YubiKey
rsb fido2 register --user-id user@example.com --name "My YubiKey"

# Register Windows Hello
rsb fido2 register --user-id admin --name "Windows Hello"

# Register Biometric (fingerprint)
rsb fido2 register --user-id marciozebedeu --name "Fingerprint Sensor"
```

#### Authenticate with Security Key

```bash
rsb fido2 authenticate --user-id <USER_ID>

OPTIONS:
  -u, --user-id <USER_ID>        User identifier to authenticate
```

**Examples:**

```bash
# Authenticate user
rsb fido2 authenticate --user-id user@example.com

# Output:
# ✅ Authentication challenge created
#    Counter: 5
```

#### List Registered Credentials

```bash
rsb fido2 list
```

**Output:**

```
📱 FIDO2 Credentials Registered

┌─────────────────────┬──────────────────────┬─────────────────────────┐
│ User ID             │ Created At           │ Last Used               │
├─────────────────────┼──────────────────────┼─────────────────────────┤
│ user@example.com    │ 2026-04-29 14:23:45  │ 2026-04-29 16:45:12    │
│ admin               │ 2026-04-25 10:15:30  │ 2026-04-29 09:00:00    │
└─────────────────────┴──────────────────────┴─────────────────────────┘
```

#### Revoke a Credential

```bash
rsb fido2 revoke --user-id <USER_ID>

OPTIONS:
  -u, --user-id <USER_ID>        User ID of credential to revoke
```

**Examples:**

```bash
# Remove credential (will ask for confirmation)
rsb fido2 revoke --user-id user@example.com

# Output:
# 🗑️ Revoking FIDO2 Credential
# Continue? (y/n): y
# ✅ Credential revoked successfully
```

---

### Version & Help

Display version and help information.

```bash
rsb --version              Print version
rsb --help                 Print general help
rsb <COMMAND> --help       Print command help
rsb help <COMMAND>         Print command help (alternative)
```

**Examples:**

```bash
rsb --version
rsb backup --help
rsb help restore
```

---

## Examples

### Daily Automated Backup

Create cron job:

```bash
# Edit crontab
crontab -e

# Add entry for daily 2 AM backup
0 2 * * * /usr/local/bin/rsb backup --profile docs.toml --quiet
```

### Encrypted Cloud Backup

```bash
# Create profile with S3
rsb create-profile --name cloud-backup \
  --source ~/Documents \
  --dest s3://my-backups \
  --s3-bucket my-backups \
  --s3-region us-east-1 \
  --s3-endpoint https://s3.amazonaws.com \
  --encrypt

# Run backup
rsb backup --profile cloud-backup.toml

# Restore later
rsb restore --backup s3://my-backups --output ~/restored
```

### Backup with Verification

```bash
# Backup and verify
rsb backup --profile docs.toml --verify

# Later, verify without restore
rsb verify --backup /backup/docs
```

### Selective Restore

```bash
# List what would be restored
rsb restore --backup /backup/docs --output /tmp/preview --dry-run

# Restore specific file types
rsb restore --backup /backup/docs --output ~/restored --files "*.pdf,*.doc*"
```

---

## Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | General error |
| 2 | Configuration error |
| 3 | File not found |
| 4 | Permission denied |
| 5 | Invalid credentials |
| 6 | Backup verification failed |
| 127 | Command not found |

---

## Tips & Tricks

### Dry Run Pattern

Always use `--dry-run` first:

```bash
rsb restore --backup /backup --output ~/restored --dry-run
# Review output, then run without --dry-run
rsb restore --backup /backup --output ~/restored
```

### Verbose Debugging

Enable debug logging:

```bash
RUST_LOG=debug rsb backup --profile docs.toml --verbose
```

### Parallel Processing

Increase threads for faster backup:

```bash
rsb backup --profile docs.toml --threads 8
```

### Compression Trade-offs

```bash
rsb backup --profile docs.toml --compression 3  # Fast (default)
rsb backup --profile docs.toml --compression 9  # Slow but small
rsb backup --profile docs.toml --no-compress    # Fastest
```

---

## Troubleshooting

### "Permission denied"

```bash
# Fix backup directory permissions
chmod 700 /backup/docs

# Run with elevated privileges if necessary
sudo rsb backup --profile docs.toml
```

### "Profile not found"

```bash
# List available profiles
rsb list-profiles

# Create if missing
rsb create-profile --name docs --source ~/Documents --dest /backup
```

### "Incorrect password"

```bash
# Try again (passwords are case-sensitive)
rsb restore --backup /backup/docs --output ~/restored
# (get prompted for password)
```

### Out of Memory

```bash
# Reduce thread count
rsb backup --profile docs.toml --threads 2

# Or use smaller profiles
# Split into multiple profiles and backup separately
```

---

## Support

- 📧 Email: marciozebedeu@rsbshield.co.ao
- 🌐 Website: [rsbshield.co.ao](https://rsbshield.co.ao)
- 🐛 Issues: [github.com/zebedeu/rs-shield](https://github.com/zebedeu/rs-shield)

## Related

- [User Guide](/docs/USER_GUIDE.md)
- [Developer Guide](/docs/DEVELOPER_GUIDE.md)
- [Security Policy](/SECURITY.md)

---

*Last Updated: February 2026*
