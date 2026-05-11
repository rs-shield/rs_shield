# GitHub Actions Setup Guide

This guide explains how to set up GitHub for RS Shield CI/CD.

## 1. Repository Secrets

Add these secrets to: **Settings → Secrets and variables → Actions**

### CARGO_TOKEN

How to get it:

1. Go to https://crates.io/me
2. Copy API Token from "Account Settings"
3. Add as secret named `CARGO_TOKEN`

### CODECOV_TOKEN (Optional)

For code coverage reports:

1. Go to https://codecov.io/gh/zebedeu/rs-shield
2. Copy the token displayed
3. Add as secret named `CODECOV_TOKEN`

## 2. Branch Protection Rules

Setup for `main` branch: **Settings → Branches → Add rule**

### Basic Rules

```
Branch name pattern: main

Required checks:
✅ Test Suite (Ubuntu, macOS, Windows)
✅ Rustfmt
✅ Clippy
✅ Security Audit
✅ Documentation

✅ Require pull request reviews before merging
   Minimum: 1 review
   
✅ Require status checks to pass before merging

✅ Require branches to be up to date before merging

✅ Restrict who can push to matching branches
   (Optional - for larger teams)
```

### Advanced Settings

```
✅ Include administrators
✅ Allow force pushes: No
✅ Allow deletions: No
```

## 3. Actions Permissions

**Settings → Actions → General**

```
Actions permissions:
✅ Allow all actions and reusable workflows

Workflow permissions:
✅ Read and write permissions
✅ Allow GitHub Actions to create and approve pull requests
```

## 4. GitHub Pages (Optional)

For automatic documentation publishing:

**Settings → Pages**

```
Source: GitHub Actions
Branch: (automatic)
```

Add `PUBLISH_DOCS` workflows will deploy to:
`https://zebedeu.github.io/rs-shield/`

## 5. Webhook Configuration (For External Tools)

If you want to integrate with:
- Slack notifications
- Discord notifications
- Custom webhooks

**Settings → Webhooks & services**

Add integrations as needed.

## 6. Team Access

**Settings → Collaboration & access**

Grant team members appropriate roles:

| Role | Can |
|------|-----|
| Admin | Everything |
| Maintain | Manage workflows, delete branches |
| Write | Push, merge PRs |
| Triage | Review PRs, manage issues |
| Read | View only |

## 7. Enable Features

In repository **Settings → Features**:

```
✅ Issues
✅ Discussions
✅ Projects
✅ Wiki (optional)
```

## 8. Labels (For PR Organization)

Go to **Issues → Labels** and create:

```
bug          - Bugs and defects
enhancement  - Feature requests
documentation - Documentation
performance  - Performance improvements
security     - Security issues
dependencies - Dependency updates
good first issue - Good for new contributors
help wanted  - Help appreciated
wontfix      - Will not be fixed
duplicate    - Duplicate of another issue
```

## 9. Milestone Configuration

**Issues → Milestones** - Create:

```
v0.2.0 - Target date
v0.3.0 - Target date
v1.0.0 - Target date
```

## 10. Code Review Settings

**Settings → Code security & analysis**

Enable:

```
✅ Code scanning with CodeQL (if available)
✅ Secret scanning
✅ Dependency graph
✅ Dependabot alerts
✅ Dependabot updates
```

## 11. Dependabot Configuration

Create `.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
      time: "03:00"
    open-pull-requests-limit: 5
    reviewers:
      - "zebedeu"
    labels:
      - "dependencies"
```

## 12. Pull Request Template

Create `.github/pull_request_template.md`:

```markdown
## Description
<!-- Brief description of changes -->

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation

## Related Issues
Fixes #(issue number)

## Testing
<!-- How was this tested? -->

## Checklist
- [ ] Tests pass
- [ ] Documentation updated
- [ ] No new warnings
- [ ] Commits follow conventional format
```

## 13. Issue Templates

Create `.github/ISSUE_TEMPLATE/bug_report.md`:

```markdown
## Description
<!-- Clear description of the bug -->

## Steps to Reproduce
1. ...
2. ...

## Expected Behavior
<!-- What should happen -->

## Actual Behavior
<!-- What actually happens -->

## Environment
- OS: [e.g., macOS 14.0]
- Rust: [e.g., 1.73.0]
- Version: [e.g., 0.1.0]
```

## 14. Repository Settings

**Settings → General**

```
✅ Issues
✅ Discussions
✅ Projects

Merge options:
☐ Allow merge commits
✅ Allow squash merging
✅ Allow rebase merging

Default branch: main

Auto-delete head branches: ✅
```

## 15. View Workflow Runs

**Actions tab** - See all workflow runs

For each workflow:
- View logs
- See timing
- Check artifacts
- Re-run if needed

## Troubleshooting

### Workflow Not Running

1. Check if enabled: Settings → Actions
2. Check branch pattern in `.yml`
3. Check triggers (push, pull_request, schedule)
4. Wait a few seconds (GitHub can be slow)

### Tests Failing on CI but Passing Locally

1. Different OS behavior
2. Dependency version mismatch
3. Timing issues
4. Cache issues (clear cache in Actions)

### Secrets Not Found

1. Secret name must match exactly (case-sensitive)
2. Must be in **Actions** secrets (not Dependabot)
3. Only available after commit to main
4. Take up to 1 minute to propagate

### Rate Limits

If hitting GitHub API limits:
1. Increase cron schedule interval
2. Use `schedule-cron` with longer gaps
3. Contact GitHub support for higher limits

---

## Next Steps

1. ✅ Add secrets (CARGO_TOKEN, CODECOV_TOKEN)
2. ✅ Set up branch protection on `main`
3. ✅ Create pull request template
4. ✅ Configure Dependabot
5. ✅ Enable code scanning
6. ✅ Set up GitHub Pages (optional)

---

*GitHub Actions Setup - Last Updated: February 2026*
