# Migration guide

## Upgrading from 0.1.0-alpha.8 to 0.1.0-alpha.9

0.1.0-alpha.9 changes the encryption format of repositories, how S3
credentials are loaded and a few CLI flags. Read this page before you point
the new version at an existing repository. Full details are in the
[CHANGELOG](../CHANGELOG.md), [FORMAT.md](FORMAT.md) and
[PERFORMANCE.md](PERFORMANCE.md).

### 1. Before you upgrade

> [!WARNING]
> **Writes are one-way.** Once 0.1.0-alpha.9 writes encrypted data to a
> repository (an encrypted backup, `rsb key migrate` or `rsb key rotate`),
> 0.1.0-alpha.8 and older can no longer read it: every new encrypted object
> and manifest uses format v3 with a key id, which older binaries do not
> understand. There is no downgrade command.

- Upgrade every machine and every scheduled job (cron, systemd, launchd,
  `rsb schedule` output) that writes to the same repository at the same
  time. An old client that still runs against an upgraded repository fails
  on v3 objects.
- If you might need to go back to alpha.8, copy the repository first (a copy
  of the backup directory, or a bucket copy / S3 object versioning). New
  backups only add objects, but `rsb key migrate` and `rsb key rotate`
  rewrite existing objects in place.
- Run `rsb verify` with the old version first, so you start from a
  repository you know is healthy.

### 2. Upgrading an encrypted repository to format v3

alpha.9 reads v1, v2 and v3 objects, even mixed in one repository, so
existing backups keep restoring with no action. The repository moves to v3
in two steps.

**Step 1: create the repository key (`keys/repo.key`).** v3 repositories
keep a random data key (DEK) in `keys/repo.key`, wrapped with a key derived
from your password with Argon2id. Either the first encrypted backup made
with alpha.9 creates it automatically, or you create it explicitly:

```bash
rsb login user@example.com                 # every `rsb key` command needs a session
rsb key init mybackup.toml                 # profile (local or S3 destination)
rsb key init --repo /path/to/backup        # or a local repository directory
```

The password is only read from an interactive prompt. If the repository
already holds encrypted snapshots, the password must open the newest one,
so a mistyped password cannot split the repository between two passwords.
`rsb key init` then counts the existing v1/v2 objects and offers to migrate
them (`--yes` migrates without asking).

**Step 2: re-encrypt the old data (`rsb key migrate`).** Until you migrate,
the key file also stores the old password (`legacy_secret`, encrypted under
the new key) so v1/v2 objects stay readable. To finish the upgrade:

```bash
rsb key migrate mybackup.toml
```

This re-encrypts every v1/v2 object and manifest as v3 (including objects
no snapshot references, so a later backup cannot deduplicate against them),
verifies every snapshot with the v3 key alone, and only then removes the
old password from the key file. It runs under the exclusive repository
lock, so stop backups to that repository while it runs. If it is
interrupted, the repository stays readable; run the same command again to
resume. `rsb key change-password` and `rsb key rotate` are refused until the
migration has finished.

Unencrypted repositories need no key and no migration.

### 3. Back up the repository key

> [!CAUTION]
> **Losing `keys/repo.key` means losing every v3 backup in the repository.**
> The password alone cannot decrypt v3 data: it only unwraps the data key
> stored in that file.

Export the key as soon as it exists, and again after every
`rsb key rotate` (a rotation replaces the data key and makes older exports
useless for new data):

```bash
rsb key export mybackup.toml --output ~/safe/repo.key   # file, mode 0600
rsb key export mybackup.toml --paper                     # printable, checksummed
```

The export is still protected by your password, but anyone who has the
export *and* the password can decrypt every backup, so store it like the
password, and not only inside the repository it protects. To restore a
lost or damaged key file:

```bash
rsb key import ~/safe/repo.key mybackup.toml
rsb key import paper-key.txt --repo /path/to/backup --force   # replace a different key file
```

Import checks that the key opens with the password and decrypts this
repository before it writes anything; `--force` first copies the existing
key file to `keys/repo.key.bak-<timestamp>`.

