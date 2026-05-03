//! View widget contributions — Kanban, Calendar View, and Gantt chart.
//!
//! These are standalone `WidgetContribution` declarations for complex
//! data-driven view components. They follow the same pure-data pattern
//! as the domain engine widget contributions but live in the widget
//! module since they are cross-domain view primitives.

use crate::widget::{
    widget, DataQuery, FieldSpec, LayoutDirection, SelectOption, SignalSpec, TemplateNode,
    WidgetCategory, WidgetContribution,
};
use serde_json::json;

/// Returns all view widget contributions.
pub fn view_contributions() -> Vec<WidgetContribution> {
    vec![
        kanban_view(),
        calendar_view(),
        gantt_chart(),
        gallery_view(),
        inbox_view(),
        timeline_view(),
    ]
}

/// Kanban board view — groups items into columns by a configurable field.
pub fn kanban_view() -> WidgetContribution {
    widget("kanban-board", "Kanban Board")
        .description("Drag-and-drop kanban board that groups items into columns")
        .icon("columns")
        .category(WidgetCategory::DataTable)
        .field(FieldSpec::text("group_field", "Group Field").required())
        .field(FieldSpec::text("card_title_field", "Card Title Field").required())
        .field(FieldSpec::text("card_subtitle_field", "Card Subtitle Field"))
        .field(FieldSpec::boolean("show_counts", "Show Column Counts").with_default(json!(true)))
        .field(
            FieldSpec::number(
                "column_width",
                "Column Width",
                crate::widget::NumericBounds::min(100.0),
            )
            .with_default(json!(280)),
        )
        .query(DataQuery::for_type("task").sort_asc("order"))
        .data_key("items")
        .signal(
            SignalSpec::new("card-clicked", "A card was clicked")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
        )
        .signal(
            SignalSpec::new("card-moved", "A card was moved between columns").with_payload(vec![
                FieldSpec::text("item_id", "Item ID"),
                FieldSpec::text("from_column", "Source Column"),
                FieldSpec::text("to_column", "Target Column"),
            ]),
        )
        .signal(
            SignalSpec::new("column-clicked", "A column header was clicked")
                .with_payload(vec![FieldSpec::text("column_id", "Column ID")]),
        )
        .size(4, 3)
        .min_size(2, 2)
        .template(TemplateNode::horizontal(
            12,
            8,
            vec![TemplateNode::repeater(
                "columns",
                TemplateNode::vertical(
                    8,
                    8,
                    vec![
                        TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::repeater(
                            "cards",
                            TemplateNode::vertical(
                                4,
                                8,
                                vec![
                                    TemplateNode::text_binding("title"),
                                    TemplateNode::conditional(
                                        "subtitle",
                                        TemplateNode::text_binding("subtitle"),
                                    ),
                                ],
                            ),
                            "No cards",
                        ),
                    ],
                ),
                "No columns",
            )],
        ))
        .build()
}

