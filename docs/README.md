# autospec Rust Rewrite Docs

This folder is the planning corpus for the Rust rewrite of autospec.

These docs are meant to serve two purposes:

1. define the v1 behavior of the Rust binary precisely enough to implement against
2. provide a seed doc set that the legacy autospec loop can tighten further

Recommended read order:

1. [rust-v1-spec.md](rust-v1-spec.md)
2. [rust-architecture.md](rust-architecture.md)
3. [rust-roadmap.md](rust-roadmap.md)
4. [rust-implementation-setup.md](rust-implementation-setup.md)

Recommended refinement workflow with the legacy implementation:

```bash
python legacy/run.py --scope strict docs/rust-v1-spec.md
python legacy/run.py --scope strict docs/rust-architecture.md
python legacy/run.py --scope strict docs/rust-roadmap.md
python legacy/run.py --scope strict docs/rust-implementation-setup.md
python legacy/run.py --scope ripple docs/rust-v1-spec.md
```

Use `strict` first so the seed docs stabilize independently. Only use `ripple` once the core terms and constraints are stable.
