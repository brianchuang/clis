use crate::scanner::Workspace;
use std::path::Path;
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// A single event in the cross-worktree timeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    /// Unix timestamp of the event.
    pub timestamp: u64,
    /// Workspace label (project/name).
    pub workspace: String,
    /// Kind of event.
    pub kind: EventKind,
    /// Human-readable description.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventKind {
    Commit,
    WorktreeCreated,
}

/// Gather recent git log entries from a single worktree.
/// Returns up to `max_per_ws` events, newest first.
fn gather_commits(ws_path: &Path, label: &str, max_per_ws: usize) -> Vec<Event> {
    let output = Command::new("git")
        .args([
            "log",
            &format!("--max-count={max_per_ws}"),
            "--format=%ct\t%s",
        ])
        .current_dir(ws_path)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let (ts_str, msg) = line.split_once('\t')?;
            let timestamp: u64 = ts_str.parse().ok()?;
            Some(Event {
                timestamp,
                workspace: label.to_string(),
                kind: EventKind::Commit,
                message: msg.to_string(),
            })
        })
        .collect()
}

/// Detect worktree creation time from the earliest commit or directory metadata.
fn worktree_creation_event(ws: &Workspace) -> Option<Event> {
    // Use the first commit timestamp as a proxy for worktree creation
    let output = Command::new("git")
        .args(["log", "--reverse", "--max-count=1", "--format=%ct"])
        .current_dir(&ws.path)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let ts: u64 = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse()
        .ok()?;

    Some(Event {
        timestamp: ts,
        workspace: ws.label(),
        kind: EventKind::WorktreeCreated,
        message: format!("created worktree (branch: {})", ws.branch.as_deref().unwrap_or("unknown")),
    })
}

/// Gather events from all workspaces, merge chronologically (newest first).
pub fn gather_timeline(workspaces: &[Workspace], max_per_ws: usize) -> Vec<Event> {
    let mut events: Vec<Event> = workspaces
        .iter()
        .flat_map(|ws| {
            let mut ws_events = gather_commits(&ws.path, &ws.label(), max_per_ws);
            if let Some(creation) = worktree_creation_event(ws) {
                // Only include if not already covered by a commit at the same timestamp
                if !ws_events.iter().any(|e| e.timestamp == creation.timestamp) {
                    ws_events.push(creation);
                }
            }
            ws_events
        })
        .collect();

    // Sort newest first
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events
}

/// Format a unix timestamp as HH:MM on the same day, or "Mon DD HH:MM" if older.
pub fn format_timestamp(ts: u64) -> String {
    let event_time = UNIX_EPOCH + Duration::from_secs(ts);
    let now = SystemTime::now();
    let age = now.duration_since(event_time).unwrap_or(Duration::ZERO);

    // If within last 24 hours, show just time
    if age.as_secs() < 86400 {
        let secs_today = ts % 86400;
        let hours = (secs_today / 3600) % 24;
        let minutes = (secs_today % 3600) / 60;
        // Adjust for local timezone offset
        format_local_time(ts)
            .unwrap_or_else(|| format!("{hours:02}:{minutes:02}"))
    } else {
        format_local_datetime(ts)
            .unwrap_or_else(|| crate::scanner::format_age(age))
    }
}

/// Format timestamp as local HH:MM using the `date` command.
fn format_local_time(ts: u64) -> Option<String> {
    let output = Command::new("date")
        .args(["-r", &ts.to_string(), "+%H:%M"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Format timestamp as "Mon DD HH:MM" using the `date` command.
fn format_local_datetime(ts: u64) -> Option<String> {
    let output = Command::new("date")
        .args(["-r", &ts.to_string(), "+%b %d %H:%M"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

/// Format a single timeline event as a display line.
pub fn format_event(event: &Event) -> String {
    let time = format_timestamp(event.timestamp);
    let icon = match event.kind {
        EventKind::Commit => "●",
        EventKind::WorktreeCreated => "◆",
    };
    format!("  {:<14} {:<24} {} {}", time, event.workspace, icon, event.message)
}

/// Format the full timeline for display.
pub fn format_timeline(events: &[Event], limit: usize) -> String {
    let mut out = String::new();
    for event in events.iter().take(limit) {
        out.push_str(&format_event(event));
        out.push('\n');
    }
    out
}

/// Merge and sort events by timestamp (newest first). Pure function for testing.
pub fn merge_events(mut events: Vec<Event>) -> Vec<Event> {
    events.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(ts: u64, workspace: &str, kind: EventKind, msg: &str) -> Event {
        Event {
            timestamp: ts,
            workspace: workspace.to_string(),
            kind,
            message: msg.to_string(),
        }
    }

    #[test]
    fn merge_events_sorts_newest_first() {
        let events = vec![
            make_event(1000, "proj/alpha", EventKind::Commit, "old commit"),
            make_event(3000, "proj/beta", EventKind::Commit, "newest"),
            make_event(2000, "proj/alpha", EventKind::Commit, "middle"),
        ];

        let merged = merge_events(events);
        assert_eq!(merged[0].timestamp, 3000);
        assert_eq!(merged[1].timestamp, 2000);
        assert_eq!(merged[2].timestamp, 1000);
    }

    #[test]
    fn merge_events_empty() {
        let merged = merge_events(vec![]);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_events_single() {
        let events = vec![make_event(1000, "proj/a", EventKind::Commit, "only one")];
        let merged = merge_events(events.clone());
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0], events[0]);
    }

    #[test]
    fn merge_events_preserves_different_workspaces_at_same_time() {
        let events = vec![
            make_event(1000, "proj/a", EventKind::Commit, "first"),
            make_event(1000, "proj/b", EventKind::Commit, "second"),
        ];

        let merged = merge_events(events);
        assert_eq!(merged.len(), 2);
        // Both at same timestamp — order is stable
    }

    #[test]
    fn format_event_commit() {
        let event = make_event(1700000000, "my-app/london", EventKind::Commit, "Add JWT refresh");
        let line = format_event(&event);
        assert!(line.contains("my-app/london"));
        assert!(line.contains("●"));
        assert!(line.contains("Add JWT refresh"));
    }

    #[test]
    fn format_event_worktree_created() {
        let event = make_event(1700000000, "my-app/berlin", EventKind::WorktreeCreated, "created worktree (branch: fix-typo)");
        let line = format_event(&event);
        assert!(line.contains("my-app/berlin"));
        assert!(line.contains("◆"));
        assert!(line.contains("created worktree"));
    }

    #[test]
    fn format_timeline_respects_limit() {
        let events = vec![
            make_event(3000, "proj/a", EventKind::Commit, "third"),
            make_event(2000, "proj/b", EventKind::Commit, "second"),
            make_event(1000, "proj/c", EventKind::Commit, "first"),
        ];

        let output = format_timeline(&events, 2);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 2);
    }

    #[test]
    fn format_timeline_empty() {
        let output = format_timeline(&[], 10);
        assert!(output.is_empty());
    }

    #[test]
    fn format_timeline_limit_exceeds_count() {
        let events = vec![
            make_event(2000, "proj/a", EventKind::Commit, "only"),
        ];

        let output = format_timeline(&events, 100);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 1);
    }

    #[test]
    fn event_kind_icons_are_distinct() {
        let commit = format_event(&make_event(1000, "p/a", EventKind::Commit, "msg"));
        let created = format_event(&make_event(1000, "p/a", EventKind::WorktreeCreated, "msg"));
        assert!(commit.contains("●"));
        assert!(created.contains("◆"));
        assert!(!commit.contains("◆"));
        assert!(!created.contains("●"));
    }
}
