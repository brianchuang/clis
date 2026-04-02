# Contributing

Thanks for your interest in contributing! Here's how to get started.

## Finding work

Check the `ROADMAP.md` in each crate directory (`crates/cdt/ROADMAP.md`, `crates/rippy/ROADMAP.md`) for open items. Pick any unchecked task and open a PR. For larger features, open an issue first so we can discuss the approach.

## Development setup

```bash
# Clone and build
git clone https://github.com/brianchuang/clis.git
cd clis
cargo build --workspace

# Run tests
cargo test --workspace

# Run individual tools
cargo run -p cdt             # launch cdt TUI
cargo run -p cdt -- ls       # cdt CLI
cargo run -p rippy           # launch rippy TUI
cargo run -p rippy -- list   # rippy CLI
```

Requires Rust 1.70+. Rippy requires macOS.

## Workspace structure

This is a Cargo workspace with three crates:

- `crates/tui-core` — shared vim-modal TUI primitives (key handling, navigation, fuzzy filtering)
- `crates/cdt` — Conductor workspace navigator
- `crates/rippy` — clipboard history manager

When adding shared TUI functionality, add it to `tui-core`. When adding app-specific features, work in the relevant crate.

## Workflow

1. Fork the repo and create a branch from `main`:
   - `feat/<name>` for new features
   - `fix/<name>` for bug fixes
2. Make your changes, keeping the scope focused on one thing
3. Write tests (see below)
4. Make sure `cargo test --workspace` passes and `cargo build --workspace` produces no new warnings
5. Commit with a clear, concise message explaining what and why
6. Open a PR against `main` with:
   - A short summary of what changed
   - Example usage (if adding CLI features)
   - Test plan

## Testing

Every PR should include tests.

- **tui-core tests**: key handling, navigation, fuzzy filtering — in `#[cfg(test)]` module
- **cdt tests**: integration tests in `tests/` — scanner (tempfile), TUI state, cache
- **rippy tests**: colocated `#[cfg(test)]` modules — DB (in-memory SQLite), formatting

Run the full suite with `cargo test --workspace`, or test a specific crate with `cargo test -p <crate>`.

## Code guidelines

- Follow the existing patterns in the codebase
- Keep it simple — avoid unnecessary abstractions or speculative features
- Don't add features beyond what the PR is scoped to
- Prefer pure functions over stateful methods — push I/O to the edges
- Keep serialization contracts stable (`Workspace`, `ClipEntry`, `--json` output)
