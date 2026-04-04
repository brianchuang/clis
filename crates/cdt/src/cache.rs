use crate::scanner::Workspace;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const CACHE_DIR: &str = "cdt";
const CACHE_FILE: &str = "workspaces.json";
const DEFAULT_TTL_SECS: u64 = 30;

#[derive(serde::Serialize, serde::Deserialize)]
struct CacheEnvelope {
    /// Absolute path of the workspace root that was scanned.
    root: PathBuf,
    /// When the scan completed.
    scanned_at: SystemTime,
    workspaces: Vec<Workspace>,
}

fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join(CACHE_DIR).join(CACHE_FILE))
}

/// Write scan results to disk cache.
pub fn save(root: &Path, workspaces: &[Workspace]) {
    let Some(path) = cache_path() else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let envelope = CacheEnvelope {
        root: root.to_path_buf(),
        scanned_at: SystemTime::now(),
        workspaces: workspaces.to_vec(),
    };
    if let Ok(json) = serde_json::to_string(&envelope) {
        let _ = fs::write(&path, json);
    }
}

/// Attempt to load cached workspaces.
/// Returns None if no cache, wrong root, or stale beyond TTL.
pub fn load(root: &Path) -> Option<Vec<Workspace>> {
    let path = cache_path()?;
    let data = fs::read_to_string(&path).ok()?;
    let envelope: CacheEnvelope = serde_json::from_str(&data).ok()?;

    // Must be for the same root directory.
    if envelope.root != root {
        return None;
    }

    // Check TTL.
    let age = SystemTime::now()
        .duration_since(envelope.scanned_at)
        .unwrap_or(Duration::MAX);
    if age > Duration::from_secs(DEFAULT_TTL_SECS) {
        return None;
    }

    // Quick structural check: if the set of workspace dirs on disk differs
    // from the cache, invalidate. This catches newly created or deleted
    // workspaces without running any git commands.
    if let Ok(current_paths) = crate::scanner::collect_workspace_paths(root) {
        let cached_set: HashSet<PathBuf> =
            envelope.workspaces.iter().map(|w| w.path.clone()).collect();
        let disk_set: HashSet<PathBuf> = current_paths.iter().map(|(_, _, p)| p.clone()).collect();
        if cached_set != disk_set {
            return None;
        }
    }

    Some(envelope.workspaces)
}

/// Delete the cache file.
pub fn clear() {
    if let Some(path) = cache_path() {
        let _ = fs::remove_file(&path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, UNIX_EPOCH};
    use tempfile::TempDir;

    fn make_ws(project: &str, name: &str, path: &Path) -> Workspace {
        Workspace {
            project: project.into(),
            name: name.into(),
            path: path.to_path_buf(),
            merged: Some(false),
            branch: Some("feat-x".into()),
            last_commit: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            dirty: true,
            pr: None,
        }
    }

    #[test]
    fn envelope_round_trip() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let ws = make_ws("proj", "city", &root.join("proj").join("city"));

        let envelope = CacheEnvelope {
            root: root.to_path_buf(),
            scanned_at: SystemTime::now(),
            workspaces: vec![ws],
        };

        let json = serde_json::to_string(&envelope).unwrap();
        let decoded: CacheEnvelope = serde_json::from_str(&json).unwrap();

        assert_eq!(decoded.root, root);
        assert_eq!(decoded.workspaces.len(), 1);
        assert_eq!(decoded.workspaces[0].project, "proj");
        assert_eq!(decoded.workspaces[0].name, "city");
        assert_eq!(decoded.workspaces[0].branch, Some("feat-x".into()));
        assert!(decoded.workspaces[0].dirty);
    }
}
