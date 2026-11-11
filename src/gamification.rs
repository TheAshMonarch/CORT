use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

pub const DEFAULT_LESSON_POINTS: i64 = 10;
pub const POINTS_PER_CORRECT_ANSWER: i64 = 15;
pub const PERFECT_QUIZ_BONUS_POINTS: i64 = 25;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BadgeInfo {
    pub code: String,
    pub title: String,
    pub description: String,
}

/// Computes the updated streak count given the student's previous activity date (`YYYY-MM-DD`)
/// and the current activity date (`YYYY-MM-DD`).
pub fn compute_updated_streak(
    previous_streak: i64,
    last_activity_date: Option<&str>,
    today_str: &str,
) -> i64 {
    let Ok(today) = NaiveDate::parse_from_str(today_str, "%Y-%m-%d") else {
        return previous_streak.max(1);
    };

    if let Some(last_str) = last_activity_date {
        if let Ok(last_date) = NaiveDate::parse_from_str(last_str, "%Y-%m-%d") {
            let diff = (today - last_date).num_days();
            if diff == 0 {
                return previous_streak.max(1);
            } else if diff == 1 {
                return previous_streak + 1;
            }
        }
    }

    1
}

/// Evaluates which badges a student qualifies for and returns the full updated badge list
/// along with any newly unlocked badges.
pub fn evaluate_badges(
    existing_badges: &[String],
    total_points: i64,
    streak_days: i64,
    completed_lessons_count: i64,
    completed_quizzes_count: i64,
    achieved_perfect_quiz: bool,
) -> (Vec<String>, Vec<BadgeInfo>) {
    let mut badges = existing_badges.to_vec();
    let mut newly_awarded = Vec::new();

    let candidates = [
        (
            "first_steps",
            "First Steps",
            "Completed your first offline lesson or quiz",
            completed_lessons_count >= 1 || completed_quizzes_count >= 1,
        ),
        (
            "module_reader",
            "Avid Reader",
            "Completed 3 or more curriculum lessons",
            completed_lessons_count >= 3,
        ),
        (
            "quiz_ace",
            "Quiz Ace",
            "Scored 100% on a curriculum quiz",
            achieved_perfect_quiz,
        ),
        (
            "streak_3_days",
            "3-Day Streak",
            "Studied for 3 consecutive days",
            streak_days >= 3,
        ),
        (
            "streak_7_days",
            "7-Day Streak",
            "Studied for 7 consecutive days",
            streak_days >= 7,
        ),
        (
            "scholar_100",
            "Rising Scholar",
            "Earned 100+ learning points",
            total_points >= 100,
        ),
        (
            "scholar_500",
            "Grand Scholar",
            "Earned 500+ learning points",
            total_points >= 500,
        ),
    ];

    for (code, title, description, unlocked) in candidates {
        if unlocked && !badges.iter().any(|b| b == code) {
            badges.push(code.to_string());
            newly_awarded.push(BadgeInfo {
                code: code.to_string(),
                title: title.to_string(),
                description: description.to_string(),
            });
        }
    }

    (badges, newly_awarded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streak_calculation() {
        // First activity ever
        assert_eq!(compute_updated_streak(0, None, "2026-11-11"), 1);
        // Same day activity preserves streak
        assert_eq!(
            compute_updated_streak(3, Some("2026-11-11"), "2026-11-11"),
            3
        );
        // Consecutive day increments streak
        assert_eq!(
            compute_updated_streak(3, Some("2026-11-10"), "2026-11-11"),
            4
        );
        // Missed day resets streak to 1
        assert_eq!(
            compute_updated_streak(5, Some("2026-11-08"), "2026-11-11"),
            1
        );
    }

    #[test]
    fn test_badge_evaluation() {
        let existing = vec![];
        let (all, new_badges) = evaluate_badges(&existing, 120, 3, 1, 1, true);
        assert!(all.contains(&"first_steps".to_string()));
        assert!(all.contains(&"quiz_ace".to_string()));
        assert!(all.contains(&"streak_3_days".to_string()));
        assert!(all.contains(&"scholar_100".to_string()));
        assert_eq!(new_badges.len(), 4);
    }
}
