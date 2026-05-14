# Security Policy

## Overview

RS Shield is designed with security as a foundational principle. This document outlines security practices, threat model, and incident reporting.

## Encryption & Cryptography

### Backup Encryption

- **Algorithm:** AES-256-GCM (NIST approved)
- **Key Derivation:** PBKDF2 with 600,000 iterations
- **Salt:** 16-byte cryptographically secure random
- **Authentication:** HMAC embedded in ciphertext
- **IV:** Per-file random IV, stored with ciphertext

### Security Key & Device Flow Authentication

- **Protocol:** WebAuthn (FIDO2) + OAuth 2.0 Device Authorization Grant (RFC 8628)
- **Security Features:** 
  - Signature-based authentication (Hardware-backed)
  - Monotonic counter validation to prevent authenticator cloning
  - Short-lived JWT sessions (15-30 minutes)
  - JTI (JWT ID) tracking for instant session revocation

### Integrity Verification

- **Algorithm:** BLAKE3
- **Purpose:** Detect corruption or tampering
- **Scope:** All file contents and metadata
- **Storage:** Manifest file (encrypted with backup)

### Key Management

1. **Password-Based:**
   - Converted to encryption key via PBKDF2
   - Never stored in plaintext
   - 600k iterations to resist brute force

2. **Keyring & DEK (Data Encryption Key):**
   - A **DEK** is generated using a cryptographically secure RNG.
   - The DEK is stored in the OS Keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service).
   - Security Keycredentials and S3 secrets are encrypted with AES-256-GCM using a key derived from the DEK and a unique salt via PBKDF2.

3. **Memory Safety (Zeroize):**
   - Sensitive keys (DEK, PBKDF2 derived keys, S3 secrets) are cleared from memory immediately after use using the `zeroize` trait to prevent data remnants in RAM.

4. **S3 Credentials:**
   - Not accessible to other applications
   - Stored only in OS keyring
   - Never in configuration files
   - Access requires a valid Security Keysession.

## Secure Coding Practices

### Input Validation

- All file paths validated and normalized
- S3 keys sanitized against injection
- Configuration values type-checked via TOML
- Pattern matching done with regex only (not code execution)

### Middleware & Server Security

- Written in Rust (memory safe language)
- No unsafe blocks except where necessary
- **Security Headers:** Implementation of CSP (Content Security Policy), HSTS, X-Frame-Options (DENY), and X-Content-Type-Options (nosniff).
- **Audit Logging:** Every authentication attempt (success/failure) and sensitive action is logged with IP and User-Agent tracking.
- **Rate Limiting:** Protection against brute-force on auth endpoints (e.g., 5 attempts per minute).
- **Session Validation:** Double validation (Local JWT check + Remote JTI check against server-side session store).

### Dependency Management

```bash
# Check for known vulnerabilities
cargo audit

# Review dependencies
cargo tree
```

## Threat Model

### Protections

✅ **Confidentiality:** AES-256-GCM encryption
✅ **Integrity:** BLAKE3 verification + HMAC
✅ **Authentication:** Password-based key derivation
✅ **Access Control:** File-level encryption
✅ **Secure Transport:** TLS for S3 and Auth API connections
✅ **Phishing Resistance:** FIDO2/WebAuthn hardware authentication
✅ **Replay Protection:** Unique JTI per session and monotonic counters

### Out of Scope

❌ **Metadata Encryption:** Filenames, sizes not encrypted (by design)
❌ **Cross-user Isolation:** Trust model assumes single user per backup
❌ **Supply Chain Security:** Dependency compromise not addressed
❌ **Quantum Resistance:** Not designed for quantum threat landscape

## Known Limitations

1. **Master Password:** If compromised, all credentials at risk
   - Mitigation: Strong password, OS-level access control

2. **Unencrypted Metadata:** Filenames visible in backups
   - Mitigation: Keep backup storage in secure location

3. **Plaintext Configuration:** S3 bucket names visible in config
   - Mitigation: Store config files with restricted permissions (600)

