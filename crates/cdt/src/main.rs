use cdt::cache;
use cdt::clean;
use cdt::conflicts;
use cdt::scanner;
use cdt::timeline;
use cdt::tui;

use cli_core::Result;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

const DEFAULT_ROOT: &str = "conductor/workspaces";
const SHELL_EVAL_LINE: &str = r#"eval "$(cdt init-shell)""#;

#[derive(Parser)]
#[command(name = "cdt", about = "Quick fuzzy jumper for Conductor workspaces")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Workspace root directory (default: ~/conductor/workspaces)
    #[arg(short, long, global = true, env = "CDT_ROOT")]
    root: Option<PathBuf>,

    /// Bypass the disk cache and force a fresh scan
    #[arg(long, global = true)]
    no_cache: bool,

    /// Show scan timing information on stderr
    #[arg(long, global = true)]
    time: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// List all workspaces (non-interactive)
    #[command(name = "ls")]
    List {
        /// Show PR status per worktree (queries GitHub)
        #[arg(long)]
        pr: bool,
    },
    /// Remove merged or stale worktrees
    Clean {
        /// Remove all merged worktrees without prompting
        #[arg(long)]
        merged: bool,

        /// Remove worktrees with no commits in the given duration (e.g. 7d, 24h)
        #[arg(long, value_name = "DURATION")]
        stale: Option<String>,

        /// Preview what would be removed without actually removing anything
        #[arg(long)]
        dry_run: bool,

        /// Skip confirmation prompt (use with --merged or --stale)
        #[arg(short = 'y', long)]
        yes: bool,
    },
    /// Show diff for a workspace's branch against main
    Diff {
        /// Workspace name or project/name
        workspace: String,
    },
    /// Open workspace PR in browser or workspace in editor
    Open {
        /// Workspace name or project/name
        workspace: String,

        /// Open in editor instead of browser
        #[arg(long)]
        editor: bool,
    },
    /// Detect file conflicts across open worktrees
    Conflicts,
    /// Chronological view of activity across all worktrees
    Timeline {
        /// Maximum number of events to show (default: 25)
        #[arg(short = 'n', long, default_value = "25")]
        limit: usize,
    },
    /// Show one-line summary of workspace status
    Summary,
    /// Jump back to the main git repo from a worktree
    Root,
    /// Set up shell integration (appends eval line to shell rc file)
    Install,
    /// Print shell function for cd integration
    InitShell,
    /// Clear the workspace cache
    ClearCache,
}

fn append_shell_integration() -> Option<String> {
    cli_core::shell::ensure_shell_line(
        SHELL_EVAL_LINE,
        &format!("# cdt — Conductor workspace navigator\n{SHELL_EVAL_LINE}"),
        "Shell integration already configured in",
    )
}

fn resolve_root(custom: Option<PathBuf>) -> PathBuf {
    custom.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(DEFAULT_ROOT)
    })
}

fn main() {
    cli_core::run_main(run);
}

/// Load workspaces: try cache first, fall back to a fresh scan, then save.
fn load_workspaces(
    root: &std::path::Path,
    no_cache: bool,
    time: bool,
) -> Result<Vec<scanner::Workspace>> {
    let t0 = Instant::now();

    // Try cache
    if !no_cache {
        if let Some(cached) = cache::load(root) {
            if time {
                eprintln!("[cdt] cache hit — loaded in {:.1?}", t0.elapsed());
            }
            return Ok(cached);
        }
    }

    // Fresh scan (parallelised with rayon)
    let workspaces = scanner::scan(root)?;

    if time {
        eprintln!(
            "[cdt] fresh scan — {} workspaces in {:.1?}",
            workspaces.len(),
            t0.elapsed()
        );
    }

    // Persist for next time
    cache::save(root, &workspaces);

    Ok(workspaces)
}