/// Calendar view — week/month/day display of calendar events.
pub fn calendar_view() -> WidgetContribution {
    widget("calendar-view", "Calendar View")
        .description("Calendar with month, week, and day view modes")
        .icon("calendar")
        .category(WidgetCategory::DataTable)
        .field(FieldSpec::select(
            "view_mode",
            "View Mode",
            vec![
                SelectOption::new("month", "Month"),
                SelectOption::new("week", "Week"),
                SelectOption::new("day", "Day"),
            ],
        ))
        .field(FieldSpec::text("start_field", "Start Field").required())
        .field(FieldSpec::text("end_field", "End Field").required())
        .field(FieldSpec::text("title_field", "Title Field").required())
        .field(FieldSpec::boolean("show_weekends", "Show Weekends").with_default(json!(true)))
        .query(DataQuery::for_type("calendar-event"))
        .data_key("events")
        .signal(
            SignalSpec::new("event-clicked", "A calendar event was clicked")
                .with_payload(vec![FieldSpec::text("event_id", "Event ID")]),
        )
        .signal(
            SignalSpec::new("date-clicked", "A date cell was clicked")
                .with_payload(vec![FieldSpec::text("date", "Date")]),
        )
        .signal(
            SignalSpec::new("event-moved", "An event was moved to a new time range").with_payload(
                vec![
                    FieldSpec::text("event_id", "Event ID"),
                    FieldSpec::text("new_start", "New Start"),
                    FieldSpec::text("new_end", "New End"),
                ],
            ),
        )
        .size(4, 3)
        .min_size(2, 2)
        .template(TemplateNode::vertical(
            8,
            12,
            vec![
                TemplateNode::Container {
                    direction: LayoutDirection::Horizontal,
                    gap: Some(8),
                    padding: None,
                    children: vec![
                        TemplateNode::component("button", json!({"label": "Previous"})),
                        TemplateNode::DataBinding {
                            field: "current_period".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::component("button", json!({"label": "Next"})),
                    ],
                },
                TemplateNode::repeater(
                    "day_cells",
                    TemplateNode::vertical(
                        2,
                        4,
                        vec![
                            TemplateNode::text_binding("day_label"),
                            TemplateNode::Repeater {
                                source: "events".into(),
                                item_template: Box::new(TemplateNode::text_binding("title")),
                                empty_label: None,
                            },
                        ],
                    ),
                    "No days",
                ),
            ],
        ))
        .build()
}

/// Gantt chart — timeline view with task bars and optional dependencies.
pub fn gantt_chart() -> WidgetContribution {
    widget("gantt-chart", "Gantt Chart")
        .description("Timeline view with task bars, progress, and dependency lines")
        .icon("bar-chart")
        .category(WidgetCategory::Custom)
        .field(FieldSpec::text("start_field", "Start Field").required())
        .field(FieldSpec::text("end_field", "End Field").required())
        .field(FieldSpec::text("name_field", "Name Field").required())
        .field(FieldSpec::text("progress_field", "Progress Field"))
        .field(
            FieldSpec::boolean("show_dependencies", "Show Dependencies").with_default(json!(true)),
        )
        .field(FieldSpec::select(
            "time_scale",
            "Time Scale",
            vec![
                SelectOption::new("day", "Day"),
                SelectOption::new("week", "Week"),
                SelectOption::new("month", "Month"),
            ],
        ))
        .query(DataQuery::for_type("task").sort_asc("start"))
        .data_key("tasks")
        .signal(
            SignalSpec::new("task-clicked", "A task bar was clicked")
                .with_payload(vec![FieldSpec::text("task_id", "Task ID")]),
        )
        .signal(
            SignalSpec::new("task-resized", "A task bar was resized").with_payload(vec![
                FieldSpec::text("task_id", "Task ID"),
                FieldSpec::text("new_start", "New Start"),
                FieldSpec::text("new_end", "New End"),
            ]),
        )
        .signal(
            SignalSpec::new("dependency-clicked", "A dependency line was clicked").with_payload(
                vec![
                    FieldSpec::text("source_id", "Source ID"),
                    FieldSpec::text("target_id", "Target ID"),
                ],
            ),
        )
        .size(4, 2)
        .min_size(3, 2)
        .template(TemplateNode::horizontal(
            0,
            8,
            vec![
                TemplateNode::vertical(
                    4,
                    8,
                    vec![
                        TemplateNode::component("heading", json!({"body": "Tasks", "level": 4})),
                        TemplateNode::repeater(
                            "tasks",
                            TemplateNode::text_binding("name"),
                            "No tasks",
                        ),
                    ],
                ),
                TemplateNode::vertical(
                    4,
                    8,
                    vec![
                        TemplateNode::text_binding("timeline_header"),
                        TemplateNode::repeater(
                            "task_bars",
                            TemplateNode::Container {
                                direction: LayoutDirection::Horizontal,
                                gap: Some(0),
                                padding: None,
                                children: vec![
                                    TemplateNode::text_binding("bar"),
                                    TemplateNode::conditional(
                                        "progress",
                                        TemplateNode::text_binding("progress"),
                                    ),
                                ],
                            },
                            "No task bars",
                        ),
                    ],
                ),
            ],
        ))
        .build()
}

/// Gallery view — image/card grid with preview support.
pub fn gallery_view() -> WidgetContribution {
    widget("gallery-view", "Gallery")
        .description("Grid of image or card thumbnails with preview")
        .icon("grid")
        .category(WidgetCategory::DataTable)
        .field(FieldSpec::text("image_field", "Image Field").required())
        .field(FieldSpec::text("title_field", "Title Field"))
        .field(FieldSpec::text("subtitle_field", "Subtitle Field"))
        .field(
            FieldSpec::number(
                "columns",
                "Columns",
                crate::widget::NumericBounds::min_max(1.0, 12.0),
            )
            .with_default(json!(4)),
        )
        .field(
            FieldSpec::number("gap", "Gap (px)", crate::widget::NumericBounds::min(0.0))
                .with_default(json!(8)),
        )
        .field(FieldSpec::select(
            "aspect_ratio",
            "Aspect Ratio",
            vec![
                SelectOption::new("square", "Square (1:1)"),
                SelectOption::new("landscape", "Landscape (16:9)"),
                SelectOption::new("portrait", "Portrait (3:4)"),
                SelectOption::new("auto", "Auto"),
            ],
        ))
        .query(DataQuery::for_type("media-asset"))
        .data_key("items")
        .signal(
            SignalSpec::new("item-clicked", "A gallery item was clicked")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
        )
        .signal(SignalSpec::selection("item"))
        .size(4, 3)
        .min_size(2, 2)
        .template(TemplateNode::vertical(
            8,
            8,
            vec![TemplateNode::repeater(
                "items",
                TemplateNode::vertical(
                    4,
                    4,
                    vec![
                        TemplateNode::DataBinding {
                            field: "image_url".into(),
                            component_id: "image".into(),
                            prop_key: "src".into(),
                        },
                        TemplateNode::conditional("title", TemplateNode::text_binding("title")),
                    ],
                ),
                "No items",
            )],
        ))
        .build()
}

/// Inbox view — threaded message list.
pub fn inbox_view() -> WidgetContribution {
    widget("inbox-view", "Inbox")
        .description("Threaded message list with read/unread state")
        .icon("inbox")
        .category(WidgetCategory::DataTable)
        .field(FieldSpec::text("sender_field", "Sender Field").required())
        .field(FieldSpec::text("subject_field", "Subject Field").required())
        .field(FieldSpec::text("body_field", "Body Field"))
        .field(FieldSpec::text("date_field", "Date Field"))
        .field(FieldSpec::boolean("show_preview", "Show Body Preview").with_default(json!(true)))
        .field(FieldSpec::boolean("group_threads", "Group by Thread").with_default(json!(true)))
        .query(DataQuery::for_type("message").sort_desc("date"))
        .data_key("messages")
        .signal(
            SignalSpec::new("message-clicked", "A message was clicked")
                .with_payload(vec![FieldSpec::text("message_id", "Message ID")]),
        )
        .signal(
            SignalSpec::new("message-starred", "A message was starred/unstarred").with_payload(
                vec![
                    FieldSpec::text("message_id", "Message ID"),
                    FieldSpec::text("starred", "Starred"),
                ],
            ),
        )
        .signal(
            SignalSpec::new("thread-expanded", "A thread was expanded")
                .with_payload(vec![FieldSpec::text("thread_id", "Thread ID")]),
        )
        .size(3, 4)
        .min_size(2, 2)
        .template(TemplateNode::vertical(
            0,
            0,
            vec![TemplateNode::repeater(
                "messages",
                TemplateNode::vertical(
                    2,
                    12,
                    vec![
                        TemplateNode::Container {
                            direction: LayoutDirection::Horizontal,
                            gap: Some(8),
                            padding: None,
                            children: vec![
                                TemplateNode::text_binding("sender"),
                                TemplateNode::text_binding("date"),
                            ],
                        },
                        TemplateNode::text_binding("subject"),
                        TemplateNode::conditional("preview", TemplateNode::text_binding("preview")),
                    ],
                ),
                "No messages",
            )],
        ))
        .build()
}

/// Timeline view — chronological event stream.
pub fn timeline_view() -> WidgetContribution {
    widget("timeline-view", "Timeline")
        .description("Chronological stream of events and activities")
        .icon("clock")
        .category(WidgetCategory::DataTable)
        .field(FieldSpec::text("title_field", "Title Field").required())
        .field(FieldSpec::text("date_field", "Date Field").required())
        .field(FieldSpec::text("description_field", "Description Field"))
        .field(FieldSpec::text("icon_field", "Icon Field"))
        .field(FieldSpec::boolean("show_timestamps", "Show Timestamps").with_default(json!(true)))
        .field(FieldSpec::boolean("group_by_date", "Group by Date").with_default(json!(true)))
        .query(DataQuery::for_type("activity").sort_desc("date"))
        .data_key("events")
        .signal(
            SignalSpec::new("event-clicked", "A timeline event was clicked")
                .with_payload(vec![FieldSpec::text("event_id", "Event ID")]),
        )
        .size(3, 4)
        .min_size(2, 2)
        .template(TemplateNode::vertical(
            0,
            8,
            vec![TemplateNode::repeater(
                "events",
                TemplateNode::horizontal(
                    12,
                    8,
                    vec![
                        TemplateNode::conditional(
                            "timestamp",
                            TemplateNode::text_binding("timestamp"),
                        ),
                        TemplateNode::vertical(
                            2,
                            0,
                            vec![
                                TemplateNode::text_binding("title"),
                                TemplateNode::conditional(
                                    "description",
                                    TemplateNode::text_binding("description"),
                                ),
                            ],
                        ),
                    ],
                ),
                "No events",
            )],
        ))
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn view_contributions_count() {
        let views = view_contributions();
        assert_eq!(views.len(), 6);
    }

    #[test]
    fn kanban_has_correct_id_and_category() {
        let kanban = kanban_view();
        assert_eq!(kanban.id, "kanban-board");
        assert!(matches!(kanban.category, WidgetCategory::DataTable));
    }

    #[test]
    fn calendar_has_view_mode_options() {
        let cal = calendar_view();
        let view_mode = cal
            .config_fields
            .iter()
            .find(|f| f.key == "view_mode")
            .expect("calendar-view should have a view_mode config field");
        match &view_mode.kind {
            crate::widget::FieldKind::Select(opts) => {
                let values: Vec<&str> = opts.iter().map(|o| o.value.as_str()).collect();
                assert_eq!(values, vec!["month", "week", "day"]);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn gantt_has_data_query() {
        let gantt = gantt_chart();
        let query = gantt
            .data_query
            .as_ref()
            .expect("gantt should have a data_query");
        assert_eq!(query.object_type.as_deref(), Some("task"));
        assert_eq!(query.sort.len(), 1);
        assert_eq!(query.sort[0].field, "start");
        assert!(!query.sort[0].descending);
    }

    #[test]
    fn all_views_have_signals() {
        for view in view_contributions() {
            assert!(
                !view.signals.is_empty(),
                "view '{}' should have at least one signal",
                view.id
            );
        }
    }

    #[test]
    fn all_views_have_data_keys() {
        for view in view_contributions() {
            assert!(
                view.data_key.is_some(),
                "view '{}' should have a data_key",
                view.id
            );
        }
    }

    #[test]
    fn gallery_has_aspect_ratio_options() {
        let gallery = gallery_view();
        assert_eq!(gallery.id, "gallery-view");
        let aspect = gallery
            .config_fields
            .iter()
            .find(|f| f.key == "aspect_ratio")
            .expect("gallery should have aspect_ratio field");
        match &aspect.kind {
            crate::widget::FieldKind::Select(opts) => {
                assert_eq!(opts.len(), 4);
            }
            other => panic!("expected Select, got {other:?}"),
        }
    }

    #[test]
    fn inbox_has_thread_grouping() {
        let inbox = inbox_view();
        assert_eq!(inbox.id, "inbox-view");
        assert!(inbox.config_fields.iter().any(|f| f.key == "group_threads"));
        assert!(inbox.data_query.as_ref().unwrap().sort[0].descending);
    }

    #[test]
    fn timeline_sorts_by_date_descending() {
        let tl = timeline_view();
        assert_eq!(tl.id, "timeline-view");
        let query = tl.data_query.as_ref().unwrap();
        assert_eq!(query.sort[0].field, "date");
        assert!(query.sort[0].descending);
    }
}
