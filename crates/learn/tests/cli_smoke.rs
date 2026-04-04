use std::fs;
use std::process::Command;

use tempfile::TempDir;

fn learn_bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_learn"))
}

#[test]
fn init_creates_vault_structure() {
    let dir = TempDir::new().unwrap();
    let output = learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Vault initialized"));
    assert!(dir.path().join("Concepts").is_dir());
    assert!(dir.path().join("Reviews").is_dir());
    assert!(dir.path().join("Templates").is_dir());
    assert!(dir.path().join(".learning-system").is_dir());
    assert!(dir.path().join("Templates/concept.md").exists());
}

#[test]
fn init_creates_claude_commands() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let commands_dir = dir.path().join(".claude/commands");
    assert!(commands_dir.is_dir());
    assert!(commands_dir.join("review-generate.md").exists());
    assert!(commands_dir.join("review-grade.md").exists());
    assert!(commands_dir.join("concept-refine.md").exists());

    let content = fs::read_to_string(commands_dir.join("review-generate.md")).unwrap();
    assert!(content.contains("learn review generate"));
}

#[test]
fn init_does_not_overwrite_existing_commands() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // User customizes a command
    let cmd_path = dir.path().join(".claude/commands/review-generate.md");
    fs::write(&cmd_path, "custom content").unwrap();

    // Re-init without --force should preserve the custom content
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let content = fs::read_to_string(&cmd_path).unwrap();
    assert_eq!(content, "custom content");
}

#[test]
fn init_force_overwrites_commands() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let cmd_path = dir.path().join(".claude/commands/review-generate.md");
    fs::write(&cmd_path, "custom content").unwrap();

    // Re-init with --force should overwrite
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    let content = fs::read_to_string(&cmd_path).unwrap();
    assert!(content.contains("learn review generate"));
}

#[test]
fn init_force_overwrites() {
    let dir = TempDir::new().unwrap();

    // First init
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Second init with --force
    let output = learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap(), "--force"])
        .output()
        .unwrap();

    assert!(output.status.success());
}

