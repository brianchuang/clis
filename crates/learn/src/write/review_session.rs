use std::fs;
use std::path::Path;

use regex::Regex;

use crate::types::{Grade, ReviewItem};

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

pub fn fill_grades(file_path: &Path, grades: &[Grade]) -> Result<(), String> {
    let content = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read review file: {e}"))?;

    // Build lookup: term → grade
    let grade_by_term: std::collections::HashMap<String, &Grade> = grades
        .iter()
        .map(|g| {
            let term = Path::new(&g.concept_path)
                .file_stem()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            (term, g)
        })
        .collect();

    // Split into sections by ### heading
    let heading_re = Regex::new(r"(?m)^### ").unwrap();
    let positions: Vec<usize> = heading_re.find_iter(&content).map(|m| m.start()).collect();

    if positions.is_empty() {
        // No sections to fill
        return Ok(());
    }

    let mut sections: Vec<String> = Vec::new();

    // Content before first ###
    if positions[0] > 0 {
        sections.push(content[..positions[0]].to_string());
    }

    for (i, &start) in positions.iter().enumerate() {
        let end = if i + 1 < positions.len() {
            positions[i + 1]
        } else {
            content.len()
        };
        let section = &content[start..end];

        let term_re = Regex::new(r"(?m)^### (.+)").unwrap();
        let term = term_re
            .captures(section)
            .map(|c| c[1].trim().to_string());

        let filled = if let Some(ref t) = term {
            if let Some(grade) = grade_by_term.get(t.as_str()) {
                let score_re = Regex::new(r"(?m)^Score:\s*$").unwrap();
                let feedback_re = Regex::new(r"(?m)^Feedback:\s*$").unwrap();
                let hint_re = Regex::new(r"(?m)^Hint:\s*$").unwrap();
                let next_re = Regex::new(r"(?m)^Next review:\s*$").unwrap();

                let s = score_re
                    .replace(section, &format!("Score: {}", grade.score))
                    .to_string();
                let s = feedback_re
                    .replace(&s, &format!("Feedback: {}", grade.feedback))
                    .to_string();
                let s = hint_re
                    .replace(&s, &format!("Hint: {}", grade.hint))
                    .to_string();
                let s = next_re
                    .replace(&s, &format!("Next review: {} day(s)", grade.next_review_days))
                    .to_string();
                s
            } else {
                section.to_string()
            }
        } else {
            section.to_string()
        };

        sections.push(filled);
    }

    let updated = sections.join("");

    // Atomic write
    let tmp_path = file_path.with_extension("md.tmp");
    fs::write(&tmp_path, &updated)
        .map_err(|e| format!("Failed to write tmp: {e}"))?;
    fs::rename(&tmp_path, file_path)
        .map_err(|e| format!("Failed to rename: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
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

    #[test]
    fn fill_grades_replaces_placeholders() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("review.md");
        fs::write(
            &path,
            "---\ntype: review-session\n---\n\n### Token Bucket\nPrompt Type: contrast\nPrompt: How does it differ?\n\nMy answer:\nBurst vs smooth.\n\nScore:\nFeedback:\nHint:\nNext review:\n",
        )
        .unwrap();

        let grades = vec![Grade {
            concept_path: "Concepts/Token Bucket.md".into(),
            score: 4,
            feedback: "Good answer.".into(),
            hint: "Consider queue depth.".into(),
            next_review_days: 6,
        }];

        fill_grades(&path, &grades).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Score: 4"));
        assert!(content.contains("Feedback: Good answer."));
        assert!(content.contains("Hint: Consider queue depth."));
        assert!(content.contains("Next review: 6 day(s)"));
    }
}
