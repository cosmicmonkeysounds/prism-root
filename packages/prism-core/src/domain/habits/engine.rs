//! Habits engine — streak computation, completion rates, wellness.
//!
//! Port of `@helm/habits` TypeScript module. Pure functions over
//! [`HabitLog`] and [`WellnessCategory`] slices — no mutable state,
//! no side effects.

use chrono::NaiveDate;

use super::types::{HabitLog, StreakInfo, WellnessCategory, WellnessSummary};

// ── Date parsing helper ──────────────────────────────────────────

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

// ── Streak computation ───────────────────────────────────────────

/// Compute streak statistics from a slice of habit logs.
///
/// Logs are sorted by date internally. Only entries with
/// `completed == true` contribute to streaks. Consecutive days
/// with at least one completion form a streak; any gap of one or
/// more days breaks it.
pub fn compute_streak(logs: &[HabitLog]) -> StreakInfo {
    // Collect and sort completed dates (deduplicated).
    let mut dates: Vec<NaiveDate> = logs
        .iter()
        .filter(|l| l.completed)
        .filter_map(|l| parse_date(&l.date))
        .collect();
    dates.sort();
    dates.dedup();

    if dates.is_empty() {
        return StreakInfo {
            current_streak: 0,
            longest_streak: 0,
            total_completions: 0,
            last_completed: None,
        };
    }

    let total_completions = dates.len() as u32;
    let last_completed = dates.last().map(|d| d.format("%Y-%m-%d").to_string());

    // Walk dates to find the longest streak.
    let mut longest_streak: u32 = 1;
    let mut running: u32 = 1;

    for i in 1..dates.len() {
        let gap = dates[i].signed_duration_since(dates[i - 1]).num_days();
        if gap == 1 {
            running += 1;
        } else {
            running = 1;
        }
        if running > longest_streak {
            longest_streak = running;
        }
    }

    // The current streak is the run ending at the last date.
    // Walk backwards from the end.
    let mut current_streak: u32 = 1;
    for i in (1..dates.len()).rev() {
        let gap = dates[i].signed_duration_since(dates[i - 1]).num_days();
        if gap == 1 {
            current_streak += 1;
        } else {
            break;
        }
    }

    StreakInfo {
        current_streak,
        longest_streak,
        total_completions,
        last_completed,
    }
}

// ── Completion rate ──────────────────────────────────────────────

/// Fraction of days in `[from, to]` that have at least one completed
/// log entry. Returns 0.0 if the range is empty or no logs fall
/// within it.
pub fn completion_rate(logs: &[HabitLog], from: &NaiveDate, to: &NaiveDate) -> f64 {
    if to < from {
        return 0.0;
    }

    let total_days = to.signed_duration_since(*from).num_days() + 1;
    if total_days <= 0 {
        return 0.0;
    }

    let completed_days: usize = logs
        .iter()
        .filter(|l| l.completed)
        .filter_map(|l| parse_date(&l.date))
        .filter(|d| d >= from && d <= to)
        .collect::<std::collections::HashSet<_>>()
        .len();

    completed_days as f64 / total_days as f64
}

// ── Wellness summary ─────────────────────────────────────────────

