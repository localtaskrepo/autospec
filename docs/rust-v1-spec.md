# autospec Rust Binary v1 Spec

## Purpose

The Rust binary is a standalone CLI that reproduces the useful behavior of the legacy Python autospec loop while being small, fast, and ready for extension.

The v1 goal is parity-first, not feature expansion.

## Product goals

- Preserve the core doc-convergence workflow from the legacy Python implementation.
- Produce a small, distributable binary suitable for Homebrew, Scoop, and direct GitHub release downloads.
- Keep runtime behavior simple and observable through explicit logs and results files.
- Keep the internal design easy to extend with selected post-v1 features from [`IDEAS.md`](../IDEAS.md).

## Non-goals for v1

- Do not add new product modes beyond `strict`, `ripple`, and `sweep`.
- Do not add remote services, background daemons, or async orchestration.
- Do not add plugin systems or dynamic runtime loading.
- Do not implement context caching, growth guards, parallel execution, or model tiering in v1.
- Do not require a config file.

## Required behavior parity

The Rust binary must preserve these core workflow behaviors from the legacy Python script.
Agent selection in v1 intentionally extends the legacy behavior with Gemini support and built-in auto-detection as defined in this spec:

- accept a file target or directory target
- collect markdown docs recursively from the target scope
- support `strict`, `ripple`, and `sweep` scope modes
- support built-in agent modes for Copilot, Claude, Codex, Gemini, and a custom command
- auto-detect installed built-in agent CLIs when the user does not specify `--agent`
- default auto-detection priority to Copilot first, then Claude, then Codex, then Gemini
- fail fast with a clear error if no supported built-in agent CLI is installed and no custom command is supplied
- build prompts from a canonical prompt template plus iteration-aware context
- track scope snapshots by file content, not by agent response text
- stop on no-diff convergence
- stop on repeated-state oscillation
- stop on low-delta near-convergence after the configured streak length
- stop on iteration limit
- stop on per-iteration agent timeout
- support optional per-iteration commits
- support dirty-doc guards with explicit override
- support optional no-branch mode
- record per-iteration logs and tabular run results
- warn when seed docs are too sparse to produce project-specific output reliably

## CLI surface

