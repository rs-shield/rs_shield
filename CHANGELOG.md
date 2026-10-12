# Changelog

All notable changes to RS Shield will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed
- `--backup <DIR>` on `backup`, `prune`, `schedule` and `watch` now names
  the repository directory, as it already did for `restore`, `verify` and
  `snapshots`. It used to be read as the profile `.toml` path, so
  `rsb schedule --backup <DIR>` generated a cron line running
  `backup "<DIR>"`. `backup`, `schedule` and `watch` use the profile (in
  DIR or the current directory) whose destination is DIR and fail with
  instructions when there is none or several; `prune --backup <DIR>` prunes
  the repository directly, without a profile. A file passed to `--backup`
  is refused with a hint to pass it as the CONFIG argument, and CONFIG and
  `--backup` can no longer be combined on these commands.
- `rsb schedule` checks that the profile exists and loads, uses
  `--storage s3` for S3 profiles, and for profiles with relative paths
  (`--portable`) runs the job from the current directory.
- `rsb prune` resolves the key like `backup` (RSB_ENCRYPTION_KEY, keyring,
  prompt) and refuses an encrypted repository without one before deleting
  anything; `--dry-run` lists the snapshots it would delete.

### Fixed
- `rsb` (CLI) ignored `RUST_LOG` and always logged at WARN. `RUST_LOG` is
  now honoured (default: warnings only; an invalid value is reported and
  ignored), and log lines go to stderr instead of stdout.
- `rsb create-profile --dest s3://bucket` created a local `s3:` directory
  and stored the URL as a local path. It now creates an S3 profile (same
  fields as `--s3-bucket`) without touching the local disk. Bucket names
  are validated and key prefixes (`s3://bucket/prefix`), which are not
  supported, are rejected. `backup --storage local` refuses such a profile
  and the `snapshots` commands report that they only read local
  repositories.
- The desktop app no longer logs the length of the S3 access/secret keys at
  debug level.

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
