mod clipboard;
mod config;
mod db;
mod hotkey;
mod mcp;
mod terminal;
mod tui;
mod watcher;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(name = "rippy", about = "macOS clipboard history manager")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// List recent clipboard entries
    List {
        /// Number of entries to show
        #[arg(short, long, default_value = "20")]
        count: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Search clipboard history
    Search {
        /// Search query
        query: String,
        /// Max results
        #[arg(short, long, default_value = "20")]
        count: usize,
        /// Output as JSON
        #[arg(long)]
        json: bool,
    },
    /// Copy a history entry back to clipboard by ID
    Copy {
        /// Entry ID
        id: i64,
    },
    /// Clear all clipboard history
    Clear,
    /// Install as a launchd service for 24/7 clipboard monitoring
    Install,
    /// Uninstall the launchd service
    Uninstall,
    /// Configure the global hotkey
    Hotkey {
        #[command(subcommand)]
        action: HotkeyAction,
    },
    /// Start MCP server (stdio transport) for AI assistant integration
    Mcp,
    /// Watch clipboard (used internally by launchd)
    #[command(hide = true)]
    Watch,
}

#[derive(Subcommand)]
enum HotkeyAction {
    /// Show current hotkey configuration
    Show,
    /// Set the hotkey
    Set {
        /// Key name (e.g. v, c, space, f1)
        #[arg(long)]
        key: Option<String>,
        /// Comma-separated modifiers (e.g. cmd,shift)
        #[arg(long)]
        modifiers: Option<String>,
        /// Terminal app: auto, Terminal, iTerm2, Alacritty, WezTerm
        #[arg(long)]
        terminal: Option<String>,
    },
    /// Test the hotkey listener (runs in foreground)
    Test,
}

