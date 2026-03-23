# autospec

autospec started as a practical experiment inside a project that leaned hard into writing specs before writing code.

We deliberately built up a large set of product, architecture, UI, and design docs before implementation because we wanted coding agents to have a solid documentation base to work from. That helped, but it also exposed a new bottleneck: the docs themselves were often close to usable without being quite precise enough, consistent enough, or explicit enough to drive implementation without follow-up questions.

The idea behind autospec was simple: if we can run an agent in a loop against a spec, compare the text before and after each pass, and stop when the changes flatten out, then we can try to make the specification itself converge before we ask another agent to write code from it.

This project is the result of repeatedly trying that idea on a real repository, watching it fail, and then tightening both the code and the prompt until the results became useful.

## Who this is for

autospec is for teams that already keep meaningful project knowledge in markdown.

It is most useful if you have:

- product specs that drive engineering work
- UI or API docs that need to stay consistent with canonical docs
- a docs-heavy repo where coding agents read markdown before writing code
- a willingness to tune prompts and workflow for your own domain

It is probably not a good fit if your project has no written specs at all, or if you want a one-shot generator that invents an entire product definition from nothing.

## What it does

autospec runs an AI coding agent in a convergence loop.

For each iteration it:

1. builds a prompt around the current target doc or scope
2. lets the agent edit the docs directly
3. snapshots the resulting text
4. compares the new text to the previous iteration
5. stops when the doc stabilizes, oscillates, times out, or hits the iteration limit

It supports three working modes:

- `strict`: only the target doc is in scope
- `ripple`: focus on one doc, but allow limited cross-doc fixes in the surrounding folder
- `sweep`: review every doc in a folder and edit the smallest set needed to resolve real issues

## Current capabilities

The current implementation already covers a fair amount of the workflow:

- convergence detection from text snapshots rather than prompt-string matching
- repeated-state detection to stop obvious oscillation
- `strict`, `ripple`, and `sweep` execution modes
- multi-agent execution via Copilot, Codex, Claude, or a custom command
- optional goal injection for targeted runs
- optional reasoning-effort passthrough for agents that support it
- configurable per-iteration timeout, including disabled timeout for long runs
- optional scope-size cap for users who want a safety rail on broad runs
- no-commit mode for experimentation without git writes
- dirty-doc guardrails with explicit opt-out
- per-iteration logs and tabular results tracking
- sparse-seed warnings for repos that are too thin to produce project-specific output reliably

## Current status

autospec is still an experiment.

It works well enough to be useful, but it is not a finished product.

What works well today:

- single-doc improvement runs
- bounded multi-file runs such as a UI folder or design folder
- repo-wide `ripple` runs from a canonical doc

What still needs care:

- full-repo `sweep` runs across a large docs tree
- very sparse or nearly empty seed docs
- prompt tuning for repositories that use different document structure or terminology than the project that shaped the initial prompt

## What we learned by dogfooding it

autospec improved because we used it against a real project's docs and then treated each failure as a product bug.

Some of the major changes came directly from those experiments:

- convergence detection moved from prompt-string matching to text snapshots
- repeated-state detection was added to catch obvious oscillation
- iteration-aware prompting was added so later passes stop polishing and start saying `CONVERGED`
- broad sweeps were tightened to prefer canonical docs first and shrink the touched-file set over time
- timeout handling became configurable instead of hard-coded
- large-scope runs now expose enough logging to see which files actually moved and by how much

That matters because the tool is not just a script. The prompt and the loop design are part of the product.

## What the experiments looked like

The early version did not converge reliably.

Initial live runs on a real docs-heavy project produced patterns like:

- repeated 10 to 30 line churn on already-good docs
- cross-doc edits that were broader than intended
- oscillation between competing wordings
- time spent on style changes after the real issues were already fixed

After tightening the prompt and loop logic, the behavior changed materially:

- small docs converged in 2 to 4 iterations
- medium docs that previously failed began converging consistently
- `docs/ui` folder sweeps became practical
- repo-wide `ripple` runs became reliable
- full `sweep docs/` runs became far more disciplined, though still not the strongest mode

This is the main reason the README emphasizes experimentation rather than claiming the tool is universally solved.

