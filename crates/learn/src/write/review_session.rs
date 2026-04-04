use std::fs;
use std::path::Path;

use crate::types::ReviewItem;

pub fn render_review_session(items: &[ReviewItem], date: &str) -> String {
    let now = chrono::Utc::now().to_rfc3339();
    let mut output = format!(
        "---\ntype: review-session\ndate: {date}\ngenerated_at: {now}\n---\n\n# Daily Recall — {date}\n"
    );

    for item in items {
        output.push_str(&format!(
            "\n### {}\nPrompt Type: {}\nPrompt: {}\n\nMy answer:\n\n\nScore:\nFeedback:\nHint:\nNext review:\n",
            item.concept_term, item.prompt_type, item.prompt
        ));
    }

    output
}

pub fn write_review_session(
    vault_root: &Path,
    items: &[ReviewItem],
    date: &str,
    force: bool,
) -> Result<String, String> {
    let reviews_dir = vault_root.join("Reviews");
    fs::create_dir_all(&reviews_dir)
        .map_err(|e| format!("Failed to create Reviews dir: {e}"))?;

    let file_path = reviews_dir.join(format!("{date}.md"));

    if file_path.exists() && !force {
        return Err(format!(
            "Review file already exists: {}. Use --force to overwrite.",
            file_path.display()
        ));
    }

    let content = render_review_session(items, date);

    // Atomic write
    let tmp_path = file_path.with_extension("md.tmp");
    fs::write(&tmp_path, &content)
        .map_err(|e| format!("Failed to write tmp: {e}"))?;
    fs::rename(&tmp_path, &file_path)
        .map_err(|e| format!("Failed to rename: {e}"))?;

    Ok(file_path.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn render_review_session_has_structure() {
        let items = vec![ReviewItem {
            concept_path: "Concepts/Test.md".into(),
            concept_term: "Test".into(),
            prompt_type: "definition".into(),
            prompt: "Explain Test.".into(),
        }];
        let output = render_review_session(&items, "2025-01-15");
        assert!(output.contains("# Daily Recall — 2025-01-15"));
        assert!(output.contains("### Test"));
        assert!(output.contains("Prompt Type: definition"));
        assert!(output.contains("Score:"));
        assert!(output.contains("My answer:"));
    }

    #[test]
    fn write_review_session_creates_file() {
        let dir = TempDir::new().unwrap();
        let items = vec![ReviewItem {
            concept_path: "Concepts/Test.md".into(),
            concept_term: "Test".into(),
            prompt_type: "definition".into(),
            prompt: "Explain Test.".into(),
        }];

        let path = write_review_session(dir.path(), &items, "2025-01-15", false).unwrap();
        assert!(Path::new(&path).exists());
    }

    #[test]
    fn write_review_session_errors_on_duplicate() {
        let dir = TempDir::new().unwrap();
        let items = vec![ReviewItem {
            concept_path: "Concepts/Test.md".into(),
            concept_term: "Test".into(),
            prompt_type: "definition".into(),
            prompt: "Explain Test.".into(),
        }];

        write_review_session(dir.path(), &items, "2025-01-15", false).unwrap();
        let result = write_review_session(dir.path(), &items, "2025-01-15", false);
        assert!(result.is_err());
    }

    #[test]
    fn write_review_session_force_overwrites() {
        let dir = TempDir::new().unwrap();
        let items = vec![ReviewItem {
            concept_path: "Concepts/Test.md".into(),
            concept_term: "Test".into(),
            prompt_type: "definition".into(),
            prompt: "Explain Test.".into(),
        }];

        write_review_session(dir.path(), &items, "2025-01-15", false).unwrap();
        let result = write_review_session(dir.path(), &items, "2025-01-15", true);
        assert!(result.is_ok());
    }

}
