//! Focus planner engine — daily context generation, plan scoring,
//! and planning-method helpers.
//!
//! Port of `@helm/focus-planner` logic. Pure functions over the
//! types in [`super::types`]; no IO or side effects.

use super::types::{DailyContext, DailyContextItem, DailyPlan, PlanScore, PlanningMethod};

// ── Daily context generation ─────────────────────────────────────

/// Merge tasks, events, habits, and reminders into a single
/// [`DailyContext`], sorted by priority then by time.
pub fn generate_daily_context(
    tasks: &[DailyContextItem],
    events: &[DailyContextItem],
    habits: &[DailyContextItem],
    reminders: &[DailyContextItem],
    date: &str,
) -> DailyContext {
    let mut items: Vec<DailyContextItem> =
        Vec::with_capacity(tasks.len() + events.len() + habits.len() + reminders.len());
    items.extend_from_slice(tasks);
    items.extend_from_slice(events);
    items.extend_from_slice(habits);
    items.extend_from_slice(reminders);

    sort_by_priority(&mut items);

    DailyContext {
        date: date.to_string(),
        task_count: tasks.len() as u32,
        event_count: events.len() as u32,
        habit_count: habits.len() as u32,
        reminder_count: reminders.len() as u32,
        items,
    }
}

// ── Sorting ──────────────────────────────────────────────────────

/// Sort items by priority (ascending — 1 is highest) then by time
/// (items with a time come before items without, lexicographic
/// within).
pub fn sort_by_priority(items: &mut [DailyContextItem]) {
    items.sort_by(|a, b| {
        a.priority
            .cmp(&b.priority)
            .then_with(|| match (&a.time, &b.time) {
                (Some(ta), Some(tb)) => ta.cmp(tb),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            })
    });
}

// ── Plan scoring ─────────────────────────────────────────────────

/// Compute a [`PlanScore`] summarizing a daily plan's outcomes.
pub fn score_plan(plan: &DailyPlan) -> PlanScore {
    let items_planned = plan.items.len() as u32;
    let items_completed = plan.items.iter().filter(|i| i.completed).count() as u32;
    let completion_rate = plan_completion_rate(plan);

    let (avg_energy, avg_focus) = if plan.check_ins.is_empty() {
        (0.0, 0.0)
    } else {
        let n = plan.check_ins.len() as f64;
        let energy_sum: f64 = plan.check_ins.iter().map(|c| c.energy_level as f64).sum();
        let focus_sum: f64 = plan.check_ins.iter().map(|c| c.focus_level as f64).sum();
        (energy_sum / n, focus_sum / n)
    };

    PlanScore {
        completion_rate,
        avg_energy,
        avg_focus,
        items_planned,
        items_completed,
    }
}

// ── Completion rate ──────────────────────────────────────────────

/// Fraction of plan items completed (0.0 if no items).
pub fn plan_completion_rate(plan: &DailyPlan) -> f64 {
    if plan.items.is_empty() {
        return 0.0;
    }
    let completed = plan.items.iter().filter(|i| i.completed).count() as f64;
    completed / plan.items.len() as f64
}

// ── Suggested item count ─────────────────────────────────────────

