use cdt::scanner::{find_workspace, summarize, Workspace};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

fn ws(project: &str, name: &str, merged: Option<bool>, dirty: bool) -> Workspace {
    Workspace {
        project: project.into(),
        name: name.into(),
        path: PathBuf::from(format!("/ws/{project}/{name}")),
        merged,
        branch: Some(format!("feat-{name}")),
        last_commit: Some(SystemTime::now() - Duration::from_secs(3600)),
        dirty,
        pr: None,
    }
}

// --- find_workspace ---

#[test]
fn find_workspace_exact_label() {
    let workspaces = vec![
        ws("proj-a", "london", Some(false), false),
        ws("proj-b", "london", Some(false), false),
    ];
    let found = find_workspace(&workspaces, "proj-a/london").unwrap();
    assert_eq!(found.project, "proj-a");
}

#[test]
fn find_workspace_by_name_unique() {
    let workspaces = vec![
        ws("proj", "london", Some(false), false),
        ws("proj", "paris", Some(false), false),
    ];
    let found = find_workspace(&workspaces, "paris").unwrap();
    assert_eq!(found.name, "paris");
}

#[test]
fn find_workspace_ambiguous() {
    let workspaces = vec![
        ws("proj-a", "london", Some(false), false),
        ws("proj-b", "london", Some(false), false),
    ];
    let err = find_workspace(&workspaces, "london").unwrap_err();
    assert!(
        err.contains("ambiguous"),
        "expected ambiguous error, got: {err}"
    );
    assert!(err.contains("proj-a/london"));
    assert!(err.contains("proj-b/london"));
}

#[test]
fn find_workspace_not_found() {
    let workspaces = vec![ws("proj", "london", Some(false), false)];
    let err = find_workspace(&workspaces, "tokyo").unwrap_err();
    assert!(
        err.contains("no workspace"),
        "expected not-found error, got: {err}"
    );
}

#[test]
fn find_workspace_empty_list() {
    let err = find_workspace(&[], "anything").unwrap_err();
    assert!(err.contains("no workspace"));
}

// --- summarize ---

#[test]
fn summarize_mixed_statuses() {
    let workspaces = vec![
        ws("proj", "a", Some(false), false),
        ws("proj", "b", Some(false), false),
        ws("proj", "c", Some(true), false),
        ws("proj", "d", None, true),
    ];
    let summary = summarize(&workspaces);
    assert!(summary.contains("4 workspace(s)"), "got: {summary}");
    assert!(summary.contains("2 open"));
    assert!(summary.contains("1 merged"));
    assert!(summary.contains("1 dirty"));
    assert!(summary.contains("1 unknown"));
}

#[test]
fn summarize_all_open() {
    let workspaces = vec![
        ws("proj", "a", Some(false), false),
        ws("proj", "b", Some(false), false),
    ];
    let summary = summarize(&workspaces);
    assert!(summary.contains("2 open"), "got: {summary}");
    assert!(!summary.contains("merged"));
    assert!(!summary.contains("dirty"));
}

#[test]
fn summarize_empty() {
    assert_eq!(summarize(&[]), "No workspaces.");
}

#[test]
fn summarize_dirty_counted_separately() {
    // A workspace can be both open and dirty — dirty is an orthogonal count
    let workspaces = vec![
        ws("proj", "a", Some(false), true),
        ws("proj", "b", Some(true), true),
    ];
    let summary = summarize(&workspaces);
    assert!(summary.contains("2 dirty"), "got: {summary}");
    assert!(summary.contains("1 open"));
    assert!(summary.contains("1 merged"));
}