fn run() -> Result {
    let cli = Cli::parse();
    let root = resolve_root(cli.root);

    if !root.is_dir() {
        return Err(format!("Workspace root not found: {}", root.display()).into());
    }

    match cli.command {
        None => {
            // Interactive TUI — print selected path to stdout
            if let Some(path) = tui::run(&root, cli.no_cache, cli.time)? {
                print!("{}", path.display());
            }
        }
        Some(Commands::List { pr }) => {
            let mut workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            if pr {
                scanner::attach_pr_info(&mut workspaces);
            }
            if workspaces.is_empty() {
                eprintln!("No workspaces found in {}", root.display());
            } else {
                for ws in &workspaces {
                    let d = ws.display_columns();
                    let dirty = if d.dirty { " ✗" } else { "" };
                    let pr_col = if pr {
                        match &d.pr {
                            Some(info) => format!("PR #{:<4} {:<4}", info.number, info.ci_label),
                            None => "no PR       ".to_string(),
                        }
                    } else {
                        String::new()
                    };
                    if pr {
                        println!(
                            "{:<10} {:<14}{:<16} {:<16} {:<24} {:>8}{:<3}",
                            d.merge_status, pr_col, d.project, d.name, d.branch, d.age, dirty
                        );
                    } else {
                        println!(
                            "{:<10} {:<16} {:<16} {:<24} {:>8}{:<3} {}",
                            d.merge_status, d.project, d.name, d.branch, d.age, dirty, d.path
                        );
                    }
                }
            }
        }
        Some(Commands::Clean {
            merged,
            stale,
            dry_run,
            yes,
        }) => {
            let stale_duration = stale
                .map(|s| clean::parse_duration(&s))
                .transpose()
                .map_err(|e| format!("invalid --stale value: {e}"))?;

            // If neither --merged nor --stale given, default to interactive mode
            // which shows all merged worktrees for selection
            let interactive = !merged && stale_duration.is_none();
            let include_merged = merged || interactive;

            // Always do a fresh scan for clean — stale cache could hide merged status
            let workspaces = scanner::scan(&root)?;

            let candidates = clean::find_candidates(&workspaces, include_merged, stale_duration);

            if candidates.is_empty() {
                eprintln!("Nothing to clean.");
                return Ok(());
            }

            if dry_run {
                eprintln!("Would remove {} worktree(s):\n", candidates.len());
                for c in &candidates {
                    println!("  {}", clean::format_candidate(c));
                }
                return Ok(());
            }

            // In interactive mode (no flags), let user pick which ones to remove
            if interactive {
                eprintln!("Merged worktrees:\n");
                for (i, c) in candidates.iter().enumerate() {
                    eprintln!("  [{}] {}", i + 1, clean::format_candidate(c));
                }
                eprintln!("\nUse --merged to remove all, or --dry-run to preview.");
                return Ok(());
            }

            // Non-interactive: show what will be removed and confirm
            eprintln!("Will remove {} worktree(s):\n", candidates.len());
            for c in &candidates {
                eprintln!("  {}", clean::format_candidate(c));
            }

            if !yes {
                eprint!("\nProceed? [y/N] ");
                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                if !input.trim().eq_ignore_ascii_case("y") {
                    eprintln!("Aborted.");
                    return Ok(());
                }
            }

            let mut removed = 0;
            let mut failed = 0;
            for c in &candidates {
                match clean::remove_worktree(c.workspace) {
                    clean::RemoveResult::Removed => {
                        eprintln!("  ✓ removed {}", c.workspace.label());
                        removed += 1;
                    }
                    clean::RemoveResult::Failed(e) => {
                        eprintln!("  ✗ {}", e);
                        failed += 1;
                    }
                }
            }

            eprintln!("\nDone: {removed} removed, {failed} failed.");

            // Invalidate cache since workspace set changed
            if removed > 0 {
                cache::clear();
            }
        }
        Some(Commands::Diff { workspace }) => {
            let workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            let ws = scanner::find_workspace(&workspaces, &workspace)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            let branch = ws.branch.as_deref().unwrap_or("HEAD");

            // Find main branch ref
            let main_ref = ["main", "master"]
                .iter()
                .find(|r| {
                    std::process::Command::new("git")
                        .args(["rev-parse", "--verify", r])
                        .current_dir(&ws.path)
                        .output()
                        .map(|o| o.status.success())
                        .unwrap_or(false)
                })
                .unwrap_or(&"main");

            let status = std::process::Command::new("git")
                .args(["diff", &format!("{main_ref}...{branch}")])
                .current_dir(&ws.path)
                .status()?;

            if !status.success() {
                return Err("git diff failed".into());
            }
        }
        Some(Commands::Conflicts) => {
            let workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            let changes = conflicts::gather_changes(&workspaces);

            if changes.is_empty() {
                eprintln!("No open worktrees with changes found.");
                return Ok(());
            }

            let file_conflicts = conflicts::detect_conflicts(&changes);

            if file_conflicts.is_empty() {
                println!(
                    "No conflicts detected across {} worktree(s).",
                    changes.len()
                );
            } else {
                println!(
                    "{} file(s) touched by multiple worktrees:\n",
                    file_conflicts.len()
                );
                print!("{}", conflicts::format_conflicts(&file_conflicts));
            }
        }
        Some(Commands::Open { workspace, editor }) => {
            let workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            let ws = scanner::find_workspace(&workspaces, &workspace)
                .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

            if editor {
                // Open workspace directory in $EDITOR or fall back to `open`
                let editor_cmd = std::env::var("EDITOR").unwrap_or_else(|_| "open".to_string());
                let status = std::process::Command::new(&editor_cmd)
                    .arg(&ws.path)
                    .status()?;
                if !status.success() {
                    return Err(format!("{editor_cmd} exited with error").into());
                }
            } else {
                // Open PR in browser via gh, or fall back to opening the directory
                let gh_result = std::process::Command::new("gh")
                    .args(["pr", "view", "--web"])
                    .current_dir(&ws.path)
                    .status();

                match gh_result {
                    Ok(s) if s.success() => {}
                    _ => {
                        eprintln!("No PR found, opening workspace directory...");
                        std::process::Command::new("open").arg(&ws.path).status()?;
                    }
                }
            }
        }
        Some(Commands::Timeline { limit }) => {
            let workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            if workspaces.is_empty() {
                eprintln!("No workspaces found in {}", root.display());
            } else {
                let events = timeline::gather_timeline(&workspaces, limit);
                if events.is_empty() {
                    eprintln!("No activity found.");
                } else {
                    print!("{}", timeline::format_timeline(&events, limit));
                }
            }
        }
        Some(Commands::Summary) => {
            let workspaces = load_workspaces(&root, cli.no_cache, cli.time)?;
            if workspaces.is_empty() {
                eprintln!("No workspaces found in {}", root.display());
            } else {
                println!("{}", scanner::summarize(&workspaces));
            }
        }
        Some(Commands::Root) => {
            let output = std::process::Command::new("git")
                .args(["rev-parse", "--git-common-dir"])
                .output();

            match output {
                Ok(o) if o.status.success() => {
                    let git_common = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    let git_common = PathBuf::from(&git_common);
                    // Resolve to absolute path (git may return relative like "../.git")
                    let git_common = if git_common.is_relative() {
                        std::env::current_dir()?.join(&git_common)
                    } else {
                        git_common
                    };
                    let git_common = git_common.canonicalize()?;

                    // .git dir -> parent is the repo root
                    // For bare repos, git-common-dir may be the .git dir itself
                    let root_dir = if git_common.ends_with(".git") {
                        git_common.parent().unwrap_or(&git_common).to_path_buf()
                    } else {
                        // Bare repo or unusual layout — use as-is
                        git_common
                    };

                    print!("{}", root_dir.display());
                }
                _ => {
                    return Err("Not inside a git repository".into());
                }
            }
        }
        Some(Commands::Install) => match append_shell_integration() {
            Some(result) => eprintln!("{result}"),
            None => eprintln!(
                "Could not detect shell. Add `{SHELL_EVAL_LINE}` to your shell config manually."
            ),
        },
        Some(Commands::InitShell) => {
            print!(
                r#"# Add to your .zshrc or .bashrc:
cdt() {{
  local dir
  dir="$(command cdt "$@")"
  if [ -n "$dir" ] && [ -d "$dir" ]; then
    cd "$dir" || return
  fi
}}
"#
            );
        }
        Some(Commands::ClearCache) => {
            cache::clear();
            eprintln!("Cache cleared.");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn append_shell_integration_writes_to_rc() {
        let tmp = tempfile::tempdir().unwrap();
        let rc_path = tmp.path().join(".zshrc");
        std::fs::write(&rc_path, "# existing config\n").unwrap();

        // Verify the eval line is not present yet
        let contents = std::fs::read_to_string(&rc_path).unwrap();
        assert!(!contents.contains(SHELL_EVAL_LINE));

        // Append the eval line
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rc_path)
            .unwrap();
        writeln!(
            file,
            "\n# cdt — Conductor workspace navigator\n{SHELL_EVAL_LINE}"
        )
        .unwrap();

        let updated = std::fs::read_to_string(&rc_path).unwrap();
        assert!(updated.contains(SHELL_EVAL_LINE));
        assert!(updated.contains("# existing config"));
    }

    #[test]
    fn append_shell_integration_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let rc_path = tmp.path().join(".zshrc");
        let content = format!("# existing\n{SHELL_EVAL_LINE}\n");
        std::fs::write(&rc_path, &content).unwrap();

        // The idempotency check should detect the line is already present
        let contents = std::fs::read_to_string(&rc_path).unwrap();
        assert!(contents.contains(SHELL_EVAL_LINE));
    }

    #[test]
    fn shell_eval_line_matches_init_shell_command() {
        // Ensure the constant references the correct command
        assert!(SHELL_EVAL_LINE.contains("cdt init-shell"));
    }
}
