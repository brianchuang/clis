use cdt::cache;
use cdt::scanner;
use cdt::tui;

use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::time::Instant;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

const DEFAULT_ROOT: &str = "conductor/workspaces";

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
    /// Print shell function for cd integration
    InitShell,
    /// Clear the workspace cache
    ClearCache,
}

fn resolve_root(custom: Option<PathBuf>) -> PathBuf {
    custom.unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("~"))
            .join(DEFAULT_ROOT)
    })
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
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
        eprintln!("[cdt] fresh scan — {} workspaces in {:.1?}", workspaces.len(), t0.elapsed());
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
