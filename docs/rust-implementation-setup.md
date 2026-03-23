# autospec Rust Implementation Setup

## Purpose

This doc defines the initial repository setup needed to start implementing the Rust binary described in [rust-v1-spec.md](rust-v1-spec.md), [rust-architecture.md](rust-architecture.md), and [rust-roadmap.md](rust-roadmap.md) without reopening those earlier planning decisions.

## Immediate repository goals

- keep the legacy Python implementation intact under `legacy/`
- start the Rust rewrite at the repository root
- make the Rust crate layout compatible with GitHub Actions, Homebrew, and Scoop distribution
- make test fixtures and fake-agent infrastructure part of the repo from the beginning

## Initial repository layout

Recommended layout:

```text
autospec/
  Cargo.toml
  Cargo.lock
  rust-toolchain.toml
  README.md
  IDEAS.md
  docs/
  legacy/
  templates/
    default_prompt.md
  src/
    lib.rs
    main.rs
    cli.rs
    config.rs
    error.rs
    prompt.rs
    docs.rs
    state.rs
    diff.rs
    agent.rs
    git.rs
    output.rs
    engine.rs
  tests/
    cli.rs
    integration.rs
  tests/fixtures/
    docs-basic/
    docs-ripple/
    docs-sweep/
  tests/support/
    fake_agent.py
  .github/
    workflows/
      ci.yml
      release.yml
```

## Prompt asset migration

- copy the canonical prompt from `legacy/prompt.md` to `templates/default_prompt.md`
- treat `templates/default_prompt.md` as the editable prompt source for the Rust binary
- embed `templates/default_prompt.md` at compile time with `include_str!()`
- keep `legacy/prompt.md` only as migration history until the rewrite is stable

## Runtime artifact policy

The Rust binary must write runtime artifacts under the user-supplied target repository and must not write logs or results into the binary install location.

Initial runtime layout:

```text
<target-repo>/
  .autospec/
    logs/
    results.tsv
```

`logs/` stores both per-iteration logs and per-run `.log` files. `results.tsv` is the tabular run-history file described in [rust-v1-spec.md](rust-v1-spec.md). This layout must be implemented in the first functional Rust version so package installs behave correctly from the start.

## Agent setup requirements

Built-in agent support must include:

- Copilot CLI
- Claude CLI
- Codex CLI
- Gemini CLI

Custom agent support must also be present through `--agent custom --agent-cmd <template>`.

Auto-detection policy:

1. treat a built-in CLI as installed only when its executable is discoverable on `PATH`
2. choose Copilot if installed
3. otherwise choose Claude if installed
4. otherwise choose Codex if installed
5. otherwise choose Gemini if installed
6. otherwise fail with a clear error that lists the supported built-in CLIs and explains `--agent custom --agent-cmd <template>`

Explicit override policy:

- `--agent` always overrides auto-detection
- if the explicitly requested built-in CLI is missing, fail immediately and do not fall back to a different built-in CLI
- `--agent custom` requires a non-empty `--agent-cmd <template>`

## Minimal Cargo setup requirements

The initial `Cargo.toml` should include:

- package metadata required for public distribution: `name`, `version`, `edition`, `license`, `description`, and `repository`
- a release profile tuned for small binaries with `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"`, `opt-level = "z"`, and `panic = "abort"`
- a dedicated `ci` profile for `cargo test --profile ci` and CI smoke builds; it must avoid the release-only size optimizations above so CI does not pay release-link cost
- only the v1 runtime dependencies defined in [rust-architecture.md](rust-architecture.md): `clap` with the `derive` and `env` features enabled, plus `thiserror`, `walkdir`, `sha2`, `csv`, `chrono`, `wait-timeout`, `similar`, `shell-words`, and `which`
- only the v1 dev dependencies defined in [rust-architecture.md](rust-architecture.md): `assert_cmd`, `predicates`, `tempfile`, `once_cell`, and `pretty_assertions`

`rust-toolchain.toml` must pin the `stable` channel and include the `rustfmt` and `clippy` components because the first CI version runs both tools.

The initial crate should pass `cargo check --all-targets` before any product logic is ported.

## Initial implementation order

1. create Cargo metadata and rust-toolchain pin
2. create module files and empty public interfaces
3. copy the prompt into `templates/default_prompt.md`
4. implement CLI parsing and config normalization
5. implement scope discovery and prompt rendering
6. implement snapshot, diff, and output primitives
7. implement agent detection and execution
8. implement git helpers
9. implement the convergence engine
10. add integration tests and fake-agent coverage

## Initial CI setup requirements

Before release automation, CI should already validate:

- `cargo fmt --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- unit tests
- integration tests
- at least one binary-level smoke test against a fixture repo using `tests/support/fake_agent.py`

The first CI version does not need release packaging, but it must block release work until the Rust core behavior is under test.

## Definition of ready-to-implement

The repository is ready for full Rust implementation when all of these are true:

- `docs/rust-v1-spec.md`, `docs/rust-architecture.md`, `docs/rust-roadmap.md`, and this doc do not contradict each other on CLI surface, agent selection, runtime artifacts, module layout, or dependency policy
- the prompt source location is fixed to `templates/default_prompt.md`, with `legacy/prompt.md` retained only as migration history
- the module layout is fixed to the root-level library-plus-binary crate shown above and described in [rust-architecture.md](rust-architecture.md)
- the dependency set is fixed to the v1 runtime and dev dependency lists in [rust-architecture.md](rust-architecture.md)
- the agent detection rules are fixed to the priority order and override behavior in [rust-v1-spec.md](rust-v1-spec.md)
- the runtime artifact location is fixed to `<target-repo>/.autospec/`
- the first fixture strategy is fixed to `tests/fixtures/docs-basic/`, `tests/fixtures/docs-ripple/`, `tests/fixtures/docs-sweep/`, and `tests/support/fake_agent.py`

This doc exists to keep those setup decisions closed while implementation begins.
