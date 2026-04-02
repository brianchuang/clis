# rippy

A fast clipboard history manager for macOS with a vim-style TUI.

## Install

```
cargo install --path .
```

## Quick start

```bash
# Launch the TUI (interactive picker)
rippy

# Install as a background service (monitors clipboard 24/7)
rippy install

# Uninstall the background service
rippy uninstall
```

## TUI keybindings

rippy uses vim-style modal keybindings. It starts in **Normal mode**.

### Normal mode

| Key | Action |
|---|---|
| `j` / `k` / arrows | Move down / up |
| `G` | Jump to bottom |
| `gg` | Jump to top |
| `Ctrl+d` / `Ctrl+u` | Half-page down / up |
| `Enter` | Copy selected entry to clipboard and quit |
| `dd` | Delete selected entry |
| `/` or `i` | Enter Insert mode (search) |
| `q` or `Esc` | Quit |
| `Ctrl+C` | Quit (works in any mode) |

### Insert mode (search)

| Key | Action |
|---|---|
| Any character | Fuzzy-filter entries |
| `Backspace` | Delete last character |
| `Ctrl+u` | Clear entire search |
| `Enter` | Copy selected entry to clipboard and quit |
| Arrows | Navigate while searching |
| `Esc` | Return to Normal mode |

The title bar shows `[NORMAL]` or `[INSERT]` and the border color changes (cyan / green) so you always know which mode you're in.

## CLI commands

```bash
rippy list [-c COUNT]       # List recent entries (default: 20)
rippy search QUERY [-c N]   # Fuzzy search history
rippy copy ID               # Copy entry by ID back to clipboard
rippy clear                 # Delete all history
```

## Global hotkey

When the background service is running, a global hotkey opens rippy in your terminal.

```bash
rippy hotkey show                              # Show current config
rippy hotkey set --key v --modifiers cmd,shift  # Set hotkey (default: Cmd+Shift+V)
rippy hotkey set --terminal iTerm2             # Set preferred terminal
rippy hotkey test                              # Test the listener
```

Supported terminals: Terminal, iTerm2, Alacritty, WezTerm (or `auto` to detect).

The hotkey requires **Accessibility permission**:
System Settings > Privacy & Security > Accessibility

## Data

All data is stored in `~/.local/share/rippy/`:

| File | Purpose |
|---|---|
| `history.db` | SQLite clipboard history |
| `config.toml` | Hotkey and terminal settings |

## Requirements

- macOS (uses native `NSPasteboard` and `CGEventTap` APIs)
- Rust 1.70+
