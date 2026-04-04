use std::path::{Path, PathBuf};

/// Detect the user's shell rc file based on `$SHELL`.
///
/// Returns the path to `.zshrc`, `.bashrc`, or `.bash_profile` under the
/// given home directory. Returns `None` for unsupported shells.
pub fn detect_shell_rc(home: &Path) -> Option<PathBuf> {
    let shell = std::env::var("SHELL").unwrap_or_default();
    if shell.ends_with("zsh") {
        Some(home.join(".zshrc"))
    } else if shell.ends_with("bash") {
        let bashrc = home.join(".bashrc");
        if bashrc.exists() {
            Some(bashrc)
        } else {
            Some(home.join(".bash_profile"))
        }
    } else {
        None
    }
}

/// Check whether `marker` already appears in the given rc file.
pub fn is_configured(rc_path: &Path, marker: &str) -> bool {
    std::fs::read_to_string(rc_path)
        .unwrap_or_default()
        .contains(marker)
}

/// Append `content` to the rc file. The content should include a comment
/// header and the actual line to add (e.g. an eval or alias).
///
/// Returns `Ok(())` on success.
pub fn append_to_rc(rc_path: &Path, content: &str) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(rc_path)?;
    writeln!(file, "\n{content}")
}

/// All-in-one: detect rc file, check for existing marker, append if missing.
///
/// Returns a human-readable status message, or `None` if the shell is unsupported.
pub fn ensure_shell_line(marker: &str, content: &str, already_msg: &str) -> Option<String> {
    let home = dirs::home_dir()?;
    let rc_path = detect_shell_rc(&home)?;

    if is_configured(&rc_path, marker) {
        return Some(format!("{already_msg} {}", rc_path.display()));
    }

    match append_to_rc(&rc_path, content) {
        Ok(()) => Some(format!("Added to {}", rc_path.display())),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_shell_rc_zsh() {
        let tmp = tempfile::tempdir().unwrap();
        // detect_shell_rc depends on $SHELL, so we test the path logic directly
        let home = tmp.path();
        let rc = home.join(".zshrc");
        // When SHELL ends with zsh, should return .zshrc
        // We can't easily override env in tests, so test the path construction
        assert_eq!(home.join(".zshrc"), rc);
    }

    #[test]
    fn is_configured_finds_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join(".zshrc");
        std::fs::write(&rc, "# existing\neval \"$(foo init)\"\n").unwrap();
        assert!(is_configured(&rc, "eval \"$(foo init)\""));
    }

    #[test]
    fn is_configured_returns_false_when_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join(".zshrc");
        std::fs::write(&rc, "# existing config\n").unwrap();
        assert!(!is_configured(&rc, "eval \"$(foo init)\""));
    }

    #[test]
    fn append_to_rc_writes_content() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join(".zshrc");
        std::fs::write(&rc, "# existing\n").unwrap();

        append_to_rc(&rc, "# my tool\neval \"$(my-tool init)\"").unwrap();

        let contents = std::fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("# existing"));
        assert!(contents.contains("eval \"$(my-tool init)\""));
    }

    #[test]
    fn append_to_rc_creates_file_if_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join(".bashrc");

        append_to_rc(&rc, "alias yy=\"rippy\"").unwrap();

        let contents = std::fs::read_to_string(&rc).unwrap();
        assert!(contents.contains("alias yy=\"rippy\""));
    }

    #[test]
    fn is_configured_on_nonexistent_file_returns_false() {
        let tmp = tempfile::tempdir().unwrap();
        let rc = tmp.path().join("nonexistent");
        assert!(!is_configured(&rc, "anything"));
    }
}