### 4. S3 credentials

S3 secrets are now looked up in this order:

1. the environment: `AWS_ACCESS_KEY_ID` and `AWS_SECRET_ACCESS_KEY`
   (and `AWS_SESSION_TOKEN`, if set);
2. credentials typed into the desktop app for the current session (memory
   only);
3. the encrypted credentials file `~/.rs-shield/s3_credentials.enc`, whose
   master password is kept in the system keyring;
4. an interactive prompt, only when a terminal is attached (never in cron,
   systemd or the desktop app).

**`access_key` / `secret_key` in a profile TOML are no longer read.** They
are only used, with a security warning, when you set
`RSB_ALLOW_CONFIG_CREDS=1`, and only after the sources above found nothing.
This opt-in exists to keep old jobs running while you move the secrets;
plan to remove it.

To move secrets out of a profile:

- **Encrypted store (desktops and servers with a keyring):**

  ```bash
  rsb config s3-credentials --bucket my-backups --region us-east-1 \
      --endpoint https://s3.example.com --access-key AKIA...
  ```

  The secret key is prompted for (avoid `--secret-key`: it shows up in `ps`
  and shell history). The secrets go to `~/.rs-shield/s3_credentials.enc`;
  the TOML written by this command only holds bucket, region and endpoint.
  In the desktop app, enable "Remember credentials" in the S3 settings to
  save to the same encrypted file.
- **Environment variables (cron, CI, containers):** export
  `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` from a file that only the
  job's user can read (for example a systemd `EnvironmentFile` with mode
  0600). `rsb schedule` now generates jobs that read secrets this way.

Then delete `access_key` and `secret_key` from every profile TOML, unset
`RSB_ALLOW_CONFIG_CREDS`, and rotate the keys with your provider if the
files were ever readable by other users or committed anywhere.

The encryption password follows a similar rule: pass it through
`RSB_ENCRYPTION_KEY`, the keyring or the prompt. `--key`/`-k` still works
but is deprecated and prints a warning, because arguments are visible in
`ps` and shell history. A profile that requires encryption now fails when
no key is available, instead of backing up unencrypted.

### 5. CLI changes

- **Log in again after upgrading.** Session tokens are now signed with a
  random secret stored in the OS keyring (alpha.8 used a fixed key), so a
  session saved by an older version fails with
  `Session expired or invalid (InvalidSignature)`. Run
  `rsb login <USER_ID>` once.
- **`rsb prune -k` is now `rsb prune -l`.** The short form of
  `--keep-last` changed from `-k` to `-l`, so `-k` consistently means
  `--key` on the other commands. Update scripts that use the short form, or
  switch to `--keep-last`.
- **`rsb prune` needs `--confirm`** to delete anything; without it (and
  without `--dry-run`) it refuses and exits with an error. Add `--confirm`
  to scheduled prune jobs:

  ```bash
  rsb prune mybackup.toml -l 10 --confirm
  ```

- **`rsb verify` exits non-zero** when it finds problems; scripts that
  ignored its output now see failures. `rsb verify --repair` deletes
  objects proven corrupt so the next backup uploads them again.
- **`rsb restore --files` / `--date`** now work (in alpha.8 they were
  accepted and ignored). A `--files` selection that matches nothing fails
  without writing anything.
- **`--backup <DIR>` always means the repository directory.** On
  `backup`, `prune`, `schedule` and `watch` it used to be read as the path
  of the profile `.toml`. Scripts that pass a profile with `--backup` now
  fail with a hint: pass the profile as the first argument instead
  (`rsb backup mybackup.toml`). With a directory, `backup`, `schedule` and
  `watch` use the profile (in that directory or the current one) whose
  destination is that directory, and `prune --backup <DIR>` prunes the
  repository without a profile. A profile argument and `--backup` can no
  longer be given together on these commands. Regenerate cron/systemd jobs
  created with `rsb schedule --backup`.
- **CLI logs go to stderr**, not stdout, and follow `RUST_LOG` (default:
  warnings only). Scripts that parsed warnings from stdout must read
  stderr.

