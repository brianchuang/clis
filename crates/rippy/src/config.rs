use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Config {
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
    #[serde(default)]
    pub history: HistoryConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HistoryConfig {
    /// Maximum number of entries to keep. Oldest entries are pruned on insert.
    #[serde(default = "default_max_entries")]
    pub max_entries: usize,
    /// Auto-delete entries whose clipboard content disappears within this many seconds
    /// (e.g. password manager copies). 0 = disabled.
    #[serde(default)]
    pub auto_expire_seconds: u64,
}

fn default_max_entries() -> usize {
    10_000
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            max_entries: default_max_entries(),
            auto_expire_seconds: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_key")]
    pub key: String,
    #[serde(default = "default_modifiers")]
    pub modifiers: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TerminalConfig {
    #[serde(default = "default_app")]
    pub app: String,
}

fn default_key() -> String {
    "v".into()
}
fn default_modifiers() -> Vec<String> {
    vec!["cmd".into(), "shift".into()]
}
fn default_app() -> String {
    "auto".into()
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            key: default_key(),
            modifiers: default_modifiers(),
        }
    }
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self { app: default_app() }
    }
}

impl Config {
    pub fn path(data_dir: &Path) -> PathBuf {
        data_dir.join("config.toml")
    }

    pub fn load(data_dir: &Path) -> Self {
        let path = Self::path(data_dir);
        match std::fs::read_to_string(&path) {
            Ok(contents) => toml::from_str(&contents).unwrap_or_default(),
            Err(_) => Config::default(),
        }
    }

    pub fn save(&self, data_dir: &Path) -> std::io::Result<()> {
        let path = Self::path(data_dir);
        let contents = toml::to_string_pretty(self).expect("serialize config");
        std::fs::write(path, contents)
    }
}

/// Map a human-readable key name to a macOS virtual keycode.
pub fn keycode_for(name: &str) -> Option<u16> {
    Some(match name.to_lowercase().as_str() {
        "a" => 0x00,
        "s" => 0x01,
        "d" => 0x02,
        "f" => 0x03,
        "h" => 0x04,
        "g" => 0x05,
        "z" => 0x06,
        "x" => 0x07,
        "c" => 0x08,
        "v" => 0x09,
        "b" => 0x0B,
        "q" => 0x0C,
        "w" => 0x0D,
        "e" => 0x0E,
        "r" => 0x0F,
        "y" => 0x10,
        "t" => 0x11,
        "1" => 0x12,
        "2" => 0x13,
        "3" => 0x14,
        "4" => 0x15,
        "6" => 0x16,
        "5" => 0x17,
        "9" => 0x19,
        "7" => 0x1A,
        "8" => 0x1C,
        "0" => 0x1D,
        "o" => 0x1F,
        "u" => 0x20,
        "i" => 0x22,
        "p" => 0x23,
        "l" => 0x25,
        "j" => 0x26,
        "k" => 0x28,
        "n" => 0x2D,
        "m" => 0x2E,
        "space" => 0x31,
        "escape" | "esc" => 0x35,
        "f1" => 0x7A,
        "f2" => 0x78,
        "f3" => 0x63,
        "f4" => 0x76,
        "f5" => 0x60,
        "f6" => 0x61,
        "f7" => 0x62,
        "f8" => 0x64,
        "f9" => 0x65,
        "f10" => 0x6D,
        "f11" => 0x67,
        "f12" => 0x6F,
        _ => return None,
    })
}

/// Map modifier name to CGEventFlags bitmask.
pub fn modifier_flag(name: &str) -> Option<u64> {
    Some(match name.to_lowercase().as_str() {
        "cmd" | "command" => 1 << 20,        // kCGEventFlagMaskCommand
        "shift" => 1 << 17,                  // kCGEventFlagMaskShift
        "ctrl" | "control" => 1 << 18,       // kCGEventFlagMaskControl
        "alt" | "option" | "opt" => 1 << 19, // kCGEventFlagMaskAlternate
        _ => return None,
    })
}

/// Combine modifier names into a single bitmask.
pub fn modifiers_mask(names: &[String]) -> u64 {
    names
        .iter()
        .filter_map(|n| modifier_flag(n))
        .fold(0, |acc, f| acc | f)
}

/// Convert a HotkeyConfig to a macOS pbs key equivalent string for
/// assigning keyboard shortcuts to Services via `defaults write pbs`.
/// Format: modifier symbols followed by lowercase key.
/// Symbols: @ = Cmd, $ = Shift, ^ = Ctrl, ~ = Option
pub fn pbs_key_equivalent(cfg: &HotkeyConfig) -> String {
    let mut eq = String::new();
    for m in &cfg.modifiers {
        match m.to_lowercase().as_str() {
            "cmd" | "command" => eq.push('@'),
            "shift" => eq.push('$'),
            "ctrl" | "control" => eq.push('^'),
            "alt" | "option" | "opt" => eq.push('~'),
            _ => {}
        }
    }
    eq.push_str(&cfg.key.to_lowercase());
    eq
}

