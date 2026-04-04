use std::fs;
use std::path::{Path, PathBuf};

use crate::types::VaultConfig;

fn default_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".config")
        .join("learn")
        .join("config.json")
}

pub fn resolve_vault_path(flag_path: Option<&str>) -> Result<PathBuf, String> {
    // 1. --vault flag
    if let Some(p) = flag_path {
        let resolved = PathBuf::from(p).canonicalize().map_err(|_| {
            format!("Vault path does not exist: {p}")
        })?;
        return Ok(resolved);
    }

    // 2. LEARN_VAULT env var
    if let Ok(env_path) = std::env::var("LEARN_VAULT") {
        let resolved = PathBuf::from(&env_path).canonicalize().map_err(|_| {
            format!("Vault path from LEARN_VAULT does not exist: {env_path}")
        })?;
        return Ok(resolved);
    }

    // 3. ~/.config/learn/config.json
    let config_path = default_config_path();
    if config_path.exists() {
        let raw = fs::read_to_string(&config_path)
            .map_err(|e| format!("Failed to read config: {e}"))?;
        let config: serde_json::Value =
            serde_json::from_str(&raw).map_err(|e| format!("Invalid config JSON: {e}"))?;
        if let Some(vault_path) = config.get("vaultPath").and_then(|v| v.as_str()) {
            let resolved = PathBuf::from(vault_path).canonicalize().map_err(|_| {
                format!("Vault path from config does not exist: {vault_path}")
            })?;
            return Ok(resolved);
        }
    }

    Err("No vault configured. Use --vault, set LEARN_VAULT, or run learn init.".into())
}

pub fn write_config_pointer(vault_path: &Path) -> Result<(), String> {
    let config_path = default_config_path();
    if let Some(parent) = config_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| format!("Failed to create config dir: {e}"))?;
    }
    let abs = fs::canonicalize(vault_path)
        .unwrap_or_else(|_| vault_path.to_path_buf());
    let json = serde_json::json!({ "vaultPath": abs.to_string_lossy() });
    let content = serde_json::to_string_pretty(&json).unwrap() + "\n";
    fs::write(&config_path, content)
        .map_err(|e| format!("Failed to write config: {e}"))?;
    Ok(())
}

pub fn load_vault_config(vault_root: &Path) -> VaultConfig {
    let config_path = vault_root.join(".learning-system").join("config.json");
    if !config_path.exists() {
        return VaultConfig::default();
    }
    let raw = match fs::read_to_string(&config_path) {
        Ok(r) => r,
        Err(_) => return VaultConfig::default(),
    };
    let json: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return VaultConfig::default(),
    };
    VaultConfig {
        default_review_count: json
            .get("defaultReviewCount")
            .and_then(|v| v.as_u64())
            .unwrap_or(5) as usize,
        default_domain: json
            .get("defaultDomain")
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}
