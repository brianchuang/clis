use std::path::Path;

use crate::parse::concept::parse_concept;
use crate::types::Concept;
use crate::write::frontmatter::initialize_system_fields;

fn today_string() -> String {
    chrono::Local::now().format("%Y-%m-%d").to_string()
}

pub fn get_due_concepts(
    vault_root: &Path,
    domain: Option<&str>,
    count: Option<usize>,
    date: Option<&str>,
) -> Result<Vec<Concept>, String> {
    let date = date.map(String::from).unwrap_or_else(today_string);
    let concepts_dir = vault_root.join("Concepts");
    let pattern = concepts_dir.join("**/*.md").to_string_lossy().to_string();

    let files: Vec<_> = glob::glob(&pattern)
        .map_err(|e| format!("Glob error: {e}"))?
        .flatten()
        .collect();

    let mut concepts: Vec<Concept> = Vec::new();

    for file in files {
        // Initialize system fields on first encounter
        initialize_system_fields(&file, &date)?;
        // Re-parse after potential initialization
        let concept = parse_concept(&file);

        // Filter: due today or earlier, or never reviewed
        let is_due = concept
            .next_review
            .as_ref()
            .map(|nr| nr.as_str() <= date.as_str())
            .unwrap_or(true);

        if !is_due {
            continue;
        }

        // Filter by domain if specified
        if let Some(d) = domain {
            if concept.domain.as_deref() != Some(d) {
                continue;
            }
        }

        concepts.push(concept);
    }

    // Sort by mastery ascending (lowest mastery first)
    concepts.sort_by(|a, b| a.mastery.partial_cmp(&b.mastery).unwrap());

    // Limit count
    if let Some(n) = count {
        concepts.truncate(n);
    }

    Ok(concepts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_vault() -> TempDir {
        let dir = TempDir::new().unwrap();
        let concepts = dir.path().join("Concepts");
        fs::create_dir_all(&concepts).unwrap();

        fs::write(
            concepts.join("Token Bucket.md"),
            "---\nterm: Token Bucket\ndomain: Systems\n_mastery: 0.4\n_review_count: 3\n_next_review: '2025-01-13'\n---\nBody.\n",
        )
        .unwrap();

        fs::write(
            concepts.join("Leaky Bucket.md"),
            "---\nterm: Leaky Bucket\ndomain: Systems\n_mastery: 0.2\n_review_count: 1\n_next_review: '2025-01-10'\n---\nBody.\n",
        )
        .unwrap();

        fs::write(
            concepts.join("React Hooks.md"),
            "---\nterm: React Hooks\ndomain: Frontend\n_mastery: 0.8\n_review_count: 5\n_next_review: '2025-02-01'\n---\nBody.\n",
        )
        .unwrap();

        dir
    }

    #[test]
    fn returns_due_concepts() {
        let dir = setup_vault();
        let due = get_due_concepts(dir.path(), None, None, Some("2025-01-15")).unwrap();
        // Token Bucket and Leaky Bucket are due, React Hooks is future
        assert_eq!(due.len(), 2);
    }

    #[test]
    fn excludes_future_concepts() {
        let dir = setup_vault();
        let due = get_due_concepts(dir.path(), None, None, Some("2025-01-12")).unwrap();
        // Only Leaky Bucket (2025-01-10) is due on 2025-01-12
        assert_eq!(due.len(), 1);
        assert_eq!(due[0].term.as_deref(), Some("Leaky Bucket"));
    }

    #[test]
    fn filters_by_domain() {
        let dir = setup_vault();
        let due =
            get_due_concepts(dir.path(), Some("Systems"), None, Some("2025-01-15")).unwrap();
        assert_eq!(due.len(), 2);
        for c in &due {
            assert_eq!(c.domain.as_deref(), Some("Systems"));
        }
    }

    #[test]
    fn limits_by_count() {
        let dir = setup_vault();
        let due = get_due_concepts(dir.path(), None, Some(1), Some("2025-01-15")).unwrap();
        assert_eq!(due.len(), 1);
    }

    #[test]
    fn sorts_by_mastery_ascending() {
        let dir = setup_vault();
        let due = get_due_concepts(dir.path(), None, None, Some("2025-01-15")).unwrap();
        assert!(due.len() >= 2);
        assert!(due[0].mastery <= due[1].mastery);
    }

    #[test]
    fn concepts_with_no_frontmatter_are_due() {
        let dir = TempDir::new().unwrap();
        let concepts = dir.path().join("Concepts");
        fs::create_dir_all(&concepts).unwrap();
        fs::write(concepts.join("Bare.md"), "Just notes.\n").unwrap();

        let due = get_due_concepts(dir.path(), None, None, Some("2025-01-15")).unwrap();
        assert_eq!(due.len(), 1);
    }
}
