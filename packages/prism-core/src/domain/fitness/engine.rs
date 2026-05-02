//! Fitness engine — calorie estimation, personal bests.
//!
//! Port of `@helm/fitness` TypeScript module. Provides MET-based
//! calorie estimation, per-exercise personal-best tracking, and
//! aggregate fitness summaries.

use std::collections::HashMap;

use serde_json::json;

use super::types::{
    CalorieEstimate, ExerciseSet, FitnessLog, FitnessSummary, PersonalBest, MET_CYCLING,
    MET_RUNNING, MET_SWIMMING, MET_WALKING, MET_WEIGHT_TRAINING, MET_YOGA,
};

// ── Calorie Estimation ──────────────────────────────────────────

/// Estimate calories burned using the MET formula.
///
/// Formula: `calories = MET * weight_kg * (duration_minutes / 60.0)`
pub fn estimate_calories(
    duration_minutes: f64,
    weight_kg: f64,
    met_value: f64,
) -> CalorieEstimate {
    let calories = met_value * weight_kg * (duration_minutes / 60.0);
    CalorieEstimate {
        calories,
        met_value,
        duration_minutes,
    }
}

// ── MET Lookup ──────────────────────────────────────────────────

/// Look up a common MET value by activity name (case-insensitive).
/// Returns `MET_WEIGHT_TRAINING` (6.0) as the default for unknown
/// activities.
pub fn lookup_met_value(activity: &str) -> f64 {
    match activity.to_lowercase().as_str() {
        "walking" | "walk" => MET_WALKING,
        "running" | "run" | "jogging" | "jog" => MET_RUNNING,
        "cycling" | "cycle" | "biking" | "bike" => MET_CYCLING,
        "swimming" | "swim" => MET_SWIMMING,
        "weights" | "weight training" | "weightlifting" | "strength" | "strength training" => {
            MET_WEIGHT_TRAINING
        }
        "yoga" | "stretching" => MET_YOGA,
        _ => MET_WEIGHT_TRAINING,
    }
}

// ── Personal Bests ──────────────────────────────────────────────

/// Compute best single-set volume from a slice of sets.
fn best_set_metrics(sets: &[ExerciseSet]) -> (f64, u32, f64) {
    let mut max_weight: f64 = 0.0;
    let mut max_reps: u32 = 0;
    let mut max_volume: f64 = 0.0;

    for set in sets {
        if set.weight_kg > max_weight {
            max_weight = set.weight_kg;
        }
        if set.reps > max_reps {
            max_reps = set.reps;
        }
        let volume = set.weight_kg * set.reps as f64;
        if volume > max_volume {
            max_volume = volume;
        }
    }

    (max_weight, max_reps, max_volume)
}

/// Compute personal bests across all logs, grouped by exercise
/// (activity name). Only logs with set data contribute to PRs.
pub fn compute_personal_bests(logs: &[FitnessLog]) -> Vec<PersonalBest> {
    // Track per-exercise: (max_weight, max_reps, max_volume, date)
    let mut bests: HashMap<String, (f64, u32, f64, String)> = HashMap::new();

    for log in logs {
        let sets = match &log.sets {
            Some(s) if !s.is_empty() => s,
            _ => continue,
        };

        let (w, r, v) = best_set_metrics(sets);

        let entry = bests
            .entry(log.activity.clone())
            .or_insert((0.0, 0, 0.0, log.date.clone()));

        let mut updated = false;
        if w > entry.0 {
            entry.0 = w;
            updated = true;
        }
        if r > entry.1 {
            entry.1 = r;
            updated = true;
        }
        if v > entry.2 {
            entry.2 = v;
            updated = true;
        }
        if updated {
            entry.3 = log.date.clone();
        }
    }

    let mut result: Vec<PersonalBest> = bests
        .into_iter()
        .map(|(exercise, (max_weight_kg, max_reps, max_volume, achieved_date))| PersonalBest {
            exercise,
            max_weight_kg,
            max_reps,
            max_volume,
            achieved_date,
        })
        .collect();

    // Sort by exercise name for deterministic output.
    result.sort_by(|a, b| a.exercise.cmp(&b.exercise));
    result
}

// ── Fitness Summary ─────────────────────────────────────────────

