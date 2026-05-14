# RS Shield 🛡️

**RS Shield** is an enterprise-grade backup and recovery solution built entirely in Rust. It provides military-grade encryption, S3/S3-compatible storage support, incremental backups, real-time sync, and a modern desktop interface.

## ✨ Key Features

- 🔐 **AES-256-GCM Encryption** - Military-grade encryption for all backed-up data
- 📦 **Incremental Backups** - Efficient backup strategy that only backs up changed files  
- 🌐 **S3 Compatible** - Support for AWS S3, MinIO, DigitalOcean Spaces, Wasabi, CloudFlare R2
- ⚡ **High Performance** - Multi-threaded processing with Zstd compression
- 🖥️ **Desktop App** - Modern UI with real-time monitoring and drag-and-drop support
- 🔄 **Real-Time Sync** - Watch folders and auto-backup on file changes
- 📊 **Detailed Reports** - Comprehensive backup reports with statistics
- 🔔 **Smart Notifications** - Email, webhooks, and system notifications
- 🔋 **Resource Aware** - Automatic pauses on low battery or high CPU

## 🚀 Quick Start

### Prerequisites
- Rust 1.70+ ([Install](https://rustup.rs/))
- macOS, Linux, or Windows
- Optional: S3-compatible storage

### Installation & Usage

```bash
# Clone repository
git clone https://github.com/zebedeu/rs-shield.git
cd rs-shield

# Desktop App
./run-desktop.sh

# Or use CLI
cargo run --bin rsb-cli -- --help
```

## 📚 Documentation

- **[User Guide](/docs/USER_GUIDE.md)** - Complete guide for end users
- **[Developer Guide](/docs/DEVELOPER_GUIDE.md)** - Architecture and technical details
- **[CLI Reference](/docs/CLI.md)** - Command-line interface documentation
- **[Security Policy](/SECURITY.md)** - Encryption, security practices, and vulnerability reporting

## 🏗️ Project Structure

```
rs-shield/
├── rsb-cli/        # Command-line interface
├── rsb-sdk/       # Core encryption & backup engine
├── rsb-desktop/    # Desktop GUI (Dioxus + Tailwind)
└── tests/          # Integration tests
```

## 🔒 Security

- **Security KeyAuthentication** - Secure login via hardware keys for CLI and Desktop
- **Chunked encryption** (512MB chunks) for optimal S3 performance

## 📊 Performance

- **Backup Speed:** 500+ MB/s on SSD
- **Memory Usage:** ~50MB typical
- **Compression:** 40-70% for text/document files
- **S3 Chunks:** 512MB for optimal multipart uploads

## 🛠️ Development

```bash
# Build all components
cargo build --release

# Run tests
cargo test --workspace

# Run specific tests
cargo test --package rsb-sdk realtime --

# Format code
cargo fmt

# Check for issues 
cargo clippy --all-targets
```

## 📄 Configuration Example

```toml
source_path = "/home/user/documents"
destination_path = "/mnt/backup"
backup_mode = "incremental"
encryption_key = "your-secure-password"
compression_level = 3

[s3]
bucket = "my-backups"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"

[[s3_buckets]]
name = "primary"
region = "us-east-1"
endpoint = "https://s3.amazonaws.com"
```

## 🤝 Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for details.

## 📋 Roadmap

- [ ] Web dashboard
- [ ] Advanced scheduling (cron integration)
- [ ] Machine learning deduplication
- [ ] Mobile app
- [ ] Audit logging
- [ ] Multi-user support

## 📄 License

MIT License - see [LICENSE](LICENSE) for details

## 🙏 Acknowledgments

Built with:
- [Rust](https://www.rust-lang.org/) - The language
- [Tokio](https://tokio.rs/) - Async runtime
- [Dioxus](https://dioxuslabs.com/) - Desktop UI
- [AWS SDK Rust](https://github.com/awslabs/aws-sdk-rust) - S3 support

---

**RS Shield** - Enterprise-grade backup for everyone 🛡️
