use crate::scanner::Workspace;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime};

/// Reasons a workspace was selected for removal.
#[derive(Debug, Clone, PartialEq)]
pub enum CleanReason {
    Merged,
    Stale(Duration),
}

/// A workspace tagged with the reason it's a candidate for removal.
#[derive(Debug, Clone)]
pub struct CleanCandidate<'a> {
    pub workspace: &'a Workspace,
    pub reason: CleanReason,
}

/// Result of attempting to remove a single worktree.
#[derive(Debug)]
pub enum RemoveResult {
    Removed,
    Failed(String),
}

/// Filter workspaces to only those whose branch has been merged.
pub fn find_merged(workspaces: &[Workspace]) -> Vec<CleanCandidate<'_>> {
    workspaces
        .iter()
        .filter(|ws| ws.merged == Some(true))
        .map(|ws| CleanCandidate {
            workspace: ws,
            reason: CleanReason::Merged,
        })
        .collect()
}

/// Filter workspaces with no commit activity in at least `threshold`.
pub fn find_stale(workspaces: &[Workspace], threshold: Duration) -> Vec<CleanCandidate<'_>> {
    let now = SystemTime::now();
    workspaces
        .iter()
        .filter_map(|ws| {
            let age = ws.last_commit.and_then(|t| now.duration_since(t).ok())?;
            if age >= threshold {
                Some(CleanCandidate {
                    workspace: ws,
                    reason: CleanReason::Stale(age),
                })
            } else {
                None
            }
        })
        .collect()
}

/// Combined filter: merged OR stale (deduplicating, preferring Merged reason).
pub fn find_candidates<'a>(
    workspaces: &'a [Workspace],
    include_merged: bool,
    stale_threshold: Option<Duration>,
) -> Vec<CleanCandidate<'a>> {
    let mut seen = std::collections::HashSet::new();
    let mut candidates = Vec::new();

    if include_merged {
        for c in find_merged(workspaces) {
            seen.insert(c.workspace.path.clone());
            candidates.push(c);
        }
    }

    if let Some(threshold) = stale_threshold {
        for c in find_stale(workspaces, threshold) {
            if seen.insert(c.workspace.path.clone()) {
                candidates.push(c);
            }
        }
    }

    candidates
}

/// Format a `CleanCandidate` as a human-readable line.
pub fn format_candidate(c: &CleanCandidate<'_>) -> String {
    let d = c.workspace.display_columns();
    let dirty_marker = if d.dirty { " [dirty]" } else { "" };
    let reason = match &c.reason {
        CleanReason::Merged => "merged".to_string(),
        CleanReason::Stale(age) => format!("stale ({})", crate::scanner::format_age(*age)),
    };
    format!(
        "{:<24} {:<24} {:<14}{}",
        c.workspace.label(),
        d.branch,
        reason,
        dirty_marker,
    )
}

/// Find the parent git repository for a worktree by looking for
/// `.git` being a file (worktree link) rather than a directory.
fn find_parent_repo(ws_path: &Path) -> Option<std::path::PathBuf> {
    let git_path = ws_path.join(".git");
    if git_path.is_file() {
        // Worktree — .git is a file containing "gitdir: /path/to/repo/.git/worktrees/<name>"
        let content = std::fs::read_to_string(&git_path).ok()?;
        let gitdir = content.strip_prefix("gitdir: ")?.trim();
        // Walk up from .git/worktrees/<name> to the repo root
        let p = std::path::Path::new(gitdir);
        // .git/worktrees/<name> -> .git -> repo
        p.parent()?.parent()?.parent().map(|p| p.to_path_buf())
    } else if git_path.is_dir() {
        // Regular repo — parent is the repo itself
        Some(ws_path.to_path_buf())
    } else {
        None
    }
}

/// Remove a worktree via `git worktree remove`, falling back to directory removal.
pub fn remove_worktree(ws: &Workspace) -> RemoveResult {
    // Try `git worktree remove` from the parent repo
    if let Some(parent_repo) = find_parent_repo(&ws.path) {
        let output = Command::new("git")
            .args(["worktree", "remove", "--force"])
            .arg(&ws.path)
            .current_dir(&parent_repo)
            .output();

        match output {
            Ok(o) if o.status.success() => return RemoveResult::Removed,
            _ => {} // fall through to directory removal
        }
    }

    // Fallback: just remove the directory
    match std::fs::remove_dir_all(&ws.path) {
        Ok(()) => RemoveResult::Removed,
        Err(e) => RemoveResult::Failed(format!("failed to remove {}: {e}", ws.path.display())),
    }
}

