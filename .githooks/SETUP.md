# Git Hooks Setup

To enable automatic local quality checks:

```bash
git config core.hooksPath .githooks
```

## What the hooks do

Pre-commit hook:
- runs `cargo fmt --all`
- re-stages only Rust files that were already staged before formatting
- preserves partial staging by stashing unstaged and untracked changes temporarily

Pre-push hook:
- runs `cargo fmt --all --check`
- runs `cargo clippy --all-targets --all-features -- -D warnings`
- runs `cargo test --all-targets`
- runs `cargo build --profile ci`

## Bypass

```bash
git commit --no-verify
git push --no-verify
```
