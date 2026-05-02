//! Goals engine — goal hierarchy, milestone tracking, progress.
//!
//! Port of `@helm/goals` TypeScript module. Computes progress from
//! milestones or child goals, checks deadline proximity, and
//! rolls up progress through a goal hierarchy.

use chrono::NaiveDate;
use indexmap::IndexMap;

use super::types::*;
use crate::domain::calendar::engine::parse_date_str;

pub fn compute_goal_progress(
    goal: &Goal,
    milestones: &[Milestone],
    child_progresses: &[f64],
) -> GoalProgress {
    let goal_milestones: Vec<&Milestone> =
        milestones.iter().filter(|m| m.goal_id == goal.id).collect();

    let milestones_completed = goal_milestones.iter().filter(|m| m.completed).count() as u32;
    let milestones_total = goal_milestones.len() as u32;

    let progress = match &goal.progress_mode {
        ProgressMode::Manual { progress } => progress.clamp(0.0, 1.0),
        ProgressMode::Milestones => {
            if milestones_total == 0 {
                0.0
            } else {
                milestones_completed as f64 / milestones_total as f64
            }
        }
        ProgressMode::Children => {
            if child_progresses.is_empty() {
                0.0
            } else {
                child_progresses.iter().sum::<f64>() / child_progresses.len() as f64
            }
        }
    };

    let children_completed = child_progresses.iter().filter(|&&p| p >= 1.0).count() as u32;
    let children_total = child_progresses.len() as u32;

    GoalProgress {
        goal_id: goal.id.clone(),
        progress,
        milestones_completed,
        milestones_total,
        children_completed,
        children_total,
    }
}

pub fn compute_hierarchy_progress(goals: &[Goal], milestones: &[Milestone]) -> Vec<GoalProgress> {
    let goal_map: IndexMap<&str, &Goal> = goals.iter().map(|g| (g.id.as_str(), g)).collect();
    let mut children_map: IndexMap<&str, Vec<&str>> = IndexMap::new();
    for goal in goals {
        if let Some(pid) = &goal.parent_id {
            children_map.entry(pid.as_str()).or_default().push(&goal.id);
        }
    }

    let mut progress_map: IndexMap<String, f64> = IndexMap::new();
    let mut result: Vec<GoalProgress> = Vec::new();

    // Topological order: process leaves first.
    let order = topo_sort_goals(goals);
    for goal_id in &order {
        let goal = match goal_map.get(goal_id.as_str()) {
            Some(g) => g,
            None => continue,
        };
        let child_progresses: Vec<f64> = children_map
            .get(goal_id.as_str())
            .map(|kids| {
                kids.iter()
                    .filter_map(|kid| progress_map.get(*kid).copied())
                    .collect()
            })
            .unwrap_or_default();

        let gp = compute_goal_progress(goal, milestones, &child_progresses);
        progress_map.insert(goal_id.clone(), gp.progress);
        result.push(gp);
    }

    result
}

fn topo_sort_goals(goals: &[Goal]) -> Vec<String> {
    let mut children_map: IndexMap<String, Vec<String>> = IndexMap::new();
    for goal in goals {
        if let Some(pid) = &goal.parent_id {
            children_map
                .entry(pid.clone())
                .or_default()
                .push(goal.id.clone());
        }
    }

    let mut ordered = Vec::new();
    let mut visited: IndexMap<String, bool> = IndexMap::new();

    fn visit(
        id: &str,
        children_map: &IndexMap<String, Vec<String>>,
        visited: &mut IndexMap<String, bool>,
        ordered: &mut Vec<String>,
    ) {
        if visited.get(id).copied().unwrap_or(false) {
            return;
        }
        visited.insert(id.to_string(), true);
        if let Some(kids) = children_map.get(id) {
            for kid in kids {
                visit(kid, children_map, visited, ordered);
            }
        }
        ordered.push(id.to_string());
    }

    // Start from roots, but post-order means leaves come first.
    for goal in goals {
        visit(&goal.id, &children_map, &mut visited, &mut ordered);
    }

    ordered
}

pub fn check_deadline_alerts(goals: &[Goal], today: &NaiveDate) -> Vec<GoalAlert> {
    let mut alerts = Vec::new();
    for goal in goals {
        if matches!(goal.status, GoalStatus::Completed | GoalStatus::Abandoned) {
            continue;
        }
        if let Some(target) = &goal.target_date {
            if let Some(due) = parse_date_str(target) {
                let days = (due - *today).num_days();
                let alert = if days < 0 {
                    DeadlineAlert::Overdue
                } else if days <= 7 {
                    DeadlineAlert::DueSoon
                } else {
                    DeadlineAlert::OnTrack
                };
                alerts.push(GoalAlert {
                    goal_id: goal.id.clone(),
                    alert,
                    days_until_due: days,
                });
            }
        }
    }
    alerts
}