fn data_dir() -> PathBuf {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rippy");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn db_path() -> PathBuf { data_dir().join("history.db") }

fn with_store<T>(f: impl FnOnce(&db::Store) -> std::result::Result<T, rusqlite::Error>) -> Result<T> {
    let store = db::Store::open(&db_path())?;
    Ok(f(&store)?)
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result {
    let cli = Cli::parse();

    match cli.command {
        None => tui::run(&db_path())?,
        Some(Commands::List { count, json }) => print!("{}", cmd_list(count, json)?),
        Some(Commands::Search { query, count, json }) => print!("{}", cmd_search(&query, count, json)?),
        Some(Commands::Copy { id }) => println!("{}", cmd_copy(id)?),
        Some(Commands::Clear) => println!("{}", cmd_clear()?),
        Some(Commands::Hotkey { action }) => cmd_hotkey(action)?,
        Some(Commands::Install) => println!("{}", cmd_install()?),
        Some(Commands::Uninstall) => println!("{}", cmd_uninstall()?),
        Some(Commands::Mcp) => tokio::runtime::Runtime::new()?.block_on(mcp::run(db_path()))?,
        Some(Commands::Watch) => cmd_watch()?,
    }
    Ok(())
}

fn cmd_list(count: usize, json: bool) -> Result<String> {
    with_store(|store| store.recent(count))
        .map(|entries| if json {
            format_entries_json(&entries)
        } else {
            format_entries(&entries, "No clipboard history yet. Run `rippy` to start.")
        })
}

fn cmd_search(query: &str, count: usize, json: bool) -> Result<String> {
    let q = query.to_string();
    with_store(move |store| store.search(&q, count))
        .map(|entries| if json {
            format_entries_json(&entries)
        } else {
            format_entries(&entries, "No matches found.")
        })
}

fn cmd_copy(id: i64) -> Result<String> {
    with_store(|store| store.get(id))?
        .map(|entry| {
            clipboard::set_clipboard(&entry.content);
            format!("Copied to clipboard: {}", truncate(&entry.content, 60))
        })
        .ok_or_else(|| format!("Entry {id} not found.").into())
}

fn cmd_clear() -> Result<String> {
    with_store(|store| store.clear())
        .map(|count| format!("Cleared {count} entries."))
}

fn app_bundle_dir() -> std::path::PathBuf {
    dirs::home_dir().unwrap().join("Applications").join("Rippy.app")
}

/// Create a minimal macOS .app bundle containing the rippy binary.
///
/// Why: macOS Accessibility permissions (required for CGEventTap-based global
/// hotkeys) only work reliably with .app bundles. Raw binaries launched by
/// launchd won't appear in System Settings > Privacy & Security > Accessibility,
/// and AXIsProcessTrustedWithOptions won't show its prompt dialog for them.
///
/// Wrapping the binary in a .app bundle (with an Info.plist that declares a
/// CFBundleIdentifier) lets macOS identify it as a proper app, so:
///   1. The native Accessibility prompt dialog works
///   2. "Rippy" appears by name in the Accessibility list
///   3. The user can toggle permission on without hunting for a raw binary path
///
/// The bundle is placed in ~/Applications/Rippy.app and the launchd plist
/// points to the binary inside it, not the original cargo-installed binary.
fn create_app_bundle(rippy_bin: &str) -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let app_dir = app_bundle_dir();
    let macos_dir = app_dir.join("Contents").join("MacOS");
    std::fs::create_dir_all(&macos_dir)?;

    let info_plist = app_dir.join("Contents").join("Info.plist");
    std::fs::write(&info_plist, r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.rippy.watcher</string>
    <key>CFBundleName</key>
    <string>Rippy</string>
    <key>CFBundleExecutable</key>
    <string>rippy</string>
</dict>
</plist>"#)?;

    // Copy the binary into the .app bundle (not symlink — macOS resolves
    // symlinks and grants permission to the target, defeating the purpose)
    let dest = macos_dir.join("rippy");
    std::fs::copy(rippy_bin, &dest)?;
    Ok(dest)
}

fn cmd_install() -> Result<String> {
    let plist_path = plist_path();
    let rippy_bin = std::env::current_exe()?
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    let bundle_bin = create_app_bundle(&rippy_bin)?;
    let bundle_bin_str = bundle_bin.to_string_lossy().to_string();

    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.rippy.watcher</string>
    <key>ProgramArguments</key>
    <array>
        <string>{bundle_bin_str}</string>
        <string>watch</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>"#
    );

    std::fs::write(&plist_path, plist)?;

    std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()?;

    let mut msg = "Installed launchd service.\nClipboard is now monitored 24/7, even when rippy isn't open.".to_string();
    msg.push_str(&format!("\nApp bundle: {}", app_bundle_dir().display()));
    msg.push_str(&format!(
        "\n\nGlobal hotkey ({}) is active. To change it: rippy hotkey set --key <key> --modifiers <mods>",
        config::format_hotkey(&config::Config::load(&data_dir()).hotkey)
    ));
    msg.push_str("\n\nNote: The hotkey requires Accessibility permission.");
    msg.push_str("\n  Grant it to \"Rippy\" in System Settings > Privacy & Security > Accessibility");
    Ok(msg)
}

fn cmd_uninstall() -> Result<String> {
    let plist_path = plist_path();

    if plist_path.exists() {
        std::process::Command::new("launchctl")
            .args(["unload", &plist_path.to_string_lossy()])
            .status()?;
        std::fs::remove_file(&plist_path)?;
    } else {
        return Ok("No launchd service installed.".to_string());
    }

    // Remove the .app bundle
    let app_dir = app_bundle_dir();
    if app_dir.exists() {
        std::fs::remove_dir_all(&app_dir)?;
    }

    Ok("Uninstalled launchd service and removed Rippy.app bundle.".to_string())
}

fn cmd_hotkey(action: HotkeyAction) -> Result {
    let dir = data_dir();
    match action {
        HotkeyAction::Show => {
            let cfg = config::Config::load(&dir);
            println!("Hotkey:   {}", config::format_hotkey(&cfg.hotkey));
            println!("Terminal: {}", cfg.terminal.app);
            println!("\nConfig file: {}", config::Config::path(&dir).display());
        }
        HotkeyAction::Set { key, modifiers, terminal } => {
            let mut cfg = config::Config::load(&dir);
            if let Some(k) = &key {
                if config::keycode_for(k).is_none() {
                    return Err(format!("Unknown key: '{k}'. Use a letter, number, or f1-f12.").into());
                }
                cfg.hotkey.key = k.clone();
            }
            if let Some(m) = &modifiers {
                let mods: Vec<String> = m.split(',').map(|s| s.trim().to_lowercase()).collect();
                for name in &mods {
                    if config::modifier_flag(name).is_none() {
                        return Err(format!("Unknown modifier: '{name}'. Use cmd, shift, ctrl, or alt.").into());
                    }
                }
                cfg.hotkey.modifiers = mods;
            }
            if let Some(t) = terminal {
                cfg.terminal.app = t;
            }
            cfg.save(&dir)?;
            println!("Updated hotkey: {}", config::format_hotkey(&cfg.hotkey));
            println!("Terminal: {}", cfg.terminal.app);
            println!("\nRestart the service for changes to take effect:");
            println!("  rippy uninstall && rippy install");
        }
        HotkeyAction::Test => {
            let cfg = config::Config::load(&dir);
            if !hotkey::check_accessibility(true) {
                eprintln!("Warning: Accessibility permission not granted. A system dialog should appear.");
                eprintln!();
            }
            println!("Listening for {}... Press Ctrl+C to stop.", config::format_hotkey(&cfg.hotkey));
            use std::sync::atomic::AtomicBool;
            use std::sync::Arc;
            let running = Arc::new(AtomicBool::new(true));
            signal_hook::flag::register(signal_hook::consts::SIGINT, running.clone()).ok();
            hotkey::install_and_run(&cfg, running);
        }
    }
    Ok(())
}

fn cmd_watch() -> Result {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    let running = Arc::new(AtomicBool::new(true));
    signal_hook::flag::register(signal_hook::consts::SIGTERM, running.clone()).ok();
    signal_hook::flag::register(signal_hook::consts::SIGINT, running.clone()).ok();

    let w = watcher::Watcher::spawn(&db_path());
    let cfg = config::Config::load(&data_dir());

    if !hotkey::check_accessibility(true) {
        eprintln!("Hotkey disabled: Accessibility permission not granted. A system dialog should appear.");
        eprintln!("Falling back to clipboard watching only.");
        while running.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    } else {
        hotkey::install_and_run(&cfg, running);
    }

    w.stop();
    Ok(())
}

fn plist_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Library/LaunchAgents/com.rippy.watcher.plist")
}

