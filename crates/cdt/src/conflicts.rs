use crate::scanner::Workspace;
use std::collections::HashMap;
use std::path::Path;
use std::process::Command;

/// A file touched by multiple worktrees — a potential merge conflict.
#[derive(Debug, Clone)]
pub struct FileConflict {
    /// The file path (relative to repo root).
    pub file: String,
    /// Labels of workspaces that touch this file.
    pub workspaces: Vec<String>,
}

/// Get the list of files changed in a workspace's branch vs the main branch.
/// Returns file paths relative to the repo root.
pub fn changed_files(ws_path: &Path, branch: &str) -> Vec<String> {
    let main_ref = ["main", "master"]
        .iter()
        .find(|r| {
            Command::new("git")
                .args(["rev-parse", "--verify", r])
                .current_dir(ws_path)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        })
        .unwrap_or(&"main");

    Command::new("git")
        .args(["diff", "--name-only", &format!("{main_ref}...{branch}")])
        .current_dir(ws_path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Given a pre-computed map of workspace label -> changed files, find files
/// touched by two or more workspaces. Pure function, no I/O.
pub fn detect_conflicts(changes: &[(String, Vec<String>)]) -> Vec<FileConflict> {
    // file -> list of workspace labels that touch it
    let mut file_map: HashMap<&str, Vec<&str>> = HashMap::new();

    for (label, files) in changes {
        for file in files {
            file_map.entry(file.as_str()).or_default().push(label.as_str());
        }
    }

    let mut conflicts: Vec<FileConflict> = file_map
        .into_iter()
        .filter(|(_, ws)| ws.len() > 1)
        .map(|(file, ws)| FileConflict {
            file: file.to_string(),
            workspaces: ws.into_iter().map(String::from).collect(),
        })
        .collect();

    // Sort by number of conflicting workspaces (most first), then by file name
    conflicts.sort_by(|a, b| {
        b.workspaces
            .len()
            .cmp(&a.workspaces.len())
            .then_with(|| a.file.cmp(&b.file))
    });

    conflicts
}

/// Collect changed files for all open (non-merged) workspaces that have a branch.
pub fn gather_changes(workspaces: &[Workspace]) -> Vec<(String, Vec<String>)> {
    workspaces
        .iter()
        .filter(|ws| ws.merged != Some(true)) // skip already-merged
        .filter_map(|ws| {
            let branch = ws.branch.as_deref()?;
            // Skip workspaces on main/master/HEAD — nothing to diff
            if branch == "main" || branch == "master" || branch == "HEAD" {
                return None;
            }
            let files = changed_files(&ws.path, branch);
            if files.is_empty() {
                return None;
            }
            Some((ws.label(), files))
        })
        .collect()
}

/// Format conflict output for display.
pub fn format_conflicts(conflicts: &[FileConflict]) -> String {
    let mut out = String::new();

    for c in conflicts {
        out.push_str(&format!("  {} ({})\n", c.file, c.workspaces.join(", ")));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_conflicts_finds_overlapping_files() {
        let changes = vec![
            ("proj/london".to_string(), vec!["src/auth.rs".to_string(), "src/main.rs".to_string()]),
            ("proj/tokyo".to_string(), vec!["src/auth.rs".to_string(), "src/db.rs".to_string()]),
            ("proj/berlin".to_string(), vec!["src/db.rs".to_string(), "README.md".to_string()]),
        ];

        let conflicts = detect_conflicts(&changes);
        assert_eq!(conflicts.len(), 2);

        // auth.rs touched by london + tokyo
        let auth = conflicts.iter().find(|c| c.file == "src/auth.rs").unwrap();
        assert_eq!(auth.workspaces.len(), 2);
        assert!(auth.workspaces.contains(&"proj/london".to_string()));
        assert!(auth.workspaces.contains(&"proj/tokyo".to_string()));

        // db.rs touched by tokyo + berlin
        let db = conflicts.iter().find(|c| c.file == "src/db.rs").unwrap();
        assert_eq!(db.workspaces.len(), 2);
        assert!(db.workspaces.contains(&"proj/tokyo".to_string()));
        assert!(db.workspaces.contains(&"proj/berlin".to_string()));
    }

    #[test]
    fn detect_conflicts_no_overlap() {
        let changes = vec![
            ("proj/london".to_string(), vec!["src/auth.rs".to_string()]),
            ("proj/tokyo".to_string(), vec!["src/db.rs".to_string()]),
        ];

        let conflicts = detect_conflicts(&changes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_empty_input() {
        let conflicts = detect_conflicts(&[]);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_single_workspace() {
        let changes = vec![
            ("proj/london".to_string(), vec!["src/auth.rs".to_string(), "src/main.rs".to_string()]),
        ];

        let conflicts = detect_conflicts(&changes);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn detect_conflicts_three_way_overlap() {
        let changes = vec![
            ("proj/london".to_string(), vec!["src/auth.rs".to_string()]),
            ("proj/tokyo".to_string(), vec!["src/auth.rs".to_string()]),
            ("proj/berlin".to_string(), vec!["src/auth.rs".to_string()]),
        ];

        let conflicts = detect_conflicts(&changes);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].workspaces.len(), 3);
    }

    #[test]
    fn detect_conflicts_sorted_by_severity_then_name() {
        let changes = vec![
            ("proj/a".to_string(), vec!["z.rs".to_string(), "a.rs".to_string()]),
            ("proj/b".to_string(), vec!["z.rs".to_string(), "a.rs".to_string()]),
            ("proj/c".to_string(), vec!["z.rs".to_string()]),
        ];

        let conflicts = detect_conflicts(&changes);
        assert_eq!(conflicts.len(), 2);
        // z.rs (3 workspaces) should come before a.rs (2 workspaces)
        assert_eq!(conflicts[0].file, "z.rs");
        assert_eq!(conflicts[0].workspaces.len(), 3);
        assert_eq!(conflicts[1].file, "a.rs");
        assert_eq!(conflicts[1].workspaces.len(), 2);
    }

    #[test]
    fn format_conflicts_output() {
        let conflicts = vec![
            FileConflict {
                file: "src/auth.rs".to_string(),
                workspaces: vec!["proj/london".to_string(), "proj/tokyo".to_string()],
            },
        ];

        let output = format_conflicts(&conflicts);
        assert!(output.contains("src/auth.rs"));
        assert!(output.contains("proj/london"));
        assert!(output.contains("proj/tokyo"));
    }
}
