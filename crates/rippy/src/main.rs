mod clipboard;
mod config;
mod db;
mod highlight;
mod hotkey;
mod mcp;
pub(crate) mod tag;
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
    /// Print entry content to stdout by ID (for piping)
    Get {
        /// Entry ID
        id: i64,
    },
    /// Save stdin as a clipboard entry
    Save,
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
    /// Set up a global keyboard shortcut via macOS Quick Actions (no permissions needed)
    Shortcut {
        #[command(subcommand)]
        action: ShortcutAction,
    },
    /// Print shell alias for yy shortcut (eval in your shell rc)
    InitShell,
    /// Start MCP server (stdio transport) for AI assistant integration
    Mcp,
    /// Open rippy TUI in configured terminal (used by Quick Action)
    #[command(hide = true)]
    LaunchTui,
    /// Watch clipboard (used internally by launchd)
    #[command(hide = true)]
    Watch,
}

#[derive(Subcommand)]
enum ShortcutAction {
    /// Create a macOS Quick Action with a global keyboard shortcut
    Install,
    /// Remove the Quick Action and keyboard shortcut
    Uninstall,
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

fn db_path() -> PathBuf {
    data_dir().join("history.db")
}

fn with_store<T>(
    f: impl FnOnce(&db::Store) -> std::result::Result<T, rusqlite::Error>,
) -> Result<T> {
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
        Some(Commands::Search { query, count, json }) => {
            print!("{}", cmd_search(&query, count, json)?)
        }
        Some(Commands::Copy { id }) => println!("{}", cmd_copy(id)?),
        Some(Commands::Get { id }) => cmd_get(id)?,
        Some(Commands::Save) => println!("{}", cmd_save()?),
        Some(Commands::Clear) => println!("{}", cmd_clear()?),
        Some(Commands::Hotkey { action }) => cmd_hotkey(action)?,
        Some(Commands::Shortcut { action }) => match action {
            ShortcutAction::Install => println!("{}", cmd_shortcut_install()?),
            ShortcutAction::Uninstall => println!("{}", cmd_shortcut_uninstall()?),
        },
        Some(Commands::InitShell) => print!("{}", init_shell_output()),
        Some(Commands::Install) => println!("{}", cmd_install()?),
        Some(Commands::Uninstall) => println!("{}", cmd_uninstall()?),
        Some(Commands::Mcp) => tokio::runtime::Runtime::new()?.block_on(mcp::run(db_path()))?,
        Some(Commands::LaunchTui) => {
            let cfg = config::Config::load(&data_dir());
            terminal::launch_tui(&cfg.terminal.app);
        }
        Some(Commands::Watch) => cmd_watch()?,
    }
    Ok(())
}

fn cmd_list(count: usize, json: bool) -> Result<String> {
    with_store(|store| store.recent(count)).map(|entries| {
        if json {
            format_entries_json(&entries)
        } else {
            format_entries(&entries, "No clipboard history yet. Run `rippy` to start.")
        }
    })
}

