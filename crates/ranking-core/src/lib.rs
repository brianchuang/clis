use chrono::{DateTime, Local};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;

/// Score items by fuzzy match against a query. Returns `(index, score)` pairs
/// with scores normalized to `0.0..=1.0`. Non-matching items are excluded.
/// `text_fn` extracts the searchable string from each item.
pub fn score_fuzzy<T, F>(items: &[T], query: &str, text_fn: F) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> String,
{
    if query.is_empty() || items.is_empty() {
        return Vec::new();
    }

    let matcher = SkimMatcherV2::default();
    let scored: Vec<(usize, i64)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            matcher
                .fuzzy_match(&text_fn(item), query)
                .map(|score| (i, score))
        })
        .collect();

    let max_score = scored.iter().map(|(_, s)| *s).max().unwrap_or(1).max(1);
    scored
        .into_iter()
        .map(|(i, s)| (i, s as f64 / max_score as f64))
        .collect()
}

/// Score items by recency. Returns `(index, score)` pairs with scores in `0.0..=1.0`.
/// Uses exponential decay: score is `1.0` at `now`, `0.5` at `half_life_hours`.
/// `time_fn` extracts the timestamp from each item.
pub fn score_recency<T, F>(items: &[T], time_fn: F, half_life_hours: f64) -> Vec<(usize, f64)>
where
    F: Fn(&T) -> DateTime<Local>,
{
    let now = Local::now();
    let decay = (0.5_f64).ln() / half_life_hours;

    items
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let age_hours = (now - time_fn(item)).num_seconds() as f64 / 3600.0;
            let score = (decay * age_hours).exp().clamp(0.0, 1.0);
            (i, score)
        })
        .collect()
}