/// Build an aggregate fitness summary from a collection of logs.
///
/// `default_weight_kg` is used for calorie estimation when a log
/// does not specify `weight_kg`.
pub fn fitness_summary(logs: &[FitnessLog], default_weight_kg: f64) -> FitnessSummary {
    let total_workouts = logs.len() as u32;
    let mut total_duration_minutes = 0.0;
    let mut total_calories = 0.0;

    for log in logs {
        total_duration_minutes += log.duration_minutes;

        let weight = log.weight_kg.unwrap_or(default_weight_kg);
        let met = log.met_value.unwrap_or_else(|| lookup_met_value(&log.activity));
        let est = estimate_calories(log.duration_minutes, weight, met);
        total_calories += est.calories;
    }

    let personal_bests = compute_personal_bests(logs);

    FitnessSummary {
        total_workouts,
        total_duration_minutes,
        total_calories,
        personal_bests,
    }
}

// ── Widget Contributions ────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, NumericBounds, QuerySort, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };

    vec![
        WidgetContribution {
            id: "fitness-log".into(),
            label: "Fitness Log".into(),
            description: "Workout history and calorie tracking".into(),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::number("limit", "Limit", NumericBounds::unbounded())
                    .with_default(json!(20)),
            ],
            signals: vec![
                SignalSpec::new("log-selected", "A workout log was selected")
                    .with_payload(vec![FieldSpec::text("log_id", "Log ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("new-log", "New Log", "add"),
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
            ],
            default_size: WidgetSize::new(2, 2),
            data_query: Some(DataQuery {
                object_type: Some("fitness_log".into()),
                sort: vec![QuerySort {
                    field: "date".into(),
                    descending: true,
                }],
                limit: Some(20),
                ..Default::default()
            }),
            data_key: Some("logs".into()),
            data_fields: vec![
                FieldSpec::text("activity", "Activity"),
                FieldSpec::text("date", "Date"),
                FieldSpec::number(
                    "duration_minutes",
                    "Duration (min)",
                    NumericBounds::unbounded(),
                ),
                FieldSpec::number("calories", "Calories", NumericBounds::unbounded()),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Workout Log", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "logs".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "log"}),
                            }),
                            empty_label: Some("No workouts logged".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "personal-bests".into(),
            label: "Personal Bests".into(),
            description: "PR board — all-time personal records".into(),
            category: WidgetCategory::Display,
            config_fields: vec![],
            signals: vec![
                SignalSpec::new("exercise-selected", "An exercise was selected")
                    .with_payload(vec![FieldSpec::text("exercise", "Exercise")]),
            ],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh")],
            default_size: WidgetSize::new(2, 1),
            data_query: Some(DataQuery {
                object_type: Some("fitness_log".into()),
                ..Default::default()
            }),
            data_key: Some("personal_bests".into()),
            data_fields: vec![
                FieldSpec::text("exercise", "Exercise"),
                FieldSpec::number("max_weight_kg", "Max Weight (kg)", NumericBounds::unbounded()),
                FieldSpec::number("max_reps", "Max Reps", NumericBounds::unbounded()),
                FieldSpec::number("max_volume", "Max Volume", NumericBounds::unbounded()),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Personal Bests", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "personal_bests".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "record"}),
                            }),
                            empty_label: Some("No records yet".into()),
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

    // ── estimate_calories ────────────────────────────────────────

    #[test]
    fn estimate_calories_basic() {
        // 30 min, 75 kg, MET 9.8 (running)
        // Expected: 9.8 * 75.0 * (30.0 / 60.0) = 367.5
        let est = estimate_calories(30.0, 75.0, MET_RUNNING);
        assert!((est.calories - 367.5).abs() < 0.01);
        assert_eq!(est.met_value, MET_RUNNING);
        assert_eq!(est.duration_minutes, 30.0);
    }

    #[test]
    fn estimate_calories_one_hour_walking() {
        // 60 min, 70 kg, MET 3.5 (walking)
        // Expected: 3.5 * 70.0 * 1.0 = 245.0
        let est = estimate_calories(60.0, 70.0, MET_WALKING);
        assert!((est.calories - 245.0).abs() < 0.01);
    }

    #[test]
    fn estimate_calories_zero_duration() {
        let est = estimate_calories(0.0, 80.0, MET_CYCLING);
        assert_eq!(est.calories, 0.0);
    }

    #[test]
    fn estimate_calories_zero_weight() {
        let est = estimate_calories(30.0, 0.0, MET_SWIMMING);
        assert_eq!(est.calories, 0.0);
    }

    // ── lookup_met_value ─────────────────────────────────────────

    #[test]
    fn lookup_met_known_activities() {
        assert_eq!(lookup_met_value("walking"), MET_WALKING);
        assert_eq!(lookup_met_value("Running"), MET_RUNNING);
        assert_eq!(lookup_met_value("CYCLING"), MET_CYCLING);
        assert_eq!(lookup_met_value("swim"), MET_SWIMMING);
        assert_eq!(lookup_met_value("Yoga"), MET_YOGA);
        assert_eq!(lookup_met_value("weights"), MET_WEIGHT_TRAINING);
        assert_eq!(lookup_met_value("strength training"), MET_WEIGHT_TRAINING);
    }

    #[test]
    fn lookup_met_unknown_defaults_to_weight_training() {
        assert_eq!(lookup_met_value("rock climbing"), MET_WEIGHT_TRAINING);
        assert_eq!(lookup_met_value(""), MET_WEIGHT_TRAINING);
    }

    // ── compute_personal_bests ───────────────────────────────────

    #[test]
    fn personal_bests_single_exercise() {
        let logs = vec![
            FitnessLog {
                id: "1".into(),
                date: "2026-04-01".into(),
                activity: "bench press".into(),
                duration_minutes: 45.0,
                weight_kg: Some(80.0),
                met_value: None,
                sets: Some(vec![
                    ExerciseSet { reps: 8, weight_kg: 60.0, duration_seconds: None },
                    ExerciseSet { reps: 6, weight_kg: 80.0, duration_seconds: None },
                ]),
                notes: None,
            },
            FitnessLog {
                id: "2".into(),
                date: "2026-04-15".into(),
                activity: "bench press".into(),
                duration_minutes: 45.0,
                weight_kg: Some(80.0),
                met_value: None,
                sets: Some(vec![
                    ExerciseSet { reps: 10, weight_kg: 70.0, duration_seconds: None },
                    ExerciseSet { reps: 5, weight_kg: 90.0, duration_seconds: None },
                ]),
                notes: None,
            },
        ];

        let pbs = compute_personal_bests(&logs);
        assert_eq!(pbs.len(), 1);
        assert_eq!(pbs[0].exercise, "bench press");
        assert_eq!(pbs[0].max_weight_kg, 90.0);
        assert_eq!(pbs[0].max_reps, 10);
        // Max volume: max(60*8=480, 80*6=480, 70*10=700, 90*5=450) = 700
        assert!((pbs[0].max_volume - 700.0).abs() < 0.01);
    }

    #[test]
    fn personal_bests_multiple_exercises() {
        let logs = vec![
            FitnessLog {
                id: "1".into(),
                date: "2026-04-01".into(),
                activity: "squat".into(),
                duration_minutes: 40.0,
                weight_kg: None,
                met_value: None,
                sets: Some(vec![
                    ExerciseSet { reps: 5, weight_kg: 120.0, duration_seconds: None },
                ]),
                notes: None,
            },
            FitnessLog {
                id: "2".into(),
                date: "2026-04-01".into(),
                activity: "deadlift".into(),
                duration_minutes: 30.0,
                weight_kg: None,
                met_value: None,
                sets: Some(vec![
                    ExerciseSet { reps: 3, weight_kg: 180.0, duration_seconds: None },
                ]),
                notes: None,
            },
        ];

        let pbs = compute_personal_bests(&logs);
        assert_eq!(pbs.len(), 2);
        // Sorted by exercise name.
        assert_eq!(pbs[0].exercise, "deadlift");
        assert_eq!(pbs[0].max_weight_kg, 180.0);
        assert_eq!(pbs[1].exercise, "squat");
        assert_eq!(pbs[1].max_weight_kg, 120.0);
    }

    #[test]
    fn personal_bests_empty_logs() {
        let pbs = compute_personal_bests(&[]);
        assert!(pbs.is_empty());
    }

    #[test]
    fn personal_bests_logs_without_sets_are_skipped() {
        let logs = vec![FitnessLog {
            id: "1".into(),
            date: "2026-04-01".into(),
            activity: "running".into(),
            duration_minutes: 30.0,
            weight_kg: Some(75.0),
            met_value: Some(MET_RUNNING),
            sets: None,
            notes: None,
        }];

        let pbs = compute_personal_bests(&logs);
        assert!(pbs.is_empty());
    }

    #[test]
    fn personal_bests_empty_sets_are_skipped() {
        let logs = vec![FitnessLog {
            id: "1".into(),
            date: "2026-04-01".into(),
            activity: "bench press".into(),
            duration_minutes: 45.0,
            weight_kg: None,
            met_value: None,
            sets: Some(vec![]),
            notes: None,
        }];

        let pbs = compute_personal_bests(&logs);
        assert!(pbs.is_empty());
    }

    // ── fitness_summary ──────────────────────────────────────────

    #[test]
    fn fitness_summary_basic() {
        let logs = vec![
            FitnessLog {
                id: "1".into(),
                date: "2026-04-01".into(),
                activity: "running".into(),
                duration_minutes: 30.0,
                weight_kg: Some(75.0),
                met_value: Some(MET_RUNNING),
                sets: None,
                notes: None,
            },
            FitnessLog {
                id: "2".into(),
                date: "2026-04-02".into(),
                activity: "walking".into(),
                duration_minutes: 60.0,
                weight_kg: None,
                met_value: None,
                sets: None,
                notes: None,
            },
        ];

        let summary = fitness_summary(&logs, 70.0);
        assert_eq!(summary.total_workouts, 2);
        assert_eq!(summary.total_duration_minutes, 90.0);

        // Log 1: 9.8 * 75.0 * 0.5 = 367.5
        // Log 2: 3.5 * 70.0 * 1.0 = 245.0 (default weight, looked-up MET)
        let expected_calories = 367.5 + 245.0;
        assert!((summary.total_calories - expected_calories).abs() < 0.01);
    }

    #[test]
    fn fitness_summary_empty_logs() {
        let summary = fitness_summary(&[], 70.0);
        assert_eq!(summary.total_workouts, 0);
        assert_eq!(summary.total_duration_minutes, 0.0);
        assert_eq!(summary.total_calories, 0.0);
        assert!(summary.personal_bests.is_empty());
    }

    #[test]
    fn fitness_summary_uses_default_weight() {
        let logs = vec![FitnessLog {
            id: "1".into(),
            date: "2026-04-01".into(),
            activity: "cycling".into(),
            duration_minutes: 45.0,
            weight_kg: None,
            met_value: None,
            sets: None,
            notes: None,
        }];

        let summary = fitness_summary(&logs, 80.0);
        // MET_CYCLING = 7.5, weight = 80 (default), duration = 45 min
        // 7.5 * 80.0 * (45.0 / 60.0) = 450.0
        assert!((summary.total_calories - 450.0).abs() < 0.01);
    }

    #[test]
    fn fitness_summary_includes_personal_bests() {
        let logs = vec![FitnessLog {
            id: "1".into(),
            date: "2026-04-01".into(),
            activity: "squat".into(),
            duration_minutes: 40.0,
            weight_kg: Some(85.0),
            met_value: Some(MET_WEIGHT_TRAINING),
            sets: Some(vec![
                ExerciseSet { reps: 5, weight_kg: 100.0, duration_seconds: None },
            ]),
            notes: None,
        }];

        let summary = fitness_summary(&logs, 70.0);
        assert_eq!(summary.personal_bests.len(), 1);
        assert_eq!(summary.personal_bests[0].exercise, "squat");
        assert_eq!(summary.personal_bests[0].max_weight_kg, 100.0);
    }

    // ── widget_contributions ─────────────────────────────────────

    #[test]
    fn widget_contributions_returns_2_widgets() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 2);
        let ids: Vec<&str> = widgets.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"fitness-log"));
        assert!(ids.contains(&"personal-bests"));
    }

    #[test]
    fn widget_contributions_have_display_category() {
        use crate::widget::WidgetCategory;
        let widgets = widget_contributions();
        for w in &widgets {
            assert!(matches!(w.category, WidgetCategory::Display));
        }
    }
}
