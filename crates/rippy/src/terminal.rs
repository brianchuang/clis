use std::path::PathBuf;
use std::process::Command;

/// Detect which terminal app to use.
pub fn detect_terminal(pref: &str) -> &str {
    match pref {
        "auto" => {
            if PathBuf::from("/Applications/iTerm.app").exists() {
                "iTerm2"
            } else {
                "Terminal"
            }
        }
        other => other,
    }
}

/// Launch the rippy TUI in a terminal window.
pub fn launch_tui(terminal_pref: &str) {
    let bin = rippy_binary_path();
    let terminal = detect_terminal(terminal_pref);

    let script = match terminal {
        "iTerm2" | "iterm2" | "iterm" => format!(
            r#"tell application "iTerm2"
                activate
                create window with default profile command "{bin} ; exit"
            end tell"#
        ),
        "Alacritty" | "alacritty" => format!(
            r#"do shell script "open -a Alacritty --args -e {bin}"
            "#
        ),
        "WezTerm" | "wezterm" => format!(
            r#"do shell script "open -a WezTerm --args start -- {bin}"
            "#
        ),
        _ => format!(
            r#"tell application "Terminal"
                activate
                do script "{bin} ; exit"
            end tell"#
        ),
    };

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .ok();
}

fn rippy_binary_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "rippy".into())
}
