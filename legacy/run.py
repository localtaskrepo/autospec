#!/usr/bin/env python3
"""autospec — Autonomous doc convergence.

Iteratively improves documentation files using an AI coding agent until
the agent reports no further changes needed (convergence).

Usage:
    python autospec/run.py                          # all docs in docs/
    python autospec/run.py docs/product.md          # single doc
    python autospec/run.py docs/ui/                 # all docs in a directory
    python autospec/run.py --agent claude --model sonnet docs/product.md

Supported agents: copilot, codex, claude, custom
"""

from __future__ import annotations

import argparse
import csv
import difflib
import hashlib
import os
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

# ── Defaults ──

DEFAULTS = {
    "agent": "copilot",
    "model": "gpt-5.4",
    "effort": "",
    "agent_timeout": 600,
    "max_iters": 10,
    "threshold": 10,
    "stable_iters": 2,
    "scope": "strict",
    "max_scope_files": 0,
    "doc_dir": "docs",
    "skip_readmes": False,
    "allow_dirty": False,
    "no_commit": False,
    "no_branch": False,
    "dry_run": False,
}

SCOPES = ("strict", "ripple", "sweep")

# ── Agent definitions ──
#
# Each agent defines how to invoke its CLI in non-interactive mode.
# Fields:
#   cmd:          argument list template. Placeholders: {model}, {prompt}, {log}, {cwd}, {effort}
#   effort_flag:  extra args appended when --effort is set (optional)
#   stdin:        if True, prompt is piped via stdin instead of {prompt} (optional)
#
# For "custom", the user supplies the full command template via --agent-cmd.

AGENTS = {
    "copilot": {
        "cmd": [
            "copilot", "-p", "{prompt}",
            "--yolo",
            "--model", "{model}",
            "--share={log}",
            "--no-alt-screen",
        ],
        "effort_flag": ["--effort", "{effort}"],
    },
    "codex": {
        "cmd": [
            "codex", "exec",
            "--full-auto",
            "-m", "{model}",
            "-C", "{cwd}",
            "-o", "{log}",
            "-",  # read prompt from stdin
        ],
        "stdin": True,
    },
    "claude": {
        "cmd": [
            "claude", "-p", "{prompt}",
            "--dangerously-skip-permissions",
            "--model", "{model}",
            "--output-format", "text",
            "--no-session-persistence",
        ],
    },
}


# ── Helpers ──


def collect_docs(target: str, repo_root: Path, skip_readmes: bool) -> list[str]:
    """Collect markdown files under target, returning repo-relative paths."""
    target_path = repo_root / target

    if target_path.is_file():
        return [str(target_path.relative_to(repo_root))]

    if not target_path.is_dir():
        print(f"ERROR: '{target}' is not a file or directory.", file=sys.stderr)
        print(
            "autospec improves existing markdown docs. Create a seed .md file first if you want to bootstrap a new project.",
            file=sys.stderr,
        )
        sys.exit(1)

    docs = sorted(
        str(p.relative_to(repo_root))
        for p in target_path.rglob("*.md")
        if p.is_file()
    )

    if skip_readmes:
        docs = [d for d in docs if not d.endswith("README.md")]

    return docs


SCOPE_TEMPLATES = {
    "strict": "Do NOT touch files other than the target doc.",
    "ripple": (
        "Focus on `{doc}`. You may also edit other docs in `{scope_dir}/` "
        "when necessary for consistency — but keep cross-doc changes minimal."
    ),
    "sweep": (
        "Review all docs in `{scope_dir}/`. Prefer the smallest set of file edits "
        "needed to resolve concrete issues."
    ),
}


