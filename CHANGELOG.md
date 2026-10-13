# Changelog

All notable changes to RS Shield will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Security
- A backup into an encrypted repository (one with `keys/repo.key`) without
  an encryption key is now refused before anything is read or written, in
  the SDK, so for the CLI, the desktop app and real-time sync alike. It
  used to store every file **unencrypted** in that repository without any
  warning. An empty key counts as no key.
- Notification secrets (SMTP password, Slack/Discord webhook URLs, Telegram
  bot token) are no longer stored in plaintext in
  `~/.rs-shield/integrations.json`. They go to the OS keyring (service
  `rsb-shield`) or, without one, to the encrypted
  `~/.rs-shield/integrations_secrets.enc` (same AES-256-GCM/PBKDF2 format as
  `s3_credentials.enc`, passphrase from `RSB_INTEGRATIONS_PASSPHRASE`); the
  JSON keeps only a reference. Existing files are migrated on first load by
  the desktop app or `rsb config list`: each secret is stored and read back
  before the file is rewritten atomically (0600) without it; if no secure
  store is available the file is left untouched and a warning is logged.
  `RSB_SMTP_PASSWORD`, `RSB_SLACK_WEBHOOK_URL`, `RSB_TELEGRAM_BOT_TOKEN` and
  `RSB_DISCORD_WEBHOOK_URL` override the stored values (headless/cron). The
  shared logic is in the new `rsb_sdk::integrations` module; `Debug` of
  `EmailConfig`, `ChatIntegration` and the integration settings now redacts
  the secrets. `rsb config reset` also removes them from the keyring.

### Breaking
- `rsb snapshots delete` now also deletes the data objects no other
  snapshot uses (like `rsb prune`), so it requires `--confirm`, exactly like
  `rsb prune`; without it (or `--dry-run`) it refuses and exits with a
  non-zero status.
- `rsb prune --retention <DAYS>` is now age based: it deletes the
  snapshots taken more than DAYS days ago (by the time in the snapshot id)
  and keeps every newer one, never the newest snapshot. It used to turn the
  value into a snapshot **count** (`30d` kept the newest 30 snapshots,
  `6m` 26, a bare `10` 10 snapshots), so the same command now deletes a
  different set of snapshots. A bare number is a number of days; `d`, `w`,
  `m` (30 days) and `y` (365 days) are accepted; 0, unknown units and more
  than 36500 days are rejected instead of falling back to 10 snapshots.
  With `--keep-last` as well, a snapshot is kept if either rule keeps it
  (`--keep-last` used to silently override `--retention`). See the
  [migration guide](docs/MIGRATION.md). The CLI and the desktop app now
  plan prunes with the same SDK code (`rsb_sdk::core::RetentionPolicy`).
- `rsb_sdk::utils::calculate_retention_backups` was removed (it computed
  the count above); use `rsb_sdk::core::parse_retention_days` and
  `RetentionPolicy`.

### Added
- Desktop restore: the "Date" field is a date picker plus an optional time
  picker (local time) with a clear button, instead of free text. No time
  means the end of that day, as with `rsb restore --date YYYY-MM-DD`.
- `rsb_sdk::snapshot`: snapshot listing, details, diff and delete with
  structured results (id, date, encrypted flag, file count, original and
  stored size, per-snapshot read errors), used by both the CLI and the
  desktop app so they show the same data. `rsb_sdk::core::prune_with_stats`
  returns what a prune deleted.
- `rsb snapshots list --json` and `rsb snapshots show --json`.
- `rsb_sdk::core::{RetentionPolicy, select_by_policy,
  plan_prune_by_policy_with_storage, parse_retention_days}`: count and/or
  age retention, used by `rsb prune` and the desktop prune.
- `rsb prune --dry-run` also shows how many data objects would be deleted
  when the key is available, and `rsb prune` prints what it deleted.
- `rsb snapshots delete --dry-run`: shows the snapshot and the number of
  data objects that would be deleted, without deleting anything
  (`rsb_sdk::snapshot::preview_delete_snapshot`,
  `rsb_sdk::core::preview_prune`).
- Desktop FIDO2 tab (was a placeholder): lists the registered security
  keys and generates new recovery codes for the signed-in user, like
  `rsb auth list` / `generate-codes`.
