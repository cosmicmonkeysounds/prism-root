//! Named routines for the command palette.
//!
//! Six workflow routines that Flux exposes through the command palette
//! and keyboard shortcuts. Each routine is a named sequence of steps
//! the user follows — the data model here, the execution in the shell.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Routine {
    pub id: String,
    pub label: String,
    pub description: String,
    pub shortcut: Option<String>,
    pub category: RoutineCategory,
    pub steps: Vec<RoutineStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutineCategory {
    Planning,
    Review,
    Capture,
    Finance,
    Wellness,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoutineStep {
    pub label: String,
    pub action: RoutineAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RoutineAction {
    OpenPanel(String),
    RunCommand(String),
    ShowPrompt(String),
    NavigateTo(String),
}

pub fn default_routines() -> Vec<Routine> {
    vec![
        Routine {
            id: "morning-planning".into(),
            label: "Morning Planning".into(),
            description: "Review today's tasks, set priorities, and plan your focus blocks".into(),
            shortcut: Some("Ctrl+Shift+M".into()),
            category: RoutineCategory::Planning,
            steps: vec![
                RoutineStep {
                    label: "Open Day Planner".into(),
                    action: RoutineAction::OpenPanel("day-planner".into()),
                },
                RoutineStep {
                    label: "Review overdue tasks".into(),
                    action: RoutineAction::RunCommand("flux.filter-overdue".into()),
                },
                RoutineStep {
                    label: "Set today's focus items".into(),
                    action: RoutineAction::ShowPrompt(
                        "What are your top 3 priorities today?".into(),
                    ),
                },
            ],
        },
        Routine {
            id: "weekly-review".into(),
            label: "Weekly Review".into(),
            description: "Review the week's progress, update goals, and plan next week".into(),
            shortcut: None,
            category: RoutineCategory::Review,
            steps: vec![
                RoutineStep {
                    label: "Review completed tasks".into(),
                    action: RoutineAction::RunCommand("flux.filter-completed-this-week".into()),
                },
                RoutineStep {
                    label: "Update goal progress".into(),
                    action: RoutineAction::OpenPanel("goals".into()),
                },
                RoutineStep {
                    label: "Review upcoming deadlines".into(),
                    action: RoutineAction::RunCommand("flux.filter-due-next-week".into()),
                },
                RoutineStep {
                    label: "Check project health".into(),
                    action: RoutineAction::OpenPanel("projects".into()),
                },
            ],
        },
        Routine {
            id: "quick-capture".into(),
            label: "Quick Capture".into(),
            description: "Capture a thought, task, or note without context switching".into(),
            shortcut: Some("Ctrl+Shift+C".into()),
            category: RoutineCategory::Capture,
            steps: vec![RoutineStep {
                label: "Open capture input".into(),
                action: RoutineAction::RunCommand("flux.quick-create".into()),
            }],
        },
        Routine {
            id: "end-of-day".into(),
            label: "End of Day".into(),
            description: "Log time, update task statuses, and review tomorrow".into(),
            shortcut: None,
            category: RoutineCategory::Review,
            steps: vec![
                RoutineStep {
                    label: "Review today's time entries".into(),
                    action: RoutineAction::OpenPanel("time-tracking".into()),
                },
                RoutineStep {
                    label: "Update in-progress tasks".into(),
                    action: RoutineAction::RunCommand("flux.filter-in-progress".into()),
                },
                RoutineStep {
                    label: "Preview tomorrow".into(),
                    action: RoutineAction::RunCommand("flux.show-tomorrow".into()),
                },
            ],
        },
        Routine {
            id: "finance-check".into(),
            label: "Finance Check".into(),
            description: "Review outstanding invoices and recent transactions".into(),
            shortcut: None,
            category: RoutineCategory::Finance,
            steps: vec![
                RoutineStep {
                    label: "Check overdue invoices".into(),
                    action: RoutineAction::RunCommand("flux.filter-overdue-invoices".into()),
                },
                RoutineStep {
                    label: "Review recent transactions".into(),
                    action: RoutineAction::OpenPanel("finance".into()),
                },
            ],
        },
        Routine {
            id: "wellness-checkin".into(),
            label: "Wellness Check-in".into(),
            description: "Log habits, review streaks, and check wellness score".into(),
            shortcut: None,
            category: RoutineCategory::Wellness,
            steps: vec![
                RoutineStep {
                    label: "Log today's habits".into(),
                    action: RoutineAction::OpenPanel("habits".into()),
                },
                RoutineStep {
                    label: "Review wellness score".into(),
                    action: RoutineAction::NavigateTo("life-dashboard".into()),
                },
            ],
        },
    ]
}

pub fn find_routine(id: &str) -> Option<Routine> {
    default_routines().into_iter().find(|r| r.id == id)
}

pub fn routines_by_category(category: RoutineCategory) -> Vec<Routine> {
    default_routines()
        .into_iter()
        .filter(|r| r.category == category)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_routines_count() {
        assert_eq!(default_routines().len(), 6);
    }

    #[test]
    fn all_routines_have_steps() {
        for routine in default_routines() {
            assert!(
                !routine.steps.is_empty(),
                "routine '{}' has no steps",
                routine.id
            );
        }
    }

    #[test]
    fn unique_ids() {
        let routines = default_routines();
        let mut ids: Vec<&str> = routines.iter().map(|r| r.id.as_str()).collect();
        let len = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), len);
    }

    #[test]
    fn find_routine_by_id() {
        assert!(find_routine("morning-planning").is_some());
        assert!(find_routine("nonexistent").is_none());
    }

    #[test]
    fn filter_by_category() {
        let review = routines_by_category(RoutineCategory::Review);
        assert_eq!(review.len(), 2);
        for r in &review {
            assert_eq!(r.category, RoutineCategory::Review);
        }
    }

    #[test]
    fn routine_serde_roundtrip() {
        let routine = find_routine("weekly-review").unwrap();
        let json = serde_json::to_string(&routine).unwrap();
        let back: Routine = serde_json::from_str(&json).unwrap();
        assert_eq!(back.id, "weekly-review");
        assert_eq!(back.steps.len(), 4);
    }
}
