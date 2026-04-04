use std::fs;
use std::path::Path;

use regex::Regex;

pub struct ReviewAnswer {
    pub term: String,
    pub prompt_type: String,
    pub prompt: String,
    pub answer: String,
}

pub struct GradedItem {
    pub term: String,
    pub score: u32,
    pub feedback: String,
    pub hint: String,
}

struct ParsedBlock {
    term: String,
    prompt_type: String,
    prompt: String,
    answer: String,
    score: Option<u32>,
    feedback: Option<String>,
    hint: Option<String>,
}

fn parse_blocks(raw: &str) -> Vec<ParsedBlock> {
    let heading_re = Regex::new(r"(?m)^### ").unwrap();
    let mut results = Vec::new();

    // Split by ### headings, keeping the delimiter
    let mut positions: Vec<usize> = heading_re.find_iter(raw).map(|m| m.start()).collect();
    if positions.is_empty() {
        return results;
    }
    positions.push(raw.len());

    for window in positions.windows(2) {
        let block = &raw[window[0]..window[1]];

        let term_re = Regex::new(r"(?m)^### (.+)").unwrap();
        let term = match term_re.captures(block) {
            Some(cap) => cap[1].trim().to_string(),
            None => continue,
        };

        let prompt_type_re = Regex::new(r"(?m)^Prompt Type:\s*(.+)").unwrap();
        let prompt_type = prompt_type_re
            .captures(block)
            .map(|c| c[1].trim().to_string())
            .unwrap_or_default();

        let prompt_re = Regex::new(r"(?m)^Prompt:\s*(.+)").unwrap();
        let prompt = prompt_re
            .captures(block)
            .map(|c| c[1].trim().to_string())
            .unwrap_or_default();

        // Extract answer: text between "My answer:" line and "Score:" line
        let answer = {
            let mut ans = String::new();
            if let Some(start) = block.find("My answer:") {
                let after = &block[start + "My answer:".len()..];
                // Skip the rest of the "My answer:" line
                let after = after.strip_prefix('\n').unwrap_or(after);
                // Take everything up to the next field marker
                let end_markers = ["Score:", "Feedback:", "Hint:", "Next review:"];
                let end = end_markers
                    .iter()
                    .filter_map(|m| after.find(m))
                    .min()
                    .unwrap_or(after.len());
                ans = after[..end].trim().to_string();
            }
            ans
        };

        let score_re = Regex::new(r"(?m)^Score:\s*(\d+)").unwrap();
        let score = score_re
            .captures(block)
            .map(|c| c[1].parse::<u32>().unwrap());

        let feedback_re = Regex::new(r"(?m)^Feedback:\s*(.+)").unwrap();
        let feedback = feedback_re.captures(block).map(|c| c[1].trim().to_string());

        let hint_re = Regex::new(r"(?m)^Hint:\s*(.+)").unwrap();
        let hint = hint_re.captures(block).map(|c| c[1].trim().to_string());

        results.push(ParsedBlock {
            term,
            prompt_type,
            prompt,
            answer,
            score,
            feedback,
            hint,
        });
    }

    results
}

pub fn parse_review_session(file_path: &Path) -> Vec<ReviewAnswer> {
    let raw = fs::read_to_string(file_path).unwrap_or_default();
    parse_blocks(&raw)
        .into_iter()
        .map(|b| ReviewAnswer {
            term: b.term,
            prompt_type: b.prompt_type,
            prompt: b.prompt,
            answer: b.answer,
        })
        .collect()
}

pub fn parse_answered_reviews(file_path: &Path) -> Vec<ReviewAnswer> {
    parse_review_session(file_path)
        .into_iter()
        .filter(|a| !a.answer.is_empty())
        .collect()
}

pub fn parse_graded_items(file_path: &Path) -> Vec<GradedItem> {
    let raw = fs::read_to_string(file_path).unwrap_or_default();
    parse_blocks(&raw)
        .into_iter()
        .filter_map(|b| {
            b.score.map(|score| GradedItem {
                term: b.term,
                score,
                feedback: b.feedback.unwrap_or_default(),
                hint: b.hint.unwrap_or_default(),
            })
        })
        .collect()
}

pub fn resolve_concept_path(vault_root: &Path, term: &str) -> Option<String> {
    let concepts_dir = vault_root.join("Concepts");
    let pattern = concepts_dir.join("**/*.md").to_string_lossy().to_string();
    for entry in glob::glob(&pattern).ok()?.flatten() {
        if entry.file_stem().map(|s| s.to_string_lossy().to_string()) == Some(term.to_string()) {
            return Some(entry.to_string_lossy().to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn write_review(dir: &Path, content: &str) -> std::path::PathBuf {
        let path = dir.join("review.md");
        fs::write(&path, content).unwrap();
        path
    }

    const SAMPLE_REVIEW: &str = r#"---
type: review-session
date: 2025-01-15
---

# Daily Recall — 2025-01-15

### Token Bucket
Prompt Type: contrast
Prompt: How does this differ from Leaky Bucket?

My answer:
Token bucket allows bursts while leaky bucket smooths traffic.

Score: 4
Feedback: Good comparison, missed queue depth.
Hint: Consider how each handles burst traffic differently.
Next review: 6 day(s)

### Leaky Bucket
Prompt Type: definition
Prompt: Explain your understanding of Leaky Bucket.

My answer:


Score:
Feedback:
Hint:
Next review:
"#;

    #[test]
    fn parse_review_session_extracts_all_items() {
        let dir = TempDir::new().unwrap();
        let path = write_review(dir.path(), SAMPLE_REVIEW);
        let items = parse_review_session(&path);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].term, "Token Bucket");
        assert_eq!(items[1].term, "Leaky Bucket");
    }

    #[test]
    fn parse_answered_reviews_filters_empty() {
        let dir = TempDir::new().unwrap();
        let path = write_review(dir.path(), SAMPLE_REVIEW);
        let items = parse_answered_reviews(&path);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].term, "Token Bucket");
    }

    #[test]
    fn parse_graded_items_extracts_scored() {
        let dir = TempDir::new().unwrap();
        let path = write_review(dir.path(), SAMPLE_REVIEW);
        let items = parse_graded_items(&path);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].term, "Token Bucket");
        assert_eq!(items[0].score, 4);
        assert_eq!(items[0].feedback, "Good comparison, missed queue depth.");
    }

    #[test]
    fn resolve_concept_path_finds_file() {
        let dir = TempDir::new().unwrap();
        let concepts = dir.path().join("Concepts");
        fs::create_dir_all(&concepts).unwrap();
        fs::write(concepts.join("Token Bucket.md"), "content").unwrap();

        let result = resolve_concept_path(dir.path(), "Token Bucket");
        assert!(result.is_some());
        assert!(result.unwrap().contains("Token Bucket.md"));
    }

    #[test]
    fn resolve_concept_path_returns_none_for_missing() {
        let dir = TempDir::new().unwrap();
        let concepts = dir.path().join("Concepts");
        fs::create_dir_all(&concepts).unwrap();

        let result = resolve_concept_path(dir.path(), "Nonexistent");
        assert!(result.is_none());
    }
}
