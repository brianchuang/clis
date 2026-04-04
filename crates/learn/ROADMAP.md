# Learn Roadmap

Learn is a spaced-repetition CLI for Obsidian vaults. It manages concept notes with
system-owned frontmatter, generates review sessions, and grades answers — designed
to work with AI agents (via Claude Code commands) or standalone.

---

## v0.1 — Core loop (current)

The basic review cycle: create concepts, generate sessions, grade answers.

- [x] `learn init` — scaffold vault structure (Concepts/, Reviews/, Templates/, .learning-system/)
- [x] `learn status` — show due count and mastery breakdown by domain
- [x] `learn concept new <name>` — create blank concept note from template
- [x] `learn concept refine` — list unclassified notes for AI-powered suggestions
- [x] `learn review generate` — generate daily review session with random prompt types
- [x] `learn review grade` — parse graded items and update concept frontmatter
- [x] Spaced repetition scheduling (score-based interval scaling + EMA mastery)
- [x] Atomic file writes (tmp + rename) for all mutations
- [x] System field guard — only writes `_`-prefixed frontmatter, never user-owned fields
- [x] Config resolution chain: `--vault` flag → `LEARN_VAULT` env → `~/.config/learn/config.json`

## v0.2 — Agent integration

Make the CLI a first-class tool for Claude Code agents.

- [x] `.claude/commands/` templates for review-generate, review-grade, concept-refine
- [ ] `learn review grade --auto` — agent grades answers using rubric from CLAUDE.md
- [ ] `learn concept refine --apply` — write AI suggestions to frontmatter with confirmation
- [ ] MCP server for vault access (list concepts, query due, read note body)

## v0.3 — Better review experience

- [ ] TUI for review sessions (vim-style, using tui-core)
- [ ] Interactive concept picker with fuzzy search
- [ ] Review history viewer — past sessions with score trends
- [ ] Mastery dashboard — sparkline graphs per domain

## v0.4 — Vault intelligence

- [ ] Duplicate detection across concept notes
- [ ] Wikilink graph — find orphaned concepts or missing connections
- [ ] Domain auto-classification from note content
- [ ] Bulk import — scan a directory of markdown files and initialize system fields

---

## Out of scope

- Note editing / content authoring — that's Obsidian's job
- Sync / multi-device — vault is a local directory, sync via git or Obsidian Sync
- Flashcard UI — this is a CLI tool, not Anki
