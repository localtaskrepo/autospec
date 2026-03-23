# autospec Rust Architecture

## Design goals

- Keep the binary small and operationally boring.
- Keep the convergence engine testable without live AI CLIs.
- Preserve the legacy behavior before pursuing new architecture.
- Make extension points explicit, but keep the v1 implementation simple.

Behavioral requirements such as CLI flags, scope semantics, stop conditions, and runtime artifact locations are owned by [rust-v1-spec.md](rust-v1-spec.md). This doc fixes the Rust module boundaries and type ownership used to implement that behavior.

## Crate shape

Use a library-plus-binary layout.

- `src/lib.rs` exposes the internal modules for tests.
- `src/main.rs` is a thin wrapper around CLI parsing, config normalization, and engine execution.

This avoids pushing the full orchestration flow into `main.rs` and makes the convergence loop directly testable.

## Module layout

- `src/cli.rs`
  Defines clap-facing argument structs and enums that mirror the required v1 flags and env-backed values. It must not perform path resolution, sentinel-value normalization, or repository checks.

- `src/config.rs`
  Converts raw CLI input and env values into a validated `RuntimeConfig`. This module normalizes sentinel values before execution starts: `--agent-timeout 0` becomes no timeout, `--max-scope-files 0` becomes unlimited, target and `--doc-dir` paths are resolved against the repo root, and invalid combinations such as `--agent custom` without `--agent-cmd` fail here.

- `src/error.rs`
  Defines a shared error type for failures that abort a run before a terminal `results.tsv` status can be written. The type must distinguish configuration and scope errors from unavailable agents, invalid custom command templates, agent command-not-found, pre-commit git failures such as dirty-scope checks or branch creation, and output-write failure.

- `src/prompt.rs`
  Owns the embedded default prompt and renders the iteration-aware prompt text. Rendering inputs must cover the scope mode, prompt target text, optional goal, current iteration, max iterations, last delta string, tracked-file count, and last changed-file count.

- `src/docs.rs`
  Collects markdown files recursively, resolves the focus doc and scope root from a file-or-directory target, applies `--skip-readmes`, enforces the optional scope-size cap only for `ripple` and `sweep`, and produces sparse-seed warnings. It must raise a hard error for a missing target path or an empty markdown scope.

- `src/state.rs`
  Reads tracked files into a `ScopeSnapshot` ordered by repo-relative path and computes deterministic scope hashes from exact path-plus-content state. Identical tracked file states must always produce the same hash regardless of filesystem traversal order.

- `src/diff.rs`
  Computes per-file and cross-scope line insertion/deletion counts and the ordered changed-file list for the tracked markdown set. This module owns the `+<insertions>/-<deletions>` formatting used by logs and `results.tsv`.

- `src/agent.rs`
  Defines built-in agent specifications for Copilot, Claude, Codex, and Gemini; built-in installation detection; default auto-detection priority `copilot -> claude -> codex -> gemini`; explicit built-in selection failure when the requested CLI is missing; custom command parsing; and subprocess execution with distinct timeout, command-not-found, and nonzero-exit results.

- `src/git.rs`
  Runs git subprocesses for dirty-scope checks, optional branch creation as `autospec/YYYYMMDD-HHMM`, staging of changed tracked files, and per-iteration commits.

- `src/output.rs`
  For non-dry-run executions, creates `.autospec/logs/`, ensures `.autospec/results.tsv` exists with the header `doc\titerations\tstatus\tdelta\ttimestamp`, appends tab-delimited result rows, and writes per-iteration and per-run logs. In all modes it formats human-readable console output.

- `src/engine.rs`
  Owns preflight validation, mode dispatch, and the exact iteration order: render prompt, snapshot before, run agent, snapshot after, check no-diff convergence, compute delta, check repeated-state oscillation, optionally commit, emit logs and console output, then evaluate low-delta and max-iteration stop rules. It must apply the stop conditions from [rust-v1-spec.md](rust-v1-spec.md) without embedding CLI parsing or scope discovery logic.

## Core types

Suggested core types for v1:

- `RuntimeConfig`
  Fully validated run settings passed into the engine. By the time this type exists, path resolution, env fallback, and sentinel normalization must already be complete.

- `Scope`
  Resolved scope metadata: mode `strict|ripple|sweep`, scope root, optional focus doc, and the ordered tracked markdown paths for this run.

- `AgentKind`
  Exact built-in and explicit selection enum: `Copilot`, `Claude`, `Codex`, `Gemini`, or `Custom`.

- `AgentAvailability`
  Built-in discovery result for one built-in agent: either installed with a resolved executable path or missing. Custom commands are validated separately and are not part of auto-detection.

- `ScopePlan`
  Derived run identifiers and scope data needed by the engine: prompt target text, results key, log slug, focus doc, and tracked files.