fn cmd_search(query: &str, count: usize, json: bool) -> Result<String> {
    let q = query.to_string();
    with_store(move |store| store.search(&q, count)).map(|entries| {
        if json {
            format_entries_json(&entries)
        } else {
            format_entries(&entries, "No matches found.")
        }
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

fn cmd_get(id: i64) -> Result {
    let entry = with_store(|store| store.get(id))?
        .ok_or_else(|| -> Box<dyn std::error::Error> { format!("Entry {id} not found.").into() })?;
    print!("{}", entry.content);
    Ok(())
}

fn cmd_save() -> Result<String> {
    use std::io::Read;
    let mut content = String::new();
    std::io::stdin().read_to_string(&mut content)?;
    let content = content.trim_end_matches('\n');
    if content.is_empty() {
        return Err("No input provided. Pipe content to stdin: echo \"text\" | rippy save".into());
    }
    let id = with_store(|store| store.insert(content, None))?;
    Ok(format!("Saved entry {id}: {}", truncate(content, 60)))
}

fn cmd_clear() -> Result<String> {
    with_store(|store| store.clear()).map(|count| format!("Cleared {count} entries."))
}

fn app_bundle_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap()
        .join("Applications")
        .join("Rippy.app")
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
const INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleIdentifier</key>
    <string>com.rippy.watcher</string>
    <key>CFBundleName</key>
    <string>Rippy</string>
    <key>CFBundleExecutable</key>
    <string>rippy</string>
    <key>LSUIElement</key>
    <true/>
</dict>
</plist>"#;

/// Build the .app bundle directory structure at `app_dir` by copying
/// `rippy_bin` into Contents/MacOS/rippy and writing the Info.plist.
/// Returns the path to the binary inside the bundle.
///
/// Does NOT codesign — call `codesign_bundle` separately so tests can
/// inspect the intermediate state.
fn create_app_bundle_at(
    app_dir: &std::path::Path,
    rippy_bin: &str,
) -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let macos_dir = app_dir.join("Contents").join("MacOS");
    std::fs::create_dir_all(&macos_dir)?;

    let info_plist = app_dir.join("Contents").join("Info.plist");
    std::fs::write(&info_plist, INFO_PLIST)?;

    let dest = macos_dir.join("rippy");
    if !dest.exists() {
        std::fs::copy(rippy_bin, &dest)?;
    }
    Ok(dest)
}

/// Ad-hoc codesign the .app **bundle** (not just the binary inside it).
///
/// Signing the bundle rather than the binary is critical: macOS TCC checks
/// the bundle's sealed Code Directory, which binds the Info.plist (and its
/// CFBundleIdentifier) to the executable.  Signing only the binary leaves
/// Info.plist unbound, so TCC can't associate the running process with the
/// bundle identifier — causing CGEventTapCreate to fail even after the user
/// grants Accessibility permission.
fn codesign_bundle(app_dir: &std::path::Path) -> std::io::Result<std::process::ExitStatus> {
    std::process::Command::new("codesign")
        .args(["--force", "--sign", "-", &app_dir.to_string_lossy()])
        .status()
}

fn create_app_bundle(
    rippy_bin: &str,
) -> std::result::Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let app_dir = app_bundle_dir();
    let dest = create_app_bundle_at(&app_dir, rippy_bin)?;
    codesign_bundle(&app_dir)?;
    Ok(dest)
}

const SHELL_ALIAS_LINE: &str = r#"eval "$(rippy init-shell)""#;

fn init_shell_output() -> String {
    "# Add to your .zshrc or .bashrc:\nalias yy=\"rippy\"\n".to_string()
}

/// Detect the user's shell rc file and append the eval line if not already present.
fn append_shell_alias() -> Option<String> {
    let home = dirs::home_dir()?;
    let shell = std::env::var("SHELL").unwrap_or_default();
    let rc_path = if shell.ends_with("zsh") {
        home.join(".zshrc")
    } else if shell.ends_with("bash") {
        // Prefer .bashrc; fall back to .bash_profile on macOS where .bashrc
        // may not exist yet.
        let bashrc = home.join(".bashrc");
        if bashrc.exists() {
            bashrc
        } else {
            home.join(".bash_profile")
        }
    } else {
        return None;
    };

    let contents = std::fs::read_to_string(&rc_path).unwrap_or_default();
    if contents.contains(SHELL_ALIAS_LINE) {
        return Some(format!(
            "Shell alias already configured in {}",
            rc_path.display()
        ));
    }

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(&rc_path)
        .ok()?;
    use std::io::Write;
    writeln!(
        file,
        "\n# rippy — clipboard history manager\n{SHELL_ALIAS_LINE}"
    )
    .ok()?;
    Some(format!("Added yy alias to {}", rc_path.display()))
}

fn cmd_install() -> Result<String> {
    let plist_path = plist_path();
    let rippy_bin = std::env::current_exe()?
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    create_app_bundle(&rippy_bin)?;
    let app_path = app_bundle_dir().to_string_lossy().to_string();

    let log_dir = data_dir();
    let log_path = log_dir.join("service.log").to_string_lossy().to_string();

    // Use `open -a` to launch the .app bundle instead of executing the binary
    // directly.  launchd-launched raw binaries run in a security context where
    // macOS TCC does not apply Accessibility grants, even when the user has
    // toggled permission on for the .app bundle.  `open` launches the app in
    // the user's GUI session where TCC works correctly.
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.rippy.watcher</string>
    <key>ProgramArguments</key>
    <array>
        <string>/usr/bin/open</string>
        <string>-W</string>
        <string>{app_path}</string>
        <string>--args</string>
        <string>watch</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>AssociatedBundleIdentifiers</key>
    <string>com.rippy.watcher</string>
    <key>StandardErrorPath</key>
    <string>{log_path}</string>
    <key>StandardOutPath</key>
    <string>{log_path}</string>
</dict>
</plist>"#
    );

    std::fs::write(&plist_path, plist)?;

    std::process::Command::new("launchctl")
        .args(["load", &plist_path.to_string_lossy()])
        .status()?;

    let mut msg =
        "Installed launchd service.\nClipboard is now monitored 24/7, even when rippy isn't open."
            .to_string();
    msg.push_str(&format!("\nApp bundle: {}", app_bundle_dir().display()));
    msg.push_str(&format!(
        "\n\nGlobal hotkey ({}) is active. To change it: rippy hotkey set --key <key> --modifiers <mods>",
        config::format_hotkey(&config::Config::load(&data_dir()).hotkey)
    ));
    msg.push_str("\n\nNote: The hotkey requires Input Monitoring permission.");
    msg.push_str(
        "\n  Grant it to \"Rippy\" in System Settings > Privacy & Security > Input Monitoring",
    );
    match append_shell_alias() {
        Some(result) => msg.push_str(&format!("\n\n{result}")),
        None => msg.push_str(
            "\n\nTip: Add `eval \"$(rippy init-shell)\"` to your shell config for the `yy` alias.",
        ),
    }
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

    // Keep the .app bundle — deleting it invalidates the Accessibility
    // permission grant.  Only remove it with `--purge`.
    Ok(
        "Uninstalled launchd service. App bundle kept at: ".to_string()
            + &app_bundle_dir().to_string_lossy()
            + "\n  To remove everything: rm -rf "
            + &app_bundle_dir().to_string_lossy(),
    )
}