/// Format a hotkey config as a human-readable string.
pub fn format_hotkey(cfg: &HotkeyConfig) -> String {
    let mods: Vec<&str> = cfg
        .modifiers
        .iter()
        .map(|m| match m.as_str() {
            "cmd" | "command" => "Cmd",
            "shift" => "Shift",
            "ctrl" | "control" => "Ctrl",
            "alt" | "option" | "opt" => "Opt",
            other => other,
        })
        .collect();
    format!("{}+{}", mods.join("+"), cfg.key.to_uppercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_max_entries() {
        let cfg = Config::default();
        assert_eq!(cfg.history.max_entries, 10_000);
        assert_eq!(cfg.history.auto_expire_seconds, 0);
    }

    #[test]
    fn parse_config_with_history_section() {
        let toml_str = r#"
[history]
max_entries = 500
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.history.max_entries, 500);
        assert_eq!(cfg.history.auto_expire_seconds, 0);
    }

    #[test]
    fn parse_config_with_auto_expire() {
        let toml_str = r#"
[history]
max_entries = 1000
auto_expire_seconds = 15
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.history.auto_expire_seconds, 15);
    }

    #[test]
    fn parse_config_without_history_uses_default() {
        let toml_str = r#"
[hotkey]
key = "v"
"#;
        let cfg: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(cfg.history.max_entries, 10_000);
    }

    // --- keycode mapping ---

    #[test]
    fn keycode_for_all_letters() {
        for ch in 'a'..='z' {
            let name = ch.to_string();
            assert!(
                keycode_for(&name).is_some(),
                "keycode_for('{name}') should be Some"
            );
        }
    }

    #[test]
    fn keycode_for_case_insensitive() {
        assert_eq!(keycode_for("v"), keycode_for("V"));
        assert_eq!(keycode_for("f1"), keycode_for("F1"));
    }

    #[test]
    fn keycode_for_digits() {
        for d in '0'..='9' {
            let name = d.to_string();
            assert!(
                keycode_for(&name).is_some(),
                "keycode_for('{name}') should be Some"
            );
        }
    }

    #[test]
    fn keycode_for_special_keys() {
        assert!(keycode_for("space").is_some());
        assert!(keycode_for("escape").is_some());
        assert!(keycode_for("esc").is_some());
        assert_eq!(keycode_for("escape"), keycode_for("esc"));
    }

    #[test]
    fn keycode_for_function_keys() {
        for i in 1..=12 {
            let name = format!("f{i}");
            assert!(
                keycode_for(&name).is_some(),
                "keycode_for('{name}') should be Some"
            );
        }
    }

    #[test]
    fn keycode_for_unknown_returns_none() {
        assert!(keycode_for("").is_none());
        assert!(keycode_for("backspace").is_none());
        assert!(keycode_for("tab").is_none());
    }

    #[test]
    fn keycode_v_is_0x09() {
        // This specific value is what macOS sends for the 'v' key
        assert_eq!(keycode_for("v"), Some(0x09));
    }

    // --- modifier mapping ---

    #[test]
    fn modifier_flag_all_aliases() {
        // cmd aliases
        assert_eq!(modifier_flag("cmd"), modifier_flag("command"));
        assert!(modifier_flag("cmd").is_some());

        // ctrl aliases
        assert_eq!(modifier_flag("ctrl"), modifier_flag("control"));

        // alt aliases
        assert_eq!(modifier_flag("alt"), modifier_flag("option"));
        assert_eq!(modifier_flag("alt"), modifier_flag("opt"));

        assert!(modifier_flag("shift").is_some());
    }

    #[test]
    fn modifier_flag_unknown_returns_none() {
        assert!(modifier_flag("super").is_none());
        assert!(modifier_flag("").is_none());
        assert!(modifier_flag("meta").is_none());
    }

    #[test]
    fn modifier_flags_are_distinct_powers_of_two() {
        let flags: Vec<u64> = ["cmd", "shift", "ctrl", "alt"]
            .iter()
            .map(|n| modifier_flag(n).unwrap())
            .collect();
        // Each flag should be a single bit
        for &f in &flags {
            assert_eq!(
                f.count_ones(),
                1,
                "modifier flag {f:#x} should be a single bit"
            );
        }
        // All flags should be distinct
        for i in 0..flags.len() {
            for j in (i + 1)..flags.len() {
                assert_ne!(flags[i], flags[j], "modifier flags should be distinct");
            }
        }
    }

    // --- modifiers_mask ---

    #[test]
    fn modifiers_mask_empty() {
        assert_eq!(modifiers_mask(&[]), 0);
    }

    #[test]
    fn modifiers_mask_combines_flags() {
        let mask = modifiers_mask(&["cmd".into(), "shift".into()]);
        let expected = modifier_flag("cmd").unwrap() | modifier_flag("shift").unwrap();
        assert_eq!(mask, expected);
    }

    #[test]
    fn modifiers_mask_ignores_unknown() {
        let mask = modifiers_mask(&["cmd".into(), "bogus".into()]);
        assert_eq!(mask, modifier_flag("cmd").unwrap());
    }

    // --- pbs_key_equivalent ---

    #[test]
    fn pbs_key_equivalent_default_cmd_shift_v() {
        let cfg = HotkeyConfig::default();
        assert_eq!(pbs_key_equivalent(&cfg), "@$v");
    }

    #[test]
    fn pbs_key_equivalent_ctrl_alt() {
        let cfg = HotkeyConfig {
            key: "f5".into(),
            modifiers: vec!["ctrl".into(), "alt".into()],
        };
        assert_eq!(pbs_key_equivalent(&cfg), "^~f5");
    }

    #[test]
    fn pbs_key_equivalent_no_modifiers() {
        let cfg = HotkeyConfig {
            key: "space".into(),
            modifiers: vec![],
        };
        assert_eq!(pbs_key_equivalent(&cfg), "space");
    }

    #[test]
    fn pbs_key_equivalent_aliases() {
        let cfg = HotkeyConfig {
            key: "a".into(),
            modifiers: vec!["command".into(), "option".into(), "control".into()],
        };
        // command → @, option → ~, control → ^
        assert_eq!(pbs_key_equivalent(&cfg), "@~^a");
    }

    // --- format_hotkey ---

    #[test]
    fn format_hotkey_default() {
        let cfg = HotkeyConfig::default();
        assert_eq!(format_hotkey(&cfg), "Cmd+Shift+V");
    }

    #[test]
    fn format_hotkey_single_modifier() {
        let cfg = HotkeyConfig {
            key: "a".into(),
            modifiers: vec!["ctrl".into()],
        };
        assert_eq!(format_hotkey(&cfg), "Ctrl+A");
    }

    #[test]
    fn format_hotkey_all_modifiers() {
        let cfg = HotkeyConfig {
            key: "f5".into(),
            modifiers: vec!["cmd".into(), "shift".into(), "ctrl".into(), "alt".into()],
        };
        assert_eq!(format_hotkey(&cfg), "Cmd+Shift+Ctrl+Opt+F5");
    }

    // --- config persistence roundtrip ---

    #[test]
    fn config_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config {
            hotkey: HotkeyConfig {
                key: "f1".into(),
                modifiers: vec!["ctrl".into(), "alt".into()],
            },
            terminal: TerminalConfig {
                app: "Alacritty".into(),
            },
            history: HistoryConfig {
                max_entries: 500,
                auto_expire_seconds: 15,
            },
        };
        cfg.save(tmp.path()).unwrap();
        let loaded = Config::load(tmp.path());
        assert_eq!(loaded.hotkey.key, "f1");
        assert_eq!(loaded.hotkey.modifiers, vec!["ctrl", "alt"]);
        assert_eq!(loaded.terminal.app, "Alacritty");
        assert_eq!(loaded.history.max_entries, 500);
        assert_eq!(loaded.history.auto_expire_seconds, 15);
    }

    #[test]
    fn config_load_missing_file_returns_defaults() {
        let tmp = tempfile::tempdir().unwrap();
        let cfg = Config::load(tmp.path());
        assert_eq!(cfg.hotkey.key, "v");
        assert_eq!(cfg.hotkey.modifiers, vec!["cmd", "shift"]);
        assert_eq!(cfg.terminal.app, "auto");
    }

    #[test]
    fn full_hotkey_config_to_bitmask_roundtrip() {
        // The default Cmd+Shift+V config should produce a mask that,
        // when combined with the correct keycode, matches a simulated event.
        let cfg = HotkeyConfig::default();
        let keycode = keycode_for(&cfg.key).unwrap();
        let mask = modifiers_mask(&cfg.modifiers);

        assert_eq!(keycode, 0x09); // 'v'
        assert_eq!(mask, (1 << 20) | (1 << 17)); // cmd | shift
        assert_eq!(format_hotkey(&cfg), "Cmd+Shift+V");
    }
}