def build_prompt(
    template: str, doc: str, scope: str, scope_dir: str, goal: str,
    iteration: int = 1, max_iters: int = 10, last_delta: str = "",
    scope_file_count: int = 1, last_changed_files: int = 0,
) -> str:
    """Substitute placeholders into the prompt template."""
    scope_instruction = SCOPE_TEMPLATES[scope].format(doc=doc, scope_dir=scope_dir)
    goal_block = f"\n\n## Goal\n\n{goal}" if goal else ""
    scope_block = ""

    if scope == "sweep":
        scope_block = (
            f"\n\nThis sweep covers **{scope_file_count} docs** in `{scope_dir}/`. "
            "Prefer the smallest touched-file set that resolves concrete problems."
        )

        if iteration >= 2 and last_changed_files > 0:
            scope_block += (
                f" Previous iteration touched **{last_changed_files} file(s)**. "
                "Touch fewer files this pass unless widening the sweep is strictly necessary."
            )

        if scope_dir == "docs":
            scope_block += (
                "\n\nThis is a **full docs-tree sweep**. Treat it as a canonical-consistency "
                "pass, not a rewrite of every page:\n\n"
                "- Prefer fixing authoritative docs first: `entity-dictionary.md`, "
                "`permissions-matrix.md`, `matching-privacy-matrix.md`, "
                "`lifecycle-rules.md`, `validation-contract.md`, `product.md`, "
                "`configuration.md`, and `database.md`\n"
                "- Only edit downstream or leaf docs when a contradiction would otherwise "
                "leave that page unimplementable\n"
                "- Do NOT propagate wording cleanup across many dependent docs in the same pass\n"
                "- In later iterations, shrink the touched set rather than widening it"
            )

    # Build iteration-aware context block
    if iteration == 1:
        iter_block = (
            "\n\n## Iteration Context\n\n"
            "This is the **first pass**. Focus on substantive issues: "
            "vague requirements, missing states/transitions, incorrect cross-references, "
            "and gaps that would block implementation. Ignore cosmetic concerns."
            f"{scope_block}"
        )
    elif iteration <= 3:
        iter_block = (
            f"\n\n## Iteration Context\n\n"
            f"This is **iteration {iteration} of {max_iters}**. "
            f"Previous iteration changed {last_delta} lines.\n\n"
            "The doc has already been reviewed and improved. Only make changes "
            "that fix a **concrete problem**: a rule that is ambiguous, a state "
            "that is missing, a cross-reference that is wrong, or a value that "
            "is unspecified. Do not reword for style. Do not expand existing "
            "explanations that are already precise enough to implement from."
            f"{scope_block}"
        )
    else:
        iter_block = (
            f"\n\n## Iteration Context\n\n"
            f"This is **iteration {iteration} of {max_iters}**. "
            f"Previous iteration changed {last_delta} lines.\n\n"
            "The doc has been refined through multiple passes. **Treat the "
            "current text as the best version so far.** Only touch it if you "
            "find a clear, objective defect:\n\n"
            "- A requirement that a coding agent cannot implement without guessing\n"
            "- A factual contradiction with a cross-referenced doc\n"
            "- A missing enum value, state, or transition in a defined lifecycle\n\n"
            "If you would make fewer than 5 line changes, the doc is likely "
            "converged — respond with `CONVERGED` instead."
            f"{scope_block}"
        )

    return (
        template
        .replace("{{DOC}}", doc)
        .replace("{{SCOPE_INSTRUCTION}}", scope_instruction)
        .replace("{{GOAL}}", goal_block)
        .replace("{{ITERATION_CONTEXT}}", iter_block)
    )


def read_text(path: Path) -> str:
    """Read a UTF-8 text file, returning an empty string if missing."""
    if not path.exists():
        return ""
    return path.read_text()


def count_nonempty_lines(text: str) -> int:
    """Count lines that contain non-whitespace content."""
    return sum(1 for line in text.splitlines() if line.strip())


