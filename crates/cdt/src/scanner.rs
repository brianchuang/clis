use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Workspace {
    pub project: String,
    pub name: String,
    pub path: PathBuf,
    pub merged: Option<bool>,
    pub branch: Option<String>,
    pub last_commit: Option<SystemTime>,
    pub dirty: bool,
    #[serde(skip)]
    pub pr: Option<PrInfo>,
}

impl Workspace {
    pub fn label(&self) -> String {
        format!("{}/{}", self.project, self.name)
    }
}

/// Check if the workspace's current branch has been merged into the main branch.
/// Returns (branch_name, merged) or None if not a git repo.
fn check_merged(ws_path: &Path) -> (Option<String>, Option<bool>) {
    // Get the current branch name
    let branch = Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .current_dir(ws_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
            } else {
                None
            }
        });

    let branch_name = match &branch {
        Some(b) if b == "main" || b == "master" || b == "HEAD" => return (branch.clone(), None),
        Some(b) => b.clone(),
        None => return (None, None),
    };

    // Check if the branch has been merged into origin/main (or origin/master)
    let merged = ["origin/main", "origin/master"]
        .iter()
        .find_map(|main_ref| {
            Command::new("git")
                .args(["merge-base", "--is-ancestor", &branch_name, main_ref])
                .current_dir(ws_path)
                .output()
                .ok()
                .map(|o| o.status.success())
        });

    (branch, merged)
}

/// Get the timestamp of the last commit in a worktree.
fn last_commit_time(ws_path: &Path) -> Option<SystemTime> {
    Command::new("git")
        .args(["log", "-1", "--format=%ct"])
        .current_dir(ws_path)
        .output()
        .ok()
        .and_then(|o| {
            if o.status.success() {
                let secs: u64 = String::from_utf8_lossy(&o.stdout).trim().parse().ok()?;
                Some(UNIX_EPOCH + Duration::from_secs(secs))
            } else {
                None
            }
        })
}

