# Contributing

Thanks for helping improve autospec.

## Quick dev setup

Prereqs:

- Rust stable with `rustfmt` and `clippy` installed
- at least one supported agent CLI if you want to run the full tool manually

Build and validate:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-targets
cargo build --profile ci
```

## Git hooks

Enable the repo-local hooks once per clone:

```bash
git config core.hooksPath .githooks
```

The hooks provide two guardrails:

- pre-commit runs `cargo fmt --all` and preserves partial staging for staged Rust files
- pre-push runs the same formatting, clippy, test, and `ci` profile build checks enforced in CI

More detail is in `.githooks/SETUP.md`.

## Project structure

- `src/` contains the Rust CLI implementation
- `templates/default_prompt.md` is the built-in prompt template used for runs
- `tests/` contains integration coverage and fixture-based behavior checks
- `legacy/` keeps the original Python implementation as migration history and behavioral reference
- `docs/` contains planning and architecture notes for the Rust rewrite

## Working on behavior

- Prefer small, focused changes that preserve the current CLI contract unless you are intentionally changing behavior.
- Add or update tests for behavior changes, especially around convergence logic, scope handling, and agent selection.
- Keep README and related docs in sync when install commands, supported agents, or runtime behavior change.

## Release flow

The release workflow is tag-driven and expects Cargo version parity.

1. Update `version` in `Cargo.toml`.
2. Run the local validation commands above.
3. Commit the release changes.
4. Create and push a matching tag in the form `vX.Y.Z`.

GitHub Actions then:

- verifies the tag matches `Cargo.toml`
- builds signed release artifacts
- publishes the GitHub release
- updates the shared Homebrew and Scoop package repositories

## Submitting changes

- Prefer small PRs or focused commits.
- Explain behavior changes clearly.
- Include tests for nontrivial logic changes.
- Update docs alongside user-facing changes.