- Desktop "Repository key" screen: export `keys/repo.key` (JSON or paper
  form, 0600, never overwriting), import an exported key file (`--force`
  keeps a backup of the replaced one) and change the repository password —
  the SDK calls behind `rsb key export/import/change-password`.
- Desktop create profile: encryption (`--encrypt`), with the password
  stored in the OS keyring under the profile name, where backup, restore,
  verify and prune (desktop and CLI) find it.
- Desktop prune: "Keep the last N snapshots" (`rsb prune -l N`) besides the
  age-based retention, and a Preview button (`--dry-run`).
- Desktop verify: snapshot selection and `--repair`.
- Desktop restore: `--versioned` and `--verify` options; with a date, the
  chosen snapshot is shown.
- Desktop backup: "Verify the backup afterwards" (`rsb backup --verify`).

### Fixed
- `rsb restore --files` (and the desktop/SDK `include` patterns) compare
  paths after normalizing both the pattern and the stored path to Unicode
  NFC, so `café` typed in NFC selects a file backed up on macOS (stored in
  NFD) and vice versa. The file is still restored under its stored name.
- An age-based prune with a huge number of days no longer panics on date
  overflow (it deletes nothing).
- Desktop: the interface mixed Portuguese and English — most screens
  (navigation, profiles, real-time sync, snapshots, integrations, repository
  key, FIDO2, diagnostics, schedule, prune, verify, backup, restore, login)
  had hardcoded text that ignored the language setting. Every user-visible
  text now comes from the Portuguese/English catalogs, so switching the
  language updates all screens; CLI command names, file names and SDK error
  details stay as they are. Unit tests check that both catalogs have the
  same keys, no empty text and the same `{0}` placeholders, and that every
  key used by the UI exists.
- Desktop: the Snapshots tab listed nothing (loading, details, compare and
  delete were placeholders). It now lists the snapshots of a profile (local
  destination or S3 bucket), a backup folder or the app's S3 bucket, newest
  first, with date, file count, size and an encrypted marker; shows a
  snapshot's files; compares two snapshots; deletes a snapshot after
  confirmation; shows errors instead of an empty list; runs off the UI
  thread; and reloads the remembered repository when the tab is opened
  again (e.g. after a backup or a prune).
- `rsb snapshots` worked only on local repositories: a profile with an S3
  bucket was refused, or its local `destination_path` was read instead of
  the bucket. The profile is now opened like `backup`/`restore` do (S3
  credentials from the environment, the encrypted credentials file or a
  prompt), and `~`/`$VAR` in `destination_path` are expanded.
- `rsb snapshots list` sorted ids as plain strings, so `…Z-2` came before
  `…Z-10` and second-resolution ids before millisecond ones of the same
  second; it now sorts by the time in the id like restore and prune.
- `rsb snapshots list` showed every unreadable manifest as
  `[Encrypted/Invalid]`: encrypted snapshots without a key are now marked
  as such (their id and date are still listed) and corrupt ones show the
  reason. A plaintext snapshot in a repository that is now encrypted is no
  longer reported as unreadable when the key is given.
- `rsb snapshots list/show/diff` ignored the profile's keyring key; they
  now use `RSB_ENCRYPTION_KEY`, the keyring, then a prompt in a terminal.
- `rsb snapshots show/diff` did not validate snapshot ids before using
  them as storage keys.
- `rsb snapshots delete` removed the manifest file directly: local only,
  without the repository lock (it could race a running backup or prune) and
  leaving the snapshot's data behind. It now runs the prune machinery on the
  snapshot: exclusive lock, dedup-index epoch bump, and deletion of the data
  objects no other snapshot references; an encrypted repository needs the
  key.
- Prune reads every manifest it keeps before deleting anything, so a
  missing key or an unreadable manifest aborts it with the repository
  untouched (previously the selected manifests were already deleted).
- Desktop: e-mail/Slack/Telegram/Discord notifications configured in the
  Integrations screen are now sent after backup, restore and verify. The
  operations looked for `integrations.json` next to the profile or the
  backup folder instead of `~/.rs-shield/integrations.json`, where the
  screen saves it (and the backup screen sent nothing without a profile).
  The file is now written with mode 0600.