def warn_sparse_seed_docs(files: list[str], repo_root: Path) -> None:
    """Warn when the available seed docs are too sparse for project-specific output."""
    if not files:
        return

    nonempty_counts = [count_nonempty_lines(read_text(repo_root / path)) for path in files]
    total_nonempty = sum(nonempty_counts)

    if len(files) == 1 and total_nonempty <= 2:
        print(
            "WARNING: seed docs are almost empty. autospec will likely converge to a conservative 'requirements unspecified' baseline rather than inventing a project spec.",
            file=sys.stderr,
        )
        print(
            "Add a few lines describing entities, flows, or constraints if you want a more useful first draft.",
            file=sys.stderr,
        )
        return

    if len(files) == 1 and total_nonempty <= 12:
        print(
            "WARNING: only one short seed doc was found. autospec can bootstrap from this, but the result will usually stay generic unless the doc names concrete entities, states, and constraints.",
            file=sys.stderr,
        )
        return

    if len(files) <= 3 and total_nonempty <= 30:
        print(
            "WARNING: the in-scope docs are very sparse. autospec works best when there is at least one reasonably concrete seed doc to anchor terminology and scope.",
            file=sys.stderr,
        )


def _log(path: Path, line: str) -> None:
    """Append a line to a log file."""
    with open(path, "a") as f:
        f.write(line + "\n")


def run_agent(
    agent_name: str,
    agent_cmd_override: str | None,
    prompt: str,
    log_path: Path,
    model: str,
    effort: str,
    agent_timeout: int,
    repo_root: Path,
    dry_run: bool,
) -> bool:
    """Invoke the agent CLI. Blocks until completion."""
    if dry_run:
        print(f"  [dry-run] {agent_name} --model {model}")
        return True

    if agent_cmd_override:
        # Custom agent: user provides the template
        cmd_str = agent_cmd_override.format(
            prompt=prompt, model=model, effort=effort,
            log=str(log_path), cwd=str(repo_root),
        )
        cmd = cmd_str.split()
        stdin_input = None
    else:
        agent = AGENTS[agent_name]
        fmt = dict(
            prompt=prompt, model=model, effort=effort,
            log=str(log_path), cwd=str(repo_root),
        )
        cmd = [part.format(**fmt) for part in agent["cmd"]]

        # Append effort flag if the agent supports it and effort is set
        if effort and "effort_flag" in agent:
            cmd.extend(part.format(**fmt) for part in agent["effort_flag"])

        stdin_input = prompt if agent.get("stdin") else None

    timeout = None if agent_timeout <= 0 else agent_timeout

    try:
        result = subprocess.run(
            cmd,
            cwd=str(repo_root),
            input=stdin_input,
            capture_output=True,
            text=True,
            timeout=timeout,
        )
        # Print last few lines of output for visibility
        lines = (result.stdout + result.stderr).strip().splitlines()
        for line in lines[-5:]:
            print(f"  {line}")
    except subprocess.TimeoutExpired:
        if agent_timeout <= 0:
            print("  ✗ Agent timed out")
        else:
            print(f"  ✗ Agent timed out ({agent_timeout}s)")
        return False
    except FileNotFoundError:
        print(f"  ✗ Agent CLI '{cmd[0]}' not found. Is it installed?", file=sys.stderr)
        sys.exit(1)

    # For claude, write output to log since it doesn't have --share
    if agent_name == "claude" and not agent_cmd_override:
        log_path.write_text(result.stdout if result.returncode == 0 else result.stderr)

    return True


def git(*args: str, cwd: Path) -> subprocess.CompletedProcess[str]:
    """Run a git command."""
    return subprocess.run(
        ["git", *args], cwd=str(cwd), capture_output=True, text=True
    )


def has_uncommitted_changes(doc: str, cwd: Path) -> bool:
    """Check if the doc has uncommitted changes vs HEAD."""
    r = git("diff", "--quiet", "--", doc, cwd=cwd)
    return r.returncode != 0


