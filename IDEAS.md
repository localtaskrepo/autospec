# autospec — Improvement Ideas

Collected during initial development and testing.

## Context Caching (Phase 2)

The agent re-reads the same canonical docs every iteration even though they don't change within a single doc's convergence run. Pre-bundle static context to reduce token cost and iteration time.

- Pre-concatenate static docs (AGENTS.md, docs/README.md, docs.instructions.md, canonical contracts) into `autospec/context.md` before the loop starts
- Rebuild the bundle between docs (not between iterations of the same doc)
- Add `--bundle-context` flag to the script
- Estimated saving: ~30-60 seconds and 500-800 lines of input tokens per iteration

## Clean/Cached Context Verification

Cached context risks false convergence — the agent says "no changes" because it recognizes the context, not because the doc is actually stable.

State machine:
```
CACHED no-changes → trigger CLEAN verify pass
CLEAN no-changes  → truly CONVERGED
CLEAN changes     → back to CACHED loop
```

Config knobs:
```bash
CACHE_MODE=auto          # auto | always | never
CLEAN_VERIFY=1           # require clean-context pass before declaring convergence
CACHE_FLUSH_EVERY=5      # force clean context every N iterations
```

`never` = current behavior (always fresh, safest). `always` = max speed. `auto` = cached + clean verification + periodic flush.

## Run Order Strategy

Canonical docs should converge first since downstream docs reference them. Current recommendation:

1. Canonical contracts: entity-dictionary, permissions-matrix, lifecycle-rules, matching-privacy-matrix, validation-contract
2. Platform specs: architecture, product, philosophy, network-communication, database, etc.
3. Leaf docs: ui/, design/, use-cases/

Consider encoding this as a run-order file or `--phase` flag.

## Growth Guard

Detect unbounded doc growth and warn or stop. Observed in testing: `feature-flags.md` grew from 300→706 lines.

- Track doc size (lines) at start and after each iteration
- Warn if doc exceeds 1.5× original size
- Halt with `growth-exceeded` status if doc exceeds 2× original size
- `--max-growth <factor>` flag (default: 2.0)

This still matters for broad sweeps, even though the prompt tuning reduced the worst blow-ups substantially.

## Output / Reporting

- Generate a summary report after all docs finish (markdown table: doc, iterations, status, size delta) — **basic version done** (printed to stdout + `results.tsv`)
- Track doc size (lines) over iterations to detect unbounded growth — **not yet implemented** (see Growth Guard above)

## Parallelization

Independent docs (same layer, no cross-references) could run in parallel. Would need:
- Multiple codex exec processes
- Separate git worktrees or staggered commits
- Probably not worth the complexity until single-threaded runs are well-understood

## Model Selection

- Different models for different phases: cheap model (gpt-4.1-mini) for early big-change iterations, expensive model (gpt-5.4) for final convergence verification
- `--model-verify` flag for a different model on the clean verification pass
- Cost tracking per doc per iteration in results.tsv

## Publishing as Open Source

If autospec proves useful, extract it into a standalone repo.

### What needs work before publishing
- **Example project** — small self-contained doc set people can run against immediately
- **License** — MIT (matching autoresearch)
- **Results visualization** — simple script or notebook to plot iterations-to-convergence per doc
- **Standalone repo cleanup** — trim remaining repo-specific examples, copy only the files that belong in the extracted project, and make the default prompt less tied to one docs layout