The v1 binary must preserve the legacy flag names, environment variable names, and shared-setting semantics except for the runtime artifact location explicitly changed in [Output contract](#output-contract) and the v1 agent-selection additions defined below.

For v1, the target repository root is the current working directory when the binary starts.
Resolve relative `target` and `--doc-dir` paths against that directory.
Absolute `target` and `--doc-dir` paths are valid only when they stay inside that directory tree; otherwise fail before starting.
All repo-relative paths, `.autospec/` output paths, and the `{cwd}` agent placeholder are defined from that directory.

Required CLI parameters and flags:

- `target [path]` optional positional path to a file or directory; when provided, it is the effective target and `--doc-dir` / `DOC_DIR` are ignored; when omitted, use `--doc-dir` / `DOC_DIR`; if the resolved path does not exist, fail before starting
- `--scope strict|ripple|sweep`
- `--goal <text>` optional free-form string; default empty string
- `--max-iters <n>` where `n` is an integer `>= 1`; default `10`
- `--threshold <n>` where `n` is an integer `>= 0`; default `10`
- `--stable-iters <n>` where `n` is an integer `>= 1`; default `2`
- `--agent copilot|claude|codex|gemini|custom`
- `--agent-cmd <template>` custom command template; valid only when the effective agent is `custom`
- `--model <name>` opaque model string passed through to the agent; default `gpt-5.4`
- `--effort <level>` opaque effort string passed through unchanged to built-in agents that declare effort support; default empty string
- `--agent-timeout <seconds>` where `<seconds>` is an integer `>= 0`; `0` disables timeout; default `600`
- `--doc-dir <path>` default target path used when the positional target is omitted; it may resolve to either a file or directory under the repo root; default `docs`
- `--skip-readmes` boolean flag; default `false`
- `--allow-dirty` boolean flag; default `false`
- `--no-commit` boolean flag; default `false`
- `--no-branch` boolean flag; default `false`
- `--max-scope-files <n>` where `n` is an integer `>= 0`; `0` means unlimited; enforce this cap only for `ripple` and `sweep`; default `0`
- `--dry-run` boolean flag; default `false`

Required environment variable support:

- `AGENT`
- `AGENT_CMD`
- `MODEL`
- `EFFORT`
- `AGENT_TIMEOUT`
- `MAX_ITERS`
- `THRESHOLD`
- `STABLE_ITERS`
- `SCOPE`
- `MAX_SCOPE_FILES`
- `GOAL`
- `DOC_DIR`
- `SKIP_READMES`
- `ALLOW_DIRTY`
- `NO_COMMIT`
- `NO_BRANCH`
- `DRY_RUN`

Normalization rules:

- For every setting other than `--agent`, an explicit CLI value overrides the corresponding environment variable.
- `--agent` overrides `AGENT`; if `--agent` is unset and `AGENT` is set, use `AGENT` directly; run built-in auto-detection only when both are unset.
- If neither CLI nor environment provides a value, use the defaults listed above.
- After CLI/env resolution, validate the effective target path the same way whether it came from the positional `target` or from `--doc-dir` / `DOC_DIR`.
- For boolean environment variables (`SKIP_READMES`, `ALLOW_DIRTY`, `NO_COMMIT`, `NO_BRANCH`, `DRY_RUN`), the value `1` means `true`; any other value or an unset variable means `false`.
- For enum-backed environment variables, `AGENT` must be one of `copilot|claude|codex|gemini|custom` and `SCOPE` must be one of `strict|ripple|sweep`; any other value fails before starting.
- If an integer environment variable cannot be parsed as a base-10 integer in its valid range, fail before starting the run.

Default agent-selection behavior when `--agent` and `AGENT` are both unset:

1. detect whether the Copilot CLI is installed and use it if present
2. otherwise detect Claude CLI and use it if present
3. otherwise detect Codex CLI and use it if present
4. otherwise detect Gemini CLI and use it if present
5. otherwise fail with an error that lists the supported built-in CLIs and explains how to use `--agent custom --agent-cmd ...`

Override behavior:

- if the user explicitly selects a built-in agent with `--agent`, that choice overrides the priority list
- if the requested built-in agent is not installed, fail immediately instead of silently falling back to a different built-in agent
- if the effective agent is `custom` (from `--agent` or `AGENT`), a command template from `--agent-cmd` or `AGENT_CMD` is required
- if `--agent` is not `custom`, ignore `--agent-cmd` and `AGENT_CMD`

## Scope semantics

- Resolve `scope_dir` before the run starts: for a file target, `scope_dir` is the target file's parent directory; for a directory target, `scope_dir` is the target directory itself. Store `scope_dir` and every collected doc path as repo-relative paths with `/` separators and no trailing slash; represent the repo root as `.`.
- Collect directory targets recursively from `scope_dir` using `*.md`, sort the resulting repo-relative paths lexically, and use that order for any per-doc directory run.
- `--skip-readmes` removes files whose basename is `README.md` from recursive directory collection and scope discovery, but it must not suppress an explicitly targeted file named `README.md`.
- If the resolved target or tracked scope contains zero markdown files after applying `--skip-readmes`, fail before starting.
- For `ripple` and `sweep`, apply `--max-scope-files` after `--skip-readmes` to the resolved tracked scope; if the tracked scope contains more files than the configured nonzero cap, fail before starting.

### strict

- Only the target doc is tracked for diffs and convergence.
- The agent instruction must explicitly forbid edits outside the target doc.
- If the target is a directory, run one independent strict convergence loop per collected markdown file.

### ripple

- The target doc remains the focus doc.
- The whole surrounding scope directory is tracked for changes.
- Cross-doc edits are allowed only when needed for consistency.
- User-facing output must identify when files outside the focus doc changed.
- If the target is a file, run one convergence loop with that file as the focus doc and all markdown files in `scope_dir` as the tracked scope.
- If the target is a directory, run one convergence loop per collected markdown file; each loop uses one focus doc but the same full `scope_dir` tracked scope.

### sweep

- All markdown docs in the scope directory are tracked for changes.
- The agent must be instructed to prefer the smallest touched-file set.
- User-facing output must show how many files changed per iteration and which files changed.
- Sweep always runs as a single convergence loop for the full `scope_dir`.
- If the target is a file, sweep the file's parent directory rather than treating the single file as a sweep of size one.

## Convergence rules

The convergence engine must use repository file state as the source of truth.

### hard stop conditions

- no file changes between the pre-agent and post-agent snapshots; write status `converged`
- repeated full-scope snapshot hash indicating oscillation; seed the seen-state set with the initial pre-run snapshot and write status `oscillating`
- agent timeout; write status `agent-timeout`
- agent process exits nonzero before timeout; write status `agent-failed`
- commit failure when commits are enabled; write status `commit-failed`
- maximum iterations reached without another stop condition; write status `not-converged`

### near-convergence rule

- sum insertions and deletions across the tracked scope after each iteration
- if total changed lines are less than or equal to `threshold`, increment the low-delta streak
- otherwise reset the low-delta streak
- if the streak reaches `stable_iters` after at least two completed iterations, mark the run as converged and write status `converged`

Sparse-seed warnings are advisory only and must not fail the run. The warning thresholds must match the legacy behavior:

- 1 in-scope file with `<= 2` non-empty lines: warn that requirements are almost unspecified
- 1 in-scope file with `<= 12` non-empty lines: warn that output will likely stay generic
- `<= 3` in-scope files with a combined `<= 30` non-empty lines: warn that the scope is very sparse

## Output contract

The v1 binary must preserve the legacy output categories but may change their storage location to fit a standalone install.

Required outputs:

- human-readable console progress
- per-iteration log files
- per-run `.log` file
- `results.tsv` with doc key, iteration count, status, delta, and UTC timestamp

Required storage layout for the standalone binary:

- write runtime artifacts under `.autospec/` inside the target repository
- use `.autospec/logs/` for per-iteration logs and run logs
- use `.autospec/results.tsv` for tabular history

`results.tsv` must be tab-separated, append-only for non-dry-run executions, and start with this header row:

- `doc\titerations\tstatus\tdelta\ttimestamp`

Row semantics:

- `doc` is the repo-relative focus doc path for `strict` and `ripple`
- `doc` is `sweep:<scope_dir>` for `sweep`
- `iterations` is the completed iteration count at stop time
- `status` must be one of `converged`, `oscillating`, `agent-timeout`, `agent-failed`, `commit-failed`, or `not-converged`
- `delta` is `+<insertions>/-<deletions>` for stop conditions reached after a diff is computed, otherwise the empty string
- `timestamp` is a UTC ISO 8601 timestamp

Dry-run mode must not create `.autospec/`, write logs, or append `results.tsv` rows.
Failures that abort before a terminal convergence outcome is reached, including invalid configuration, empty markdown scope, dirty-doc guard failures, unavailable built-in agents, invalid custom command templates, branch-creation failures, and agent command-not-found errors, must not append a `results.tsv` row.

This is the one intentional divergence from the legacy script that should be accepted in v1. A distributed binary must not try to write into its own install location.

## Git behavior

- If commits are enabled and branch creation is enabled, create a branch named `autospec/YYYYMMDD-HHMM` before the first iteration; if branch creation fails, abort before modifying files.
- If commits are enabled, stage only tracked markdown files changed in that iteration and commit them after each successful iteration.
- If commits are disabled, leave the working tree changed but do not stage or commit.
- If dirty-doc guards are enabled, fail before starting when the in-scope docs for the selected mode already differ from `HEAD`.
- In `strict`, the dirty-doc guard checks only the active focus doc for each loop. In `ripple` and `sweep`, it checks every markdown file in the resolved `scope_dir`.
- Dry-run mode must skip dirty-doc checks, branch creation, staging, and commits.

## Agent execution behavior

- Built-in agent command templates must remain part of the binary.
- Built-in agent templates must cover Copilot, Claude, Codex, and Gemini.
- Custom command execution must be supported through a templated command string.
- The built-in executable names used for discovery and execution must be `copilot`, `claude`, `codex`, and `gemini`.
- The custom command template must support exactly these placeholders: `{prompt}`, `{model}`, `{effort}`, `{log}`, and `{cwd}`.
- The custom command template must be tokenized safely using shell-style word parsing; do not invoke it through `sh -c` and do not use naive whitespace splitting.
- If the effective custom command template is not valid shell-style syntax, fail before the first iteration without modifying files.
- The agent runner must support stdin-fed prompts for agents that require it.
- The agent runner must expose timeout failure distinctly from command-not-found and nonzero-exit behavior.
- If a built-in agent is selected explicitly or by auto-detection and its executable is unavailable before the first iteration, fail before modifying files.
- If the resolved executable for a built-in or custom agent cannot be spawned, stop immediately without appending a `results.tsv` row; this is distinct from the terminal status `agent-failed`.
- If the agent process exits nonzero during an iteration, stop the current doc or sweep immediately with status `agent-failed`.
- The agent runner must separate command discovery from command execution so built-in agent auto-detection is cheap and testable.

## Performance constraints

The distributed binary should be optimized for size and startup latency.

Required constraints:

- no async runtime in v1
- no embedded scripting runtime
- no git library dependency in v1
- no dynamic plugin loading in v1
- release builds optimized for small binary size

## Extensibility constraints

The internal design must support future addition of these feature families without a rewrite:

- cached context and clean verification passes
- run ordering or phase-aware execution
- growth guard and size-based stopping rules
- richer results reporting
- optional parallel execution where safe
- alternate model strategies for verification passes

The v1 design does not need to implement these features, but it must avoid coupling that would make them awkward to add.