### 6. New objects in the repository

alpha.9 adds these objects. None of them is needed to read a repository,
and you should not edit or delete them by hand:

| Object | Purpose |
|--------|---------|
| `keys/repo.key` | Repository key file (v3); see sections 2 and 3. |
| `keys/repo.key.bak-<timestamp>` | Copy of the previous key file, kept while a password change, migration or rotation rewrites it (and kept after `rsb key import --force`). |
| `meta/repo-id` | Random repository id, created by the first backup; keys the local dedup cache. |
| `meta/epoch` | Rewritten by prune, `verify --repair`, `key migrate` and `key rotate` before they delete or rewrite data; a dedup cache written under another epoch is discarded. |

Lock objects (`locks/<uuid>.lock`) now also record the holder's host, pid
and process start time, so a lock left by a dead process on the same host
is released at once. Locks written by older versions still expire by
timestamp.

### 7. Local dedup cache

To avoid one storage request per already-known object, backups keep a
local index of objects this machine has verified, in the user cache
directory:

| OS | Location |
|----|----------|
| Linux | `$XDG_CACHE_HOME/rs-shield/dedup-index/` (default `~/.cache/rs-shield/dedup-index/`) |
| macOS | `~/Library/Caches/rs-shield/dedup-index/` |
| Windows | `%LOCALAPPDATA%\rs-shield\dedup-index\` |

It is only a cache. Deleting the directory is always safe: the next backup
checks every object against the storage again and rebuilds it. Entries are
trusted for at most 24 hours, a sample is re-checked on every backup, and
`rsb verify` still checks the repository itself. The safety rules are in
[PERFORMANCE.md](PERFORMANCE.md#dedup-index).

The CLI and the desktop app have no option to turn it off yet. Programs
using `rsb-sdk` can disable it per backup with
`BackupOptions { dedup_index: DedupIndexOptions::disabled(), .. }`
(`rsb_sdk::core::dedup_index::DedupIndexOptions`), or point it elsewhere
with `DedupIndexOptions::dir`.

### 8. For `rsb-sdk` users

The public API changed: the backup, restore and verify entry points take
`rsb_sdk::options::{BackupOptions, RestoreOptions, VerifyOptions}` instead
of long argument lists, the `*_with_cancellation` and
`perform_verify_with_repair` variants were removed (cancellation and
`repair` are option fields), and public functions return
`rsb_sdk::error::RsbError`. See the CHANGELOG for the full list.

### 9. Notification secrets leave `integrations.json`

`~/.rs-shield/integrations.json` used to hold the SMTP password, the
Slack/Discord webhook URLs and the Telegram bot token in plaintext. They
now live in the OS keyring (service `rsb-shield`, accounts
`integration:<name>:...`) and the file keeps only the other fields plus a
reference such as `"bot_token": { "stored": "keyring" }`.

The migration is automatic: the first time the desktop app or
`rsb config list` loads the file, each secret is written to the keyring,
read back, and only then is the file rewritten (atomically, mode 0600)
without it. A warning is logged once. Nothing else is needed.

Machines without a usable keyring (headless Linux servers, cron jobs):

- set `RSB_INTEGRATIONS_PASSPHRASE` to keep the secrets in the encrypted
  file `~/.rs-shield/integrations_secrets.enc` instead (same format as
  `s3_credentials.enc`); `RSB_INTEGRATIONS_SECRET_STORE=file` forces it,
  `=keyring` forbids it;
- or provide the secrets at run time with `RSB_SMTP_PASSWORD`,
  `RSB_SLACK_WEBHOOK_URL`, `RSB_TELEGRAM_BOT_TOKEN` and
  `RSB_DISCORD_WEBHOOK_URL`; these override the stored values.

If neither the keyring nor the passphrase is available the file is left
unchanged (the secrets keep working) and a warning says so; `rsb config
list` marks them as `PLAINTEXT`. Older versions cannot read the rewritten
file (they see no integrations): after downgrading, configure the
integrations again.
