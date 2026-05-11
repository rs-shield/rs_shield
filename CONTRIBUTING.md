# Contributing to RS Shield

Thank you for your interest in contributing to RS Shield! We welcome contributions from everyone.

## Code of Conduct

This project and everyone participating in it is governed by our Code of Conduct. By participating, you are expected to uphold this code.

**Be respectful, inclusive, and professional in all interactions.**

## How to Contribute

### Reporting Bugs

1. **Check existing issues** - Search issues before reporting
2. **Use bug template** - Include:
   - RS Shield version
   - Operating system and version
   - Detailed steps to reproduce
   - Expected behavior
   - Actual behavior
   - Screenshots/logs if applicable
3. **Be specific** - Vague reports slow down fixes

### Suggesting Features

1. **Check discussions** - Look for existing proposals
2. **Describe the use case** - Why is this feature needed?
3. **Provide examples** - Show how it would be used
4. **Consider scope** - Does it fit RS Shield's goals?

### Submitting Pull Requests

1. **Fork the repository**
   ```bash
   git clone https://github.com/yourusername/rs-shield.git
   ```

2. **Create a feature branch**
   ```bash
   git checkout -b feature/my-feature
   ```

3. **Make changes** following code style guidelines

4. **Write tests** for new functionality
   ```bash
   cargo test --workspace
   ```

5. **Update documentation**
   - Code comments
   - README if needed
   - Docs if UI changed

6. **Commit with meaningful messages**
   ```bash
   git commit -m "feat: add feature description"
   ```

7. **Push and create PR**
   ```bash
   git push origin feature/my-feature
   ```

8. **Fill out PR template** - Include:
   - Description of changes
   - Related issues
   - Testing performed
   - Screenshots (if UI changes)

## Development Guidelines

### Code Style

- **Rust:** Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- **Format code:** `cargo fmt`
- **Lint code:** `cargo clippy --all-targets`
- **Comments:** Add rustdoc for public items

### Testing Requirements

- ✅ All tests pass: `cargo test --workspace`
- ✅ No clippy warnings: `cargo clippy --all-targets`
- ✅ Proper formatting: `cargo fmt --check`
- ✅ For UI changes: Test on macOS, Linux, Windows if possible

### Commit Messages

Use conventional commits format:

```
feat: add new feature
fix: resolve bug in backup
docs: update API documentation
test: add tests for encryption
chore: update dependencies
refactor: simplify code structure
perf: improve backup speed
```

### Branch Naming

- `feature/description` - New features
- `fix/description` - Bug fixes
- `docs/description` - Documentation updates
- `chore/description` - Maintenance tasks

## Project Structure

See [DEVELOPER_GUIDE.md](docs/DEVELOPER_GUIDE.md) for architecture details and module descriptions.

## Testing

```bash
# All tests
cargo test --workspace

# Specific package
cargo test -p rsb-core

# With output
cargo test -- --nocapture

# Performance tests
cargo test --release -- --ignored
```

## Documentation

- **README.md** - Overview and quick start
- **docs/USER_GUIDE.md** - End-user documentation
- **docs/DEVELOPER_GUIDE.md** - Technical documentation
- **Code comments** - Rustdoc for public APIs

## Areas We Need Help With

### High Priority
- [ ] Cross-platform testing (macOS, Linux, Windows)
- [ ] Performance optimization
- [ ] Security audits
- [ ] Documentation improvements

### Medium Priority
- [ ] Feature implementations from roadmap
- [ ] Bug fixes
- [ ] Code refactoring
- [ ] Error handling improvements

### Community Contributions
- [ ] Translations
- [ ] Sample backup scripts
- [ ] Integration examples
- [ ] Tutorial videos

## Pull Request Process

1. Ensure all tests pass
2. Update relevant documentation
3. Add entry to CHANGELOG if major change
4. Wait for code review
5. Address any feedback
6. Maintainers will merge when approved

## Recognition

Contributors are recognized in:
- CONTRIBUTORS file
- GitHub contributors page
- Release notes for major contributions

## Questions?

- 📖 [Documentation](docs/)
- 💬 [GitHub Discussions](https://github.com/zebedeu/rs-shield/discussions)
- 📧 Email: support@yourdomain.com
- 🐛 [Open an Issue](https://github.com/zebedeu/rs-shield/issues)

---

## License

By contributing to RS Shield, you agree that your contributions will be licensed under the MIT License.

Thank you for helping make RS Shield better! 🎉
