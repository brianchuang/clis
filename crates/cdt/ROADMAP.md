# cdt roadmap

## Philosophy

Conductor handles orchestration: creating worktrees, launching agents, assigning tasks, managing PRs. **cdt does not duplicate that.** cdt is the fast, terminal-native layer for navigating, inspecting, and cleaning up what Conductor creates. It owns the git-level intelligence that a GUI doesn't surface.

## Current (v0.1)

- [x] Fuzzy-find TUI with vim keybindings
- [x] `cdt ls` with merge status (merged / open / unknown)
- [x] Shell integration (`cdt init-shell`)
- [x] Configurable workspace root (`--root` / `CDT_ROOT`)

---

## v0.2 — Richer `cdt ls`

Make the listing actually actionable instead of just navigable.

- [x] **Branch name column** — already in the `Workspace` struct, just not displayed
- [x] **Last commit age** — `3h ago` vs `12d ago` instant staleness signal
- [x] **Dirty working tree indicator** — uncommitted changes flag (important before cleanup)
- [x] **`cdt ls --pr`** — PR status per worktree via `gh pr list`
  ```
  ✓ merged               my-app  memphis   feat-auth
  ● open   PR #42 ✓ci    my-app  london    feat-login
  ● open   PR #38 ✗ci    my-app  tokyo     refactor-db
  ● open   no PR         my-app  berlin    fix-typo
  ```

## v0.3 — `cdt clean`

Daily hygiene. Conductor creates worktrees but doesn't clean them up. They pile up fast when you're spinning up 5-10 agents a day.

- [x] `cdt clean` — interactive: select merged worktrees to remove
- [x] `cdt clean --merged` — auto-remove all merged worktrees
- [x] `cdt clean --stale 7d` — remove worktrees with no commits in N days
- [x] `cdt clean --dry-run` — preview what would be removed
- [x] Handles both `git worktree remove` and directory cleanup

## v0.4 — Quick actions

Do things to worktrees without cd-ing into them.

- [x] **`cdt diff <workspace>`** — `git diff main...<branch>` for any worktree from anywhere. "What did this agent actually change?"
- [x] **`cdt open <workspace>`** — open the PR in browser (`gh pr view --web`) or the workspace in your editor
- [x] **`cdt summary`** — one-liner rollup: "4 open, 3 merged, 2 stale, 1 failing CI"

## v0.5 — Cross-worktree intelligence

The unique value cdt adds: understanding the git layer across your whole fleet.

- [x] **`cdt conflicts`** — diff worktrees against each other to detect overlapping file changes *before* merge time. Conductor creates PRs but doesn't warn you that two agents both modified `auth.rs`
- [x] **`cdt timeline`** — git-log-style chronological view across ALL worktrees
  ```
  14:02  my-app/london   committed "Add JWT refresh token logic"
  13:58  my-app/tokyo    PR #38 CI failed
  13:45  my-app/berlin   created worktree
  13:30  my-app/london   PR #42 opened
  ```
  The "what happened while I was at lunch?" command.

## v0.6 — TUI preview pane

Inspect workspaces without leaving the TUI.

- [x] **Diff preview pane** — press `p` to toggle a side panel showing `git diff --stat` for the selected worktree vs main. Answers "what did this agent change?" at a glance
- [x] **Preview scrolling** — `Ctrl+j` / `Ctrl+k` to scroll the preview content
- [x] **Lazy caching** — diff stat computed on first view per workspace, cached for the session
- [x] **Colored diff stat** — file lines colored by change type (green additions, red deletions, yellow mixed)

---

## Out of scope

These are Conductor's job — not ours:

- Worktree creation / agent launching
- Task decomposition / assignment
- Live agent monitoring dashboard
- PR creation / merge conflict resolution
- Agent-to-agent dependency orchestration