- Desktop backup: a loaded profile is used as `rsb backup <profile>` uses
  it, and its encryption key is found like in the CLI (typed key →
  `RSB_ENCRYPTION_KEY` → the profile's OS keyring entry → legacy TOML key).
  Before, only a key written in the TOML was read, so a profile created with
  `--encrypt` (key in the keyring) was backed up **unencrypted** without any
  warning; such a backup is now refused when no key is found. Profiles are
  read with the SDK loader (minimal profiles without `exclude_patterns` /
  `backup_mode` no longer fail; `max_threads` and S3 settings are kept), and
  loading one no longer overwrites the app-wide settings.
- Desktop backup: Cancel keeps the screen busy until the backup has really
  stopped (a second backup could be started while the first was still
  running); errors of files that failed are listed instead of a bare
  status; invalid numbers in Settings are reported instead of silently
  ignored; the history no longer records the duration as the backup size;
  dry runs are not recorded or notified; HTML reports go to
  `~/.rs-shield/reports/` instead of the app's working directory.
- Desktop restore: the encryption key is found like in the CLI (keyring,
  `RSB_ENCRYPTION_KEY`), existing files are no longer overwritten unless
  "Overwrite existing files" (`--force`) is ticked (it was always forced),
  a backup folder typed or picked after loading a profile is used instead
  of being silently ignored, and restoring straight from the S3 bucket of
  Settings is possible again (the button stayed disabled without a folder).
- Desktop verify: a verification that found problems is reported as a
  failure with the problems listed (like the exit code of `rsb verify`),
  not as a plain status; the key is found like in the CLI and a loaded
  profile's repository (also S3) is verified instead of a dummy
  configuration.
- Desktop prune: an encrypted repository is no longer pruned without its
  key (refused before anything is deleted, like `rsb prune`); the key is
  found like in the CLI; invalid retention input is reported instead of
  silently becoming 30 days; the result says what was deleted, or that
  nothing matched, instead of always "Prune completed".
- `rsb backup --verify` and `rsb restore --verify` pass the encryption key
  to the verification: with the key in the OS keyring or
  `RSB_ENCRYPTION_KEY` (not in the TOML), verifying an encrypted backup
  failed.
- Desktop schedule: the generated job is the one `rsb schedule` prints
  (shared code in `rsb_sdk::schedule`): the encryption key is read from a
  0600 env file instead of being written as `--key "<key>"` into the
  crontab/unit, `--storage` matches the profile and the profile must load.
  The "Schedule Now" button, which reported success without scheduling
  anything, was removed.
- Desktop create profile: profiles are written like `rsb create-profile`
  (SDK `Config`, mode 0600 via `write_config_secure`, same defaults — the
  CPU pause threshold was 20%); an existing profile is no longer silently
  overwritten; names with path separators (`../x`) are rejected instead of
  writing outside `~/.rsb-desktop`; an S3 profile no longer needs a local
  destination (`s3://<bucket>`); a failure to save the S3 credentials is
  reported instead of only logged (the screen also claimed they were
  "saved in config").
- Desktop diagnostics: the screen showed hard-coded results ("250 GB
  free", "Encryption key available", "Backup 2 hours ago"). It now runs
  the same repository check as `rsb diagnose --backup <dir>` on a chosen
  folder and reports the registered FIDO2 keys and the last backup from
  the history.
- Desktop backup integrity: objects are counted under `data/clear` and
  `data/enc` with the SDK validation used by `rsb restore --backup` (it
  always reported 0 data files and counted temp files as snapshots), off
  the UI thread.
- `rsb diagnose` counts only snapshot manifests (not temp files of
  interrupted writes); its result fields are public in the SDK.
- Desktop real-time sync now uses the engine of `rsb watch` (shared as
  `rsb_sdk::realtime::mirror_and_backup_sync`). It used to copy every file
  of the source every 2 seconds, start a backup only when the number of
  copied files grew (modified or deleted files never triggered one), drop
  the backup error text, ignore the ignore-pattern editor, abort folders
  that took over 30 s to copy, and never send the webhook (its future was
  never polled). Stopping lets a running backup finish and reports changes
  that are still pending; a profile can be used like `rsb watch <profile>`.
  Battery health is computed against the design capacity.
- Desktop dashboard: CPU usage always showed 0% (a new `System` was
  created for each sample) and the metrics were read on the UI thread
  every 2 s; the activity counters were read once at start-up and never
  showed the operations of the running session. Both are now refreshed
  off the UI thread.
- Desktop login: an unreadable FIDO2 credentials file is reported instead
  of looking like "identifier not found", and a first-time user is told to
  register a key with `rsb auth register`. Settings: the compression
  slider allows the zstd levels up to 22 (it stopped at 11).

### Removed
- `rsb_sdk::snapshot::{repository::SnapshotRepository, model::Snapshot,
  model::SnapshotFile}`: unused, and they read `snapshots/*.json` files no
  version ever wrote. Use `rsb_sdk::snapshot`.


## [0.1.0-alpha.9] - 2026-10-12

Read the [migration guide](docs/MIGRATION.md) before upgrading: once this
version writes encrypted data to a repository, older versions can no longer
read it.

### Breaking
- Encryption format v3 (see [docs/FORMAT.md](docs/FORMAT.md)). New encrypted
  objects and manifests are written as v3, with a key id; 0.1.0-alpha.8 and
  older cannot read a repository after this version has written encrypted
  data to it. v1/v2 data is still read, also mixed with v3.
- Encrypted v3 repositories depend on `keys/repo.key`: losing it makes every
  v3 object unreadable, even with the password. Back it up with
  `rsb key export`.
- S3 `access_key` / `secret_key` in profile TOML files are no longer used
  unless `RSB_ALLOW_CONFIG_CREDS=1` is set (with a warning). Credentials are
  read from `AWS_ACCESS_KEY_ID`/`AWS_SECRET_ACCESS_KEY`, then credentials
  entered in the desktop app for the session, then the encrypted
  `~/.rs-shield/s3_credentials.enc`, then a prompt (terminal only).
- `rsb prune`: the short flag of `--keep-last` changed from `-k` to `-l`,
  and deleting now requires `--confirm` (or preview with `--dry-run`).
- `rsb verify` exits with a non-zero status when it finds problems.
- A profile that requires encryption fails when no key is available instead
  of backing up unencrypted; a failed key derivation is an error instead of
  silently disabling encryption. A global `RSB_ENCRYPTION_KEY` also
  encrypts profiles without `encrypt_patterns`.
- `rsb-sdk` API: `rsb_sdk::options::{BackupOptions, RestoreOptions,
  VerifyOptions}` replace the long argument lists; the `*_with_cancellation`
  variants and `perform_verify_with_repair` are removed (cancellation and
  `repair` are option fields); public functions return
  `rsb_sdk::error::RsbError` (`Send + Sync`, typed crypto errors such as
  `AuthenticationFailed`).
- `--backup <DIR>` on `rsb backup`, `prune`, `schedule` and `watch` names
  the repository directory, as on `restore`, `verify` and `snapshots`; it
  was read as the profile `.toml` path (so `rsb schedule --backup <DIR>`
  generated a cron line running `backup "<DIR>"`). `backup`, `schedule`
  and `watch` use the profile, in DIR or the current directory, whose
  destination is DIR, and fail with instructions when there is none or
  several; `prune --backup <DIR>` prunes the repository directly. A file
  passed to `--backup` is refused with a hint to pass it as the CONFIG
  argument, and CONFIG and `--backup` can no longer be combined on these
  commands.
- CLI log messages go to stderr instead of stdout.

### Added
- `rsb key init | migrate | change-password | export | import | rotate`:
  repository key lifecycle for format v3. `migrate` re-encrypts all v1/v2
  objects and manifests as v3 and then drops the old password; `export`
  writes the key file (mode 0600) or a checksummed paper form; `import`
  checks that a key belongs to the repository; `rotate` re-encrypts
  everything under a new data key. Commands that rewrite the key file or
  data run under the exclusive lock, verify before removing an old key or
  password and resume after an interruption; all of them require an
  authenticated session and read passwords only from a prompt.
- `rsb verify --repair`: deletes objects proven corrupt so the next backup
  uploads them again.
- `rsb restore --files <PATTERN>...`: restore only matching files (exact
  path, folder prefix or glob such as `**/*.pdf`, matched against the
  `/`-separated snapshot paths). Absolute paths and `..` are rejected; a
  selection that matches nothing fails without writing anything.
- `rsb restore --date <WHEN>`: restore the latest snapshot taken at or before
  WHEN (RFC 3339, `YYYY-MM-DD` = end of that day in local time, or
  `YYYY-MM-DD HH:MM` local time). Conflicts with `--snapshot`; fails naming
  the oldest snapshot when none is old enough.
- `rsb backup --storage <s3|local>` to override the profile's storage.
- S3-compatible providers (MinIO, R2, B2, Wasabi, Spaces, Ceph, Garage, ...):
  path-style addressing with any custom endpoint, any region (`us-east-1`
  when an endpoint has none), checksums only when required, errors mapped
  by code and HTTP status. Only Put/Get/Head/Delete and ListObjectsV2 are
  used.
- Desktop: shared S3 settings for the backup, restore, prune and verify
  screens, opt-in "Remember credentials" (saved encrypted in
  `~/.rs-shield/s3_credentials.enc`, never in the TOML), restore from S3,
  and file/date selection on the restore screen.
- Local dedup index cache (`<user cache dir>/rs-shield/dedup-index/`),
  keyed by the new optional `meta/repo-id` object and invalidated through
  `meta/epoch`, which prune, `verify --repair`, `key migrate` and
  `key rotate` rewrite. Entries are trusted for 24 h and sampled; see
  [docs/PERFORMANCE.md](docs/PERFORMANCE.md).
- SDK: `Config::default()` / `Config::new(src, dst)`, injectable storage
  (`storage` option field, `*_with_storage` prune variants) and the
  in-memory `storage::MemoryStorage`; `Storage::write_from_reader`,
  `write_file`, `write_deferred`, `write_file_deferred`, `sync` and
  `cache_identity` (all with default implementations);
  `BackupOptions::dedup_index`, `RestoreOptions::concurrency`,
  `RestoreOptions::include` / `RestoreOptions::at_or_before` and the
  `core::restore_select` module.
- Benchmarks (criterion `backup_restore`, `perf_probe`,
  `InstrumentedStorage`) with a quick subset on pull requests and the full
  set on `main`.
- Fuzzing crate `fuzz/` (blob decryption, manifests and paths, zstd
  streaming, lock parsing) with a weekly workflow.
- Tests: end-to-end FIDO2/WebAuthn with a software authenticator, S3
  integration tests run in CI against SeaweedFS, and property tests for
  backup/restore, v3 blobs and path validation.
- Documentation: [docs/FORMAT.md](docs/FORMAT.md),
  [docs/PERFORMANCE.md](docs/PERFORMANCE.md) and
  [docs/MIGRATION.md](docs/MIGRATION.md).

### Changed
- Format v3: Argon2id-wrapped data key in `keys/repo.key` (64 MiB, t=3,
  p=1 by default), STREAM AES-256-GCM chunks (1 MiB), a per-blob key derived
  with HKDF, and AAD binding the header to the object id. The first
  encrypted backup creates the key file; a wrong password fails before any
  blob is read.
- Restore runs files in parallel within the memory budget
  (`RestoreOptions::concurrency`, default 2 x CPUs, 4 to 64), prefetches
  chunks of large files and no longer sends a HEAD before every GET.
- Backup streams file content through a 1 MiB buffer (memory no longer
  grows with file size) and no longer waits 4 s on the resource monitor.
- Local storage: atomic writes in one blocking task, cheaper ordered fsync
  on macOS, one directory fsync per batch.
- Repository locks record host, pid, process start time and an owner id; a
  lock held by a dead process on the same host is released at once, and a
  busy lock reports its holder. Backups take a shared lock and prune an
  exclusive one.
- `rsb watch` runs one real, debounced backup per batch of changes (never
  two at once) and retries failures with backoff; `--key` is optional and
  `--interval` is the quiet period.
- Login tokens last 30 minutes; the local FIDO2 server port is
  configurable with `RSB_SERVER_PORT`.
- Passing secrets as arguments (`--key`/`-k`, `--secret-key`) is deprecated
  and warns; prefer `RSB_ENCRYPTION_KEY`, the keyring or the prompt.
  `rsb schedule` uses a 0600 `EnvironmentFile` instead of `--key`.
- Encrypted credential files (S3 credentials, recovery codes) use 600k
  PBKDF2 iterations with a version marker; files written with the old 100k
  iterations still open (S3 credential files are re-encrypted on load).
- Windows: manifest keys are written with `/` on every platform; restore
  accepts `\` separators in manifests written by older versions.
- `rsb-cli --version` reports the crate version.
- `rsb schedule` checks that the profile exists and loads, uses
  `--storage s3` for S3 profiles, and for profiles with relative paths
  (`--portable`) runs the job from the current directory.
- `rsb prune` resolves the key like `backup` (`RSB_ENCRYPTION_KEY`,
  keyring, prompt) and refuses an encrypted repository without one before
  deleting anything; `--dry-run` lists the snapshots it would delete.

### Security
- The JWT signing secret was a hard-coded constant; it is now a random
  256-bit secret persisted in the keyring (0600 file fallback), and
  sessions are recorded so the CLI can verify tokens locally.
- Restore refuses path traversal (`..`, absolute paths) and
  symlinked targets or parents, does not expand `~`/`$VAR` in stored paths,
  writes to a temporary file, checks the hash and only then renames.
- Decompression limits: manifests up to 256 MiB and data up to the size
  recorded in the manifest (`FileTooLarge`); restore stops writing once the
  output exceeds the recorded size. PBKDF2 work requested by a repository
  is capped at 600k iterations and 1024 cached derivations
  (`KdfLimitExceeded`); Argon2 parameters are bounded on read.
- `auth generate-codes` and `delete` require a session as that user; the
  local FIDO2 server requires a per-run token and checks `Host`; adding or
  removing keys requires a prior assertion; FIDO2 counters and consumed
  recovery codes are saved before a token is issued; the device flow is
  bound to the user and single-use; the rate limiter is bounded; several
  pending FIDO2 challenges with a TTL.
- The keyring only creates a new entry when none exists (a locked keyring
  no longer replaces the DEK or master password); sensitive files are
  written atomically with mode 0600 and directories with 0700; secret
  buffers are zeroized.
- Telegram tokens and webhook URLs are no longer logged; HTTP requests
  have timeouts, webhooks require https, and SMTP refuses credentials
  without TLS.
- Dropped `atty` (RUSTSEC-2021-0145) and the webauthn
  `danger-credential-internals` feature.

### Fixed
- S3 `list` now paginates: with more than 1000 objects, prune worked from a
  partial listing and could delete data still used by recent backups.
- Prune no longer deletes data of a backup in progress (repository lock),
  rejects `keep_last = 0` and deletes manifests before collecting data.
- Full `rsb verify` compared each chunk of a large file with the hash of the
  whole file, so chunked files always failed.
- Deduplication recorded the current compression level instead of how the
  existing object was actually stored.
- `restore --files` / `--date` were accepted and silently ignored.
- Cron backups no longer fall back to local storage when stdin is empty;
  storage comes from the profile or `--storage`.
- S3: any GET failure was reported as "not found" (an unreadable lock or
  manifest could be mistaken for a missing one), and `exists("data/")`
  never matched on S3, breaking the restore structure check.
- A missing S3 credential returns an error instead of aborting the desktop
  app.
- The FIDO2 DEK test used the real keychain entry; it now uses an isolated
  one (`RSB_FIDO2_KEYRING_SERVICE`).
- `rsb` (CLI) ignored `RUST_LOG` and always logged at WARN. `RUST_LOG` is
  now honoured (default: warnings only; an invalid value is reported and
  ignored).
- `rsb create-profile --dest s3://bucket` created a local `s3:` directory
  and stored the URL as a local path. It now creates an S3 profile (same
  fields as `--s3-bucket`) without touching the local disk. Bucket names
  that no S3-compatible provider accepts (empty, over 255 bytes, `/`, `\`,
  `?`, `#`, whitespace or control characters) and key prefixes
  (`s3://bucket/prefix`), which are not supported, are rejected; names
  outside AWS's DNS rules only print a path-style addressing warning.
  `backup --storage local` refuses such a profile and the `snapshots`
  commands report that they only read local repositories.
- The desktop app no longer logs the length of the S3 access/secret keys at
  debug level.

## [0.1.0] - 2025-12-01

### Added
- Initial release of RS Shield
- Core backup/restore functionality
- AES-256-GCM encryption
- S3-compatible storage support
- Incremental backup mode
- Command-line interface
- Basic desktop application
- Configuration file support (TOML)
- Zstd compression
- File integrity verification with BLAKE3

### Features
- Local filesystem backups
- AWS S3 integration
- MinIO support
- DigitalOcean Spaces support
- Wasabi support
- CloudFlare R2 support
- Automated prune operations
- Detailed backup reports
- Multi-threaded processing

---

## Versioning

RS Shield follows Semantic Versioning:
- **MAJOR** - Breaking changes
- **MINOR** - New features (backward compatible)
- **PATCH** - Bug fixes

## Contribution

See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.