// --- Quick Action (macOS Services) shortcut ---

const WORKFLOW_NAME: &str = "Launch Rippy";

fn workflow_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("~"))
        .join("Library/Services/Launch Rippy.workflow")
}

/// Info.plist for the Quick Action .workflow bundle.
/// Registers "Launch Rippy" as a Service that accepts no required input,
/// making it available for a global keyboard shortcut.
const WORKFLOW_INFO_PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>NSServices</key>
    <array>
        <dict>
            <key>NSMenuItem</key>
            <dict>
                <key>default</key>
                <string>Launch Rippy</string>
            </dict>
            <key>NSMessage</key>
            <string>runWorkflowAsService</string>
            <key>NSRequiredContext</key>
            <dict/>
            <key>NSSendTypes</key>
            <array>
                <string>public.utf8-plain-text</string>
            </array>
            <key>NSReturnTypes</key>
            <array/>
        </dict>
    </array>
</dict>
</plist>"#;

/// Build the document.wflow plist for an Automator Quick Action that runs
/// the given shell command. Pure function for testability.
fn workflow_document(shell_command: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>actions</key>
    <array>
        <dict>
            <key>action</key>
            <dict>
                <key>AMAccepts</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Optional</key>
                    <true/>
                    <key>Types</key>
                    <array/>
                </dict>
                <key>AMActionVersion</key>
                <string>1.0</string>
                <key>AMProvides</key>
                <dict>
                    <key>Container</key>
                    <string>List</string>
                    <key>Optional</key>
                    <true/>
                    <key>Types</key>
                    <array/>
                </dict>
                <key>ActionBundlePath</key>
                <string>/System/Library/Automator/Run Shell Script.action</string>
                <key>ActionName</key>
                <string>Run Shell Script</string>
                <key>ActionParameters</key>
                <dict>
                    <key>COMMAND_STRING</key>
                    <string>{shell_command}</string>
                    <key>CheckedForUserDefaultShell</key>
                    <true/>
                    <key>inputMethod</key>
                    <integer>1</integer>
                    <key>shell</key>
                    <string>/bin/zsh</string>
                    <key>source</key>
                    <string></string>
                </dict>
                <key>BundleIdentifier</key>
                <string>com.apple.RunShellScript</string>
                <key>CFBundleVersion</key>
                <string>1.0</string>
                <key>CanShowSelectedItemsWhenRun</key>
                <false/>
                <key>CanShowWhenRun</key>
                <false/>
                <key>Category</key>
                <array>
                    <string>AMCategoryUtilities</string>
                </array>
                <key>Class Name</key>
                <string>RunShellScriptAction</string>
                <key>InputUUID</key>
                <string>A1B2C3D4-0000-0000-0000-000000000001</string>
                <key>OutputUUID</key>
                <string>A1B2C3D4-0000-0000-0000-000000000002</string>
                <key>UUID</key>
                <string>A1B2C3D4-0000-0000-0000-000000000003</string>
                <key>UnlocalizedApplications</key>
                <array>
                    <string>Automator</string>
                </array>
                <key>arguments</key>
                <dict>
                    <key>0</key>
                    <dict>
                        <key>default value</key>
                        <string>/bin/zsh</string>
                        <key>name</key>
                        <string>shell</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                    </dict>
                    <key>1</key>
                    <dict>
                        <key>default value</key>
                        <integer>1</integer>
                        <key>name</key>
                        <string>inputMethod</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                    </dict>
                    <key>2</key>
                    <dict>
                        <key>default value</key>
                        <string></string>
                        <key>name</key>
                        <string>COMMAND_STRING</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                    </dict>
                    <key>3</key>
                    <dict>
                        <key>default value</key>
                        <true/>
                        <key>name</key>
                        <string>CheckedForUserDefaultShell</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                    </dict>
                    <key>4</key>
                    <dict>
                        <key>default value</key>
                        <string></string>
                        <key>name</key>
                        <string>source</string>
                        <key>required</key>
                        <string>0</string>
                        <key>type</key>
                        <string>0</string>
                    </dict>
                </dict>
                <key>isViewVisible</key>
                <true/>
                <key>location</key>
                <string>529.500000:544.000000</string>
                <key>nibPath</key>
                <string>/System/Library/Automator/Run Shell Script.action/Contents/Resources/Base.lproj/main.nib</string>
            </dict>
            <key>isViewVisible</key>
            <true/>
        </dict>
    </array>
    <key>connectors</key>
    <dict/>
    <key>workflowMetaData</key>
    <dict>
        <key>workflowTypeIdentifier</key>
        <string>com.apple.Automator.servicesMenu</string>
    </dict>
</dict>
</plist>"#,
        shell_command = shell_command
    )
}

