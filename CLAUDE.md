# CLAUDE.md

Instructions for AI agents working on this codebase.

## Project overview

This is a Cargo workspace containing CLI tools with shared TUI infrastructure:

- **cdt** (`crates/cdt`) — Fast terminal companion for Conductor workspaces. Fuzzy-find TUI and CLI for navigating, inspecting, and cleaning up multi-agent worktrees.
- **learn** (`crates/learn`) — Agent-assisted concept learning CLI for Obsidian vaults. Spaced-repetition scheduling, review session generation/grading, and frontmatter management.
- **rippy** (`crates/rippy`) — macOS clipboard history manager with vim-style TUI, SQLite storage, launchd service, and global hotkey.
- **tui-core** (`crates/tui-core`) — Shared vim-modal TUI primitives (key handling, navigation, fuzzy filtering, search bar rendering).

See each crate's `ROADMAP.md` for open work items.

## Build and test

```bash
cargo build --workspace       # compile all crates
cargo test --workspace        # run all tests (must pass before committing)
cargo test -p cdt             # test a specific crate
cargo test -p learn
cargo test -p rippy
cargo test -p tui-core
cargo run -p cdt              # launch cdt TUI
cargo run -p cdt -- ls        # list workspaces
cargo run -p cdt -- ls --pr   # list with PR/CI status (queries GitHub)
cargo run -p learn -- init    # scaffold vault structure
cargo run -p learn -- status  # show due concepts
cargo run -p rippy            # launch rippy TUI
cargo run -p rippy -- list    # list clipboard entries
```

## Architecture

### tui-core (`crates/tui-core/src/lib.rs`)
Shared vim-modal TUI primitives consumed by both cdt and rippy:
- `Mode` enum (Normal/Insert) and `NavAction` enum (shared navigation actions)
- `handle_key` — key dispatch returning `Option<NavAction>`, `None` for app-specific keys (Enter, custom combos)
- `apply_navigation` — pure function computing new selection index
- `compute_filtered` — generic fuzzy filtering over any item type
- `adjust_scroll` — scroll offset calculation
- `render_search_bar` — parameterized search bar widget

### learn (`crates/learn`)
- `src/main.rs` — CLI (clap), subcommand handlers (init, status, concept new/refine, review generate/grade)
- `src/types.rs` — core data types (`Concept`, `ReviewItem`, `Grade`, `VaultConfig`)
- `src/schedule.rs` — spaced repetition algorithm (`next_interval_days`, `update_mastery`)
- `src/config.rs` — vault path resolution (flag → env → config file), vault config loading
- `src/select.rs` — query due concepts with domain/count filtering, sorted by mastery
- `src/parse/concept.rs` — YAML frontmatter parsing, wikilink extraction
- `src/parse/review.rs` — review session parsing (answers, graded items, concept path resolution)
- `src/write/frontmatter.rs` — atomic system-field updates (underscore-prefixed only)
- `src/write/review_session.rs` — review file rendering, atomic writes, grade filling

Tests colocated in `#[cfg(test)]` modules plus integration tests in `tests/`.

### cdt (`crates/cdt`)
- `src/main.rs` — CLI (clap), subcommand handlers, output formatting, cache/timing orchestration
- `src/scanner.rs` — workspace discovery, parallel git inspection (rayon), PR enrichment via `gh`
- `src/tui.rs` — TUI using tui-core for navigation, app-specific rendering
- `src/cache.rs` — disk cache (`~/.cache/cdt/workspaces.json`) with TTL and structural invalidation
- `src/lib.rs` — module re-exports

Tests in `tests/` as integration tests.

### rippy (`crates/rippy`)
- `src/main.rs` — CLI (clap), subcommand handlers, output formatting
- `src/tui.rs` — TUI using tui-core for navigation, app-specific rendering (dd delete, copy-on-enter)
- `src/db.rs` — SQLite store (`ClipEntry`, CRUD operations)
- `src/clipboard.rs` — macOS NSPasteboard FFI
- `src/hotkey.rs` — global hotkey via CGEventTap
- `src/config.rs` — config.toml (hotkey, terminal settings)
- `src/terminal.rs` — terminal app detection/launching
- `src/watcher.rs` — background clipboard polling
- `src/mcp.rs` — MCP server for clipboard access

