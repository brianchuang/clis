pub fn next_interval_days(score: u32, current_interval: u32) -> u32 {
    let current = current_interval.max(1);
    if score >= 5 {
        ((current as f64 * 2.0).round() as u32).max(1)
    } else if score == 4 {
        ((current as f64 * 1.5).round() as u32).max(1)
    } else if score == 3 {
        current.max(1)
    } else {
        1
    }
}

pub fn update_mastery(current: f64, score: u32) -> f64 {
    let clamped = score.min(5);
    let normalized = clamped as f64 / 5.0;
    let raw = current * 0.8 + normalized * 0.2;
    (raw * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn score_5_doubles_interval() {
        // current interval is 3 days → next is 6 days
        assert_eq!(next_interval_days(5, 3), 6);
    }

    #[test]
    fn score_4_multiplies_by_1_5() {
        // current interval is 4 days → next is 6 days
        assert_eq!(next_interval_days(4, 4), 6);
    }

    #[test]
    fn score_3_keeps_same() {
        // current interval is 5 days → stays 5 days
        assert_eq!(next_interval_days(3, 5), 5);
    }

    #[test]
    fn score_below_3_resets_to_1() {
        assert_eq!(next_interval_days(2, 10), 1);
        assert_eq!(next_interval_days(1, 10), 1);
        assert_eq!(next_interval_days(0, 10), 1);
    }

    #[test]
    fn minimum_interval_is_1() {
        // zero interval floors to 1 before multiplying
        assert_eq!(next_interval_days(5, 0), 2);
        assert_eq!(next_interval_days(3, 0), 1);
    }

    #[test]
    fn mastery_increases_with_high_score() {
        let m = update_mastery(0.5, 5);
        assert!(m > 0.5);
    }

    #[test]
    fn mastery_decreases_with_low_score() {
        let m = update_mastery(0.5, 0);
        assert!(m < 0.5);
    }

    #[test]
    fn mastery_from_zero() {
        let m = update_mastery(0.0, 5);
        assert_eq!(m, 0.2);
    }

    #[test]
    fn mastery_approaches_1() {
        let mut m = 0.0;
        for _ in 0..50 {
            m = update_mastery(m, 5);
        }
        assert!(m > 0.99);
    }

    #[test]
    fn mastery_clamps_score_above_5() {
        // Score 10 should behave the same as score 5
        let m_clamped = update_mastery(0.5, 10);
        let m_normal = update_mastery(0.5, 5);
        assert_eq!(m_clamped, m_normal);
    }

    #[test]
    fn mastery_never_exceeds_1() {
        let mut m = 0.99;
        m = update_mastery(m, 5);
        assert!(m <= 1.0);
    }
}
