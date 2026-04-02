use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn create_workspace_tree(root: &std::path::Path, projects: &[(&str, &[&str])]) {
    for (project, workspaces) in projects {
        for ws in *workspaces {
            fs::create_dir_all(root.join(project).join(ws)).unwrap();
        }
    }
}

#[test]
fn scan_finds_two_level_workspaces() {
    let tmp = TempDir::new().unwrap();
    create_workspace_tree(tmp.path(), &[
        ("black-pearl", &["memphis", "tokyo", "warsaw"]),
        ("my-app", &["london", "paris"]),
    ]);

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert_eq!(workspaces.len(), 5);

    let labels: Vec<String> = workspaces.iter().map(|w| w.label()).collect();
    assert_eq!(labels, vec![
        "black-pearl/memphis",
        "black-pearl/tokyo",
        "black-pearl/warsaw",
        "my-app/london",
        "my-app/paris",
    ]);
}

#[test]
fn scan_returns_correct_paths() {
    let tmp = TempDir::new().unwrap();
    create_workspace_tree(tmp.path(), &[("proj", &["city"])]);

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].project, "proj");
    assert_eq!(workspaces[0].name, "city");
    assert_eq!(workspaces[0].path, tmp.path().join("proj").join("city"));
}

#[test]
fn scan_empty_root_returns_empty() {
    let tmp = TempDir::new().unwrap();
    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert!(workspaces.is_empty());
}

#[test]
fn scan_skips_files_at_project_level() {
    let tmp = TempDir::new().unwrap();
    create_workspace_tree(tmp.path(), &[("proj", &["city"])]);
    // Create a file at the project level (should be skipped)
    fs::write(tmp.path().join("README.md"), "hello").unwrap();

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert_eq!(workspaces.len(), 1);
}

#[test]
fn scan_skips_files_at_workspace_level() {
    let tmp = TempDir::new().unwrap();
    create_workspace_tree(tmp.path(), &[("proj", &["city"])]);
    // Create a file inside a project dir (should be skipped)
    fs::write(tmp.path().join("proj").join("config.toml"), "x").unwrap();

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "city");
}

#[test]
fn scan_nonexistent_root_errors() {
    let result = cdt::scanner::scan(&PathBuf::from("/tmp/does-not-exist-cdt-test"));
    assert!(result.is_err());
}

#[test]
fn scan_results_sorted_alphabetically() {
    let tmp = TempDir::new().unwrap();
    create_workspace_tree(tmp.path(), &[
        ("zebra", &["alpha"]),
        ("alpha", &["zebra"]),
    ]);

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    let labels: Vec<String> = workspaces.iter().map(|w| w.label()).collect();
    assert_eq!(labels, vec!["alpha/zebra", "zebra/alpha"]);
}

#[test]
fn scan_project_with_no_workspace_dirs() {
    let tmp = TempDir::new().unwrap();
    // Project dir exists but has no subdirs, only a file
    fs::create_dir_all(tmp.path().join("proj")).unwrap();
    fs::write(tmp.path().join("proj").join("notes.txt"), "x").unwrap();

    let workspaces = cdt::scanner::scan(tmp.path()).unwrap();
    assert!(workspaces.is_empty());
}

#[test]
fn workspace_label_format() {
    let ws = cdt::scanner::Workspace {
        project: "black-pearl".into(),
        name: "memphis".into(),
        path: PathBuf::from("/tmp/test"),
        merged: None,
        branch: None,
        last_commit: None,
        dirty: false,
        pr: None,
    };
    assert_eq!(ws.label(), "black-pearl/memphis");
}