/// Recommended number of plan items for a given method.
pub fn suggested_item_count(method: &PlanningMethod) -> u32 {
    match method {
        PlanningMethod::Mit => 1,
        PlanningMethod::ThreeThings => 3,
        PlanningMethod::TimeBlocking => 8,
        PlanningMethod::Custom { .. } => 5,
    }
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, NumericBounds, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "daily-plan".into(),
            label: "Daily Plan".into(),
            description: "Today's focus plan items with completion tracking".into(),
            icon: Some("target".into()),
            category: WidgetCategory::Display,
            config_fields: vec![FieldSpec::select(
                "method",
                "Planning Method",
                vec![
                    SelectOption::new("mit", "Most Important Task"),
                    SelectOption::new("three_things", "Three Things"),
                    SelectOption::new("time_blocking", "Time Blocking"),
                ],
            )],
            signals: vec![
                SignalSpec::new("item-toggled", "A plan item was toggled")
                    .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
                SignalSpec::new("plan-completed", "All items completed"),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("add-item", "Add Item", "plus"),
                ToolbarAction::signal("clear", "Clear", "trash"),
            ],
            default_size: WidgetSize::new(2, 2),
            min_size: Some(WidgetSize::new(1, 1)),
            data_key: Some("plan_items".into()),
            data_fields: vec![
                FieldSpec::text("title", "Title"),
                FieldSpec::boolean("completed", "Completed"),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Daily Plan", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "plan_items".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "item"}),
                            }),
                            empty_label: Some("No items planned".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "focus-check-in".into(),
            label: "Focus Check-In".into(),
            description: "Energy, focus, and mood self-rating".into(),
            icon: Some("heart".into()),
            category: WidgetCategory::Input,
            config_fields: vec![],
            signals: vec![
                SignalSpec::new("check-in-submitted", "A check-in was recorded").with_payload(
                    vec![
                        FieldSpec::number("energy", "Energy", NumericBounds::min_max(1.0, 5.0)),
                        FieldSpec::number("focus", "Focus", NumericBounds::min_max(1.0, 5.0)),
                        FieldSpec::number("mood", "Mood", NumericBounds::min_max(1.0, 5.0)),
                    ],
                ),
            ],
            toolbar_actions: vec![ToolbarAction::signal("submit", "Submit", "check")],
            default_size: WidgetSize::new(1, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Check-In", "level": 3}),
                        },
                        TemplateNode::DataBinding {
                            field: "energy".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "focus".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "mood".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "brain-dump".into(),
            label: "Brain Dump".into(),
            description: "Quick-capture list for unstructured thoughts".into(),
            icon: Some("edit".into()),
            category: WidgetCategory::Input,
            config_fields: vec![],
            signals: vec![
                SignalSpec::new("item-added", "A brain dump item was added")
                    .with_payload(vec![FieldSpec::text("text", "Text")]),
                SignalSpec::new("item-removed", "A brain dump item was removed")
                    .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("add", "Add", "plus"),
                ToolbarAction::signal("clear-all", "Clear All", "trash"),
            ],
            default_size: WidgetSize::new(2, 1),
            min_size: Some(WidgetSize::new(1, 1)),
            data_key: Some("dump_items".into()),
            data_fields: vec![FieldSpec::text("text", "Text")],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Brain Dump", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "dump_items".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "item"}),
                            }),
                            empty_label: Some("Nothing captured yet".into()),
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
    use crate::domain::focus_planner::types::{CheckIn, ContextSource, PlanItem, TimeBlock};

    // ── Helpers ──────────────────────────────────────────────────

    fn make_context_item(
        title: &str,
        source: ContextSource,
        priority: u8,
        time: Option<&str>,
    ) -> DailyContextItem {
        DailyContextItem {
            title: title.to_string(),
            source,
            priority,
            time: time.map(String::from),
        }
    }

    fn make_plan_item(id: &str, title: &str, priority: u8, completed: bool) -> PlanItem {
        PlanItem {
            id: id.to_string(),
            title: title.to_string(),
            object_id: None,
            object_type: None,
            priority,
            completed,
            time_block: None,
        }
    }

    fn make_check_in(energy: u8, focus: u8, mood: u8) -> CheckIn {
        CheckIn {
            time: "2026-05-02T10:00:00Z".to_string(),
            energy_level: energy,
            focus_level: focus,
            mood,
            notes: None,
        }
    }

    fn make_plan(items: Vec<PlanItem>, check_ins: Vec<CheckIn>) -> DailyPlan {
        DailyPlan {
            date: "2026-05-02".to_string(),
            method: PlanningMethod::ThreeThings,
            items,
            check_ins,
            brain_dump: Vec::new(),
            reflection: None,
        }
    }

    // ── generate_daily_context ───────────────────────────────────

    #[test]
    fn daily_context_merges_all_sources() {
        let tasks = vec![make_context_item(
            "Write report",
            ContextSource::Task,
            1,
            None,
        )];
        let events = vec![make_context_item(
            "Standup",
            ContextSource::Event,
            2,
            Some("09:00"),
        )];
        let habits = vec![make_context_item(
            "Meditate",
            ContextSource::Habit,
            3,
            Some("07:00"),
        )];
        let reminders = vec![make_context_item(
            "Call dentist",
            ContextSource::Reminder,
            2,
            None,
        )];

        let ctx = generate_daily_context(&tasks, &events, &habits, &reminders, "2026-05-02");

        assert_eq!(ctx.date, "2026-05-02");
        assert_eq!(ctx.task_count, 1);
        assert_eq!(ctx.event_count, 1);
        assert_eq!(ctx.habit_count, 1);
        assert_eq!(ctx.reminder_count, 1);
        assert_eq!(ctx.items.len(), 4);
    }

    #[test]
    fn daily_context_sorted_by_priority_then_time() {
        let tasks = vec![
            make_context_item("Low priority", ContextSource::Task, 5, None),
            make_context_item("High priority", ContextSource::Task, 1, None),
        ];
        let events = vec![
            make_context_item("Afternoon", ContextSource::Event, 1, Some("14:00")),
            make_context_item("Morning", ContextSource::Event, 1, Some("09:00")),
        ];

        let ctx = generate_daily_context(&tasks, &events, &[], &[], "2026-05-02");

        // Priority 1 items first, then within priority 1 sorted by time.
        assert_eq!(ctx.items[0].title, "Morning");
        assert_eq!(ctx.items[1].title, "Afternoon");
        assert_eq!(ctx.items[2].title, "High priority");
        // Priority 5 last.
        assert_eq!(ctx.items[3].title, "Low priority");
    }

    #[test]
    fn daily_context_empty_inputs() {
        let ctx = generate_daily_context(&[], &[], &[], &[], "2026-05-02");
        assert_eq!(ctx.items.len(), 0);
        assert_eq!(ctx.task_count, 0);
        assert_eq!(ctx.event_count, 0);
        assert_eq!(ctx.habit_count, 0);
        assert_eq!(ctx.reminder_count, 0);
    }

    // ── sort_by_priority ─────────────────────────────────────────

    #[test]
    fn sort_by_priority_orders_correctly() {
        let mut items = vec![
            make_context_item("C", ContextSource::Task, 3, None),
            make_context_item("A", ContextSource::Task, 1, Some("08:00")),
            make_context_item("B", ContextSource::Task, 1, Some("10:00")),
            make_context_item("D", ContextSource::Task, 2, None),
        ];
        sort_by_priority(&mut items);
        assert_eq!(items[0].title, "A"); // pri 1, time 08:00
        assert_eq!(items[1].title, "B"); // pri 1, time 10:00
        assert_eq!(items[2].title, "D"); // pri 2
        assert_eq!(items[3].title, "C"); // pri 3
    }

    #[test]
    fn sort_timed_before_untimed_at_same_priority() {
        let mut items = vec![
            make_context_item("No time", ContextSource::Task, 1, None),
            make_context_item("Has time", ContextSource::Task, 1, Some("09:00")),
        ];
        sort_by_priority(&mut items);
        assert_eq!(items[0].title, "Has time");
        assert_eq!(items[1].title, "No time");
    }

    // ── score_plan ───────────────────────────────────────────────

    #[test]
    fn score_plan_full_completion() {
        let plan = make_plan(
            vec![
                make_plan_item("1", "A", 1, true),
                make_plan_item("2", "B", 2, true),
            ],
            vec![make_check_in(4, 5, 3)],
        );
        let score = score_plan(&plan);
        assert_eq!(score.items_planned, 2);
        assert_eq!(score.items_completed, 2);
        assert!((score.completion_rate - 1.0).abs() < f64::EPSILON);
        assert!((score.avg_energy - 4.0).abs() < f64::EPSILON);
        assert!((score.avg_focus - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn score_plan_partial_completion() {
        let plan = make_plan(
            vec![
                make_plan_item("1", "A", 1, true),
                make_plan_item("2", "B", 2, false),
                make_plan_item("3", "C", 3, false),
            ],
            vec![],
        );
        let score = score_plan(&plan);
        assert_eq!(score.items_planned, 3);
        assert_eq!(score.items_completed, 1);
        assert!((score.completion_rate - 1.0 / 3.0).abs() < 1e-10);
        assert!((score.avg_energy - 0.0).abs() < f64::EPSILON);
        assert!((score.avg_focus - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn score_plan_empty() {
        let plan = make_plan(vec![], vec![]);
        let score = score_plan(&plan);
        assert_eq!(score.items_planned, 0);
        assert_eq!(score.items_completed, 0);
        assert!((score.completion_rate - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn score_plan_averages_multiple_check_ins() {
        let plan = make_plan(
            vec![make_plan_item("1", "A", 1, true)],
            vec![make_check_in(2, 4, 3), make_check_in(4, 2, 5)],
        );
        let score = score_plan(&plan);
        assert!((score.avg_energy - 3.0).abs() < f64::EPSILON);
        assert!((score.avg_focus - 3.0).abs() < f64::EPSILON);
    }

    // ── plan_completion_rate ─────────────────────────────────────

    #[test]
    fn completion_rate_none_completed() {
        let plan = make_plan(
            vec![
                make_plan_item("1", "A", 1, false),
                make_plan_item("2", "B", 2, false),
            ],
            vec![],
        );
        assert!((plan_completion_rate(&plan) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn completion_rate_all_completed() {
        let plan = make_plan(
            vec![
                make_plan_item("1", "A", 1, true),
                make_plan_item("2", "B", 2, true),
                make_plan_item("3", "C", 3, true),
            ],
            vec![],
        );
        assert!((plan_completion_rate(&plan) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn completion_rate_empty_plan() {
        let plan = make_plan(vec![], vec![]);
        assert!((plan_completion_rate(&plan) - 0.0).abs() < f64::EPSILON);
    }

    // ── suggested_item_count ─────────────────────────────────────

    #[test]
    fn suggested_counts_match_methods() {
        assert_eq!(suggested_item_count(&PlanningMethod::Mit), 1);
        assert_eq!(suggested_item_count(&PlanningMethod::ThreeThings), 3);
        assert_eq!(suggested_item_count(&PlanningMethod::TimeBlocking), 8);
        assert_eq!(
            suggested_item_count(&PlanningMethod::Custom {
                name: "Pomodoro Focus".to_string()
            }),
            5
        );
    }

    // ── Plan item with time block ────────────────────────────────

    #[test]
    fn plan_item_time_block_serializes() {
        let item = PlanItem {
            id: "tb1".to_string(),
            title: "Deep work".to_string(),
            object_id: Some("task-42".to_string()),
            object_type: Some("task".to_string()),
            priority: 1,
            completed: false,
            time_block: Some(TimeBlock {
                start_hour: 9,
                start_minute: 30,
                duration_minutes: 90,
            }),
        };
        let json = serde_json::to_string(&item).expect("serialize");
        let deser: PlanItem = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deser.id, "tb1");
        assert!(deser.time_block.is_some());
        let tb = deser.time_block.unwrap();
        assert_eq!(tb.start_hour, 9);
        assert_eq!(tb.start_minute, 30);
        assert_eq!(tb.duration_minutes, 90);
    }

    // ── Widget contributions ─────────────────────────────────────

    #[test]
    fn focus_planner_widget_contributions_count_and_ids() {
        let widgets = super::widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "daily-plan");
        assert_eq!(widgets[1].id, "focus-check-in");
        assert_eq!(widgets[2].id, "brain-dump");
    }

    #[test]
    fn daily_plan_widget_is_display_category() {
        use crate::widget::WidgetCategory;
        let widgets = super::widget_contributions();
        assert!(matches!(widgets[0].category, WidgetCategory::Display));
    }

    #[test]
    fn check_in_and_brain_dump_are_input_category() {
        use crate::widget::WidgetCategory;
        let widgets = super::widget_contributions();
        assert!(matches!(widgets[1].category, WidgetCategory::Input));
        assert!(matches!(widgets[2].category, WidgetCategory::Input));
    }

    #[test]
    fn daily_plan_widget_has_signals_and_toolbar() {
        let widgets = super::widget_contributions();
        let dp = &widgets[0];
        assert_eq!(dp.signals.len(), 2);
        assert_eq!(dp.toolbar_actions.len(), 2);
    }
}
