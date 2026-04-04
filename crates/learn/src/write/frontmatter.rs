use std::fs;
use std::path::Path;

use crate::parse::concept::split_frontmatter;

/// Reconstruct a markdown file from YAML frontmatter and body.
fn stringify(data: &serde_yaml::Value, body: &str) -> String {
    let yaml = serde_yaml::to_string(data).unwrap_or_default();
    // serde_yaml adds a trailing newline; ensure frontmatter is clean
    let yaml = yaml.trim_end_matches('\n');
    if body.is_empty() {
        format!("---\n{yaml}\n---\n")
    } else {
        format!("---\n{yaml}\n---\n{body}")
    }
}

pub fn write_system_frontmatter(
    file_path: &Path,
    updates: &[(&str, serde_yaml::Value)],
) -> Result<(), String> {
    let raw = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
    let (mut data, body) = split_frontmatter(&raw);

    let mapping = data
        .as_mapping_mut()
        .ok_or_else(|| "Frontmatter is not a mapping".to_string())?;

    for (key, value) in updates {
        // Only allow underscore-prefixed keys
        if !key.starts_with('_') {
            continue;
        }
        mapping.insert(serde_yaml::Value::String(key.to_string()), value.clone());
    }

    let output = stringify(&data, &body);

    // Atomic write
    let tmp_path = file_path.with_extension("md.tmp");
    fs::write(&tmp_path, &output).map_err(|e| format!("Failed to write tmp: {e}"))?;
    fs::rename(&tmp_path, file_path).map_err(|e| format!("Failed to rename: {e}"))?;

    Ok(())
}

pub fn initialize_system_fields(file_path: &Path, date: &str) -> Result<(), String> {
    let raw = fs::read_to_string(file_path)
        .map_err(|e| format!("Failed to read {}: {e}", file_path.display()))?;
    let (data, _body) = split_frontmatter(&raw);

    let mut updates: Vec<(&str, serde_yaml::Value)> = Vec::new();

    if data.get("_mastery").is_none() {
        updates.push((
            "_mastery",
            serde_yaml::Value::Number(serde_yaml::Number::from(0)),
        ));
    }
    if data.get("_review_count").is_none() {
        updates.push((
            "_review_count",
            serde_yaml::Value::Number(serde_yaml::Number::from(0)),
        ));
    }
    if data.get("_next_review").is_none() {
        updates.push(("_next_review", serde_yaml::Value::String(date.to_string())));
    }

    if !updates.is_empty() {
        write_system_frontmatter(file_path, &updates)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn writes_only_system_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "---\nterm: Token Bucket\n---\nBody text.\n").unwrap();

        write_system_frontmatter(
            &path,
            &[
                (
                    "_mastery",
                    serde_yaml::Value::Number(serde_yaml::Number::from(0)),
                ),
                // This should be ignored — not underscore-prefixed
                ("term", serde_yaml::Value::String("OVERWRITTEN".into())),
            ],
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        // term should NOT be overwritten
        assert!(content.contains("Token Bucket"));
        assert!(!content.contains("OVERWRITTEN"));
        assert!(content.contains("_mastery"));
    }

    #[test]
    fn preserves_body() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        fs::write(&path, "---\nterm: Test\n---\nBody with [[links]].\n").unwrap();

        write_system_frontmatter(
            &path,
            &[(
                "_review_count",
                serde_yaml::Value::Number(serde_yaml::Number::from(1)),
            )],
        )
        .unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("Body with [[links]]."));
    }

    #[test]
    fn initialize_missing_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bare.md");
        fs::write(&path, "Just notes.\n").unwrap();

        initialize_system_fields(&path, "2025-01-15").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        assert!(content.contains("_mastery"));
        assert!(content.contains("_review_count"));
        assert!(content.contains("_next_review"));
        assert!(content.contains("2025-01-15"));
    }

    #[test]
    fn does_not_overwrite_existing_system_fields() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("test.md");
        fs::write(
            &path,
            "---\n_mastery: 0.5\n_review_count: 3\n_next_review: '2025-02-01'\n---\nBody.\n",
        )
        .unwrap();

        initialize_system_fields(&path, "2025-01-15").unwrap();

        let content = fs::read_to_string(&path).unwrap();
        // Should keep the original values
        assert!(content.contains("0.5"));
        assert!(content.contains("2025-02-01"));
    }
}