fn format_entries_json(entries: &[db::ClipEntry]) -> String {
    serde_json::to_string_pretty(entries).unwrap() + "\n"
}

fn format_entries(entries: &[db::ClipEntry], empty_msg: &str) -> String {
    if entries.is_empty() {
        return format!("{empty_msg}\n");
    }
    entries
        .iter()
        .map(|e| format!("{:>5} │ {} │ {}", e.id, e.timestamp.format("%Y-%m-%d %H:%M:%S"), truncate(&e.content, 80)))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn truncate(s: &str, max: usize) -> String {
    let line = s.lines().next().unwrap_or("");
    if line.len() > max {
        format!("{}…", &line[..max])
    } else if s.lines().count() > 1 {
        format!("{line}…")
    } else {
        line.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn make_entry(id: i64, content: &str) -> db::ClipEntry {
        db::ClipEntry {
            id,
            content: content.to_string(),
            hash: "unused".to_string(),
            timestamp: chrono::Local.timestamp_opt(1700000000, 0).unwrap(),
            app_name: None,
        }
    }

    #[test]
    fn format_entries_json_empty() {
        let output = format_entries_json(&[]);
        let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed, serde_json::json!([]));
    }

    #[test]
    fn format_entries_json_roundtrip() {
        let entries = vec![
            make_entry(1, "first"),
            make_entry(2, "second"),
        ];
        let output = format_entries_json(&entries);
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&output).unwrap();
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0]["id"], 1);
        assert_eq!(parsed[0]["content"], "first");
        assert_eq!(parsed[1]["id"], 2);
        assert!(parsed[0].get("hash").is_none());
    }

    #[test]
    fn format_entries_plain_empty_shows_message() {
        let output = format_entries(&[], "No entries.");
        assert_eq!(output, "No entries.\n");
    }

    #[test]
    fn format_entries_plain_shows_id_and_content() {
        let entries = vec![make_entry(42, "hello world")];
        let output = format_entries(&entries, "empty");
        assert!(output.contains("42"));
        assert!(output.contains("hello world"));
    }

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("short", 80), "short");
    }

    #[test]
    fn truncate_long_string() {
        let long = "a".repeat(100);
        let result = truncate(&long, 10);
        assert!(result.ends_with('…'));
        assert!(result.len() <= 14); // 10 bytes + multibyte ellipsis
    }

    #[test]
    fn truncate_multiline() {
        let result = truncate("first line\nsecond line", 80);
        assert_eq!(result, "first line…");
    }
}