4. **Client-Side Encryption:** Server-side data security depends on S3 provider
   - Mitigation: Use S3 bucket policies to restrict access

## Security Best Practices for Users

### Password Management

```
✅ DO:
- Use passwords 16+ characters with mixed case, numbers, symbols
- Store passwords in secure password manager
- Change password if compromise suspected
- Use unique passwords per backup

❌ DON'T:
- Share passwords via email/chat
- Use same password as email/social media
- write passwords in plaintext files
- Give passwords to other users
```

### Backup Storage

```
✅ DO:
- Store backups on separate physical media
- Enable encryption on storage device
- Use 3-2-1 backup strategy
- Test restore periodically
- Monitor backup integrity

❌ DON'T:
- Store backups on same computer as source
- Sync backups with cloud storage (like Dropbox)
- Leave backups unencrypted on shared storage
- Share backup credentials
```

### System Security

```
✅ DO:
- Keep OS and software updated
- Use strong computer password
- Enable disk encryption (FileVault, BitLocker, LUKS)
- Restrict file permissions (chmod 700)
- Run regular security updates

❌ DON'T:
- Run RS Shield on compromised systems
- Grant excessive permissions
- Store backups on public/shared networks
- Disable security features
```

## Reporting Vulnerabilities

⚠️ **DO NOT** disclose security vulnerabilities publicly!

### Responsible Disclosure

1. **Email:** security@rsbshield.co.ao
2. **Include:**
   - Description of vulnerability
   - Steps to reproduce
   - Potential impact
   - Suggested fix (if available)
   - Your contact information

3. **Timeline:**
   - We will acknowledge receipt within 48 hours
   - We aim to patch and release within 30 days
   - Please allow time for patch development/testing

### Security Advisories

- Published after patch release
- Includes CVE assignment if applicable
- Added to CHANGELOG and GitHub releases
- Credit given to reporter (unless requested anonymously)

## Security Audit

This project welcomes security audits:
- Professional audits: Contact security@rsbshield.co.ao
- Bug bounty program: [Coming soon]
- Community reviews: File issues or security.txt

## Compliance

RS Shield is designed to comply with:

- **Data Protection:** GDPR-compatible (user retains encryption keys)
- **Encryption Standards:** NIST-approved algorithms
- **Security Practices:** OWASP guidelines
- **Dependency Safety:** Regular `cargo audit` checks

## Updates & Patches

Security updates are released:
- **Critical:** Same day, hotfix release
- **High:** Next patch release
- **Medium:** Next minor/patch release
- **Low:** Next major release or patch series

Subscribe to releases:
- 🔗 [Watch releases on GitHub](https://github.com/zebedeu/rs-shield/releases)
- 📧 [Security mailing list] (coming soon)

## Tools & Utilities

### Authenticate with FIDO2

```bash
# Start the device flow login
rsb login <user_id>
```

### Verify Backup Integrity

```bash
# Command-line verification
rsb-cli verify --backup /path/to/backup --password "your-password"

# Output shows:
# - File count
# - Total size
# - Corruption status
# - Missing files
```

### Check Dependencies

```bash
# Find vulnerabilities in dependencies
cargo audit

# Show dependency tree
cargo tree

# Update dependencies safely
cargo update
```

## Cryptographic Libraries

RS Shield uses:

- **`aes-gcm`** - NIST-approved AES-256-GCM
- **`blake3`** - Modern cryptographic hash
- **`pbkdf2`** - Key derivation function
- **`ring`** - Cryptographic primitives (OpenSSL-compatible)
- **`webauthn-rs`** - Server-side FIDO2/WebAuthn implementation
- **`rustls`** - TLS for secure connections

All dependencies are actively maintained and audited.
---

## Questions?

- 📖 [Security Documentation](/docs/)
- 📧 Email: security@yourdomain.com
- 🔐 [OpenPGP Key] (coming soon)

*Last Updated: February 2026*
