use cdt::cache;
use cdt::scanner::Workspace;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};
use tempfile::TempDir;

fn create_workspace_tree(root: &std::path::Path, projects: &[(&str, &[&str])]) {
    for (project, workspaces) in projects {
        for ws in *workspaces {
            fs::create_dir_all(root.join(project).join(ws)).unwrap();
        }
    }
}

fn make_ws(project: &str, name: &str, root: &std::path::Path) -> Workspace {
    Workspace {
        project: project.into(),
        name: name.into(),
        path: root.join(project).join(name),
        merged: Some(false),
        branch: Some("feat-x".into()),
        last_commit: Some(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
        dirty: true,
        pr: None,
    }
}

// All cache tests run in a single test function because they share one
// global cache file (~/.cache/cdt/workspaces.json) and cannot be parallelized.
#[test]
fn cache_operations() {
    // --- clear first to start clean ---
    cache::clear();

    // --- load returns None when no cache exists ---
    assert!(cache::load(&PathBuf::from("/nonexistent/path")).is_none());

    // --- save and load round trip ---
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    create_workspace_tree(root, &[("proj", &["city"])]);

    let workspaces = vec![make_ws("proj", "city", root)];
    cache::save(root, &workspaces);

    let loaded = cache::load(root).expect("cache should hit after save");
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].project, "proj");
    assert_eq!(loaded[0].name, "city");
    assert_eq!(loaded[0].branch, Some("feat-x".into()));
    assert!(loaded[0].dirty);

    // --- load returns None for a different root ---
    let tmp2 = TempDir::new().unwrap();
    create_workspace_tree(tmp2.path(), &[("proj", &["city"])]);
    assert!(cache::load(tmp2.path()).is_none());

    // --- invalidates when workspace added ---
    fs::create_dir_all(root.join("proj").join("town")).unwrap();
    assert!(cache::load(root).is_none(), "should invalidate when dir added");

    // Remove the extra dir and re-save for next sub-test
    fs::remove_dir_all(root.join("proj").join("town")).unwrap();
    cache::save(root, &workspaces);

    // Verify save restored correctly
    assert!(cache::load(root).is_some());

    // --- invalidates when workspace removed ---
    // Add a second workspace, save, then remove it
    create_workspace_tree(root, &[("proj", &["town"])]);
    let workspaces2 = vec![
        make_ws("proj", "city", root),
        make_ws("proj", "town", root),
    ];
    cache::save(root, &workspaces2);
    assert!(cache::load(root).is_some());

    fs::remove_dir_all(root.join("proj").join("town")).unwrap();
    assert!(cache::load(root).is_none(), "should invalidate when dir removed");

    // --- clear removes cache ---
    cache::save(root, &workspaces);
    cache::clear();
    assert!(cache::load(root).is_none(), "should be None after clear");
}