- `ScopeSnapshot`
  Ordered mapping from repo-relative markdown path to full file contents plus the deterministic hash of that exact tracked state.

- `FileDelta`
  One tracked file's repo-relative path together with insertion and deletion counts.

- `ScopeDelta`
  Aggregate insertion and deletion totals, the formatted `+<insertions>/-<deletions>` summary string, and the ordered changed-file list for one iteration.

- `IterationState`
  Mutable loop state: current iteration number starting at `1`, seen snapshot hashes, low-delta streak length, previous delta string, and previous changed-file count.

- `RunOutcome`
  Terminal engine result with exact variants `ConvergedNoDiff`, `ConvergedLowDelta`, `Oscillating`, `AgentTimeout`, `AgentFailed`, `CommitFailed`, and `NotConverged`. `output.rs` maps the two converged variants to the legacy `results.tsv` status value `converged`, `AgentTimeout` to `agent-timeout`, `AgentFailed` to `agent-failed`, `CommitFailed` to `commit-failed`, `Oscillating` to `oscillating`, and `NotConverged` to `not-converged`. Agent command-not-found remains an `AutospecError`.

- `AutospecError`
  User-facing error enum for failures that prevent the run from starting or prevent status reporting, including missing target docs, empty scope, dirty-doc guard failures, unavailable agents, invalid custom command templates, agent command-not-found, pre-commit git failures, and output I/O failures.

These types should replace the tuple-heavy style of the legacy script and make tests easier to read.

## Asset handling

The default prompt should be a normal markdown file in the repository and embedded at compile time.

Recommended source path:

- `templates/default_prompt.md`

Recommended embedding mechanism:

- `include_str!()`

The Rust binary must not search for an alternate prompt file at runtime in v1. `templates/default_prompt.md` is the single editable source, copied from `legacy/prompt.md` during migration as defined in [rust-implementation-setup.md](rust-implementation-setup.md).

Rationale:

- a single prompt file does not justify a heavier asset-bundling dependency
- the binary remains self-contained for Homebrew and Scoop installs
- prompt changes remain easy to diff in normal markdown

## Dependency choices

### runtime dependencies

- `clap`
  for flag parsing and env-backed defaults

- `thiserror`
  for typed errors without a heavy diagnostics stack

- `walkdir`
  for recursive markdown collection

- `sha2`
  for deterministic scope hashing

- `csv`
  for tab-delimited `.autospec/results.tsv` writing

- `chrono`
  for UTC ISO 8601 timestamps in `results.tsv`

- `wait-timeout`
  for child-process timeout handling

- `similar`
  for line-level diffing

- `shell-words`
  for safe parsing of custom command templates

- `which`
  for cross-platform executable discovery during built-in agent auto-detection

### dev dependencies

- `assert_cmd`
  for binary-level CLI tests

- `predicates`
  for output assertions

- `tempfile`
  for temporary git repos and fixture trees

- `once_cell`
  for reusable fixture setup where needed

- `pretty_assertions`
  for readable test failures on prompt and diff output

## Dependencies to avoid in v1

- `git2`
- `tokio`
- `serde`
- `serde_json`
- `serde_yaml`
- `include_dir`
- `regex`
- general logging frameworks

Reasons:

- they do not solve a current v1 requirement
- they increase binary size or mental overhead
- they encourage architecture that is broader than the current product scope

## Testing strategy

The engine must be testable without real AI agents.

Required testing layers:

- unit tests for prompt rendering, diff calculation, and snapshot hashing
- integration tests for scope discovery and config normalization
- end-to-end tests using a fake agent command against a temporary git repo

The agent module must also have deterministic tests for:

- built-in agent discovery
- explicit built-in agent selection failure when the requested CLI is missing
- default built-in agent priority: Copilot, then Claude, then Codex, then Gemini
- custom-agent precedence when selected explicitly

The fake agent command should support scripted behaviors such as:

- no-op edit
- one-pass fix then no-op
- oscillating file edits
- ripple edit touching more than one file
- sweep edit touching several files

## Extension seams for post-v1 features

The design should keep these seams explicit:

- prompt rendering separate from the engine so cached-context modes can be added later
- convergence rules separate from agent execution so clean-verify passes can be added later
- results writing separate from console output so richer reporting can be added later
- scope planning separate from execution so phase-aware run order can be added later
- built-in agent discovery separate from process execution so new CLIs can be added without rewriting the convergence engine

Do not build a generic plugin system for these. A few clear internal seams are enough.

## Binary-size policy

The Rust rewrite should treat binary size as a first-class constraint.

Required release-profile choices:

- `lto = "fat"`
- `codegen-units = 1`
- `strip = "symbols"`
- `opt-level = "z"`
- `panic = "abort"`

Also add a separate `ci` profile for `cargo test --profile ci` and CI smoke builds. That profile must avoid the release-only size optimizations above so CI does not pay full release-link cost.