def diff_stat(before_text: str, after_text: str) -> tuple[int, int, str]:
    """Return (insertions, deletions, display_str) between two text snapshots."""
    insertions = 0
    deletions = 0

    diff_lines = difflib.unified_diff(
        before_text.splitlines(),
        after_text.splitlines(),
        fromfile="before",
        tofile="after",
        lineterm="",
    )

    for line in diff_lines:
        if line.startswith(("--- ", "+++ ", "@@")):
            continue
        if line.startswith("+"):
            insertions += 1
        elif line.startswith("-"):
            deletions += 1

    if insertions == 0 and deletions == 0:
        return (0, 0, "")

    return (insertions, deletions, f"+{insertions}/-{deletions}")


def stage_and_commit_changes(
    changed_files: list[str], message: str, cwd: Path,
) -> tuple[bool, str]:
    """Stage and commit changed files, returning success and error summary."""
    for f in changed_files:
        git("add", f, cwd=cwd)

    staged = git("diff", "--cached", "--quiet", cwd=cwd)
    if staged.returncode == 0:
        return (False, "files changed but nothing was staged")

    commit = git("commit", "-m", message, "--no-verify", cwd=cwd)
    if commit.returncode != 0:
        detail = (commit.stderr or commit.stdout).strip().splitlines()
        return (False, detail[-1] if detail else "git commit failed")

    return (True, "")


def snapshot_scope(files: list[str], repo_root: Path) -> dict[str, str]:
    """Read all in-scope files, returning path → content."""
    return {f: read_text(repo_root / f) for f in files}


def snapshot_hash(snap: dict[str, str]) -> str:
    """Return a deterministic hash of a full scope snapshot."""
    h = hashlib.sha256()
    for key in sorted(snap):
        h.update(key.encode())
        h.update(snap[key].encode())
    return h.hexdigest()


def scope_diff_stat(
    before: dict[str, str], after: dict[str, str],
) -> tuple[int, int, str, list[str]]:
    """Return (insertions, deletions, display_str, changed_files) across scope."""
    total_ins = 0
    total_dels = 0
    changed: list[str] = []

    for f in sorted(set(before) | set(after)):
        ins, dels, _ = diff_stat(before.get(f, ""), after.get(f, ""))
        if ins > 0 or dels > 0:
            total_ins += ins
            total_dels += dels
            changed.append(f)

    if total_ins == 0 and total_dels == 0:
        return (0, 0, "", [])

    return (total_ins, total_dels, f"+{total_ins}/-{total_dels}", changed)


def append_result(results_file: Path, doc: str, iters: int, status: str, delta: str = "") -> None:
    """Append a row to results.tsv."""
    with open(results_file, "a", newline="") as f:
        writer = csv.writer(f, delimiter="\t")
        writer.writerow([doc, iters, status, delta, datetime.now(timezone.utc).isoformat()])


# ── Convergence loop ──


