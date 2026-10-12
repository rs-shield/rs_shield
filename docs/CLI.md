# RS Shield CLI Reference

Complete command-line interface documentation for RS Shield.

## Table of Contents

1. [Installation](#installation)
2. [Configuration](#configuration)
3. [Commands](#commands)
   - [Profile Management](#profile-management)
   - [Backup & Restore](#backup--restore)
   - [Repository Key](#repository-key-format-v3)
   - [Snapshot Management](#snapshot-management)
   - [Maintenance](#maintenance)
   - [Authentication](#authentication)
   - [Diagnostics](#diagnostics)
   - [Credentials & Settings](#credentials--settings-rsb-config)
4. [Portability Guide](#portability-guide)
5. [Deduplication](#deduplication)
6. [Examples](#examples)
7. [Exit Codes](#exit-codes)
8. [Tips & Tricks](#tips--tricks)
9. [Troubleshooting](#troubleshooting)

---

## Installation

### From Source

```bash
git clone https://github.com/zebedeu/rs-shield.git
cd rs-shield
cargo install --path ./crates/rsb-cli
```

The binary is installed as `rsb-cli`. The examples in this document call it
`rsb`; create an alias (or a symlink) to type them as shown:

```bash
alias rsb=rsb-cli
```

### Verify Installation

```bash
rsb --version
rsb --help
```

---

## Configuration

### Profile Format

Profiles are TOML configuration files defining backup parameters:

```toml
source_path = "/home/user/documents"
destination_path = "/backup/storage"
backup_mode = "incremental"             # or "full"
compression_level = 3                   # 0-22 (zstd; 0 = zstd default)
exclude_patterns = ["*.tmp", "node_modules"]
encrypt_patterns = ["*"]                # set by --encrypt

# Optional: back up to S3 instead of destination_path
# [s3]
# bucket = "my-bucket"
# region = "us-east-1"
# endpoint = "https://s3.amazonaws.com"
```

Secrets are not stored in the profile: the encryption password comes from
the `RSB_ENCRYPTION_KEY` environment variable, the OS keyring (saved by
`create-profile --encrypt`) or an interactive prompt, and S3 credentials
from `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` or
`rsb config s3-credentials` (see [MIGRATION.md](MIGRATION.md)).

### Creating a Profile

```bash
# --name, --source and --dest are required; writes mybackup.toml in the
# current directory and prompts for the encryption password
rsb create-profile \
  --name mybackup \
  --source ~/Documents \
  --dest /backup/docs \
  --mode incremental \
  --compression 6 \
  --encrypt
```

---

## Commands

### Profile Management

#### Create Profile

```bash
rsb create-profile [OPTIONS] --name <NAME> --source <SOURCE> --dest <DEST>

OPTIONS:
  -n, --name <NAME>             Profile name (writes <NAME>.toml)
  -s, --source <SOURCE>         Source directory to backup
  -d, --dest <DEST>             Destination directory
  -m, --mode <MODE>             Backup mode: incremental | full [default: incremental]
  -z, --compression <LEVEL>     Compression level 0-22 (zstd) [default: 3]
  -e, --encrypt                 Enable encryption (prompts for the password)
  -k, --password <PASSWORD>     Encryption password (deprecated: visible in
                                `ps`/shell history; omit it to be prompted)
  -x, --exclude <PATTERNS>      Exclude patterns (comma-separated)
  -b, --s3-bucket <BUCKET>      S3 bucket name
  -r, --s3-region <REGION>      S3 region
  -E, --s3-endpoint <ENDPOINT>  S3 endpoint
  --portable                    Store relative paths (move the profile between computers)
```

#### List Profiles

```bash
rsb list-profiles [OPTIONS]

OPTIONS:
  -d, --dir <DIRECTORY>         Profile directory [default: ~/.config/rs-shield]
  -f, --format <FORMAT>         Output format: table | json | csv [default: table]
```

**Examples:**

```bash
# List profiles
rsb list-profiles

# JSON output
rsb list-profiles --format json

# Custom directory
rsb list-profiles --dir ~/.backup/profiles
```

### Backup & Restore

#### Backup

Perform a backup operation.

```bash
rsb backup [OPTIONS] [CONFIG]

ARGUMENTS:
  [CONFIG]                      Path to the profile .toml (optional: auto-discovered
                                when the current directory holds exactly one profile)

OPTIONS:
  -b, --backup <BACKUP>         Used as the profile path when CONFIG is omitted
  -m, --mode <MODE>             Override backup mode: full | incremental
  -k, --key <KEY>               Encryption password (deprecated: prefer
                                RSB_ENCRYPTION_KEY, the keyring or the prompt)
  --storage <STORAGE>           s3 | local (default: what the profile configures)
  -d, --dry-run                 Preview changes without modifying files
  --no-resume                   Don't resume interrupted backups
  -v, --verify                  Verify backup integrity after completion
  --no-compress                 Disable compression for this backup
  -t, --threads <THREADS>       Number of parallel threads [default: 4]
  -r, --report                  Generate HTML backup report
  -H, --healthcheck-url <URL>   Healthchecks.io URL for monitoring
```

**Usage Examples:**

```bash
# Standard backup (using config file)
rsb backup mybackup.toml

# Encrypted backup without a prompt (cron, CI): key from the environment
RSB_ENCRYPTION_KEY='your-password' rsb backup mybackup.toml

# Force local storage even if the profile has an S3 bucket
rsb backup mybackup.toml --storage local

# With verification
rsb backup mybackup.toml --verify

# Dry run to preview
rsb backup mybackup.toml --dry-run

# Full backup (override incremental setting)
rsb backup mybackup.toml --mode full

# With custom threads
rsb backup mybackup.toml --threads 8

# Generate HTML report
rsb backup mybackup.toml --report
```

When `--storage` is omitted, a profile with an S3 bucket backs up to S3 and
any other profile backs up locally (in an interactive terminal you are asked
which storage to use). `backup`, `restore`, `verify`, `prune`, `watch`,
`snapshots delete`, `diagnose --repair` and the `key` commands need a
session from `rsb login <USER_ID>`.

#### Restore

Restore files from a backup.

```bash
rsb restore [CONFIG] --target <DIR> [OPTIONS]

OPTIONS:
  [CONFIG]                      Path to config.toml (or use --backup)
  -b, --backup <BACKUP>         Path to backup directory (portable mode)
  -t, --target <DIR>            Restore destination directory (required)
  -s, --snapshot <ID>           Restore a specific snapshot (default: latest)
  -d, --date <WHEN>             Restore the latest snapshot taken at or before WHEN
  -i, --files <PATTERN>...      Restore only matching files (repeatable)
  -k, --key <KEY>               Decryption password (prefer RSB_ENCRYPTION_KEY)
  -f, --force                   Overwrite existing files
  -V, --versioned               Restore into <DIR>/restore_<timestamp>
  -v, --verify                  Verify the snapshot after restoring
  -r, --report                  Generate an HTML report
```

**Examples:**

```bash
# Restore using config file
rsb restore mybackup.toml --target ~/restored

# Restore using backup path (portable)
rsb restore --backup /backup/docs --target ~/restored

# Restore specific snapshot
rsb restore mybackup.toml --snapshot 2026-05-25T103000.000Z --target ~/restored

# Restore with verification
rsb restore mybackup.toml --target ~/restored --verify

# Only some files: an exact path, a whole folder and a glob
rsb restore --backup /backup/docs --target ~/restored \
  --files notes/todo.txt --files photos/ --files '**/*.pdf'

# The backup as it was on 1 October 2026 (latest snapshot of that day)
rsb restore --backup /backup/docs --target ~/restored --date 2026-10-01

# ...as it was at noon, local time, or at an exact instant
rsb restore --backup /backup/docs --target ~/restored --date "2026-10-01 12:00"
rsb restore --backup /backup/docs --target ~/restored --date 2026-10-01T12:00:00Z
```

**Selecting files (`--files`).** Patterns are matched against the paths
stored in the snapshot: relative to the backup source, with `/` separators on
every platform (`\` is accepted and read as `/`). Restored files keep their
relative paths under `--target`.

| Pattern | Restores |
|---------|----------|
| `docs/report.pdf` | that file |
| `docs` | everything under `docs/` (and a file named `docs`, if any) |
| `docs/` | everything under `docs/` |
| `docs/*.pdf` | PDFs directly in `docs/` (`*` and `?` never cross `/`) |
| `*.txt` | `.txt` files at the top level only |
| `**/*.txt` | `.txt` files at any depth |
| `photos/202[45]` | everything under `photos/2024/` and `photos/2025/` |

* Globs use the `glob` crate syntax (`*`, `?`, `[...]`, `**`) and are anchored
  at the backup root; a glob that matches a folder restores the whole folder.
  Quote globs so the shell does not expand them.
* Matching is case sensitive and does not normalize Unicode.
* Absolute paths (`/etc`, `C:\...`) and `..` components are rejected.
* `--files` takes several values (`--files a b`) and can be repeated. Put the
  config path before `--files` (or use `--backup`), otherwise it is read as a
  pattern.
* If no pattern matches any file of the snapshot, the restore fails (non-zero
  exit) and nothing is written. A pattern that matches nothing while others do
  is reported as a warning.

**Choosing a snapshot by date (`--date`).** Restores the most recent snapshot
taken at or before WHEN (a snapshot taken exactly at WHEN qualifies). The
chosen snapshot id is printed and `--verify` checks that same snapshot.

| WHEN | Meaning |
|------|---------|
| `2026-10-01T12:00:00Z`, `2026-10-01T14:00:00+02:00` | that exact instant (RFC 3339) |
| `2026-10-01` | the end of that day (23:59:59.999), **local time** |
| `"2026-10-01 12:00"` | the end of that minute (12:00:59.999), local time |
| `"2026-10-01 12:00:30"` | the end of that second, local time |

Local times use the time zone of the machine running `rsb` (`T` may replace
the space). If every snapshot is newer than WHEN the command fails and names
the oldest available snapshot. `--date` and `--snapshot` cannot be combined.
Snapshot times come from the snapshot names (UTC), as listed by
`rsb snapshots list`.

#### Verify

Verify backup integrity.

```bash
rsb verify [OPTIONS] [CONFIG]

ARGUMENTS:
  [CONFIG]                      Path to config.toml (or use --backup)

OPTIONS:
  -b, --backup <BACKUP>         Path to backup directory
  -s, --snapshot <ID>           Verify specific snapshot
  -k, --key <KEY>               Decryption password (prefer RSB_ENCRYPTION_KEY)
  --quick                       Quick verification (hash only, skip decryption)
  -f, --fast                    Fast verification (hash-only, no decryption)
  --repair                      Delete objects proven corrupt (forces a full
                                check) so the next backup re-uploads them
  -q, --quiet                   Suppress output
  -r, --report                  Generate HTML verification report
```

`verify` exits with a non-zero code when it finds problems.

**Examples:**

```bash
# Verify using config
rsb verify mybackup.toml

# Verify using backup path
rsb verify --backup /backup/docs

# Quick verification (hash only)
rsb verify --backup /backup/docs --quick

# Verify specific snapshot
rsb verify mybackup.toml --snapshot 2026-05-25T103000.000Z

# Generate report
rsb verify mybackup.toml --report

# Remove corrupt objects; the next backup uploads them again
rsb verify mybackup.toml --repair
rsb backup mybackup.toml
```

### Repository Key (format v3)

Encrypted repositories keep a random data key (DEK) in `keys/repo.key`,
wrapped with your password (Argon2id). The first encrypted backup creates it
automatically. See [FORMAT.md](FORMAT.md) for details. Every `rsb key`
command requires an authenticated session (`rsb login`) and only reads
passwords from an interactive prompt (never from arguments or the
environment).

```bash
# Create keys/repo.key explicitly (upgrades an existing v1/v2 repository;
# enter the password of the existing backups). If v1/v2 data exists, it
# shows the object count/size and offers to re-encrypt it as v3 right away
# (`--yes` skips the question, e.g. in scripts).
rsb key init mybackup.toml
rsb key init --repo /path/to/backup --yes

# Re-encrypt every v1/v2 (pre-v3) object and manifest as v3, verify the
# whole repository, then drop the old password from the key file.
# Resumable: after an interruption (crash, Ctrl-C) just run it again.
rsb key migrate mybackup.toml

# Change the password (rewraps the key; no backup data is rewritten).
# Refused while v1/v2 data remains: run `rsb key migrate` first.
rsb key change-password mybackup.toml

# Optional Argon2 tuning (minimums: --m-cost-kib 19456, --t-cost 2)
rsb key change-password mybackup.toml --m-cost-kib 131072 --t-cost 3

# Back up the key file: to a file created with mode 0600 ...
rsb key export mybackup.toml --output ~/safe/repo.key
# ... or as a printable page (base32 groups, a checksum per line)
rsb key export mybackup.toml --paper

# Restore a lost or damaged key file (JSON or the retyped paper form).
# Checks the password and that the key decrypts this repository; refuses to
# replace a different existing key file unless --force (which backs it up
# to keys/repo.key.bak-<timestamp> first).
rsb key import ~/safe/repo.key mybackup.toml
rsb key import paper-key.txt --repo /path/to/backup --force

# Generate a new DEK and re-encrypt every object and manifest under it
# (requires a completed `rsb key migrate`). Resumable: run it again after
# an interruption. Old exports of the key file stop working afterwards.
rsb key rotate mybackup.toml
```

| Command | Rewrites data | Needs exclusive lock | Resumable |
|---------|---------------|----------------------|-----------|
| `init` | no (unless it runs the migration) | yes | — |
| `migrate` | v1/v2 objects and manifests | yes | yes |
| `change-password` | no | yes | — |
| `export` | no | no | — |
| `import` | no (writes `keys/repo.key`) | yes | — |
| `rotate` | every encrypted object and manifest | yes | yes |

Notes:

* An export holds the DEK wrapped by the password: it is useless without the
  password, but together with it decrypts every backup. Store it like the
  password. The unwrapped DEK is never printed.
* While `migrate` or `rotate` is interrupted, backups, restores, verify and
  prune keep working (every object is either fully old or fully new).
  `verify --fast` may report stored-hash mismatches until the command is
  run again, because manifests are updated last; a full `verify` passes.
* After a rotation, keep the new key file (`rsb key export`) and discard old
  exports and copies: they no longer open the data.

> ⚠️ Keep a copy of `keys/repo.key` (`rsb key export`): without it, v3
> backups cannot be decrypted, even with the password.

### Snapshot Management

Manage backup snapshots and versions.

#### List Snapshots

```bash
rsb snapshots list [OPTIONS]

OPTIONS:
  -c, --config <CONFIG>         Path to config.toml (optional if .toml in current dir)
  -b, --backup <BACKUP>         Path to backup directory (portable mode)
  -k, --key <KEY>               Decryption key for encrypted manifests
```

**Examples:**

```bash
# List snapshots from config
rsb snapshots list --config mybackup.toml

# List snapshots from backup path
rsb snapshots list --backup /backup/docs

# List encrypted snapshots (key from the environment instead of --key)
RSB_ENCRYPTION_KEY='your-password' rsb snapshots list --backup /backup/docs
```

#### Show Snapshot Details

```bash
rsb snapshots show [OPTIONS] <ID>

ARGUMENTS:
  <ID>                          Snapshot identifier

OPTIONS:
  -c, --config <CONFIG>         Path to config.toml
  -b, --backup <BACKUP>         Path to backup directory
  -k, --key <KEY>               Decryption key
```

**Examples:**

```bash
# Show snapshot details
rsb snapshots show 2026-05-25T103000.000Z --config mybackup.toml

# Show details from backup path
rsb snapshots show 2026-05-25T103000.000Z --backup /backup/docs
```

#### Compare Snapshots (NEW)

Compare two snapshots to see what changed.

```bash
rsb snapshots diff [OPTIONS] <FROM> <TO>

ARGUMENTS:
  <FROM>                        First snapshot ID
  <TO>                          Second snapshot ID

OPTIONS:
  -c, --config <CONFIG>         Path to config.toml
  -b, --backup <BACKUP>         Path to backup directory (portable mode)
  -k, --key <KEY>               Decryption key
```

**Examples:**

```bash
# Compare two snapshots
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --config mybackup.toml

# Compare using portable backup path
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --backup /backup/docs

# Output shows:
# ✅ Added (N files)
# ❌ Removed (N files)  
# 🔄 Modified (N files)
```

#### Delete Snapshot

```bash
rsb snapshots delete [OPTIONS] <ID>

ARGUMENTS:
  <ID>                          Snapshot identifier

OPTIONS:
  -c, --config <CONFIG>         Path to config.toml
  -b, --backup <BACKUP>         Path to backup directory
```

**Examples:**

```bash
# Delete a snapshot (needs a session from `rsb login`)
rsb snapshots delete 2026-05-20T100000.000Z --config mybackup.toml
```

Snapshot ids are the names printed by `rsb snapshots list` (UTC, for example
`2026-05-25T103000.000Z`).

### Maintenance

#### Prune

Remove old backups and optimize storage.

```bash
rsb prune [OPTIONS] [CONFIG]

ARGUMENTS:
  [CONFIG]                      Path to config.toml (optional: auto-discovered)

OPTIONS:
  -b, --backup <BACKUP>         Used as the profile path when CONFIG is omitted
  -l, --keep-last <COUNT>       Keep last N backups (the short flag was -k
                                before 0.1.0-alpha.9)
  -r, --retention <POLICY>      Retention policy: "30d", "6m", "1y", or number
  -d, --dry-run                 Preview deletions without removing
  --confirm                     Required to actually delete (unless --dry-run)
  -q, --quiet                   Suppress output
  -H, --healthcheck-url <URL>   Healthchecks.io URL
```

**Examples:**

```bash
# Preview what would be deleted
rsb prune mybackup.toml --retention 30d --dry-run

# Keep last 6 months
rsb prune mybackup.toml --retention 6m --confirm

# Keep last 10 backups
rsb prune mybackup.toml --keep-last 10 --confirm
rsb prune mybackup.toml -l 10 --confirm
```

#### Schedule

Display scheduling instructions for automated backups.

```bash
rsb schedule [OPTIONS] [CONFIG]

ARGUMENTS:
  [CONFIG]                      Path to config.toml (optional: auto-discovered)

OPTIONS:
  -b, --backup <BACKUP>         Used as the profile path when CONFIG is omitted
  -f, --format <FORMAT>         Output format: cron | systemd [default: cron]
```

**Examples:**

```bash
# Get cron format
rsb schedule mybackup.toml --format cron
# Output (the key is read from a 0600 env file, never passed as --key):
# # 1) Store the encryption key in a file readable only by you:
# #    (umask 077 && printf 'RSB_ENCRYPTION_KEY=%s\n' 'YOUR_KEY' > "/home/user/.rs-shield/rsb-backup.env")
# # 2) Add this line to your crontab (crontab -e):
# 0 3 * * * set -a && . "/home/user/.rs-shield/rsb-backup.env" && set +a && "/usr/local/bin/rsb-cli" backup "/home/user/mybackup.toml" --storage local

# Get systemd format
rsb schedule mybackup.toml --format systemd
```

#### Watch

Enable real-time file synchronization and automated backups.

```bash
rsb watch [OPTIONS] --sync-to <SYNC_TO> [CONFIG]

ARGUMENTS:
  [CONFIG]                      Path to config.toml (optional: auto-discovered)

OPTIONS:
  -b, --backup <BACKUP>         Used as the profile path when CONFIG is omitted
  -s, --sync-to <PATH>          Directory to sync changes to (required)
  -k, --key <KEY>               Encryption password (deprecated: prefer
                                RSB_ENCRYPTION_KEY, the keyring or the prompt)
  -i, --interval <SECONDS>      Quiet period: a backup starts once no change has
                                arrived for this long [default: 2]
  -H, --healthcheck-url <URL>   Healthchecks.io URL for monitoring
```

`watch` always encrypts: without a key from `RSB_ENCRYPTION_KEY`, the
keyring or the prompt it refuses to start.

**Examples:**

```bash
# Watch and auto-backup (prompts for the password, or reads RSB_ENCRYPTION_KEY)
rsb watch mybackup.toml --sync-to /tmp/sync

# With healthcheck monitoring
rsb watch mybackup.toml --sync-to /tmp/sync \
  --healthcheck-url https://healthchecks.io/ping/your-uuid
```

> For `backup`, `prune`, `schedule` and `watch`, `--backup` is read as the
> path of the profile `.toml` (the same as CONFIG), not as a repository
> directory. Use the positional CONFIG argument.

### Authentication

#### Login

Authenticate with FIDO2 security key.

```bash
rsb login [OPTIONS] <USER_ID>

ARGUMENTS:
  <USER_ID>                     User email or username

OPTIONS:
  -r, --recovery                Use recovery code instead of FIDO2
```

**Examples:**

```bash
# Login with FIDO2 key
rsb login user@example.com

# Login with recovery code
rsb login user@example.com --recovery
# Then enter recovery code when prompted
```

#### Server

Start WebAuthn authentication server.

```bash
rsb server [OPTIONS]

OPTIONS:
  -p, --port <PORT>             Server port [default: 3000]
```

**Examples:**

```bash
# Start on default port 3000
rsb server

# Start on custom port
rsb server --port 8080
```

#### Security Key Management (`rsb auth`)

Manage FIDO2/WebAuthn credentials. `register` and `authenticate` open a
browser page on `http://localhost:3000` where you enter the user id and
touch the key. Every subcommand accepts `-s, --storage <PATH>` (credential
file, default `~/.rsb/auth.json`).

```bash
rsb auth <COMMAND>

COMMANDS:
  register        Register a security key (browser flow)
  authenticate    Authenticate with a registered key (browser flow)
  list            List registered credentials
  delete          Delete the credentials of a user: delete <USER_ID>
  generate-codes  Generate recovery codes: generate-codes <USER_ID>
                  [-o, --output <FILE>] [--no-display]
  list-codes      Count remaining recovery codes: list-codes <USER_ID>
  export-codes    Export recovery codes: export-codes <USER_ID>
                  -o, --output <FILE> [-f, --format text|json]
```

**Examples:**

```bash
# Register a key, then check it is listed
rsb auth register
rsb auth list

# Generate recovery codes and save them to a file only (needs a session as
# that user: `rsb login user@example.com` first)
rsb auth generate-codes user@example.com --output ~/safe/codes.txt --no-display

# Remove the credentials of a lost key (needs a session as that user)
rsb auth delete user@example.com
```

### Diagnostics

#### Diagnose

Diagnose and repair backup issues.

```bash
rsb diagnose [OPTIONS] --backup <PATH>

OPTIONS:
  -b, --backup <PATH>           Backup path to diagnose
  -k, --key <PASSWORD>          Decryption password (prefer RSB_ENCRYPTION_KEY)
  -v, --verbose                 Detailed diagnostics output
  -j, --json                    Output in JSON format
  --repair                      Attempt to repair detected issues (needs `rsb login`)
```

**Examples:**

```bash
# Run diagnostics
rsb diagnose --backup /backup/docs

# Verbose output
rsb diagnose --backup /backup/docs --verbose

# JSON output for automation
rsb diagnose --backup /backup/docs --json

# Attempt repairs
rsb diagnose --backup /backup/docs --repair

# Diagnose encrypted backup
RSB_ENCRYPTION_KEY='your-password' rsb diagnose --backup /backup/docs --verbose
```

### Credentials & Settings (`rsb config`)

Manage credentials and settings.

```bash
rsb config <COMMAND>

COMMANDS:
  set-password             Set master password
  s3-credentials           Configure S3 credentials: --bucket <BUCKET> --region <REGION>
                           [--endpoint <URL>] [--access-key <ID>] (the secret key is prompted)
  list                     List current configuration
  reset                    Reset all settings [WARNING: destructive; needs --confirm]
```

**Examples:**

```bash
# Set master password
rsb config set-password

# Configure S3
rsb config s3-credentials --bucket my-backups --region us-east-1 --access-key AKIA...

# List config
rsb config list

# Reset (danger!)
rsb config reset --confirm
```

### Version & Help

```bash
rsb --version              Print version
rsb --help                 Print general help
rsb <COMMAND> --help       Print command help
rsb help <COMMAND>         Print command help (alternative)
```

---

## Portability Guide

### What is Portable Mode?

Portable mode allows you to:
- Restore, verify and inspect a backup without its profile: `restore`,
  `verify` and the `snapshots` commands accept `--backup <DIR>` (the backup
  directory itself)
- Create profiles that store relative paths (`create-profile --portable`)
- Restore backups on different computers
- Move backups to external drives

Making a backup always needs a profile (`rsb backup <CONFIG>`).

### Portable Backup Workflow

```bash
# 1. Create a profile that stores relative paths, then back up
rsb create-profile --name mybackup --source ~/Documents \
  --dest /media/external/mybackup --portable
rsb backup mybackup.toml

# 2. Verify on same computer (no profile needed)
rsb verify --backup /media/external/mybackup

# 3. Move to another computer and restore
# On computer B:
rsb restore --backup /media/external/mybackup --target ~/restored

# 4. Compare snapshots across systems
rsb snapshots diff 2026-05-20T100000.000Z 2026-05-25T100000.000Z --backup /media/external/mybackup
```

### Config vs Portable Mode

| Feature | Config Mode | Portable Mode |
|---------|------------|---------------|
| Command | `rsb restore config.toml --target <DIR>` | `rsb restore --backup /path --target <DIR>` |
| Requires config file | ✅ Yes | ❌ No (restore, verify, snapshots) |
| Cross-computer | ❌ Limited | ✅ Yes |
| Profile saved | ✅ Yes | ❌ No |
| Use case | Regular backups | External drives, cross-platform |

### Auto-Discovery

`backup`, `prune`, `schedule`, `watch` and the `snapshots` commands use the
profile in the current directory when no profile is given, if there is
exactly one (a `.toml` with `source_path` and `destination_path`; files such
as `Cargo.toml` are ignored). With several profiles they fail and list them.
`restore` and `verify` always need a profile or `--backup`.

```bash
# If mybackup.toml is the only profile in the current directory:
rsb backup          # Auto-discovers mybackup.toml
rsb snapshots list  # Auto-discovers mybackup.toml
```

---

## Deduplication

RS Shield automatically deduplicates backup files to save storage space.

### How It Works

1. **Content-based hashing**: Each file is hashed using its content
2. **Duplicate detection**: If a file with the same hash exists, it's skipped
3. **Automatic optimization**: Duplicate files aren't stored twice

### Benefits

- **Reduced storage**: Identical files take no extra space
- **Faster backups**: Duplicate files skip compression and encryption
- **Cross-backup deduplication**: Works across multiple snapshots

### Example

```bash
# First backup of documents folder
rsb backup mybackup.toml
# → 100 files backed up

# Backup again after adding only 10 new files
rsb backup mybackup.toml
# → 10 files backed up (90 duplicates skipped ✓)
# → Total storage used: ~105 files worth (with overhead)
```

---

## Examples

### Daily Automated Backup

Generate the job with `rsb schedule` (see [Schedule](#schedule)); it reads
the encryption key from a file only you can read:

```bash
rsb schedule mybackup.toml --format cron

# Edit crontab and paste the generated line
crontab -e
```

### Encrypted Cloud Backup

```bash
# Create profile with S3 (--dest is only used with --storage local)
rsb create-profile --name cloud-backup \
  --source ~/Documents \
  --dest ~/backups/cloud \
  --s3-bucket my-backups \
  --s3-region us-east-1 \
  --s3-endpoint https://s3.amazonaws.com \
  --encrypt

# Store the S3 credentials (the secret key is prompted)
rsb config s3-credentials --bucket my-backups --region us-east-1 --access-key AKIA...

# Run backup
rsb backup cloud-backup.toml

# Restore later
rsb restore cloud-backup.toml --target ~/restored
```

### Backup with Verification

```bash
# Backup and verify
rsb backup mybackup.toml --verify

# Later, verify without restore
rsb verify --backup /backup/docs
```

### Selective Restore

```bash
# Restore specific file types, at any depth
rsb restore --backup /backup/docs --target ~/restored --files '**/*.pdf' '**/*.doc*'

# Restore one folder as it was last Friday
rsb restore --backup /backup/docs --target ~/restored --files projects/site/ --date 2026-10-09
```

---

## Exit Codes

| Code | Meaning | Example |
|------|---------|---------|
| 0 | Success | Backup completed successfully |
| 1 | Any error | Configuration file not found, wrong password, `verify` found problems, `prune` without `--confirm` |
| 2 | Invalid command line | Unknown flag or missing required argument |
| 130 | `watch` aborted | Ctrl+C pressed twice during a backup |

---

## Tips & Tricks

### Dry Run Pattern

Use `--dry-run` first with `backup` and `prune`:

```bash
rsb prune mybackup.toml --retention 30d --dry-run
# Review output, then run with --confirm
rsb prune mybackup.toml --retention 30d --confirm
```

### Verbose Debugging

For a detailed report on a backup directory:

```bash
rsb diagnose --backup /backup/docs --verbose
```

Log messages go to stderr. By default only warnings and errors are shown;
set `RUST_LOG` (standard `tracing` filter syntax) for more:

```bash
RUST_LOG=info rsb backup mybackup.toml                      # progress of every step
RUST_LOG=rsb_sdk=debug rsb verify --backup /backup/docs     # one crate in detail
```

An invalid `RUST_LOG` value is reported and ignored (warnings only).
Passwords, keys and credentials are never logged, at any level; the AWS
request-signing traces (`aws_sigv4`, which include session tokens) stay at
warning level unless `RUST_LOG` names `aws_sigv4` explicitly. There is no
`-v/--verbose` logging flag (`diagnose --verbose` only adds detail to the
diagnostic report).

### Parallel Processing

Increase threads for faster backup:

```bash
rsb backup mybackup.toml --threads 8
```

### Compression Trade-offs

The compression level is set in the profile (`compression_level`, or
`create-profile --compression`); `--no-compress` turns it off for one backup:

```bash
rsb create-profile --name fast --source ~/Documents --dest /backup/fast --compression 3    # Fast (default)
rsb create-profile --name small --source ~/Documents --dest /backup/small --compression 19 # Slow but small
rsb backup fast.toml --no-compress                                                         # Fastest
```

---

## Troubleshooting

### "Permission denied"

```bash
# Fix backup directory permissions
chmod 700 /backup/docs

# Run with elevated privileges if necessary
sudo rsb backup mybackup.toml
```

### "Profile not found"

```bash
# List available profiles
rsb list-profiles

# Create if missing
rsb create-profile --name docs --source ~/Documents --dest /backup
```

### "Not authenticated"

```bash
# Start a session (FIDO2 key or recovery code)
rsb login user@example.com
```

### "Incorrect password"

```bash
# Try again (passwords are case-sensitive)
rsb restore --backup /backup/docs --target ~/restored
# (get prompted for password)
```

### Out of Memory

```bash
# Reduce thread count
rsb backup mybackup.toml --threads 2

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
- [Migration Guide](/docs/MIGRATION.md)
- [Security Policy](/SECURITY.md)

---

*Last Updated: October 2026*