Tests colocated in `#[cfg(test)]` modules.

## Setup

```bash
git config core.hooksPath .githooks   # enable pre-commit and pre-push hooks
```

This is required for all contributors (human or agent). The hooks run `cargo fmt --check` on commit and `cargo test + clippy` on push.

## Quick start keyword

If the user's message is just **`work`** (optionally followed by a crate name or issue number), follow the full agent workflow below end-to-end:

- `work` — find the next small unblocked item and complete it through PR creation
- `work rippy` — scope the search to that crate's issues and roadmap
- `work #14` — work on that specific GitHub issue

Pick the smallest unblocked item, implement it fully (including tests), and open a PR. Then stop.

## Agent workflow

All code in this repo is written by agents. Follow this process exactly:

1. **Find work.** Check open GitHub issues first (`gh issue list`). If none match, check `ROADMAP.md` files in each crate for unchecked items. Do not invent work that isn't tracked.
2. **Scope check.** If the issue or roadmap item is large (touches 3+ files, adds a new subcommand, or changes a public API), open a GitHub issue first and wait for approval. Small items (bug fix, single feature, test addition) can proceed directly.
3. **Read before writing.** Read the files you plan to modify. Understand the existing patterns. Do not refactor surrounding code.
4. **Branch** off `main`: `git checkout -b <owner>/<short-name>` (e.g. `brianchuang/pin-entries`)
5. **Implement** the change. Follow existing code style. No unnecessary abstractions.
6. **Write tests.** Every PR must include tests. See "Testing conventions" below.
7. **Validate locally:**
   ```bash
   cargo fmt                                  # fix formatting
   cargo test --workspace                     # all tests pass
   cargo clippy --workspace -- -D warnings    # no warnings
   ```
8. **Commit** with a concise message: what changed and why (1-2 sentences).
9. **Push and open a PR** against `main`. The PR template will guide you — fill in every section. Pre-push hooks enforce all checks before code reaches the remote.
10. **Wait for review.** Do not merge your own PR.

### What agents must NOT do

- Push directly to `main` — always use a PR
- Merge their own PRs — a human (or designated reviewer agent) merges
- Skip tests or add `#[ignore]` to make things pass
- Modify files outside the scope of the issue they're working on
- Add dependencies without justification in the PR description

## Contribution workflow (manual)

1. **Pick an item** from a crate's `ROADMAP.md` (open an issue first for large features)
2. **Read the relevant source** before writing code — understand the existing patterns
3. **Branch** off `main`: `git checkout -b feat/<short-name>` or `fix/<short-name>`
4. **Implement** the change, following existing code style (no unnecessary abstractions)
5. **Write tests** — every PR should include tests
6. **Verify**: `cargo test --workspace` passes, `cargo build --workspace` has no new warnings
7. **Commit** with a concise message: what changed and why (1-2 sentences)
8. **Push** and open a PR against `main` with a summary, example usage, and test plan

## Design philosophy

This codebase is **functional and composition-first**. Prefer pure functions that take inputs and return values over stateful methods and side effects. Build complex behavior by composing small, well-typed functions.

Concretely:
- Functions that transform data should be pure: `fn(&[T]) -> Vec<U>`, not methods that write to stdout
- Push side effects (I/O, git subprocess calls, clipboard access, DB writes) to the edges; keep core logic testable without mocks
- Compose at the call site. If two functions can be piped together, that's better than a new abstraction
- Reach for `impl Trait` or generics only when you have two or more concrete callers — not speculatively

These are CLI tools, not frameworks. The right unit of reuse is a function, not a type hierarchy.

## Code style

- No unnecessary abstractions — three similar lines > a premature helper
- `Workspace` (cdt) and `ClipEntry` (rippy) are the core data types; keep their contracts stable
- Shared TUI logic goes in `tui-core`; app-specific rendering stays in each crate
- DB tests use `Store::open(Path::new(":memory:"))` — fast, no disk I/O
- Scanner tests use `tempfile::TempDir` with fake directory trees

## Testing conventions

- Happy path + edge cases for all new functionality
- No mocks — use real in-memory databases and fake directory trees
- TUI tests construct `App` directly and assert on state after `apply_action` calls
- Run `cargo test --workspace` before committing
