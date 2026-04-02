# clis

A collection of fast, terminal-native CLI tools with shared vim-modal TUI infrastructure.

## Tools

### [cdt](crates/cdt/) — Conductor Workspace Navigator

Fast fuzzy-find TUI and CLI for navigating, inspecting, and cleaning up multi-agent worktrees managed by [Conductor](https://conductor.build).

```bash
cargo run -p cdt              # interactive TUI
cargo run -p cdt -- ls        # list workspaces
cargo run -p cdt -- ls --pr   # include PR/CI status
```

### [rippy](crates/rippy/) — Clipboard History Manager

macOS clipboard history manager with vim-style TUI, SQLite storage, launchd background service, and global hotkey.

```bash
cargo run -p rippy            # interactive TUI
cargo run -p rippy -- list    # list recent entries
cargo run -p rippy -- search "query"
```

## Shared Infrastructure

**[tui-core](crates/tui-core/)** provides shared vim-modal TUI primitives used by both tools:
- Vim-style Normal/Insert mode key handling with multi-key combos (`gg`, `dd`)
- Fuzzy filtering (skim algorithm)
- Navigation (j/k, G/gg, Ctrl+d/u, half-page)
- Search bar rendering

## Getting Started

```bash
cargo build --workspace       # build everything
cargo test --workspace        # run all tests
cargo install --path crates/cdt    # install cdt
cargo install --path crates/rippy  # install rippy
```

Requires Rust 1.70+. Rippy requires macOS.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and workflow.

## Previously

These tools were originally developed as separate repositories:
- [conductor-trees](https://github.com/brianchuang/conductor-trees)
- [rippy](https://github.com/brianchuang/rippy)

## License

[MIT](LICENSE)