def _convergence_loop(
    *,
    label: str,
    slug: str,
    prompt_doc: str,
    results_key: str,
    tracked_files: list[str],
    focus_doc: str,
    args: argparse.Namespace,
    prompt_template: str,
    repo_root: Path,
    log_dir: Path,
    results_file: Path,
) -> bool:
    """Core iteration loop shared by per-doc and sweep convergence modes.

    Args:
        label:         Display name for the header.
        slug:          Base name for log files.
        prompt_doc:    Value substituted into {{DOC}} in the prompt template.
        results_key:   Identifier written to results.tsv.
        tracked_files: Files to snapshot for convergence tracking.
        focus_doc:     Primary doc for ripple display ("also touched").
                       Empty string for strict and sweep modes.
    """
    is_sweep = args.scope == "sweep"

    print(f"\n{'═' * 51}")
    print(f"  {'SWEEP' if is_sweep else 'DOC'}: {label}")
    print(f"{'═' * 51}")

    if args.dry_run:
        iter_log = log_dir / f"{slug}_iter1.md"
        print("── iteration 1/1")
        run_agent(
            agent_name=args.agent,
            agent_cmd_override=args.agent_cmd,
            prompt=build_prompt(prompt_template, prompt_doc, args.scope, args.scope_dir, args.goal),
            log_path=iter_log,
            model=args.model,
            effort=args.effort,
            agent_timeout=args.agent_timeout,
            repo_root=repo_root,
            dry_run=True,
        )
        print("  ✓ Dry run complete")
        return True

    log_file = log_dir / f"{slug}.log"
    for old in log_dir.glob(f"{slug}_iter*.md"):
        old.unlink()
    log_file.write_text("")

    delta = ""
    low_delta_streak = 0
    last_delta = ""
    last_changed_files = 0
    initial_snap = snapshot_scope(tracked_files, repo_root)
    seen_states: set[str] = {snapshot_hash(initial_snap)}

    for iteration in range(1, args.max_iters + 1):
        iter_log = log_dir / f"{slug}_iter{iteration}.md"
        print(f"── iteration {iteration}/{args.max_iters}")

        prompt = build_prompt(
            prompt_template, prompt_doc, args.scope, args.scope_dir, args.goal,
            iteration=iteration,
            max_iters=args.max_iters,
            last_delta=last_delta,
            scope_file_count=len(tracked_files),
            last_changed_files=last_changed_files,
        )
        before_snap = snapshot_scope(tracked_files, repo_root)

        completed = run_agent(
            agent_name=args.agent,
            agent_cmd_override=args.agent_cmd,
            prompt=prompt,
            log_path=iter_log,
            model=args.model,
            effort=args.effort,
            agent_timeout=args.agent_timeout,
            repo_root=repo_root,
            dry_run=False,
        )

        if not completed:
            _log(log_file, f"[{iteration}] agent-timeout")
            append_result(results_file, results_key, iteration, "agent-timeout")
            return False

        after_snap = snapshot_scope(tracked_files, repo_root)

        if before_snap == after_snap:
            print(f"  ✓ No changes — converged after {iteration} iteration(s)")
            _log(log_file, f"[{iteration}] no-diff → converged")
            append_result(results_file, results_key, iteration, "converged")
            return True

        ins, dels, delta, changed_files = scope_diff_stat(before_snap, after_snap)
        last_delta = delta
        last_changed_files = len(changed_files)
        state_key = snapshot_hash(after_snap)

        if state_key in seen_states:
            print(f"  ⚠ Oscillation detected ({delta or '?'})")
            _log(log_file, f"[{iteration}] oscillating ({delta})")
            append_result(results_file, results_key, iteration, "oscillating", delta)
            return False

        seen_states.add(state_key)

        if not args.no_commit:
            committed, error = stage_and_commit_changes(
                changed_files,
                f"autospec: {results_key} — iteration {iteration}",
                repo_root,
            )
            if not committed:
                print(f"  ✗ Commit failed: {error}")
                _log(log_file, f"[{iteration}] commit-failed: {error}")
                append_result(results_file, results_key, iteration, "commit-failed", delta)
                return False
            action = "committing"
        else:
            action = "keeping working tree changes"

        # Display change details based on mode
        if is_sweep:
            print(f"  → {len(changed_files)} file(s) changed ({delta}), {action}")
            for cf in changed_files:
                _, _, cf_delta = diff_stat(before_snap.get(cf, ""), after_snap.get(cf, ""))
                print(f"    {cf} ({cf_delta})")
        elif focus_doc and len(changed_files) > 1:
            others = [f for f in changed_files if f != focus_doc]
            print(f"  → changes ({delta}), {action} — also touched: {', '.join(others)}")
        else:
            print(f"  → changes detected ({delta or '?'}), {action}")

        _log(log_file, f"[{iteration}] changed ({delta}) files={','.join(changed_files)}")

        if ins + dels <= args.threshold:
            low_delta_streak += 1
        else:
            low_delta_streak = 0

        if iteration >= 2 and low_delta_streak >= args.stable_iters:
            print(f"  ✓ Near-converged ({delta}) after {iteration} iteration(s)")
            _log(log_file, f"[{iteration}] near-converged")
            append_result(results_file, results_key, iteration, "converged", delta)
            return True

    print(f"  ✗ Did not converge after {args.max_iters} iterations")
    append_result(results_file, results_key, args.max_iters, "not-converged", delta)
    return False


