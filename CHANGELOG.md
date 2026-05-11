# Changelog

All notable changes to RS Shield will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Real-time file synchronization with auto-backup
- Desktop dashboard with live metrics
- System notifications for backup events
- Smart ignore pattern configuration (regex-based)
- Multi-bucket S3 support
- Battery and CPU monitoring
- Email notifications

### Changed
- Refactored backup engine for better performance
- Updated UI with Dioxus framework
- Improved encryption key management

### Fixed
- Resolved S3 upload timeout issues
- Fixed path expansion for ~ and environment variables
- Corrected incremental backup deduplication logic

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