/// Check if the worktree has uncommitted changes.
fn is_dirty(ws_path: &Path) -> bool {
    Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(ws_path)
        .output()
        .ok()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Format a duration as a human-readable relative time.
pub fn format_age(duration: Duration) -> String {
    let secs = duration.as_secs();
    if secs < 60 {
        format!("{}s ago", secs)
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

/// Canonical display columns for a workspace — the single source of truth
/// consumed by both `cdt ls` and the interactive TUI.
pub struct DisplayColumns {
    pub merge_status: &'static str,
    pub project: String,
    pub name: String,
    pub branch: String,
    pub age: String,
    pub dirty: bool,
    pub pr: Option<PrDisplayInfo>,
    pub path: String,
}

pub struct PrDisplayInfo {
    pub number: u32,
    pub ci_label: &'static str,
}

impl Workspace {
    pub fn display_columns(&self) -> DisplayColumns {
        let merge_status = match self.merged {
            Some(true) => "✓ merged",
            Some(false) => "● open",
            None => "—",
        };
        let branch = self.branch.as_deref().unwrap_or("—").to_string();
        let age = self
            .last_commit
            .and_then(|t| SystemTime::now().duration_since(t).ok())
            .map(format_age)
            .unwrap_or_else(|| "—".into());
        let pr = self.pr.as_ref().map(|info| PrDisplayInfo {
            number: info.number,
            ci_label: match info.ci_status {
                CiStatus::Pass => "✓ci",
                CiStatus::Fail => "✗ci",
                CiStatus::Pending => "⧗ci",
                CiStatus::Unknown => "",
            },
        });
        DisplayColumns {
            merge_status,
            project: self.project.clone(),
            name: self.name.clone(),
            branch,
            age,
            dirty: self.dirty,
            pr,
            path: self.path.display().to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct PrInfo {
    pub number: u32,
    pub ci_status: CiStatus,
}

#[derive(Debug, Clone)]
pub enum CiStatus {
    Pass,
    Fail,
    Pending,
    Unknown,
}

/// Fetch open PRs for a git repo, returning a map of branch_name -> PrInfo.
fn fetch_prs(repo_path: &Path) -> HashMap<String, PrInfo> {
    let mut map = HashMap::new();

    // gh pr list --json number,headRefName,statusCheckRollup
    let output = Command::new("gh")
        .args([
            "pr",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,headRefName,statusCheckRollup",
        ])
        .current_dir(repo_path)
        .output();

    let output = match output {
        Ok(o) if o.status.success() => o,
        _ => return map,
    };

    // Minimal JSON parsing without pulling in serde — each PR is small
    // Format: [{"headRefName":"...","number":42,"statusCheckRollup":[{"status":"COMPLETED","conclusion":"SUCCESS"}, ...]}]
    let text = String::from_utf8_lossy(&output.stdout);
    // Use a simple approach: split by objects
    for obj in text.split('{').skip(1) {
        let number = extract_json_u32(obj, "number");
        let branch = extract_json_str(obj, "headRefName");

        if let (Some(num), Some(br)) = (number, branch) {
            let ci = if obj.contains("\"conclusion\":\"FAILURE\"")
                || obj.contains("\"conclusion\":\"ERROR\"")
                || obj.contains("\"conclusion\":\"CANCELLED\"")
            {
                CiStatus::Fail
            } else if obj.contains("\"conclusion\":\"SUCCESS\"")
                || obj.contains("\"conclusion\":\"NEUTRAL\"")
            {
                CiStatus::Pass
            } else if obj.contains("\"status\":\"IN_PROGRESS\"")
                || obj.contains("\"status\":\"QUEUED\"")
                || obj.contains("\"status\":\"PENDING\"")
            {
                CiStatus::Pending
            } else {
                CiStatus::Unknown
            };

            map.insert(
                br,
                PrInfo {
                    number: num,
                    ci_status: ci,
                },
            );
        }
    }

    map
}

fn extract_json_str(obj: &str, key: &str) -> Option<String> {
    let pattern = format!("\"{}\":\"", key);
    let start = obj.find(&pattern)? + pattern.len();
    let end = obj[start..].find('"')? + start;
    Some(obj[start..end].to_string())
}

fn extract_json_u32(obj: &str, key: &str) -> Option<u32> {
    let pattern = format!("\"{}\":", key);
    let start = obj.find(&pattern)? + pattern.len();
    let rest = obj[start..].trim_start();
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    rest[..end].parse().ok()
}

/// Collect (project, name, path) tuples — cheap directory listing, no git calls.
pub fn collect_workspace_paths(
    root: &Path,
) -> Result<Vec<(String, String, PathBuf)>, Box<dyn std::error::Error>> {
    let mut entries = Vec::new();

    for project_entry in fs::read_dir(root)? {
        let project_entry = project_entry?;
        let project_path = project_entry.path();
        if !project_path.is_dir() {
            continue;
        }
        let project_name = project_entry.file_name().to_string_lossy().to_string();

        for ws_entry in fs::read_dir(&project_path)? {
            let ws_entry = ws_entry?;
            let ws_path = ws_entry.path();
            if !ws_path.is_dir() {
                continue;
            }
            let ws_name = ws_entry.file_name().to_string_lossy().to_string();
            entries.push((project_name.clone(), ws_name, ws_path));
        }
    }

    Ok(entries)
}

/// Scan ~/conductor/workspaces/<project>/<workspace> two levels deep.
/// Git calls are parallelized across workspaces using rayon.
pub fn scan(root: &Path) -> Result<Vec<Workspace>, Box<dyn std::error::Error>> {
    let entries = collect_workspace_paths(root)?;

    let mut workspaces: Vec<Workspace> = entries
        .into_par_iter()
        .map(|(project, name, ws_path)| {
            let (branch, merged) = check_merged(&ws_path);
            let last_commit = last_commit_time(&ws_path);
            let dirty = is_dirty(&ws_path);

            Workspace {
                project,
                name,
                path: ws_path,
                merged,
                branch,
                last_commit,
                dirty,
                pr: None,
            }
        })
        .collect();

    workspaces.sort_by_key(|a| a.label());
    Ok(workspaces)
}

/// Find a workspace by name. Matches `project/name` (exact) or just `name`
/// (unique prefix match). Returns `Err` if no match or ambiguous.
pub fn find_workspace<'a>(
    workspaces: &'a [Workspace],
    query: &str,
) -> Result<&'a Workspace, String> {
    // Exact match on label (project/name)
    if let Some(ws) = workspaces.iter().find(|w| w.label() == query) {
        return Ok(ws);
    }

    // Match on workspace name alone
    let matches: Vec<&Workspace> = workspaces.iter().filter(|w| w.name == query).collect();

    match matches.len() {
        0 => Err(format!("no workspace matching '{query}'")),
        1 => Ok(matches[0]),
        _ => {
            let labels: Vec<String> = matches.iter().map(|w| w.label()).collect();
            Err(format!(
                "'{query}' is ambiguous, matches: {}. Use project/name to disambiguate.",
                labels.join(", ")
            ))
        }
    }
}

/// Build a one-line summary of workspace status counts.
pub fn summarize(workspaces: &[Workspace]) -> String {
    let mut open = 0u32;
    let mut merged = 0u32;
    let mut dirty = 0u32;
    let mut unknown = 0u32;

    for ws in workspaces {
        match ws.merged {
            Some(true) => merged += 1,
            Some(false) => open += 1,
            None => unknown += 1,
        }
        if ws.dirty {
            dirty += 1;
        }
    }

    let mut parts = Vec::new();
    if open > 0 {
        parts.push(format!("{open} open"));
    }
    if merged > 0 {
        parts.push(format!("{merged} merged"));
    }
    if dirty > 0 {
        parts.push(format!("{dirty} dirty"));
    }
    if unknown > 0 {
        parts.push(format!("{unknown} unknown"));
    }

    if parts.is_empty() {
        "No workspaces.".to_string()
    } else {
        format!("{} workspace(s): {}", workspaces.len(), parts.join(", "))
    }
}

/// Enrich workspaces with PR info by querying `gh` once per project.
pub fn attach_pr_info(workspaces: &mut [Workspace]) {
    // Group by project to avoid redundant gh calls
    let projects: Vec<String> = workspaces
        .iter()
        .map(|w| w.project.clone())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();

    let mut all_prs: HashMap<(String, String), PrInfo> = HashMap::new();

    for project in &projects {
        // Use the first worktree of this project as the repo path
        if let Some(ws) = workspaces.iter().find(|w| &w.project == project) {
            let prs = fetch_prs(&ws.path);
            for (branch, info) in prs {
                all_prs.insert((project.clone(), branch), info);
            }
        }
    }

    for ws in workspaces.iter_mut() {
        if let Some(branch) = &ws.branch {
            if let Some(info) = all_prs.remove(&(ws.project.clone(), branch.clone())) {
                ws.pr = Some(info);
            }
        }
    }
}