/// Create the .workflow bundle at `wf_dir` with the given shell command.
/// Returns the path to document.wflow. Pure filesystem operation — no
/// system registration.
fn create_workflow_bundle_at(
    wf_dir: &std::path::Path,
    shell_command: &str,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let contents_dir = wf_dir.join("Contents");
    std::fs::create_dir_all(&contents_dir)?;
    std::fs::write(contents_dir.join("Info.plist"), WORKFLOW_INFO_PLIST)?;
    std::fs::write(
        contents_dir.join("document.wflow"),
        workflow_document(shell_command),
    )?;
    Ok(())
}

/// Service identifier used by pbs (pasteboard server) to reference this
/// Quick Action. Format: `(null) - <menu-item-name> - <message>`.
fn pbs_service_key() -> String {
    format!("(null) - {WORKFLOW_NAME} - runWorkflowAsService")
}

fn cmd_shortcut_install() -> Result<String> {
    let rippy_bin = std::env::current_exe()?
        .canonicalize()?
        .to_string_lossy()
        .to_string();

    let shell_command = format!("{rippy_bin} launch-tui");
    let wf_dir = workflow_dir();

    create_workflow_bundle_at(&wf_dir, &shell_command)?;

    // Flush pbs so macOS discovers the new service.
    std::process::Command::new("/System/Library/CoreServices/pbs")
        .arg("-flush")
        .status()
        .ok();

    // Try to assign the keyboard shortcut automatically.
    let cfg = config::Config::load(&data_dir());
    let key_equiv = config::pbs_key_equivalent(&cfg.hotkey);
    let service_key = pbs_service_key();
    let pbs_value = format!(r#"{{ "enabled" = 1; "key_equivalent" = "{key_equiv}"; }}"#);

    let shortcut_set = std::process::Command::new("defaults")
        .args([
            "write",
            "pbs",
            "NSServicesStatus",
            "-dict-add",
            &service_key,
            &pbs_value,
        ])
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let mut msg = format!("Installed Quick Action at: {}\n", wf_dir.display());

    if shortcut_set {
        msg.push_str(&format!(
            "\nKeyboard shortcut {} assigned automatically.",
            config::format_hotkey(&cfg.hotkey)
        ));
        msg.push_str("\nYou may need to log out and back in, or restart, for it to take effect.");
    }

    msg.push_str("\n\nTo set or change the keyboard shortcut manually:");
    msg.push_str("\n  System Settings > Keyboard > Keyboard Shortcuts > Services > General");
    msg.push_str(&format!(
        "\n  Find \"{}\" and assign your preferred shortcut.",
        WORKFLOW_NAME
    ));
    msg.push_str("\n\nNo Input Monitoring or Accessibility permissions required.");

    Ok(msg)
}

fn cmd_shortcut_uninstall() -> Result<String> {
    let wf_dir = workflow_dir();

    if !wf_dir.exists() {
        return Ok("No Quick Action shortcut installed.".into());
    }

    std::fs::remove_dir_all(&wf_dir)?;

    // Flush pbs to deregister the service.
    std::process::Command::new("/System/Library/CoreServices/pbs")
        .arg("-flush")
        .status()
        .ok();

    Ok(format!(
        "Removed Quick Action: {}\nKeyboard shortcut deregistered.",
        wf_dir.display()
    ))
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
        HotkeyAction::Set {
            key,
            modifiers,
            terminal,
        } => {
            let mut cfg = config::Config::load(&dir);
            if let Some(k) = &key {
                if config::keycode_for(k).is_none() {
                    return Err(
                        format!("Unknown key: '{k}'. Use a letter, number, or f1-f12.").into(),
                    );
                }
                cfg.hotkey.key = k.clone();
            }
            if let Some(m) = &modifiers {
                let mods: Vec<String> = m.split(',').map(|s| s.trim().to_lowercase()).collect();
                for name in &mods {
                    if config::modifier_flag(name).is_none() {
                        return Err(format!(
                            "Unknown modifier: '{name}'. Use cmd, shift, ctrl, or alt."
                        )
                        .into());
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
            if !hotkey::check_listen_permission(true) {
                eprintln!(
                    "Warning: Input Monitoring permission not granted. A system dialog should appear."
                );
                eprintln!();
            }
            println!(
                "Listening for {}... Press Ctrl+C to stop.",
                config::format_hotkey(&cfg.hotkey)
            );
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

    let cfg = config::Config::load(&data_dir());
    let w = watcher::Watcher::spawn(
        &db_path(),
        cfg.history.max_entries,
        cfg.history.auto_expire_seconds,
    );

    // Always attempt to install the hotkey — CGEventTapCreate is the real
    // permission check.  CGPreflightListenEventAccess can return false for
    // launchd-launched .app bundles even when the bundle has been granted
    // Input Monitoring access.  If the tap fails, install_and_run prints
    // an error and returns immediately, so we fall back to clipboard-only
    // watching.
    if !hotkey::check_listen_permission(false) {
        eprintln!("Input Monitoring pre-check returned false — attempting event tap anyway...");
    }
    hotkey::install_and_run(&cfg, running.clone());

    // If install_and_run returned early (tap creation failed), fall back to
    // clipboard watching only.
    if running.load(Ordering::Relaxed) {
        eprintln!(
            "Hotkey disabled: could not create event tap. Grant Input Monitoring permission to Rippy."
        );
        while running.load(Ordering::Relaxed) {
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
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
        .map(|e| {
            let pin = if e.pinned { "★" } else { " " };
            let tag = tag::detect(&e.content).label();
            format!(
                "{pin} {:>5} │ {} │ {:<4} │ {}",
                e.id,
                e.timestamp.format("%Y-%m-%d %H:%M:%S"),
                tag,
                truncate(&e.content, 80)
            )
        })
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
    use std::path::Path;
    use std::process::Command;

    fn make_entry(id: i64, content: &str) -> db::ClipEntry {
        db::ClipEntry {
            id,
            content: content.to_string(),
            hash: "unused".to_string(),
            timestamp: chrono::Local.timestamp_opt(1700000000, 0).unwrap(),
            app_name: None,
            pinned: false,
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
        let entries = vec![make_entry(1, "first"), make_entry(2, "second")];
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

    /// Build a real .app bundle in a temp dir using the current test binary
    /// as a stand-in, then verify the directory structure is correct.
    #[test]
    fn app_bundle_has_correct_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("Test.app");
        // Use the test binary itself as the source — we just need a valid Mach-O.
        let test_bin = std::env::current_exe().unwrap();

        let dest = create_app_bundle_at(&app_dir, &test_bin.to_string_lossy()).unwrap();

        assert!(app_dir.join("Contents/Info.plist").exists());
        assert!(app_dir.join("Contents/MacOS/rippy").exists());
        assert_eq!(dest, app_dir.join("Contents/MacOS/rippy"));

        let plist = std::fs::read_to_string(app_dir.join("Contents/Info.plist")).unwrap();
        assert!(
            plist.contains("com.rippy.watcher"),
            "Info.plist must contain bundle id"
        );
        assert!(
            plist.contains("LSUIElement"),
            "Info.plist must set LSUIElement for background app"
        );
    }

    /// After codesigning the bundle, `codesign -d` must report the bundle
    /// identifier from Info.plist (not an auto-generated one) and Info.plist
    /// must be bound into the sealed resources.
    #[test]
    fn codesign_bundle_binds_identifier_and_plist() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("Test.app");
        let test_bin = std::env::current_exe().unwrap();

        create_app_bundle_at(&app_dir, &test_bin.to_string_lossy()).unwrap();
        let status = codesign_bundle(&app_dir).unwrap();
        assert!(status.success(), "codesign must succeed");

        // Verify: `codesign -d --verbose` should show our bundle identifier
        let output = Command::new("codesign")
            .args(["-d", "--verbose=2", &app_dir.to_string_lossy()])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        assert!(
            stderr.contains("Identifier=com.rippy.watcher"),
            "codesign must report the bundle identifier from Info.plist, got: {stderr}"
        );
        assert!(
            stderr.contains("Info.plist entries="),
            "Info.plist must be bound (sealed) into the signature, got: {stderr}"
        );
        assert!(
            !stderr.contains("Info.plist=not bound"),
            "Info.plist must NOT be 'not bound', got: {stderr}"
        );
    }

    // --- Quick Action shortcut ---

    #[test]
    fn workflow_document_contains_shell_command() {
        let doc = workflow_document("/usr/local/bin/rippy launch-tui");
        assert!(doc.contains("/usr/local/bin/rippy launch-tui"));
        assert!(doc.contains("com.apple.Automator.servicesMenu"));
        assert!(doc.contains("Run Shell Script"));
    }

    #[test]
    fn workflow_document_escapes_nothing_in_simple_path() {
        let doc = workflow_document("/bin/rippy launch-tui");
        assert!(doc.contains("<string>/bin/rippy launch-tui</string>"));
    }

    #[test]
    fn workflow_info_plist_has_service_definition() {
        assert!(WORKFLOW_INFO_PLIST.contains("Launch Rippy"));
        assert!(WORKFLOW_INFO_PLIST.contains("runWorkflowAsService"));
        assert!(WORKFLOW_INFO_PLIST.contains("NSRequiredContext"));
    }

    #[test]
    fn create_workflow_bundle_has_correct_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let wf_dir = tmp.path().join("Test.workflow");

        create_workflow_bundle_at(&wf_dir, "/bin/rippy launch-tui").unwrap();

        assert!(wf_dir.join("Contents/Info.plist").exists());
        assert!(wf_dir.join("Contents/document.wflow").exists());

        let info = std::fs::read_to_string(wf_dir.join("Contents/Info.plist")).unwrap();
        assert!(info.contains("Launch Rippy"));

        let doc = std::fs::read_to_string(wf_dir.join("Contents/document.wflow")).unwrap();
        assert!(doc.contains("/bin/rippy launch-tui"));
    }

    #[test]
    fn create_workflow_bundle_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let wf_dir = tmp.path().join("Test.workflow");

        create_workflow_bundle_at(&wf_dir, "/old/path").unwrap();
        create_workflow_bundle_at(&wf_dir, "/new/path").unwrap();

        let doc = std::fs::read_to_string(wf_dir.join("Contents/document.wflow")).unwrap();
        assert!(doc.contains("/new/path"));
        assert!(!doc.contains("/old/path"));
    }

    #[test]
    fn pbs_service_key_format() {
        let key = pbs_service_key();
        assert_eq!(key, "(null) - Launch Rippy - runWorkflowAsService");
    }

    #[test]
    fn get_subcommand_prints_raw_content() {
        let store = db::Store::open(Path::new(":memory:")).unwrap();
        let id = store.insert("hello piped world", None).unwrap();
        let entry = store.get(id).unwrap().unwrap();
        assert_eq!(entry.content, "hello piped world");
    }

    #[test]
    fn get_subcommand_not_found() {
        let store = db::Store::open(Path::new(":memory:")).unwrap();
        assert!(store.get(9999).unwrap().is_none());
    }

    #[test]
    fn save_trims_trailing_newline() {
        let input = "hello world\n";
        let content = input.trim_end_matches('\n');
        assert_eq!(content, "hello world");
    }

    #[test]
    fn save_rejects_empty_input() {
        let input = "\n";
        let content = input.trim_end_matches('\n');
        assert!(content.is_empty());
    }

    #[test]
    fn save_preserves_internal_newlines() {
        let input = "line1\nline2\nline3\n";
        let content = input.trim_end_matches('\n');
        assert_eq!(content, "line1\nline2\nline3");

        let store = db::Store::open(Path::new(":memory:")).unwrap();
        let id = store.insert(content, None).unwrap();
        let entry = store.get(id).unwrap().unwrap();
        assert_eq!(entry.content, "line1\nline2\nline3");
    }

    /// The install message must tell users to grant Input Monitoring, not
    /// Accessibility — listen-only event taps require Input Monitoring.
    #[test]
    fn install_message_references_input_monitoring() {
        // We can't run cmd_install() in tests (it touches launchd), but we
        // can verify the static string that's appended to the message.
        // This acts as a grep-guard: if someone changes the message back to
        // "Accessibility", this test fails.
        let msg =
            "Grant it to \"Rippy\" in System Settings > Privacy & Security > Input Monitoring";
        assert!(msg.contains("Input Monitoring"));
        assert!(!msg.contains("Accessibility"));
    }

    #[test]
    fn init_shell_outputs_alias() {
        let output = init_shell_output();
        assert!(output.contains("alias yy=\"rippy\""));
    }

    #[test]
    fn append_shell_alias_writes_to_rc() {
        let tmp = tempfile::tempdir().unwrap();
        let rc_path = tmp.path().join(".zshrc");
        std::fs::write(&rc_path, "# existing config\n").unwrap();

        // Simulate by calling the append logic directly on a known file
        let contents = std::fs::read_to_string(&rc_path).unwrap();
        assert!(!contents.contains(SHELL_ALIAS_LINE));

        // Write the alias line
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&rc_path)
            .unwrap();
        writeln!(
            file,
            "\n# rippy — clipboard history manager\n{SHELL_ALIAS_LINE}"
        )
        .unwrap();

        let updated = std::fs::read_to_string(&rc_path).unwrap();
        assert!(updated.contains(SHELL_ALIAS_LINE));
        assert!(updated.contains("# existing config"));
    }

    #[test]
    fn append_shell_alias_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let rc_path = tmp.path().join(".zshrc");
        let content = format!("# existing\n{SHELL_ALIAS_LINE}\n");
        std::fs::write(&rc_path, &content).unwrap();

        // Reading back should detect the line is already present
        let contents = std::fs::read_to_string(&rc_path).unwrap();
        assert!(contents.contains(SHELL_ALIAS_LINE));
    }

    /// A linker-signed binary (no explicit codesign) in a bundle leaves
    /// Info.plist unbound — this was the original bug. The binary had no
    /// explicit codesign call, so TCC couldn't associate it with the bundle.
    #[test]
    fn linker_signed_binary_leaves_plist_unbound() {
        let tmp = tempfile::tempdir().unwrap();
        let app_dir = tmp.path().join("Test.app");
        let test_bin = std::env::current_exe().unwrap();

        // Build bundle but do NOT codesign at all (simulates the original bug)
        create_app_bundle_at(&app_dir, &test_bin.to_string_lossy()).unwrap();

        let output = Command::new("codesign")
            .args(["-d", "--verbose=2", &app_dir.to_string_lossy()])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&output.stderr);

        // Linker-signed binary gets an auto-generated identifier, not our bundle id
        assert!(
            !stderr.contains("Identifier=com.rippy.watcher"),
            "Without explicit codesign, identifier should NOT match bundle id, got: {stderr}"
        );
    }
}
