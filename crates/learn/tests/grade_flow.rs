use std::fs;
use std::path::Path;

use tempfile::TempDir;

use learn::parse::concept::parse_concept;
use learn::parse::review::parse_graded_items;
use learn::schedule::{next_interval_days, update_mastery};
use learn::types::{Grade, ReviewItem};
use learn::write::frontmatter::write_system_frontmatter;
use learn::write::review_session::{fill_grades, write_review_session};

#[test]
fn full_grade_flow() {
    let dir = TempDir::new().unwrap();
    let concepts_dir = dir.path().join("Concepts");
    fs::create_dir_all(&concepts_dir).unwrap();

    // Create a concept with system fields
    let concept_path = concepts_dir.join("Token Bucket.md");
    fs::write(
        &concept_path,
        r#"---
term: Token Bucket
domain: Systems
_mastery: 0.2
_review_count: 2
_next_review: "2025-01-15"
---
A token bucket controls request rate.
"#,
    )
    .unwrap();

    let date = "2025-01-15";

    // Generate review session
    let items = vec![ReviewItem {
        concept_path: concept_path.to_string_lossy().to_string(),
        concept_term: "Token Bucket".into(),
        prompt_type: "contrast".into(),
        prompt: "How does Token Bucket differ from Leaky Bucket?".into(),
    }];

    let review_path = write_review_session(dir.path(), &items, date, false).unwrap();

    // Simulate user filling in an answer + agent grading
    let content = fs::read_to_string(&review_path).unwrap();
    let with_answer = content.replace(
        "My answer:\n\n\nScore:",
        "My answer:\nToken bucket allows bursts, leaky bucket smooths.\n\nScore:",
    );
    fs::write(&review_path, &with_answer).unwrap();

    // Fill grades
    let concept = parse_concept(&concept_path);
    let interval = next_interval_days(4, concept.review_count);
    let new_mastery = update_mastery(concept.mastery, 4);

    let grades = vec![Grade {
        concept_path: concept_path.to_string_lossy().to_string(),
        score: 4,
        feedback: "Good comparison.".into(),
        hint: "Consider queue depth.".into(),
        next_review_days: interval,
    }];

    fill_grades(Path::new(&review_path), &grades).unwrap();

    // Verify grades were filled
    let graded = parse_graded_items(Path::new(&review_path));
    assert_eq!(graded.len(), 1);
    assert_eq!(graded[0].score, 4);

    // Update concept frontmatter
    let next_date = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d").unwrap()
        + chrono::Duration::days(interval as i64);
    let next_review = next_date.format("%Y-%m-%d").to_string();

    write_system_frontmatter(
        &concept_path,
        &[
            (
                "_last_reviewed",
                serde_yaml::Value::String(date.to_string()),
            ),
            (
                "_next_review",
                serde_yaml::Value::String(next_review.clone()),
            ),
            (
                "_review_count",
                serde_yaml::Value::Number(serde_yaml::Number::from(
                    concept.review_count + 1,
                )),
            ),
            ("_mastery", serde_yaml::to_value(new_mastery).unwrap()),
        ],
    )
    .unwrap();

    // Verify concept was updated
    let updated = parse_concept(&concept_path);
    assert!(updated.mastery > 0.2, "mastery should increase");
    assert_eq!(updated.review_count, 3);
    assert_eq!(updated.last_reviewed.as_deref(), Some(date));
    // User-owned fields should be untouched
    assert_eq!(updated.term.as_deref(), Some("Token Bucket"));
    assert_eq!(updated.domain.as_deref(), Some("Systems"));
}
