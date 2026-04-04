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

/// Build the AppleScript that opens a terminal with the rippy TUI.
/// Pure function — returns the script string without executing it.
pub fn build_launch_script(terminal: &str, binary_path: &str) -> String {
    match terminal {
        "iTerm2" | "iterm2" | "iterm" => format!(
            r#"tell application "iTerm2"
                activate
                create window with default profile command "{binary_path} ; exit"
            end tell"#
        ),
        "Alacritty" | "alacritty" => format!(
            r#"do shell script "open -a Alacritty --args -e {binary_path}"
            "#
        ),
        "WezTerm" | "wezterm" => format!(
            r#"do shell script "open -a WezTerm --args start -- {binary_path}"
            "#
        ),
        _ => format!(
            r#"tell application "Terminal"
                activate
                do script "{binary_path} ; exit"
            end tell"#
        ),
    }
}

/// Launch the rippy TUI in a terminal window.
pub fn launch_tui(terminal_pref: &str) {
    let bin = rippy_binary_path();
    let terminal = detect_terminal(terminal_pref);
    let script = build_launch_script(terminal, &bin);

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_terminal_explicit_passthrough() {
        assert_eq!(detect_terminal("iTerm2"), "iTerm2");
        assert_eq!(detect_terminal("Terminal"), "Terminal");
        assert_eq!(detect_terminal("Alacritty"), "Alacritty");
        assert_eq!(detect_terminal("WezTerm"), "WezTerm");
    }

    #[test]
    fn detect_terminal_auto_returns_known_terminal() {
        let result = detect_terminal("auto");
        assert!(
            result == "iTerm2" || result == "Terminal",
            "auto should resolve to iTerm2 or Terminal, got: {result}"
        );
    }

    #[test]
    fn build_launch_script_iterm2_contains_binary_path() {
        let script = build_launch_script("iTerm2", "/usr/local/bin/rippy");
        assert!(script.contains("iTerm2"), "script must target iTerm2");
        assert!(script.contains("/usr/local/bin/rippy"), "script must include binary path");
        assert!(script.contains("; exit"), "script must exit after TUI closes");
    }

    #[test]
    fn build_launch_script_iterm2_case_variants() {
        for name in ["iTerm2", "iterm2", "iterm"] {
            let script = build_launch_script(name, "/bin/rippy");
            assert!(script.contains("iTerm2"), "{name} should generate iTerm2 script");
        }
    }

    #[test]
    fn build_launch_script_terminal_is_default() {
        let script = build_launch_script("Terminal", "/bin/rippy");
        assert!(script.contains(r#"tell application "Terminal""#));
        assert!(script.contains("/bin/rippy"));
    }

    #[test]
    fn build_launch_script_unknown_falls_back_to_terminal() {
        let script = build_launch_script("SomeUnknownTerminal", "/bin/rippy");
        assert!(script.contains(r#"tell application "Terminal""#));
    }

    #[test]
    fn build_launch_script_alacritty() {
        let script = build_launch_script("Alacritty", "/bin/rippy");
        assert!(script.contains("open -a Alacritty"));
        assert!(script.contains("/bin/rippy"));
    }

    #[test]
    fn build_launch_script_wezterm() {
        let script = build_launch_script("WezTerm", "/bin/rippy");
        assert!(script.contains("open -a WezTerm"));
        assert!(script.contains("/bin/rippy"));
    }

    #[test]
    fn build_launch_script_binary_path_with_spaces() {
        let script = build_launch_script("Terminal", "/Users/me/my apps/rippy");
        assert!(script.contains("/Users/me/my apps/rippy"));
    }

    #[test]
    fn end_to_end_config_to_script() {
        // Simulate the full flow: config terminal preference → detect → build script
        let terminal = detect_terminal("auto");
        let script = build_launch_script(terminal, "/usr/local/bin/rippy");
        // Should produce a valid-looking osascript regardless of which terminal is detected
        assert!(
            script.contains("tell application") || script.contains("do shell script"),
            "script must be valid AppleScript"
        );
    }
}

fn rippy_binary_path() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.canonicalize().ok())
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "rippy".into())
}
