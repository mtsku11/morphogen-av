# Morphogen AV — Agent Notes

The canonical agent guide for this repo is **[CLAUDE.md](CLAUDE.md)**. It holds
the project purpose, the non-negotiable invariants (determinism-first, CPU
reference as ground truth, Metal parity gating, reusable analysis sidecars,
external FFmpeg, no `unwrap` in libs), the build/test commands, the workflow, and
the context-loading order. This pointer exists so any agent lands in the same
place; everything that used to live here now lives in CLAUDE.md and the docs it
links, to keep one source of truth.

- Commands + key-path map: [docs/REFERENCE.md](docs/REFERENCE.md)
- Architecture: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- Backlog (done + next): [docs/BACKLOG.md](docs/BACKLOG.md)
- Effect roadmap: [docs/EFFECTS_ROADMAP.md](docs/EFFECTS_ROADMAP.md)
- Current status / where to resume: [STATUS.md](STATUS.md)
