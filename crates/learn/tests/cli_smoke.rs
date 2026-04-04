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
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("Vault initialized"));
    assert!(dir.path().join("Concepts").is_dir());
    assert!(dir.path().join("Reviews").is_dir());
    assert!(dir.path().join("Templates").is_dir());
    assert!(dir.path().join(".learning-system").is_dir());
    assert!(dir.path().join("Templates/concept.md").exists());
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
        .args([
            "init",
            "--vault",
            dir.path().to_str().unwrap(),
            "--force",
        ])
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

    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
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
    assert!(output.status.success(), "stderr: {}", String::from_utf8_lossy(&output.stderr));
    assert!(stdout.contains("concept(s) selected"));

    // Verify a review file was created in Reviews/
    let reviews: Vec<_> = fs::read_dir(dir.path().join("Reviews"))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(reviews.len(), 1);
}