def converge_doc(
    doc: str,
    scope_files: list[str],
    args: argparse.Namespace,
    prompt_template: str,
    repo_root: Path,
    log_dir: Path,
    results_file: Path,
) -> bool:
    """Run convergence for a single focus doc (strict or ripple mode)."""
    is_ripple = args.scope == "ripple"
    return _convergence_loop(
        label=f"{doc} (ripple → {len(scope_files)} files)" if is_ripple else doc,
        slug=doc.replace("/", "__").removesuffix(".md"),
        prompt_doc=doc,
        results_key=doc,
        tracked_files=scope_files if is_ripple else [doc],
        focus_doc=doc if is_ripple else "",
        args=args,
        prompt_template=prompt_template,
        repo_root=repo_root,
        log_dir=log_dir,
        results_file=results_file,
    )


def converge_sweep(
    scope_files: list[str],
    scope_dir: str,
    args: argparse.Namespace,
    prompt_template: str,
    repo_root: Path,
    log_dir: Path,
    results_file: Path,
) -> bool:
    """Run convergence for all docs in scope (sweep mode)."""
    return _convergence_loop(
        label=f"{scope_dir}/ ({len(scope_files)} files)",
        slug=f"{scope_dir.replace('/', '__')}__sweep",
        prompt_doc=scope_dir,
        results_key=f"sweep:{scope_dir}",
        tracked_files=scope_files,
        focus_doc="",
        args=args,
        prompt_template=prompt_template,
        repo_root=repo_root,
        log_dir=log_dir,
        results_file=results_file,
    )



# ── CLI ──


