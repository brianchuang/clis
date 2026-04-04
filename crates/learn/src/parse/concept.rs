use std::fs;
use std::path::Path;

use regex::Regex;

use crate::types::Concept;

pub fn extract_wikilinks(text: &str) -> Vec<String> {
    let re = Regex::new(r"\[\[([^\]]+)\]\]").unwrap();
    re.captures_iter(text)
        .map(|cap| cap[1].to_string())
        .collect()
}

/// Split a markdown file into (frontmatter key-value pairs, body content).
/// Handles YAML frontmatter delimited by `---`.
fn parse_frontmatter(raw: &str) -> (serde_yaml::Value, String) {
    let trimmed = raw.trim_start();
    if !trimmed.starts_with("---") {
        return (serde_yaml::Value::Mapping(Default::default()), raw.to_string());
    }

    // Find the closing ---
    let after_open = &trimmed[3..];
    if let Some(close_idx) = after_open.find("\n---") {
        let yaml_str = &after_open[..close_idx];
        let body_start = close_idx + 4; // skip \n---
        let body = if body_start < after_open.len() {
            // Skip the newline after closing ---
            let rest = &after_open[body_start..];
            if rest.starts_with('\n') {
                rest[1..].to_string()
            } else {
                rest.to_string()
            }
        } else {
            String::new()
        };

        let data: serde_yaml::Value =
            serde_yaml::from_str(yaml_str).unwrap_or(serde_yaml::Value::Mapping(Default::default()));
        (data, body)
    } else {
        (serde_yaml::Value::Mapping(Default::default()), raw.to_string())
    }
}

fn get_str(data: &serde_yaml::Value, key: &str) -> Option<String> {
    data.get(key).and_then(|v| v.as_str()).map(String::from)
}

fn get_f64(data: &serde_yaml::Value, key: &str) -> Option<f64> {
    data.get(key).and_then(|v| v.as_f64())
}

fn get_u32(data: &serde_yaml::Value, key: &str) -> Option<u32> {
    data.get(key).and_then(|v| v.as_u64()).map(|n| n as u32)
}

fn get_tags(data: &serde_yaml::Value, key: &str) -> Option<Vec<String>> {
    data.get(key).and_then(|v| {
        if let serde_yaml::Value::Sequence(seq) = v {
            let tags: Vec<String> = seq
                .iter()
                .filter_map(|item| item.as_str().map(String::from))
                .collect();
            if tags.is_empty() { None } else { Some(tags) }
        } else {
            None
        }
    })
}

fn compute_current_interval(last_reviewed: Option<&str>, next_review: Option<&str>) -> u32 {
    match (last_reviewed, next_review) {
        (Some(lr), Some(nr)) => {
            let lr_date = chrono::NaiveDate::parse_from_str(lr, "%Y-%m-%d");
            let nr_date = chrono::NaiveDate::parse_from_str(nr, "%Y-%m-%d");
            match (lr_date, nr_date) {
                (Ok(lr), Ok(nr)) => (nr - lr).num_days().max(1) as u32,
                _ => 1,
            }
        }
        _ => 1,
    }
}

pub fn parse_concept(file_path: &Path) -> Concept {
    let raw = fs::read_to_string(file_path).unwrap_or_default();
    let (data, body) = parse_frontmatter(&raw);

    let filename = file_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let last_reviewed = get_str(&data, "_last_reviewed");
    let next_review = get_str(&data, "_next_review");
    let current_interval = compute_current_interval(
        last_reviewed.as_deref(),
        next_review.as_deref(),
    );

    Concept {
        path: file_path.to_string_lossy().to_string(),
        filename,

        term: get_str(&data, "term"),
        domain: get_str(&data, "domain"),
        tags: get_tags(&data, "tags"),

        mastery: get_f64(&data, "_mastery").unwrap_or(0.0),
        review_count: get_u32(&data, "_review_count").unwrap_or(0),
        current_interval,
        last_reviewed,
        next_review,
        last_prompt_type: get_str(&data, "_last_prompt_type"),

        wikilinks: extract_wikilinks(&body),
        body,
    }
}

/// Expose parse_frontmatter for use by write modules.
pub(crate) fn split_frontmatter(raw: &str) -> (serde_yaml::Value, String) {
    parse_frontmatter(raw)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn extract_wikilinks_finds_links() {
        let text = "See [[Token Bucket]] and [[Leaky Bucket]] for details.";
        let links = extract_wikilinks(text);
        assert_eq!(links, vec!["Token Bucket", "Leaky Bucket"]);
    }

    #[test]
    fn extract_wikilinks_empty() {
        assert!(extract_wikilinks("no links here").is_empty());
    }

    #[test]
    fn parse_concept_full_frontmatter() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Token Bucket.md");
        fs::write(
            &path,
            r#"---
term: Token Bucket
domain: Systems
tags:
  - rate-limiting
  - distributed-systems
_mastery: 0.4
_review_count: 3
_last_reviewed: "2025-01-10"
_next_review: "2025-01-13"
_last_prompt_type: contrast
---
A token bucket algorithm controls the rate of requests.

See also [[Leaky Bucket]].
"#,
        )
        .unwrap();

        let c = parse_concept(&path);
        assert_eq!(c.term.as_deref(), Some("Token Bucket"));
        assert_eq!(c.domain.as_deref(), Some("Systems"));
        assert_eq!(
            c.tags.as_deref(),
            Some(&["rate-limiting".to_string(), "distributed-systems".to_string()][..])
        );
        assert_eq!(c.mastery, 0.4);
        assert_eq!(c.review_count, 3);
        assert_eq!(c.last_reviewed.as_deref(), Some("2025-01-10"));
        assert_eq!(c.next_review.as_deref(), Some("2025-01-13"));
        assert_eq!(c.last_prompt_type.as_deref(), Some("contrast"));
        assert_eq!(c.wikilinks, vec!["Leaky Bucket"]);
        assert!(c.body.contains("token bucket algorithm"));
    }

    #[test]
    fn parse_concept_no_frontmatter() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("Bare.md");
        fs::write(&path, "Just some notes.").unwrap();

        let c = parse_concept(&path);
        assert_eq!(c.filename, "Bare");
        assert!(c.term.is_none());
        assert_eq!(c.mastery, 0.0);
        assert_eq!(c.review_count, 0);
    }
}
