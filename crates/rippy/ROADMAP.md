# Rippy Roadmap

Rippy is a fast clipboard history manager for macOS with a vim-style TUI.
This roadmap captures the vision for where rippy is headed.

**Suggested alias:** `alias yy="rippy"` — vim yank meets clipboard manager.

---

## v0.1 — Polish

Ship what exists with better ergonomics.

- [ ] Suggest `yy` alias in install output
- [ ] `--json` flag on `list` and `search` for scriptability
- [ ] Auto-expire entries from password managers (detect short-lived clipboard copies that disappear within ~10s)
- [ ] Configurable history retention with TTL (`default_ttl`, `max_entries` in config.toml)

## v0.2 — Better TUI

Make the picker more powerful without losing simplicity.

- [ ] Preview pane — show full content of selected entry (scrollable)
- [ ] Syntax highlighting for code snippets in preview
- [ ] Entry pinning — star entries so they never expire or get pruned
- [ ] Auto-detected content tags (url, code, path, text) shown in list view

## v0.3 — Power User

Unix philosophy: composable, scriptable, searchable.

- [ ] `yy get ID` — print entry to stdout for piping
- [ ] `echo "foo" | yy save` — capture stdin as a clipboard entry
- [ ] Semantic search via local embeddings (see below)
- [ ] Multi-select — pick several entries and paste as a batch
- [ ] Snippets mode — persistent saved clips separate from ephemeral history

### Semantic Search Design

Fuzzy matching (skim) works when you remember the exact words. Semantic search
works when you don't — "that API endpoint" or "cloud deploy config" should find
relevant entries by meaning, not string overlap.

**Constraints:**
- **Fully local.** No API calls, no data leaving the machine. This is clipboard data.
- **Fast ingest.** Embed on write in the watcher thread (~5ms per entry is fine).
- **Small model.** all-MiniLM-L6-v2 (~22MB ONNX) or similar. Bundle or download on first run.
- **No vector DB.** Store embeddings as blobs in SQLite. Brute-force cosine similarity over 10k entries is <10ms — no need for extra infrastructure.

**UX:**
```bash
yy search "kubernetes config"             # fuzzy (default, current behavior)
yy search --semantic "cloud deploy stuff"  # meaning-based
```

In the TUI, a keybinding to toggle between fuzzy and semantic mode.

**Implementation sketch:**
1. Add `embedding BLOB` column to the entries table.
2. On ingest, run content through ONNX runtime with the bundled model.
3. On semantic query, embed the query string, compute cosine similarity against stored embeddings, return top-k.
4. ONNX runtime adds ~5MB to the binary. Model downloaded to `~/.local/share/rippy/models/` on first semantic search.

## v0.4 — Quality of Life

- [ ] Image clipboard support (store as files, show thumbnails in TUI)
- [ ] Homebrew formula (`brew install rippy`)
- [ ] Raycast / Alfred extension that queries the SQLite DB directly
- [ ] Cross-machine sync (SQLite DB over iCloud / Dropbox)

---

## Out of Scope

**Agent / MCP integration.** Clipboard history is inherently sensitive (passwords,
tokens, 2FA codes, personal messages). Exposing it to LLM-backed agents would
require a full security layer (content classification, encryption at rest, tiered
access control, audit logging) that is a separate product, not a feature of a
clipboard manager. Rippy stays local-only. The `--json` CLI output provides
sufficient scriptability for local automation without the security risk.

---

## Contributing

Pick any unchecked item above and open a PR. If you want to tackle semantic search
or another large feature, open an issue first to discuss the approach.