/// Merge multiple scored index vectors with weights. Returns indices sorted by
/// weighted sum (highest first). Only indices that appear in at least one scored
/// list are included.
///
/// Each entry in `scored_lists` is a `(scores, weight)` pair.
pub fn merge_scores(scored_lists: &[(&[(usize, f64)], f64)], count: usize) -> Vec<usize> {
    let mut totals = vec![0.0_f64; count];
    let mut present = vec![false; count];

    for (scores, weight) in scored_lists {
        for &(idx, score) in *scores {
            if idx < count {
                totals[idx] += score * weight;
                present[idx] = true;
            }
        }
    }

    let mut indices: Vec<usize> = (0..count).filter(|&i| present[i]).collect();
    indices.sort_by(|&a, &b| {
        totals[b]
            .partial_cmp(&totals[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    indices
}

/// Compute filtered indices sorted by fuzzy match score.
/// `text_fn` extracts the searchable string from each item.
///
/// Wrapper around `score_fuzzy` that returns just the sorted indices.
/// For weighted ranking, use `score_fuzzy` + `merge_scores` directly.
pub fn compute_filtered<T, F>(items: &[T], query: &str, text_fn: F) -> Vec<usize>
where
    F: Fn(&T) -> String,
{
    if query.is_empty() {
        return (0..items.len()).collect();
    }

    let mut scored = score_fuzzy(items, query, text_fn);
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    scored.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Local;

    // --- score_fuzzy tests ---

    #[test]
    fn score_fuzzy_empty_query_returns_empty() {
        let items = vec!["apple", "banana"];
        let scored = score_fuzzy(&items, "", |s| s.to_string());
        assert!(scored.is_empty());
    }

    #[test]
    fn score_fuzzy_empty_items_returns_empty() {
        let items: Vec<&str> = vec![];
        let scored = score_fuzzy(&items, "test", |s| s.to_string());
        assert!(scored.is_empty());
    }

    #[test]
    fn score_fuzzy_matches_correct_items() {
        let items = vec!["apple", "banana", "apricot"];
        let scored = score_fuzzy(&items, "ap", |s| s.to_string());
        let indices: Vec<usize> = scored.iter().map(|(i, _)| *i).collect();
        assert!(indices.contains(&0)); // apple
        assert!(indices.contains(&2)); // apricot
        assert!(!indices.contains(&1)); // banana excluded
    }

    #[test]
    fn score_fuzzy_scores_normalized_to_unit_range() {
        let items = vec!["apple", "application", "banana"];
        let scored = score_fuzzy(&items, "app", |s| s.to_string());
        for &(_, score) in &scored {
            assert!(score >= 0.0 && score <= 1.0, "score {score} out of range");
        }
        // Best match should have score 1.0
        let max = scored.iter().map(|(_, s)| *s).fold(0.0_f64, f64::max);
        assert!((max - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn score_fuzzy_no_match_returns_empty() {
        let items = vec!["apple", "banana"];
        let scored = score_fuzzy(&items, "zzzzz", |s| s.to_string());
        assert!(scored.is_empty());
    }

    #[test]
    fn score_fuzzy_better_match_scores_higher() {
        let items = vec!["xyzappxyz", "app"];
        let scored = score_fuzzy(&items, "app", |s| s.to_string());
        assert_eq!(scored.len(), 2);
        let score_map: std::collections::HashMap<usize, f64> = scored.into_iter().collect();
        assert!(
            score_map[&1] >= score_map[&0],
            "exact match 'app' should score >= 'xyzappxyz'"
        );
    }

    // --- score_recency tests ---

    #[test]
    fn score_recency_now_is_one() {
        let now = Local::now();
        let items = vec![now];
        let scored = score_recency(&items, |t| *t, 24.0);
        assert_eq!(scored.len(), 1);
        assert!(
            (scored[0].1 - 1.0).abs() < 0.01,
            "score for now should be ~1.0, got {}",
            scored[0].1
        );
    }

    #[test]
    fn score_recency_at_half_life_is_half() {
        let now = Local::now();
        let half_life = 24.0;
        let old = now - chrono::Duration::hours(24);
        let items = vec![old];
        let scored = score_recency(&items, |t| *t, half_life);
        assert!(
            (scored[0].1 - 0.5).abs() < 0.01,
            "score at half-life should be ~0.5, got {}",
            scored[0].1
        );
    }

    #[test]
    fn score_recency_older_scores_lower() {
        let now = Local::now();
        let recent = now - chrono::Duration::hours(1);
        let old = now - chrono::Duration::hours(48);
        let items = vec![recent, old];
        let scored = score_recency(&items, |t| *t, 24.0);
        assert!(
            scored[0].1 > scored[1].1,
            "recent ({}) should score higher than old ({})",
            scored[0].1,
            scored[1].1
        );
    }

    #[test]
    fn score_recency_all_items_included() {
        let now = Local::now();
        let items: Vec<DateTime<Local>> = (0..5)
            .map(|i| now - chrono::Duration::hours(i * 12))
            .collect();
        let scored = score_recency(&items, |t| *t, 24.0);
        assert_eq!(scored.len(), 5);
    }

    #[test]
    fn score_recency_scores_in_unit_range() {
        let now = Local::now();
        let items: Vec<DateTime<Local>> = (0..10)
            .map(|i| now - chrono::Duration::hours(i * 24))
            .collect();
        let scored = score_recency(&items, |t| *t, 24.0);
        for &(_, score) in &scored {
            assert!(score >= 0.0 && score <= 1.0, "score {score} out of range");
        }
    }

    // --- merge_scores tests ---

    #[test]
    fn merge_scores_single_list() {
        let scores = vec![(0, 0.8), (2, 0.5), (1, 0.3)];
        let merged = merge_scores(&[(&scores, 1.0)], 3);
        assert_eq!(merged, vec![0, 2, 1]);
    }

    #[test]
    fn merge_scores_two_lists_weighted() {
        // Item 0: fuzzy=1.0, recency=0.0 => 0.7*1.0 + 0.3*0.0 = 0.7
        // Item 1: fuzzy=0.0, recency=1.0 => 0.7*0.0 + 0.3*1.0 = 0.3
        // Item 2: fuzzy=0.5, recency=0.8 => 0.7*0.5 + 0.3*0.8 = 0.59
        let fuzzy = vec![(0, 1.0), (2, 0.5)];
        let recency = vec![(1, 1.0), (2, 0.8)];
        let merged = merge_scores(&[(&fuzzy, 0.7), (&recency, 0.3)], 3);
        assert_eq!(merged, vec![0, 2, 1]);
    }

    #[test]
    fn merge_scores_empty_lists() {
        let merged = merge_scores(&[], 5);
        assert!(merged.is_empty());
    }

    #[test]
    fn merge_scores_excludes_absent_indices() {
        let scores = vec![(1, 0.5), (3, 0.8)];
        let merged = merge_scores(&[(&scores, 1.0)], 5);
        assert_eq!(merged.len(), 2);
        assert!(!merged.contains(&0));
        assert!(!merged.contains(&2));
        assert!(!merged.contains(&4));
    }

    #[test]
    fn merge_scores_weight_changes_order() {
        // Item 0: only in fuzzy (score 1.0)
        // Item 1: only in recency (score 1.0)
        let fuzzy = vec![(0, 1.0)];
        let recency = vec![(1, 1.0)];

        // Heavy fuzzy weight => item 0 first
        let merged = merge_scores(&[(&fuzzy, 0.9), (&recency, 0.1)], 2);
        assert_eq!(merged[0], 0);

        // Heavy recency weight => item 1 first
        let merged = merge_scores(&[(&fuzzy, 0.1), (&recency, 0.9)], 2);
        assert_eq!(merged[0], 1);
    }

    #[test]
    fn merge_scores_out_of_bounds_index_ignored() {
        let scores = vec![(0, 1.0), (99, 0.5)]; // index 99 exceeds count
        let merged = merge_scores(&[(&scores, 1.0)], 3);
        assert_eq!(merged, vec![0]);
    }

    #[test]
    fn merge_scores_three_lists() {
        let a = vec![(0, 1.0), (1, 0.2)];
        let b = vec![(0, 0.3), (2, 1.0)];
        let c = vec![(1, 0.9), (2, 0.1)];
        // Item 0: 0.5*1.0 + 0.3*0.3 + 0.2*0.0 = 0.59
        // Item 1: 0.5*0.2 + 0.3*0.0 + 0.2*0.9 = 0.28
        // Item 2: 0.5*0.0 + 0.3*1.0 + 0.2*0.1 = 0.32
        let merged = merge_scores(&[(&a, 0.5), (&b, 0.3), (&c, 0.2)], 3);
        assert_eq!(merged, vec![0, 2, 1]);
    }

    // --- compute_filtered tests ---

    #[test]
    fn empty_query_returns_all() {
        let items = vec!["foo", "bar", "baz"];
        let filtered = compute_filtered(&items, "", |s| s.to_string());
        assert_eq!(filtered, vec![0, 1, 2]);
    }

    #[test]
    fn filter_matches() {
        let items = vec!["apple", "banana", "apricot"];
        let filtered = compute_filtered(&items, "ap", |s| s.to_string());
        assert!(!filtered.is_empty());
        assert!(filtered.contains(&0)); // apple
        assert!(filtered.contains(&2)); // apricot
    }

    #[test]
    fn filter_no_match() {
        let items = vec!["apple", "banana"];
        let filtered = compute_filtered(&items, "zzzzz", |s| s.to_string());
        assert!(filtered.is_empty());
    }
}