def main() -> None:
    parser = argparse.ArgumentParser(
        description="autospec — Autonomous doc convergence using AI coding agents.",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
examples:
  python autospec/run.py                                    # all docs in docs/
  python autospec/run.py docs/product.md                    # single doc
  python autospec/run.py --scope ripple docs/entity-dictionary.md
  python autospec/run.py --scope sweep docs/               # converge entire folder
  python autospec/run.py --scope sweep --goal "Align lifecycle states" docs/
  python autospec/run.py --agent claude --model sonnet .    # use Claude
  AGENT=copilot MODEL=gpt-5.4 python autospec/run.py       # env vars work too
        """,
    )
    parser.add_argument("target", nargs="?", help="File or directory to process (default: docs/)")
    parser.add_argument("--agent", default=os.environ.get("AGENT", DEFAULTS["agent"]),
                        choices=list(AGENTS.keys()) + ["custom"],
                        help="Agent CLI to use (default: copilot)")
    parser.add_argument("--agent-cmd", default=os.environ.get("AGENT_CMD"),
                        help="Custom agent command template. Use {prompt}, {model}, {effort}, {log}, {cwd} placeholders.")
    parser.add_argument("--model", default=os.environ.get("MODEL", DEFAULTS["model"]),
                        help="LLM model name (default: gpt-5.4)")
    parser.add_argument("--effort", default=os.environ.get("EFFORT", DEFAULTS["effort"]),
                        help="Reasoning effort level (e.g. low, medium, high, xhigh). Passed to agents that support it.")
    parser.add_argument("--agent-timeout", type=int,
                        default=int(os.environ.get("AGENT_TIMEOUT", DEFAULTS["agent_timeout"])),
                        help="Max seconds per agent iteration; use 0 to disable the timeout (default: 600)")
    parser.add_argument("--max-iters", type=int,
                        default=int(os.environ.get("MAX_ITERS", DEFAULTS["max_iters"])),
                        help="Max iterations per doc (default: 10)")
    parser.add_argument("--threshold", type=int,
                        default=int(os.environ.get("THRESHOLD", DEFAULTS["threshold"])),
                        help="Converge when total changed lines ≤ this (default: 10)")
    parser.add_argument("--stable-iters", type=int,
                        default=int(os.environ.get("STABLE_ITERS", DEFAULTS["stable_iters"])),
                        help="Require this many consecutive low-delta iterations before near-converged (default: 2)")
    parser.add_argument("--scope", default=os.environ.get("SCOPE", DEFAULTS["scope"]),
                        choices=list(SCOPES),
                        help="Editing scope: strict (only focus doc), ripple (focus + cross-doc fixes), sweep (whole folder)")
    parser.add_argument("--max-scope-files", type=int,
                        default=int(os.environ.get("MAX_SCOPE_FILES", DEFAULTS["max_scope_files"])),
                        help="Optional cap on files allowed in ripple/sweep scope; use 0 for unlimited (default: 0)")
    parser.add_argument("--goal", default=os.environ.get("GOAL", ""),
                        help="Optional goal to inject into the prompt (e.g. 'Align lifecycle state names')")
    parser.add_argument("--doc-dir", default=os.environ.get("DOC_DIR", DEFAULTS["doc_dir"]),
                        help="Default doc directory when no target given (default: docs)")
    parser.add_argument("--skip-readmes", action="store_true",
                        default=os.environ.get("SKIP_READMES", "0") == "1",
                        help="Skip README.md files")
    parser.add_argument("--allow-dirty", action="store_true",
                        default=os.environ.get("ALLOW_DIRTY", "0") == "1",
                        help="Allow running when target docs already have uncommitted changes")
    parser.add_argument("--no-commit", action="store_true",
                        default=os.environ.get("NO_COMMIT", "0") == "1",
                        help="Do not create commits; leave doc edits in the working tree")
    parser.add_argument("--no-branch", action="store_true",
                        default=os.environ.get("NO_BRANCH", "0") == "1",
                        help="Don't create a git branch, commit on current branch")
    parser.add_argument("--dry-run", action="store_true",
                        default=os.environ.get("DRY_RUN", "0") == "1",
                        help="Print commands without executing")

    args = parser.parse_args()

    if args.agent == "custom" and not args.agent_cmd:
        parser.error("--agent custom requires --agent-cmd")

    # Resolve paths
    autospec_dir = Path(__file__).resolve().parent
    repo_root = autospec_dir.parent
    prompt_file = autospec_dir / "prompt.md"
    log_dir = autospec_dir / "logs"
    results_file = autospec_dir / "results.tsv"

    log_dir.mkdir(exist_ok=True)

    if not prompt_file.exists():
        print(f"ERROR: prompt template not found at {prompt_file}", file=sys.stderr)
        sys.exit(1)

    prompt_template = prompt_file.read_text()

    # Initialize results file
    if not args.dry_run and not results_file.exists():
        results_file.write_text("doc\titerations\tstatus\tdelta\ttimestamp\n")

    # Collect docs
    target = args.target or args.doc_dir
    docs = collect_docs(target, repo_root, args.skip_readmes)

    if not docs:
        print("No documents found.", file=sys.stderr)
        print(
            "autospec needs at least one existing markdown doc. It does not scaffold a project from an empty directory.",
            file=sys.stderr,
        )
        sys.exit(1)

    # Determine the scope directory for ripple/sweep awareness.
    # For a file target, scope_dir is its parent directory.
    # For a directory target, scope_dir is the target itself.
    target_path = repo_root / target
    if target_path.is_file():
        args.scope_dir = str(target_path.parent.relative_to(repo_root))
    else:
        args.scope_dir = target.rstrip("/")

    # Collect all scope files (used for ripple and sweep tracking)
    scope_files = collect_docs(args.scope_dir, repo_root, args.skip_readmes)

    warning_files = scope_files if args.scope != "strict" else docs
    warn_sparse_seed_docs(warning_files, repo_root)

    if args.scope != "strict" and args.max_scope_files > 0 and len(scope_files) > args.max_scope_files:
        print(
            f"ERROR: scope contains {len(scope_files)} files, exceeding --max-scope-files={args.max_scope_files}.",
            file=sys.stderr,
        )
        print("Set --max-scope-files 0 to allow unlimited scope size.", file=sys.stderr)
        sys.exit(1)

    if not args.dry_run and not args.allow_dirty:
        check_files = scope_files if args.scope != "strict" else docs
        dirty_docs = [d for d in check_files if has_uncommitted_changes(d, repo_root)]
        if dirty_docs:
            print("ERROR: in-scope docs already have uncommitted changes.", file=sys.stderr)
            for d in dirty_docs:
                print(f"  - {d}", file=sys.stderr)
            print("Use --allow-dirty to override.", file=sys.stderr)
            sys.exit(1)

    total = len(docs)

    print("autospec — doc convergence loop")
    print(f"agent:      {args.agent}")
    print(f"model:      {args.model}")
    if args.effort:
        print(f"effort:     {args.effort}")
    print(f"timeout:    {'disabled' if args.agent_timeout <= 0 else f'{args.agent_timeout}s'}")
    print(f"scope:      {args.scope}")
    if args.max_scope_files > 0:
        print(f"scope cap:  {args.max_scope_files} files")
    if args.goal:
        print(f"goal:       {args.goal}")
    print(f"max iters:  {args.max_iters}")
    print(f"threshold:  {args.threshold}")
    print(f"stable iters: {args.stable_iters}")
    print(f"target:     {target}")
    print(f"docs found: {total}")
    if args.scope != "strict":
        print(f"scope dir:  {args.scope_dir}/ ({len(scope_files)} files in scope)")
    print(f"commit mode: {not args.no_commit}")
    print(f"dry run:    {args.dry_run}")
    print()

    # Optionally create a branch
    if not args.dry_run and not args.no_commit and not args.no_branch:
        run_tag = f"autospec/{datetime.now().strftime('%Y%m%d-%H%M')}"
        checkout = git("checkout", "-b", run_tag, cwd=repo_root)
        if checkout.returncode != 0:
            detail = (checkout.stderr or checkout.stdout).strip()
            print(f"ERROR: failed to create branch {run_tag}: {detail}", file=sys.stderr)
            sys.exit(1)
        print(f"branch:     {run_tag}")

    # ── Dispatch based on scope ──

    if args.scope == "sweep":
        converged = converge_sweep(
            scope_files, args.scope_dir, args, prompt_template,
            repo_root, log_dir, results_file,
        )
        print(f"\n{'═' * 51}")
        print("  SUMMARY")
        print(f"{'═' * 51}")
        print(f"  Scope:         {args.scope_dir}/ ({len(scope_files)} files)")
        print(f"  Result:        {'converged' if converged else 'not converged'}")
        print(f"  Results:       {'(not written in dry-run)' if args.dry_run else results_file}")
        print(f"  Logs:          {log_dir}/")
        print(f"{'═' * 51}")
    else:
        converged_count = 0
        failed_count = 0

        for i, doc in enumerate(docs, 1):
            print(f"\n[{i}/{total}] {doc}")
            if converge_doc(doc, scope_files, args, prompt_template, repo_root, log_dir, results_file):
                converged_count += 1
            else:
                failed_count += 1

        print(f"\n{'═' * 51}")
        print("  SUMMARY")
        print(f"{'═' * 51}")
        print(f"  Total docs:    {total}")
        print(f"  Converged:     {converged_count}")
        print(f"  Not converged: {failed_count}")
        print(f"  Results:       {'(not written in dry-run)' if args.dry_run else results_file}")
        print(f"  Logs:          {log_dir}/")
        print(f"{'═' * 51}")


if __name__ == "__main__":
    main()