pub fn milestone_completion_rate(milestones: &[Milestone]) -> f64 {
    if milestones.is_empty() {
        return 0.0;
    }
    let completed = milestones.iter().filter(|m| m.completed).count();
    completed as f64 / milestones.len() as f64
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, NumericBounds, QuerySort, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "goal-progress".into(),
            label: "Goal Progress".into(),
            description: "Progress bar for a goal with milestone breakdown".into(),
            icon: Some("target".into()),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::boolean("show_milestones", "Show Milestones"),
                FieldSpec::boolean("show_children", "Show Child Goals"),
            ],
            signals: vec![SignalSpec::new("goal-selected", "A goal was selected")
                .with_payload(vec![FieldSpec::text("goal_id", "Goal ID")])],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh")],
            default_size: WidgetSize::new(2, 1),
            data_query: Some(DataQuery {
                object_type: Some("goal".into()),
                sort: vec![QuerySort {
                    field: "title".into(),
                    descending: false,
                }],
                ..Default::default()
            }),
            data_key: Some("goals".into()),
            data_fields: vec![
                FieldSpec::text("title", "Title"),
                FieldSpec::number("progress", "Progress", NumericBounds::min_max(0.0, 1.0)),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![TemplateNode::Repeater {
                        source: "goals".into(),
                        item_template: Box::new(TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        }),
                        empty_label: Some("No goals".into()),
                    }],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "goal-tree".into(),
            label: "Goal Tree".into(),
            description: "Hierarchical goal overview with rollup progress".into(),
            icon: Some("git-branch".into()),
            category: WidgetCategory::Display,
            signals: vec![SignalSpec::new("goal-selected", "A goal was selected")
                .with_payload(vec![FieldSpec::text("goal_id", "Goal ID")])],
            toolbar_actions: vec![
                ToolbarAction::signal("expand-all", "Expand All", "maximize"),
                ToolbarAction::signal("collapse-all", "Collapse All", "minimize"),
            ],
            default_size: WidgetSize::new(2, 2),
            data_query: Some(DataQuery {
                object_type: Some("goal".into()),
                ..Default::default()
            }),
            data_key: Some("goals".into()),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Goals", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "goals".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No goals".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "milestone-checklist".into(),
            label: "Milestone Checklist".into(),
            description: "Checklist of milestones for a goal".into(),
            icon: Some("check-square".into()),
            category: WidgetCategory::Display,
            signals: vec![
                SignalSpec::new("milestone-toggled", "A milestone was toggled")
                    .with_payload(vec![FieldSpec::text("milestone_id", "Milestone ID")]),
            ],
            toolbar_actions: vec![ToolbarAction::signal("add-milestone", "Add", "plus")],
            default_size: WidgetSize::new(2, 2),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![TemplateNode::Repeater {
                        source: "milestones".into(),
                        item_template: Box::new(TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        }),
                        empty_label: Some("No milestones".into()),
                    }],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn d(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn make_goal(id: &str, mode: ProgressMode) -> Goal {
        Goal {
            id: id.into(),
            title: format!("Goal {id}"),
            description: String::new(),
            parent_id: None,
            status: GoalStatus::InProgress,
            target_date: None,
            created_at: "2026-01-01".into(),
            progress_mode: mode,
        }
    }

    fn make_milestone(id: &str, goal_id: &str, completed: bool) -> Milestone {
        Milestone {
            id: id.into(),
            goal_id: goal_id.into(),
            title: format!("Milestone {id}"),
            completed,
            target_date: None,
            completed_at: None,
            order: 0,
        }
    }

    #[test]
    fn manual_progress() {
        let goal = make_goal("g1", ProgressMode::Manual { progress: 0.75 });
        let gp = compute_goal_progress(&goal, &[], &[]);
        assert!((gp.progress - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn manual_progress_clamped() {
        let goal = make_goal("g1", ProgressMode::Manual { progress: 1.5 });
        let gp = compute_goal_progress(&goal, &[], &[]);
        assert!((gp.progress - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn milestone_progress_all_done() {
        let goal = make_goal("g1", ProgressMode::Milestones);
        let ms = vec![
            make_milestone("m1", "g1", true),
            make_milestone("m2", "g1", true),
        ];
        let gp = compute_goal_progress(&goal, &ms, &[]);
        assert!((gp.progress - 1.0).abs() < f64::EPSILON);
        assert_eq!(gp.milestones_completed, 2);
        assert_eq!(gp.milestones_total, 2);
    }

    #[test]
    fn milestone_progress_partial() {
        let goal = make_goal("g1", ProgressMode::Milestones);
        let ms = vec![
            make_milestone("m1", "g1", true),
            make_milestone("m2", "g1", false),
            make_milestone("m3", "g1", false),
        ];
        let gp = compute_goal_progress(&goal, &ms, &[]);
        assert!((gp.progress - 1.0 / 3.0).abs() < 0.001);
    }

    #[test]
    fn milestone_progress_none() {
        let goal = make_goal("g1", ProgressMode::Milestones);
        let gp = compute_goal_progress(&goal, &[], &[]);
        assert!((gp.progress).abs() < f64::EPSILON);
    }

    #[test]
    fn children_progress_average() {
        let goal = make_goal("g1", ProgressMode::Children);
        let children = vec![0.5, 1.0, 0.0];
        let gp = compute_goal_progress(&goal, &[], &children);
        assert!((gp.progress - 0.5).abs() < f64::EPSILON);
        assert_eq!(gp.children_completed, 1);
        assert_eq!(gp.children_total, 3);
    }

    #[test]
    fn children_progress_empty() {
        let goal = make_goal("g1", ProgressMode::Children);
        let gp = compute_goal_progress(&goal, &[], &[]);
        assert!((gp.progress).abs() < f64::EPSILON);
    }

    #[test]
    fn hierarchy_progress_rollup() {
        let mut parent = make_goal("parent", ProgressMode::Children);
        parent.parent_id = None;

        let mut child1 = make_goal("child1", ProgressMode::Milestones);
        child1.parent_id = Some("parent".into());

        let mut child2 = make_goal("child2", ProgressMode::Manual { progress: 0.5 });
        child2.parent_id = Some("parent".into());

        let goals = vec![parent, child1, child2];
        let milestones = vec![
            make_milestone("m1", "child1", true),
            make_milestone("m2", "child1", true),
        ];

        let results = compute_hierarchy_progress(&goals, &milestones);
        let parent_progress = results.iter().find(|r| r.goal_id == "parent").unwrap();
        // child1 = 1.0 (2/2 milestones), child2 = 0.5 → parent = 0.75
        assert!((parent_progress.progress - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn deadline_alert_overdue() {
        let mut goal = make_goal("g1", ProgressMode::Manual { progress: 0.0 });
        goal.target_date = Some("2026-04-01".into());
        let alerts = check_deadline_alerts(&[goal], &d("2026-05-01"));
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].alert, DeadlineAlert::Overdue);
        assert_eq!(alerts[0].days_until_due, -30);
    }

    #[test]
    fn deadline_alert_due_soon() {
        let mut goal = make_goal("g1", ProgressMode::Manual { progress: 0.0 });
        goal.target_date = Some("2026-05-05".into());
        let alerts = check_deadline_alerts(&[goal], &d("2026-05-01"));
        assert_eq!(alerts[0].alert, DeadlineAlert::DueSoon);
        assert_eq!(alerts[0].days_until_due, 4);
    }

    #[test]
    fn deadline_alert_on_track() {
        let mut goal = make_goal("g1", ProgressMode::Manual { progress: 0.0 });
        goal.target_date = Some("2026-06-01".into());
        let alerts = check_deadline_alerts(&[goal], &d("2026-05-01"));
        assert_eq!(alerts[0].alert, DeadlineAlert::OnTrack);
    }

    #[test]
    fn completed_goals_excluded_from_alerts() {
        let mut goal = make_goal("g1", ProgressMode::Manual { progress: 1.0 });
        goal.status = GoalStatus::Completed;
        goal.target_date = Some("2026-04-01".into());
        let alerts = check_deadline_alerts(&[goal], &d("2026-05-01"));
        assert!(alerts.is_empty());
    }

    #[test]
    fn milestone_completion_rate_mixed() {
        let ms = vec![
            make_milestone("m1", "g1", true),
            make_milestone("m2", "g1", false),
            make_milestone("m3", "g1", true),
            make_milestone("m4", "g1", false),
        ];
        assert!((milestone_completion_rate(&ms) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn milestone_completion_rate_empty() {
        assert!((milestone_completion_rate(&[]) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn goals_without_deadline_have_no_alerts() {
        let goal = make_goal("g1", ProgressMode::Manual { progress: 0.0 });
        let alerts = check_deadline_alerts(&[goal], &d("2026-05-01"));
        assert!(alerts.is_empty());
    }

    #[test]
    fn widget_contributions_count() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "goal-progress");
        assert_eq!(widgets[1].id, "goal-tree");
        assert_eq!(widgets[2].id, "milestone-checklist");
    }
}
