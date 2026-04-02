use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
#[derive(Default)]
pub struct Config {
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub terminal: TerminalConfig,
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

fn default_key() -> String { "v".into() }
fn default_modifiers() -> Vec<String> { vec!["cmd".into(), "shift".into()] }
fn default_app() -> String { "auto".into() }

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self { key: default_key(), modifiers: default_modifiers() }
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
        "a" => 0x00, "s" => 0x01, "d" => 0x02, "f" => 0x03,
        "h" => 0x04, "g" => 0x05, "z" => 0x06, "x" => 0x07,
        "c" => 0x08, "v" => 0x09, "b" => 0x0B, "q" => 0x0C,
        "w" => 0x0D, "e" => 0x0E, "r" => 0x0F, "y" => 0x10,
        "t" => 0x11, "1" => 0x12, "2" => 0x13, "3" => 0x14,
        "4" => 0x15, "6" => 0x16, "5" => 0x17, "9" => 0x19,
        "7" => 0x1A, "8" => 0x1C, "0" => 0x1D, "o" => 0x1F,
        "u" => 0x20, "i" => 0x22, "p" => 0x23, "l" => 0x25,
        "j" => 0x26, "k" => 0x28, "n" => 0x2D, "m" => 0x2E,
        "space" => 0x31, "escape" | "esc" => 0x35,
        "f1" => 0x7A, "f2" => 0x78, "f3" => 0x63, "f4" => 0x76,
        "f5" => 0x60, "f6" => 0x61, "f7" => 0x62, "f8" => 0x64,
        "f9" => 0x65, "f10" => 0x6D, "f11" => 0x67, "f12" => 0x6F,
        _ => return None,
    })
}

/// Map modifier name to CGEventFlags bitmask.
pub fn modifier_flag(name: &str) -> Option<u64> {
    Some(match name.to_lowercase().as_str() {
        "cmd" | "command" => 1 << 20,   // kCGEventFlagMaskCommand
        "shift" => 1 << 17,             // kCGEventFlagMaskShift
        "ctrl" | "control" => 1 << 18,  // kCGEventFlagMaskControl
        "alt" | "option" | "opt" => 1 << 19, // kCGEventFlagMaskAlternate
        _ => return None,
    })
}

/// Combine modifier names into a single bitmask.
pub fn modifiers_mask(names: &[String]) -> u64 {
    names.iter().filter_map(|n| modifier_flag(n)).fold(0, |acc, f| acc | f)
}

/// Format a hotkey config as a human-readable string.
pub fn format_hotkey(cfg: &HotkeyConfig) -> String {
    let mods: Vec<&str> = cfg.modifiers.iter().map(|m| match m.as_str() {
        "cmd" | "command" => "Cmd",
        "shift" => "Shift",
        "ctrl" | "control" => "Ctrl",
        "alt" | "option" | "opt" => "Opt",
        other => other,
    }).collect();
    format!("{}+{}", mods.join("+"), cfg.key.to_uppercase())
}