/// Compute a weighted-average composite score from wellness
/// categories. Returns a score of 0.0 when the input is empty or
/// total weight is zero.
pub fn wellness_summary(categories: &[WellnessCategory]) -> WellnessSummary {
    let total_weight: f64 = categories.iter().map(|c| c.weight).sum();

    let composite_score = if total_weight > 0.0 {
        let weighted_sum: f64 = categories.iter().map(|c| c.score * c.weight).sum();
        (weighted_sum / total_weight).clamp(0.0, 1.0)
    } else {
        0.0
    };

    WellnessSummary {
        composite_score,
        categories: categories.to_vec(),
    }
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, SignalSpec, TemplateNode, ToolbarAction, WidgetCategory,
        WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "habit-tracker".into(),
            label: "Habit Tracker".into(),
            description: "Track daily habits with streak display".into(),
            icon: Some("check-circle".into()),
            category: WidgetCategory::Display,
            config_fields: vec![FieldSpec::boolean("show_streaks", "Show Streaks")],
            signals: vec![
                SignalSpec::new("habit-completed", "A habit was marked complete").with_payload(
                    vec![
                        FieldSpec::text("habit_id", "Habit ID"),
                        FieldSpec::text("date", "Date"),
                    ],
                ),
                SignalSpec::new("habit-uncompleted", "A habit completion was revoked").with_payload(
                    vec![FieldSpec::text("habit_id", "Habit ID")],
                ),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("complete", "Complete", "check"),
                ToolbarAction::signal("skip", "Skip", "forward"),
            ],
            default_size: WidgetSize::new(2, 2),
            min_size: Some(WidgetSize::new(1, 1)),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Habits", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "habits".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "name".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No habits".into()),
                        },
                        TemplateNode::Conditional {
                            field: "show_streaks".into(),
                            child: Box::new(TemplateNode::DataBinding {
                                field: "current_streak".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            fallback: None,
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "wellness-score".into(),
            label: "Wellness Score".into(),
            description: "Composite wellness score across habit categories".into(),
            icon: Some("heart".into()),
            category: WidgetCategory::Display,
            config_fields: vec![],
            signals: vec![],
            toolbar_actions: vec![],
            default_size: WidgetSize::new(1, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(16),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Wellness", "level": 3}),
                        },
                        TemplateNode::DataBinding {
                            field: "composite_score".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::Repeater {
                            source: "categories".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "name".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No categories".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "habit-heatmap".into(),
            label: "Habit Heatmap".into(),
            description: "Calendar heatmap of habit completions".into(),
            icon: Some("calendar".into()),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::text("habit_id", "Habit ID"),
                FieldSpec::boolean("show_legend", "Show Legend"),
            ],
            signals: vec![SignalSpec::new(
                "date-selected",
                "A date cell was selected",
            )
            .with_payload(vec![FieldSpec::text("date", "Date")])],
            toolbar_actions: vec![
                ToolbarAction::signal("prev-month", "Previous Month", "chevron-left"),
                ToolbarAction::signal("next-month", "Next Month", "chevron-right"),
            ],
            default_size: WidgetSize::new(3, 2),
            min_size: Some(WidgetSize::new(2, 1)),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Habit Heatmap", "level": 3}),
                        },
                        TemplateNode::DataBinding {
                            field: "heatmap_data".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Tests ────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::types::WellnessCategory;

    // ── Helper ───────────────────────────────────────────────────

    fn log(date: &str, completed: bool) -> HabitLog {
        HabitLog {
            id: format!("log-{date}"),
            habit_id: "h1".into(),
            date: date.into(),
            completed,
            value: None,
            notes: None,
        }
    }

    fn log_with_value(date: &str, completed: bool, value: f64) -> HabitLog {
        HabitLog {
            id: format!("log-{date}"),
            habit_id: "h1".into(),
            date: date.into(),
            completed,
            value: Some(value),
            notes: None,
        }
    }

    fn date(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    // ── compute_streak ───────────────────────────────────────────

    #[test]
    fn streak_empty_logs() {
        let info = compute_streak(&[]);
        assert_eq!(info.current_streak, 0);
        assert_eq!(info.longest_streak, 0);
        assert_eq!(info.total_completions, 0);
        assert!(info.last_completed.is_none());
    }

    #[test]
    fn streak_single_completed_day() {
        let logs = vec![log("2025-03-15", true)];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 1);
        assert_eq!(info.longest_streak, 1);
        assert_eq!(info.total_completions, 1);
        assert_eq!(info.last_completed.as_deref(), Some("2025-03-15"));
    }

    #[test]
    fn streak_consecutive_days() {
        let logs = vec![
            log("2025-03-10", true),
            log("2025-03-11", true),
            log("2025-03-12", true),
            log("2025-03-13", true),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 4);
        assert_eq!(info.longest_streak, 4);
        assert_eq!(info.total_completions, 4);
    }

    #[test]
    fn streak_with_gap() {
        let logs = vec![
            log("2025-03-01", true),
            log("2025-03-02", true),
            log("2025-03-03", true),
            // gap on 03-04
            log("2025-03-05", true),
            log("2025-03-06", true),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 2);
        assert_eq!(info.longest_streak, 3);
        assert_eq!(info.total_completions, 5);
        assert_eq!(info.last_completed.as_deref(), Some("2025-03-06"));
    }

    #[test]
    fn streak_ignores_incomplete_logs() {
        let logs = vec![
            log("2025-03-10", true),
            log("2025-03-11", false),
            log("2025-03-12", true),
        ];
        let info = compute_streak(&logs);
        // gap between 03-10 and 03-12 (03-11 not completed)
        assert_eq!(info.current_streak, 1);
        assert_eq!(info.longest_streak, 1);
        assert_eq!(info.total_completions, 2);
    }

    #[test]
    fn streak_unsorted_input() {
        let logs = vec![
            log("2025-03-13", true),
            log("2025-03-10", true),
            log("2025-03-12", true),
            log("2025-03-11", true),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 4);
        assert_eq!(info.longest_streak, 4);
    }

    #[test]
    fn streak_duplicate_dates_deduplicated() {
        let logs = vec![
            log("2025-03-10", true),
            log("2025-03-10", true),
            log("2025-03-11", true),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 2);
        assert_eq!(info.longest_streak, 2);
        // Deduped: only 2 unique dates
        assert_eq!(info.total_completions, 2);
    }

    #[test]
    fn streak_all_incomplete() {
        let logs = vec![
            log("2025-03-10", false),
            log("2025-03-11", false),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 0);
        assert_eq!(info.longest_streak, 0);
        assert_eq!(info.total_completions, 0);
        assert!(info.last_completed.is_none());
    }

    #[test]
    fn streak_multiple_gaps_longest_in_middle() {
        let logs = vec![
            log("2025-03-01", true),
            log("2025-03-02", true),
            // gap
            log("2025-03-05", true),
            log("2025-03-06", true),
            log("2025-03-07", true),
            log("2025-03-08", true),
            log("2025-03-09", true),
            // gap
            log("2025-03-12", true),
        ];
        let info = compute_streak(&logs);
        assert_eq!(info.current_streak, 1);
        assert_eq!(info.longest_streak, 5);
        assert_eq!(info.total_completions, 8);
    }

    // ── completion_rate ──────────────────────────────────────────

    #[test]
    fn rate_empty_logs() {
        let rate = completion_rate(&[], &date("2025-03-01"), &date("2025-03-07"));
        assert!((rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_full_completion() {
        let logs = vec![
            log("2025-03-01", true),
            log("2025-03-02", true),
            log("2025-03-03", true),
        ];
        let rate = completion_rate(&logs, &date("2025-03-01"), &date("2025-03-03"));
        assert!((rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_partial_completion() {
        let logs = vec![
            log("2025-03-01", true),
            log("2025-03-03", true),
        ];
        // 3 days in range, 2 completed
        let rate = completion_rate(&logs, &date("2025-03-01"), &date("2025-03-03"));
        let expected = 2.0 / 3.0;
        assert!((rate - expected).abs() < 1e-10);
    }

    #[test]
    fn rate_excludes_incomplete_logs() {
        let logs = vec![
            log("2025-03-01", true),
            log("2025-03-02", false),
            log("2025-03-03", true),
        ];
        let rate = completion_rate(&logs, &date("2025-03-01"), &date("2025-03-03"));
        let expected = 2.0 / 3.0;
        assert!((rate - expected).abs() < 1e-10);
    }

    #[test]
    fn rate_excludes_out_of_range_logs() {
        let logs = vec![
            log("2025-02-28", true),
            log("2025-03-01", true),
            log("2025-03-04", true),
        ];
        // Only 2025-03-01 is in range [03-01, 03-03]
        let rate = completion_rate(&logs, &date("2025-03-01"), &date("2025-03-03"));
        let expected = 1.0 / 3.0;
        assert!((rate - expected).abs() < 1e-10);
    }

    #[test]
    fn rate_inverted_range_returns_zero() {
        let logs = vec![log("2025-03-01", true)];
        let rate = completion_rate(&logs, &date("2025-03-05"), &date("2025-03-01"));
        assert!((rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn rate_single_day_range() {
        let logs = vec![log("2025-03-15", true)];
        let rate = completion_rate(&logs, &date("2025-03-15"), &date("2025-03-15"));
        assert!((rate - 1.0).abs() < f64::EPSILON);
    }

    // ── wellness_summary ─────────────────────────────────────────

    #[test]
    fn wellness_empty_categories() {
        let summary = wellness_summary(&[]);
        assert!((summary.composite_score - 0.0).abs() < f64::EPSILON);
        assert!(summary.categories.is_empty());
    }

    #[test]
    fn wellness_single_category() {
        let cats = vec![WellnessCategory {
            name: "Sleep".into(),
            score: 0.8,
            weight: 1.0,
        }];
        let summary = wellness_summary(&cats);
        assert!((summary.composite_score - 0.8).abs() < f64::EPSILON);
        assert_eq!(summary.categories.len(), 1);
    }

    #[test]
    fn wellness_weighted_average() {
        let cats = vec![
            WellnessCategory {
                name: "Sleep".into(),
                score: 0.9,
                weight: 2.0,
            },
            WellnessCategory {
                name: "Fitness".into(),
                score: 0.6,
                weight: 1.0,
            },
            WellnessCategory {
                name: "Mindfulness".into(),
                score: 0.3,
                weight: 1.0,
            },
        ];
        let summary = wellness_summary(&cats);
        // (0.9*2 + 0.6*1 + 0.3*1) / (2+1+1) = (1.8+0.6+0.3)/4 = 2.7/4 = 0.675
        assert!((summary.composite_score - 0.675).abs() < 1e-10);
    }

    #[test]
    fn wellness_zero_weight_returns_zero() {
        let cats = vec![WellnessCategory {
            name: "Nothing".into(),
            score: 1.0,
            weight: 0.0,
        }];
        let summary = wellness_summary(&cats);
        assert!((summary.composite_score - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wellness_score_clamped_to_unit_range() {
        let cats = vec![WellnessCategory {
            name: "Over".into(),
            score: 1.5,
            weight: 1.0,
        }];
        let summary = wellness_summary(&cats);
        assert!(summary.composite_score <= 1.0);
    }

    // ── Serialization round-trips ────────────────────────────────

    #[test]
    fn streak_info_serialization() {
        let info = StreakInfo {
            current_streak: 5,
            longest_streak: 10,
            total_completions: 42,
            last_completed: Some("2025-03-15".into()),
        };
        let json = serde_json::to_string(&info).expect("serialize");
        let deser: StreakInfo = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser, info);
    }

    #[test]
    fn habit_log_with_optional_fields() {
        let log = log_with_value("2025-03-15", true, 8.0);
        let json = serde_json::to_string(&log).expect("serialize");
        assert!(json.contains("\"value\":8.0"));
        let deser: HabitLog = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.value, Some(8.0));
    }

    // ── Widget contributions ─────────────────────────────────────

    #[test]
    fn habits_widget_contributions_count_and_ids() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "habit-tracker");
        assert_eq!(widgets[1].id, "wellness-score");
        assert_eq!(widgets[2].id, "habit-heatmap");
    }

    #[test]
    fn habits_widgets_are_display_category() {
        use crate::widget::WidgetCategory;
        let widgets = widget_contributions();
        for w in &widgets {
            assert!(matches!(w.category, WidgetCategory::Display));
        }
    }

    #[test]
    fn habit_tracker_has_expected_signals() {
        let widgets = widget_contributions();
        let tracker = &widgets[0];
        assert_eq!(tracker.signals.len(), 2);
        assert_eq!(tracker.signals[0].name, "habit-completed");
        assert_eq!(tracker.signals[1].name, "habit-uncompleted");
    }

    #[test]
    fn habit_heatmap_has_config_fields() {
        let widgets = widget_contributions();
        let heatmap = &widgets[2];
        assert_eq!(heatmap.config_fields.len(), 2);
        assert_eq!(heatmap.config_fields[0].key, "habit_id");
        assert_eq!(heatmap.config_fields[1].key, "show_legend");
    }
}
