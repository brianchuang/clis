use std::fs;

use tempfile::TempDir;

use learn::parse::concept::parse_concept;
use learn::parse::review::parse_graded_items;
use learn::schedule::{next_interval_days, update_mastery};
use learn::types::ReviewItem;
use learn::write::frontmatter::write_system_frontmatter;
use learn::write::review_session::write_review_session;

#[test]
fn full_grade_flow() {
    let dir = TempDir::new().unwrap();
    let concepts_dir = dir.path().join("Concepts");
    fs::create_dir_all(&concepts_dir).unwrap();

    // Create a concept with system fields
    // _last_reviewed=2025-01-12, _next_review=2025-01-15 → current_interval=3
    let concept_path = concepts_dir.join("Token Bucket.md");
    fs::write(
        &concept_path,
        r#"---
term: Token Bucket
domain: Systems
_mastery: 0.2
_review_count: 2
_last_reviewed: "2025-01-12"
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

    // Simulate user filling in an answer + score
    let content = fs::read_to_string(&review_path).unwrap();
    let with_answer = content.replace(
        "My answer:\n\n\nScore:",
        "My answer:\nToken bucket allows bursts, leaky bucket smooths.\n\nScore: 4",
    );
    fs::write(&review_path, &with_answer).unwrap();

    // Parse graded items from the review file
    let graded = parse_graded_items(std::path::Path::new(&review_path));
    assert_eq!(graded.len(), 1);
    assert_eq!(graded[0].score, 4);

    // Compute next interval using current_interval (not review_count)
    let concept = parse_concept(&concept_path);
    assert_eq!(concept.current_interval, 3); // 2025-01-15 - 2025-01-12 = 3 days
    let interval = next_interval_days(graded[0].score, concept.current_interval);
    let new_mastery = update_mastery(concept.mastery, graded[0].score);

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
    // Score 4 with interval 3 → next interval = ceil(3 * 1.5) = 5
    assert_eq!(interval, 5);
    // User-owned fields should be untouched
    assert_eq!(updated.term.as_deref(), Some("Token Bucket"));
    assert_eq!(updated.domain.as_deref(), Some("Systems"));
}