## Bootstrapping and sparse starting points

autospec improves existing docs. It is not currently a true scaffold generator.

Here is the current behavior:

- missing target file: hard error
- empty docs directory: hard error
- one small seed doc: works, but the output is usually generic
- one almost-empty seed doc: usually converges to a conservative statement that requirements are unspecified

In practice, autospec needs at least one seed markdown doc with a little real content.

If you want a useful first pass, give it some anchor points such as:

- the core nouns in the system
- one or two important workflows
- explicit constraints or out-of-scope lines
- a canonical README that explains what the project is

Without that, the safest thing for the agent to do is stay generic or refuse to invent specifics.

## Requirements

- Python 3
- an AI coding agent CLI installed locally
- markdown docs in the target repository

Tested agent CLIs:

- GitHub Copilot CLI
- OpenAI Codex CLI
- Claude Code CLI
- arbitrary custom commands via `--agent-cmd`

Git is strongly recommended, but not strictly required if you run with `--no-commit --allow-dirty`.

## Quick start

Single doc:

```bash
python autospec/run.py docs/product.md
```

Single doc, no commits:

```bash
python autospec/run.py --no-commit docs/product.md
```

Folder sweep:

```bash
python autospec/run.py --scope sweep docs/ui
```

Repo-wide ripple from a canonical doc:

```bash
python autospec/run.py --scope ripple docs/entity-dictionary.md
```

Disable per-iteration timeout for large runs:

```bash
python autospec/run.py --scope sweep --agent-timeout 0 docs/ui
```

## Important flags

- `--scope strict|ripple|sweep`
- `--goal "..."`
- `--max-iters N`
- `--threshold N`
- `--stable-iters N`
- `--agent copilot|codex|claude|custom`
- `--agent-timeout N` with `0` meaning disabled
- `--no-commit`
- `--no-branch`
- `--allow-dirty`
- `--max-scope-files N`
- `--dry-run`

## How to tune it for your own project

The default prompt is biased toward the repository that created it. That is not a bug. It is a reminder that you will probably get better results if you adapt the prompt to your own document structure and quality bar.

The highest leverage tuning points are:

1. `autospec/prompt.md`
   Add the language your project actually uses. Name the canonical docs that own cross-cutting rules. Make the agent's idea of a good spec match your repo.

2. Scope choice
   Start with `strict`. Move to `ripple` when you trust the canonical docs. Use large `sweep` runs deliberately rather than by default.

3. Seed quality
   If the starting docs are vague, autospec can only sharpen them so far. Add a few concrete nouns, states, or constraints first.

4. Iteration budget
   Small docs often converge in 2 to 4 iterations. Large section sweeps may need more time or a disabled timeout.

5. Canonical-first structure
   If your repo has authoritative docs, make them obvious. autospec performs better when it can distinguish source-of-truth docs from downstream docs.

## Recommended workflow

1. Start on one canonical doc in `strict` mode
2. Move to `ripple` mode once the canonical doc is stable
3. Use `sweep` on bounded folders such as `docs/ui` or `docs/design`
4. Treat full-repo `sweep` as a deep cleanup run, not the default mode
5. Review the logs and results before trusting the output blindly

## Output and logs

autospec writes:

- per-iteration logs in `autospec/logs/`
- run summaries in `autospec/results.tsv`

These are worth reading. The change pattern across iterations usually tells you more than the final status line.

Examples:

- `+37/-26 -> +22/-20 -> +18/-14 -> +2/-2` usually means the run is healthy and narrowing
- a sudden large jump late in a broad sweep usually means the agent found a new consistency layer rather than simply polishing text

## Limits

- It does not create a missing target doc from scratch
- It does not scaffold a project from an empty docs directory
- It can converge on output that is technically safe but too generic to be useful if the seed docs are too sparse
- Very large full-repo sweeps are still the weakest mode

## Why publish it at all

Because this pattern seems broadly useful.

Many teams are spending time on prompts, evals, and agent workflows while ignoring the quality of the docs those agents consume. autospec is an attempt to treat the spec itself as something that can be iteratively tightened and measured.

It may not end up as a standalone tool in its current form. But the experiment has already been useful, and this README is meant to be a solid starting point if and when the code moves into its own repository.