#[test]
fn concept_new_creates_note() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let output = learn_bin()
        .args([
            "concept",
            "new",
            "Test Concept",
            "--vault",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(dir.path().join("Concepts/Test Concept.md").exists());
}

#[test]
fn status_shows_due_count() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let output = learn_bin()
        .args(["status", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("Learning Status"));
    assert!(stdout.contains("Due for review:"));
}

#[test]
fn review_generate_creates_file() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Create a concept
    fs::write(
        dir.path().join("Concepts/Test.md"),
        "---\nterm: Test\n---\nSome notes.\n",
    )
    .unwrap();

    let output = learn_bin()
        .args([
            "review",
            "generate",
            "--vault",
            dir.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("concept(s) selected"));

    // Verify a review file was created in Reviews/
    let reviews: Vec<_> = fs::read_dir(dir.path().join("Reviews"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(reviews.len(), 1);
}

#[test]
fn review_grade_auto_outputs_json() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Create a concept note
    fs::write(
        dir.path().join("Concepts/Token Bucket.md"),
        "---\nterm: Token Bucket\ndomain: Systems\n---\nA token bucket controls request rate by accumulating tokens.\n",
    )
    .unwrap();

    // Create a review file with an answered but ungraded item
    fs::create_dir_all(dir.path().join("Reviews")).unwrap();
    fs::write(
        dir.path().join("Reviews/2025-01-15.md"),
        r#"---
type: review-session
date: 2025-01-15
---

# Daily Recall — 2025-01-15

### Token Bucket
Prompt Type: definition
Prompt: Explain your understanding of Token Bucket.

My answer:
It controls rate by accumulating tokens over time.

Score:
Feedback:
Hint:
Next review:
"#,
    )
    .unwrap();

    let output = learn_bin()
        .args([
            "review",
            "grade",
            "--auto",
            "--vault",
            dir.path().to_str().unwrap(),
            "--file",
            dir.path().join("Reviews/2025-01-15.md").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Parse the JSON output
    let json: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("should be valid JSON");
    assert!(json["review_file"].as_str().is_some());
    assert!(json["rubric"].as_str().is_some());

    let items = json["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["term"], "Token Bucket");
    assert_eq!(items[0]["prompt_type"], "definition");
    assert!(!items[0]["answer"].as_str().unwrap().is_empty());
    assert!(!items[0]["concept_body"].as_str().unwrap().is_empty());
}

#[test]
fn refine_apply_writes_user_frontmatter() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    // Create a bare concept note (no user fields)
    fs::write(
        dir.path().join("Concepts/Token Bucket.md"),
        "A token bucket controls request rate.\n",
    )
    .unwrap();

    let suggestions = serde_json::json!([{
        "file": "Concepts/Token Bucket.md",
        "term": "Token Bucket",
        "domain": "Systems",
        "tags": ["rate-limiting", "networking"]
    }]);

    let output = learn_bin()
        .args([
            "concept",
            "refine",
            "--apply",
            "--vault",
            dir.path().to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(suggestions.to_string().as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("Applied: 1"));

    // Verify frontmatter was written
    let content = fs::read_to_string(dir.path().join("Concepts/Token Bucket.md")).unwrap();
    assert!(content.contains("term: Token Bucket"));
    assert!(content.contains("domain: Systems"));
    assert!(content.contains("rate-limiting"));
    assert!(content.contains("networking"));
    // Body should be preserved
    assert!(content.contains("A token bucket controls request rate."));
}

#[test]
fn refine_apply_skips_system_fields() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    fs::write(
        dir.path().join("Concepts/Test.md"),
        "---\n_mastery: 0.8\n---\nNotes.\n",
    )
    .unwrap();

    // Try to sneak a system field through --apply
    let suggestions = serde_json::json!([{
        "file": "Concepts/Test.md",
        "term": "Test Concept",
        "_mastery": 0.0
    }]);

    let output = learn_bin()
        .args([
            "concept",
            "refine",
            "--apply",
            "--vault",
            dir.path().to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(suggestions.to_string().as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .unwrap();

    assert!(output.status.success());

    let content = fs::read_to_string(dir.path().join("Concepts/Test.md")).unwrap();
    assert!(content.contains("Test Concept"));
    // _mastery should NOT be overwritten
    assert!(content.contains("0.8"));
}

#[test]
fn refine_apply_handles_missing_file() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    let suggestions = serde_json::json!([{
        "file": "Concepts/Nonexistent.md",
        "term": "Ghost"
    }]);

    let output = learn_bin()
        .args([
            "concept",
            "refine",
            "--apply",
            "--vault",
            dir.path().to_str().unwrap(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child
                .stdin
                .take()
                .unwrap()
                .write_all(suggestions.to_string().as_bytes())
                .unwrap();
            child.wait_with_output()
        })
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success());
    assert!(stdout.contains("Applied: 0, failed: 1"));
    assert!(stderr.contains("File not found"));
}

#[test]
fn review_grade_auto_skips_already_graded() {
    let dir = TempDir::new().unwrap();
    learn_bin()
        .args(["init", "--vault", dir.path().to_str().unwrap()])
        .output()
        .unwrap();

    fs::write(
        dir.path().join("Concepts/Token Bucket.md"),
        "---\nterm: Token Bucket\n---\nContent.\n",
    )
    .unwrap();

    // Review file where one item is already graded
    fs::create_dir_all(dir.path().join("Reviews")).unwrap();
    fs::write(
        dir.path().join("Reviews/2025-01-15.md"),
        r#"---
type: review-session
date: 2025-01-15
---

# Daily Recall — 2025-01-15

### Token Bucket
Prompt Type: definition
Prompt: Explain Token Bucket.

My answer:
It controls rate.

Score: 4
Feedback: Good.
Hint: Consider burst behavior.
Next review: 5 day(s)
"#,
    )
    .unwrap();

    let output = learn_bin()
        .args([
            "review",
            "grade",
            "--auto",
            "--vault",
            dir.path().to_str().unwrap(),
            "--file",
            dir.path().join("Reviews/2025-01-15.md").to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success());
    assert!(stdout.contains("No answered items awaiting grades"));
}
