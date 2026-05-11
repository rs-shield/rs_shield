# CI/CD Configuration

This document explains how Continuous Integration and Continuous Deployment is configured for RS Shield.

## Overview

RS Shield uses GitHub Actions to automatically:

1. ✅ **Test** - Run tests on multiple platforms
2. 📋 **Lint** - Check code quality and formatting
3. 🔒 **Security** - Audit dependencies for vulnerabilities
4. 📊 **Coverage** - Measure test coverage
5. 📚 **Documentation** - Build and verify documentation
6. 🚀 **Release** - Build and publish binaries

## Workflows

### 1. Test (`test.yml`)

**Trigger:** Push to `main`/`develop`, Pull Requests

**Tests:**
- Runs on: Ubuntu, macOS, Windows
- Rust: Stable, Beta
- Features: All combinations tested
- Documentation: Examples tested

**Includes:**
- Multi-platform testing
- Feature flag combinations
- Documentation tests
- Memory safety checks (Miri)

### 2. Lint (`lint.yml`)

**Trigger:** Push, Pull Requests

**Checks:**
- Code formatting (`rustfmt`)
- Linting (`clippy`)
- Dependency verification (`cargo-deny`)
- Unused dependencies detection

**Fails on:**
- Formatting issues
- Clippy warnings (treats as errors)
- Denied dependencies

### 3. Security (`security.yml`)

**Trigger:** Push (on Cargo changes), Daily schedule

**Checks:**
- Vulnerability audit (`cargo audit`)
- Outdated dependencies
- SBOM generation (Software Bill of Materials)

**Output:**
- Signed SBOM artifact
- Dependency report
- Security warnings

### 4. Coverage (`coverage.yml`)

**Trigger:** Push, Weekly schedule

**Tools:**
- `cargo-tarpaulin` for coverage analysis
- Codecov.io integration

**Metrics:**
- Line coverage
- Branch coverage
- Function coverage

**Output:**
- Coverage report uploaded to Codecov
- HTML report artifact

### 5. Documentation (`docs.yml`)

**Trigger:** Changes to docs, README, or `*.rs` files

**Checks:**
- Builds Rust documentation
- Markdown linting
- Link checking

**Fails on:**
- Broken documentation links
- Markdown errors
- Unsafe doc examples

### 6. Benchmarks (`benchmark.yml`)

**Trigger:** Push to core changes

**Runs:**
- All benchmarks in `rsb-core`
- Tracks performance over time
- Compares main vs PR

**Metrics:**
- Backup performance
- Encryption speed
- Memory usage
- S3 operations

### 7. MSRV (`msrv.yml`)

**Trigger:** Push, Pull Requests

**Verifies:**
- Code compiles with Rust 1.70+
- Tests pass on MSRV
- Builds on MSRV

**Purpose:** Ensure backward compatibility

### 8. PR Checks (`pr-checks.yml`)

**Trigger:** Pull Request events

**Validations:**
- PR description is not empty
- At least one assignee
- At least one label
- Commit message format
- Draft status

## Release Workflow

### Triggering a Release

1. **Create a git tag:**
   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

2. **Automatic Actions:**
   - Creates GitHub Release
   - Builds binaries for all platforms
   - Publishes to crates.io
   - Publishes documentation
   - Creates release notes

### Artifacts Generated

For each tag `v*.*.*`:

**Binaries:**
- `rsb-cli-linux-x86_64`
- `rsb-cli-macos-x86_64`
- `rsb-cli-macos-aarch64`
- `rsb-cli-windows-x86_64.exe`

**Crates.io:**
- `rsb-core` 
- `rsb-cli`

**Documentation:**
- Published to: `https://docs.rs/rsb-core`

## Configuration

### Required Secrets

Add these to GitHub repository settings:

| Secret | Purpose |
|--------|---------|
| `CARGO_TOKEN` | Publish to crates.io |
| `CODECOV_TOKEN` | Upload coverage reports |
| `GITHUB_TOKEN` | (automatic) |

### Branch Protection Rules

Recommended settings for `main` branch:

```
✅ Require status checks to pass before merging:
  - Tests (all platforms)
  - Lint
  - Security
  - Documentation
  
✅ Require code review before merging
✅ Require branches to be up to date
✅ Restrict who can push to matching branches
```

### Concurrency

Some workflows use concurrency groups to prevent duplicate runs:

```yaml
concurrency:
  group: ${{ github.workflow }}-${{ github.event.pull_request.number }}
  cancel-in-progress: true
```

This cancels in-progress workflows when a new push is made.

## Performance

### Caching

All workflows use `rust-cache` action to:
- Cache Rust toolchain
- Cache Cargo dependencies
- Cache build artifacts

**Saves:** ~2-3 minutes per workflow

### Parallel Jobs

Tests run in parallel for:
- Operating systems (Ubuntu, macOS, Windows)
- Rust versions (stable, beta)
- Feature combinations

## Monitoring

### GitHub Actions Dashboard

View workflow status: Settings → Actions → Workflows

### Status Badges

Add to README.md:

```markdown
[![Tests](https://github.com/zebedeu/rs-shield/actions/workflows/test.yml/badge.svg)](https://github.com/zebedeu/rs-shield/actions/workflows/test.yml)
[![Lint](https://github.com/zebedeu/rs-shield/actions/workflows/lint.yml/badge.svg)](https://github.com/zebedeu/rs-shield/actions/workflows/lint.yml)
[![Security](https://github.com/zebedeu/rs-shield/actions/workflows/security.yml/badge.svg)](https://github.com/zebedeu/rs-shield/actions/workflows/security.yml)
[![Coverage](https://codecov.io/gh/zebedeu/rs-shield/branch/main/graph/badge.svg)](https://codecov.io/gh/zebedeu/rs-shield)
```

## Troubleshooting

### Workflow Failed

1. **Check logs:** Click on workflow → failed job → see error
2. **Common issues:**
   - Cache miss (usually fine, just slower)
   - Dependency issues (check `cargo audit`)
   - Platform-specific issues (check OS)
   - Timeout (increase in workflow file)

### Re-running Workflows

**Keep workflow successful after fix:**

1. Push fix to the branch
2. Workflows re-run automatically
3. Or manually trigger: Actions → Workflow → Run workflow

### Disable Workflows

Temporarily disable by:
1. Renaming `.yml` to `.yml.bak`
2. Or removing file

## Costs

**GitHub Actions Free Tier:**
- 3,000 minutes/month for private repos
- 10,000 minutes/month for public repos
- Public repos get unlimited free minutes

**RS Shield Estimate:**
- ~2 minutes per test run × ~30 runs/month = ~60 minutes

Well within free tier!

## Future Improvements

Potential additions:

- [ ] Android/iOS build support
- [ ] Website deployment (mdBook)
- [ ] Docker image builds
- [ ] Performance regression detection
- [ ] Fuzz testing
- [ ] Nightly build tracking

---

## References

- [GitHub Actions Documentation](https://docs.github.com/en/actions)
- [Rust GitHub Actions](https://github.com/dtolnay/rust-toolchain)
- [Cargo Documentation](https://doc.rust-lang.org/cargo/)

---

*CI/CD Configuration for RS Shield - Last Updated: February 2026*
