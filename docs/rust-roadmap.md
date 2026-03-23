# autospec Rust Roadmap

## Objective

Ship a standalone Rust binary that matches the useful behavior of the legacy Python loop, then extend it deliberately with the highest-value ideas already identified during dogfooding.

## Phase 1: parity-first rewrite

Deliverables:

- Rust crate with a thin binary entrypoint and internal library modules
- full CLI and environment-variable parity with the v1 contract in `docs/rust-v1-spec.md`
- `strict`, `ripple`, and `sweep` modes
- built-in agent support for Copilot, Claude, Codex, Gemini, and custom commands
- built-in agent auto-detection with priority order: Copilot, Claude, Codex, Gemini
- snapshot-based convergence, oscillation detection, and low-delta near-convergence
- repo-local runtime artifacts under `.autospec/`, matching `docs/rust-v1-spec.md` and `docs/rust-implementation-setup.md`
- deterministic test harness using a fake agent

Exit criteria:

- the Rust binary passes deterministic fixture runs for `strict`, `ripple`, and `sweep`, including no-op convergence, one-pass fix then no-op, repeated-state oscillation, ripple multi-file edits, and sweep multi-file edits
- CI runs the required fixture suite with the fake agent harness; no Phase 1 test depends on a real AI CLI

## Phase 2: distribution hardening

Deliverables:

- GitHub Actions release flow that only publishes after the tagged revision passes the required CI jobs defined in `docs/rust-implementation-setup.md`
- `Cargo.toml` package version as the only release source of truth for release tags, artifacts, Homebrew, and Scoop metadata; the git tag name must exactly equal the `package.version` string
- signed release artifacts for macOS, Linux, and Windows
- generated Homebrew formula in the configured shared tap repository
- generated Scoop manifest in the configured shared bucket repository

Exit criteria:

- creating a git tag whose name exactly matches `Cargo.toml` `package.version` publishes signed macOS, Linux, and Windows artifacts
- installing from the published Homebrew formula on macOS and running `autospec --help` succeeds
- installing from the published Scoop manifest on Windows and running `autospec --help` succeeds

## Phase 3: post-v1 product extensions

This phase draws from [`../IDEAS.md`](../IDEAS.md) and should only begin after parity and distribution are stable.

### high-priority extensions

- context caching with the clean-verification loop from [`../IDEAS.md`](../IDEAS.md): cached no-change must trigger a clean-context verification pass, clean no-change must declare convergence, and clean changes must resume the cached loop
- growth guard with a warning at 1.5x the starting doc size and a hard stop with `growth-exceeded` status at 2.0x the starting doc size
- richer output reporting with per-iteration size delta tracking in logs and `results.tsv`

These features improve runtime efficiency and safety without changing the core product definition.

### medium-priority extensions

- run-order strategy or phase-aware execution using the canonical contracts -> platform specs -> leaf docs ordering from [`../IDEAS.md`](../IDEAS.md)
- alternate verification model support, starting with a distinct model selection for clean verification passes

These features improve quality and operator control but are less urgent than safety and repeatability.

### low-priority extensions

- parallel execution only for docs in the same dependency layer and only with isolated git state per run

This should stay late. It raises coordination and git-worktree complexity and is not required to prove the product.

## Design rules for future features

Future feature work must preserve these rules:

- convergence remains file-state based, not response-text based, as defined in `docs/rust-v1-spec.md`
- the default operator experience stays local and CLI-first
- new features must not force a config file on simple runs
- new features must justify their impact on binary size and operational complexity

## Recommended implementation order for extensions

1. growth guard
2. richer reporting and size tracking
3. context caching with clean verification
4. run-order strategy
5. model verification options
6. parallel execution

Rationale:

- growth and reporting improve safety and observability first
- cached context becomes safer once reporting is better
- run order and model selection are useful but not blocking
- parallelism is operationally the most complex

## What not to do too early

- do not introduce a generic configuration file before the CLI shape proves insufficient
- do not add a broad plugin architecture before there are at least two real extension implementations
- do not optimize for full-repo parallel sweeps before bounded single-process runs are very well understood
- do not chase benchmark wins by making the core behavior harder to reason about

## Suggested use of the legacy autospec tool during migration

Use the legacy Python loop to refine this planning corpus in bounded passes.

Recommended order:

1. `docs/rust-v1-spec.md` in `strict`
2. `docs/rust-architecture.md` in `strict`
3. `docs/rust-roadmap.md` in `strict`
4. `docs/rust-implementation-setup.md` in `strict`
5. `docs/rust-v1-spec.md` in `ripple`

This gives the rewrite a better implementation contract before any major Rust coding begins.