/// Parse a human-friendly duration string like "7d", "24h", "30m".
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("empty duration".to_string());
    }

    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str
        .parse()
        .map_err(|_| format!("invalid duration: {s}"))?;

    let secs = match unit {
        "s" => num,
        "m" => num * 60,
        "h" => num * 3600,
        "d" => num * 86400,
        "w" => num * 86400 * 7,
        _ => return Err(format!("unknown unit '{unit}', expected s/m/h/d/w")),
    };

    Ok(Duration::from_secs(secs))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    fn ws(name: &str, merged: Option<bool>, age_days: Option<u64>, dirty: bool) -> Workspace {
        Workspace {
            project: "proj".into(),
            name: name.into(),
            path: PathBuf::from(format!("/ws/proj/{name}")),
            merged,
            branch: Some(format!("feat-{name}")),
            last_commit: age_days.map(|d| SystemTime::now() - Duration::from_secs(d * 86400)),
            dirty,
            pr: None,
        }
    }

    #[test]
    fn find_merged_filters_correctly() {
        let workspaces = vec![
            ws("a", Some(true), Some(1), false),
            ws("b", Some(false), Some(1), false),
            ws("c", None, Some(1), false),
            ws("d", Some(true), Some(1), true),
        ];
        let candidates = find_merged(&workspaces);
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].workspace.name, "a");
        assert_eq!(candidates[1].workspace.name, "d");
        assert!(candidates.iter().all(|c| c.reason == CleanReason::Merged));
    }

    #[test]
    fn find_stale_uses_threshold() {
        let workspaces = vec![
            ws("fresh", Some(false), Some(1), false),    // 1 day old
            ws("old", Some(false), Some(10), false),     // 10 days old
            ws("ancient", Some(false), Some(30), false), // 30 days old
            ws("nocommit", Some(false), None, false),    // no last_commit
        ];
        let candidates = find_stale(&workspaces, Duration::from_secs(7 * 86400));
        assert_eq!(candidates.len(), 2);
        assert_eq!(candidates[0].workspace.name, "old");
        assert_eq!(candidates[1].workspace.name, "ancient");
    }

    #[test]
    fn find_candidates_deduplicates() {
        let workspaces = vec![
            ws("both", Some(true), Some(30), false), // merged AND stale
            ws("only_merged", Some(true), Some(1), false),
            ws("only_stale", Some(false), Some(30), false),
            ws("neither", Some(false), Some(1), false),
        ];
        let candidates = find_candidates(&workspaces, true, Some(Duration::from_secs(7 * 86400)));
        assert_eq!(candidates.len(), 3);
        // "both" should appear once with Merged reason (takes priority)
        let both = candidates
            .iter()
            .find(|c| c.workspace.name == "both")
            .unwrap();
        assert_eq!(both.reason, CleanReason::Merged);
    }

    #[test]
    fn find_candidates_merged_only() {
        let workspaces = vec![
            ws("m", Some(true), Some(1), false),
            ws("o", Some(false), Some(30), false),
        ];
        let candidates = find_candidates(&workspaces, true, None);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].workspace.name, "m");
    }

    #[test]
    fn find_candidates_stale_only() {
        let workspaces = vec![
            ws("m", Some(true), Some(1), false),
            ws("o", Some(false), Some(30), false),
        ];
        let candidates = find_candidates(&workspaces, false, Some(Duration::from_secs(7 * 86400)));
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].workspace.name, "o");
    }

    #[test]
    fn find_candidates_no_filters_returns_empty() {
        let workspaces = vec![ws("a", Some(true), Some(30), false)];
        let candidates = find_candidates(&workspaces, false, None);
        assert!(candidates.is_empty());
    }

    #[test]
    fn format_candidate_merged() {
        let w = ws("city", Some(true), Some(1), false);
        let c = CleanCandidate {
            workspace: &w,
            reason: CleanReason::Merged,
        };
        let line = format_candidate(&c);
        assert!(line.contains("proj/city"));
        assert!(line.contains("merged"));
        assert!(!line.contains("[dirty]"));
    }

    #[test]
    fn format_candidate_stale_dirty() {
        let w = ws("city", Some(false), Some(10), true);
        let c = CleanCandidate {
            workspace: &w,
            reason: CleanReason::Stale(Duration::from_secs(10 * 86400)),
        };
        let line = format_candidate(&c);
        assert!(line.contains("stale"));
        assert!(line.contains("[dirty]"));
    }

    #[test]
    fn parse_duration_valid() {
        assert_eq!(
            parse_duration("7d").unwrap(),
            Duration::from_secs(7 * 86400)
        );
        assert_eq!(
            parse_duration("24h").unwrap(),
            Duration::from_secs(24 * 3600)
        );
        assert_eq!(parse_duration("30m").unwrap(), Duration::from_secs(30 * 60));
        assert_eq!(
            parse_duration("2w").unwrap(),
            Duration::from_secs(14 * 86400)
        );
        assert_eq!(parse_duration("60s").unwrap(), Duration::from_secs(60));
    }

    #[test]
    fn parse_duration_invalid() {
        assert!(parse_duration("").is_err());
        assert!(parse_duration("abc").is_err());
        assert!(parse_duration("7x").is_err());
        assert!(parse_duration("d").is_err());
    }
}
