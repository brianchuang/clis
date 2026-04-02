# cdt

A fast, terminal-native companion for [Conductor](https://www.conductor.build/) workspaces. Fuzzy-find, inspect, and manage your multi-agent worktrees without leaving the terminal.

Think `kubectl` to Conductor's Dashboard.

## Install

```
cargo install --path .
```

## Quick start

```bash
# Launch the TUI (fuzzy picker → cd into workspace)
cdt

# List all workspaces with merge status
cdt ls

# Shell integration (add to .zshrc / .bashrc)
eval "$(cdt init-shell)"
```

## TUI keybindings

Starts in **Insert mode** for immediate filtering.

### Insert mode (search)

| Key | Action |
|---|---|
| Any character | Fuzzy-filter workspaces |
| `Backspace` | Delete last character |
| `Ctrl+u` | Clear entire search |
| `Enter` | Select workspace and quit |
| Arrows | Navigate while searching |
| `Esc` | Return to Normal mode |

### Normal mode

| Key | Action |
|---|---|
| `j` / `k` / arrows | Move down / up |
| `G` | Jump to bottom |
| `gg` | Jump to top |
| `Ctrl+d` / `Ctrl+u` | Half-page down / up |
| `Enter` | Select workspace and quit |
| `/` or `i` | Enter Insert mode (search) |
| `q` or `Esc` | Quit |
| `Ctrl+C` | Quit (works in any mode) |

## CLI commands

```bash
cdt ls              # List workspaces with merge status
cdt init-shell      # Print shell function for cd integration
```

### `cdt ls` output

```
✓ merged  my-app           memphis          ~/conductor/workspaces/my-app/memphis
● open    my-app           london           ~/conductor/workspaces/my-app/london
  —       my-app           tokyo            ~/conductor/workspaces/my-app/tokyo
```

## Configuration

| Option | Default | Description |
|---|---|---|
| `-r` / `--root` | `~/conductor/workspaces` | Workspace root directory |
| `CDT_ROOT` env var | `~/conductor/workspaces` | Same, via environment |

## Requirements

- macOS / Linux
- Rust 1.70+
- [Conductor](https://www.conductor.build/) (or any `<root>/<project>/<workspace>` directory layout)
