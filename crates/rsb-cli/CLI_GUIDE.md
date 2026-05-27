# RS Shield CLI Quick Guide

Fast reference for RS Shield command-line interface.

## Quick Start

### 1. Create a Backup Profile

```bash
rsb create-profile --name mybackup \
  --source ~/Documents \
  --dest /backup/docs \
  --mode incremental \
  --compression 6 \
  --encrypt
```

This creates `mybackup.toml` with your backup settings.

### 2. Run a Backup

```bash
# Using config file
rsb backup mybackup.toml

# Or portable mode (no config needed)
rsb backup --backup /media/external/backup
```

### 3. Verify Backup

```bash
rsb verify --config mybackup.toml

# Or with backup path
rsb verify --backup /backup/docs
```

### 4. Restore Files

```bash
rsb restore --config mybackup.toml --output ~/restored
```

---

## Common Tasks

### Backup Modes

```bash
# Incremental (default) - only changed files
rsb backup mybackup.toml

# Full backup - all files
rsb backup mybackup.toml --mode full

# Preview without saving
rsb backup mybackup.toml --dry-run

# Verify after backup
rsb backup mybackup.toml --verify
```

### Snapshot Management

```bash
# List all snapshots
rsb snapshots list --config mybackup.toml

# Show snapshot details
rsb snapshots show 2026-05-25T10:30:00 --config mybackup.toml

# Compare two snapshots (NEW!)
rsb snapshots diff snap1 snap2 --config mybackup.toml

# Delete a snapshot
rsb snapshots delete 2026-05-25T10:30:00 --config mybackup.toml
```

### Maintenance

```bash
# Remove backups older than 30 days
rsb prune --config mybackup.toml --retention 30d --dry-run

# Actually delete (without --dry-run)
rsb prune --config mybackup.toml --retention 30d

# Keep last 10 backups
rsb prune --config mybackup.toml --keep-last 10
```

### Scheduled Backups

```bash
# Get cron format for scheduling
rsb schedule --config mybackup.toml --format cron

# Get systemd format
rsb schedule --config mybackup.toml --format systemd
```

### Real-time Sync

```bash
# Watch folder and auto-backup on changes
rsb watch --config mybackup.toml --sync-to /tmp/sync --key mypassword
```

---

## Portable Mode (NEW!)

Use backups anywhere - no config file needed:

```bash
# Create portable backup
rsb backup --backup /media/external/backup

# Verify on same computer
rsb verify --backup /media/external/backup

# Restore on different computer
rsb restore --backup /media/external/backup --output ~/restored

# Compare snapshots across systems
rsb snapshots diff snap1 snap2 --backup /media/external/backup
```

---

## Deduplication (Automatic)

Duplicate files are automatically skipped:

```bash
# First backup: 100 files
rsb backup mybackup.toml

# Second backup: only 10 new files added
rsb backup mybackup.toml
# → 90 duplicates skipped automatically ✓
```

---

## Encryption

```bash
# Backup with encryption (prompted for password)
rsb backup mybackup.toml

# Restore encrypted backup
rsb restore --config mybackup.toml --key mypassword --output ~/restored

# Verify encrypted backup
rsb verify --config mybackup.toml --key mypassword
```

---

## Performance Options

```bash
# Increase threads for faster backup (default: 4)
rsb backup mybackup.toml --threads 8

# Disable compression to speed up
rsb backup mybackup.toml --no-compress

# Generate HTML report after backup
rsb backup mybackup.toml --report
```

---

## Troubleshooting

```bash
# Quick verification (hash only, faster)
rsb verify --backup /backup/docs --quick

# Fast verification (skip decryption)
rsb verify --backup /backup/docs --fast

# Quiet mode (no progress output)
rsb backup mybackup.toml --quiet

# Generate verification report
rsb verify --config mybackup.toml --report
```

---

## Configuration File Format

Create `mybackup.toml`:

```toml
[backup]
source_path = "/home/user/documents"
destination_path = "/backup/storage"
mode = "incremental"
compression_level = 3
exclude_patterns = ["*.tmp", "node_modules/"]

[encryption]
enabled = true
password = "your_password"

[s3]
enabled = false
# bucket = "my-bucket"
# region = "us-east-1"
# endpoint = "https://s3.amazonaws.com"
```

---

## Environment Variables

```bash
# Enable debug logging
RUST_LOG=debug rsb backup mybackup.toml

# Show backtraces on error
RUST_BACKTRACE=1 rsb backup mybackup.toml
```

---

## Help

```bash
# General help
rsb --help

# Command-specific help
rsb backup --help
rsb snapshots --help
rsb snapshots diff --help

# Version
rsb --version
```
