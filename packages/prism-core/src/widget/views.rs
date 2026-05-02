//! View widget contributions — Kanban, Calendar View, and Gantt chart.
//!
//! These are standalone `WidgetContribution` declarations for complex
//! data-driven view components. They follow the same pure-data pattern
//! as the domain engine widget contributions but live in the widget
//! module since they are cross-domain view primitives.

use crate::widget::{
    DataQuery, FieldSpec, LayoutDirection, QuerySort, SelectOption, SignalSpec, TemplateNode,
    WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
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
    WidgetContribution {
        id: "kanban-board".into(),
        label: "Kanban Board".into(),
        description: "Drag-and-drop kanban board that groups items into columns".into(),
        icon: Some("columns".into()),
        category: WidgetCategory::DataTable,
        config_fields: vec![
            FieldSpec::text("group_field", "Group Field").required(),
            FieldSpec::text("card_title_field", "Card Title Field").required(),
            FieldSpec::text("card_subtitle_field", "Card Subtitle Field"),
            FieldSpec::boolean("show_counts", "Show Column Counts").with_default(json!(true)),
            FieldSpec::number(
                "column_width",
                "Column Width",
                crate::widget::NumericBounds::min(100.0),
            )
            .with_default(json!(280)),
        ],
        data_query: Some(DataQuery {
            object_type: Some("task".into()),
            sort: vec![QuerySort {
                field: "order".into(),
                descending: false,
            }],
            ..Default::default()
        }),
        data_key: Some("items".into()),
        signals: vec![
            SignalSpec::new("card-clicked", "A card was clicked")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
            SignalSpec::new("card-moved", "A card was moved between columns").with_payload(vec![
                FieldSpec::text("item_id", "Item ID"),
                FieldSpec::text("from_column", "Source Column"),
                FieldSpec::text("to_column", "Target Column"),
            ]),
            SignalSpec::new("column-clicked", "A column header was clicked")
                .with_payload(vec![FieldSpec::text("column_id", "Column ID")]),
        ],
        default_size: WidgetSize::new(4, 3),
        min_size: Some(WidgetSize::new(2, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Horizontal,
                gap: Some(12),
                padding: Some(8),
                children: vec![TemplateNode::Repeater {
                    source: "columns".into(),
                    item_template: Box::new(TemplateNode::Container {
                        direction: LayoutDirection::Vertical,
                        gap: Some(8),
                        padding: Some(8),
                        children: vec![
                            TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "heading".into(),
                                prop_key: "body".into(),
                            },
                            TemplateNode::Repeater {
                                source: "cards".into(),
                                item_template: Box::new(TemplateNode::Container {
                                    direction: LayoutDirection::Vertical,
                                    gap: Some(4),
                                    padding: Some(8),
                                    children: vec![
                                        TemplateNode::DataBinding {
                                            field: "title".into(),
                                            component_id: "text".into(),
                                            prop_key: "body".into(),
                                        },
                                        TemplateNode::Conditional {
                                            field: "subtitle".into(),
                                            child: Box::new(TemplateNode::DataBinding {
                                                field: "subtitle".into(),
                                                component_id: "text".into(),
                                                prop_key: "body".into(),
                                            }),
                                            fallback: None,
                                        },
                                    ],
                                }),
                                empty_label: Some("No cards".into()),
                            },
                        ],
                    }),
                    empty_label: Some("No columns".into()),
                }],
            },
        },
        ..Default::default()
    }
}

/// Calendar view — week/month/day display of calendar events.
pub fn calendar_view() -> WidgetContribution {
    WidgetContribution {
        id: "calendar-view".into(),
        label: "Calendar View".into(),
        description: "Calendar with month, week, and day view modes".into(),
        icon: Some("calendar".into()),
        category: WidgetCategory::DataTable,
        config_fields: vec![
            FieldSpec::select(
                "view_mode",
                "View Mode",
                vec![
                    SelectOption::new("month", "Month"),
                    SelectOption::new("week", "Week"),
                    SelectOption::new("day", "Day"),
                ],
            ),
            FieldSpec::text("start_field", "Start Field").required(),
            FieldSpec::text("end_field", "End Field").required(),
            FieldSpec::text("title_field", "Title Field").required(),
            FieldSpec::boolean("show_weekends", "Show Weekends").with_default(json!(true)),
        ],
        data_query: Some(DataQuery {
            object_type: Some("calendar-event".into()),
            ..Default::default()
        }),
        data_key: Some("events".into()),
        signals: vec![
            SignalSpec::new("event-clicked", "A calendar event was clicked")
                .with_payload(vec![FieldSpec::text("event_id", "Event ID")]),
            SignalSpec::new("date-clicked", "A date cell was clicked")
                .with_payload(vec![FieldSpec::text("date", "Date")]),
            SignalSpec::new("event-moved", "An event was moved to a new time range").with_payload(
                vec![
                    FieldSpec::text("event_id", "Event ID"),
                    FieldSpec::text("new_start", "New Start"),
                    FieldSpec::text("new_end", "New End"),
                ],
            ),
        ],
        default_size: WidgetSize::new(4, 3),
        min_size: Some(WidgetSize::new(2, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: Some(8),
                padding: Some(12),
                children: vec![
                    // Navigation header row
                    TemplateNode::Container {
                        direction: LayoutDirection::Horizontal,
                        gap: Some(8),
                        padding: None,
                        children: vec![
                            TemplateNode::Component {
                                component_id: "button".into(),
                                props: json!({"label": "Previous"}),
                            },
                            TemplateNode::DataBinding {
                                field: "current_period".into(),
                                component_id: "heading".into(),
                                prop_key: "body".into(),
                            },
                            TemplateNode::Component {
                                component_id: "button".into(),
                                props: json!({"label": "Next"}),
                            },
                        ],
                    },
                    // Day cells grid
                    TemplateNode::Repeater {
                        source: "day_cells".into(),
                        item_template: Box::new(TemplateNode::Container {
                            direction: LayoutDirection::Vertical,
                            gap: Some(2),
                            padding: Some(4),
                            children: vec![
                                TemplateNode::DataBinding {
                                    field: "day_label".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                },
                                TemplateNode::Repeater {
                                    source: "events".into(),
                                    item_template: Box::new(TemplateNode::DataBinding {
                                        field: "title".into(),
                                        component_id: "text".into(),
                                        prop_key: "body".into(),
                                    }),
                                    empty_label: None,
                                },
                            ],
                        }),
                        empty_label: Some("No days".into()),
                    },
                ],
            },
        },
        ..Default::default()
    }
}

/// Gantt chart — timeline view with task bars and optional dependencies.
pub fn gantt_chart() -> WidgetContribution {
    WidgetContribution {
        id: "gantt-chart".into(),
        label: "Gantt Chart".into(),
        description: "Timeline view with task bars, progress, and dependency lines".into(),
        icon: Some("bar-chart".into()),
        category: WidgetCategory::Custom,
        config_fields: vec![
            FieldSpec::text("start_field", "Start Field").required(),
            FieldSpec::text("end_field", "End Field").required(),
            FieldSpec::text("name_field", "Name Field").required(),
            FieldSpec::text("progress_field", "Progress Field"),
            FieldSpec::boolean("show_dependencies", "Show Dependencies").with_default(json!(true)),
            FieldSpec::select(
                "time_scale",
                "Time Scale",
                vec![
                    SelectOption::new("day", "Day"),
                    SelectOption::new("week", "Week"),
                    SelectOption::new("month", "Month"),
                ],
            ),
        ],
        data_query: Some(DataQuery {
            object_type: Some("task".into()),
            sort: vec![QuerySort {
                field: "start".into(),
                descending: false,
            }],
            ..Default::default()
        }),
        data_key: Some("tasks".into()),
        signals: vec![
            SignalSpec::new("task-clicked", "A task bar was clicked")
                .with_payload(vec![FieldSpec::text("task_id", "Task ID")]),
            SignalSpec::new("task-resized", "A task bar was resized").with_payload(vec![
                FieldSpec::text("task_id", "Task ID"),
                FieldSpec::text("new_start", "New Start"),
                FieldSpec::text("new_end", "New End"),
            ]),
            SignalSpec::new("dependency-clicked", "A dependency line was clicked").with_payload(
                vec![
                    FieldSpec::text("source_id", "Source ID"),
                    FieldSpec::text("target_id", "Target ID"),
                ],
            ),
        ],
        default_size: WidgetSize::new(4, 2),
        min_size: Some(WidgetSize::new(3, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Horizontal,
                gap: Some(0),
                padding: Some(8),
                children: vec![
                    // Left: task label column
                    TemplateNode::Container {
                        direction: LayoutDirection::Vertical,
                        gap: Some(4),
                        padding: Some(8),
                        children: vec![
                            TemplateNode::Component {
                                component_id: "heading".into(),
                                props: json!({"body": "Tasks", "level": 4}),
                            },
                            TemplateNode::Repeater {
                                source: "tasks".into(),
                                item_template: Box::new(TemplateNode::DataBinding {
                                    field: "name".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                }),
                                empty_label: Some("No tasks".into()),
                            },
                        ],
                    },
                    // Right: timeline area
                    TemplateNode::Container {
                        direction: LayoutDirection::Vertical,
                        gap: Some(4),
                        padding: Some(8),
                        children: vec![
                            TemplateNode::DataBinding {
                                field: "timeline_header".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            },
                            TemplateNode::Repeater {
                                source: "task_bars".into(),
                                item_template: Box::new(TemplateNode::Container {
                                    direction: LayoutDirection::Horizontal,
                                    gap: Some(0),
                                    padding: None,
                                    children: vec![
                                        TemplateNode::DataBinding {
                                            field: "bar".into(),
                                            component_id: "text".into(),
                                            prop_key: "body".into(),
                                        },
                                        TemplateNode::Conditional {
                                            field: "progress".into(),
                                            child: Box::new(TemplateNode::DataBinding {
                                                field: "progress".into(),
                                                component_id: "text".into(),
                                                prop_key: "body".into(),
                                            }),
                                            fallback: None,
                                        },
                                    ],
                                }),
                                empty_label: Some("No task bars".into()),
                            },
                        ],
                    },
                ],
            },
        },
        ..Default::default()
    }
}

/// Gallery view — image/card grid with preview support.
pub fn gallery_view() -> WidgetContribution {
    WidgetContribution {
        id: "gallery-view".into(),
        label: "Gallery".into(),
        description: "Grid of image or card thumbnails with preview".into(),
        icon: Some("grid".into()),
        category: WidgetCategory::DataTable,
        config_fields: vec![
            FieldSpec::text("image_field", "Image Field").required(),
            FieldSpec::text("title_field", "Title Field"),
            FieldSpec::text("subtitle_field", "Subtitle Field"),
            FieldSpec::number(
                "columns",
                "Columns",
                crate::widget::NumericBounds::min_max(1.0, 12.0),
            )
            .with_default(json!(4)),
            FieldSpec::number("gap", "Gap (px)", crate::widget::NumericBounds::min(0.0))
                .with_default(json!(8)),
            FieldSpec::select(
                "aspect_ratio",
                "Aspect Ratio",
                vec![
                    SelectOption::new("square", "Square (1:1)"),
                    SelectOption::new("landscape", "Landscape (16:9)"),
                    SelectOption::new("portrait", "Portrait (3:4)"),
                    SelectOption::new("auto", "Auto"),
                ],
            ),
        ],
        data_query: Some(DataQuery {
            object_type: Some("media-asset".into()),
            ..Default::default()
        }),
        data_key: Some("items".into()),
        signals: vec![
            SignalSpec::new("item-clicked", "A gallery item was clicked")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
            SignalSpec::new("item-selected", "A gallery item was selected")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")]),
        ],
        default_size: WidgetSize::new(4, 3),
        min_size: Some(WidgetSize::new(2, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: Some(8),
                padding: Some(8),
                children: vec![TemplateNode::Repeater {
                    source: "items".into(),
                    item_template: Box::new(TemplateNode::Container {
                        direction: LayoutDirection::Vertical,
                        gap: Some(4),
                        padding: Some(4),
                        children: vec![
                            TemplateNode::DataBinding {
                                field: "image_url".into(),
                                component_id: "image".into(),
                                prop_key: "src".into(),
                            },
                            TemplateNode::Conditional {
                                field: "title".into(),
                                child: Box::new(TemplateNode::DataBinding {
                                    field: "title".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                }),
                                fallback: None,
                            },
                        ],
                    }),
                    empty_label: Some("No items".into()),
                }],
            },
        },
        ..Default::default()
    }
}

/// Inbox view — threaded message list.
pub fn inbox_view() -> WidgetContribution {
    WidgetContribution {
        id: "inbox-view".into(),
        label: "Inbox".into(),
        description: "Threaded message list with read/unread state".into(),
        icon: Some("inbox".into()),
        category: WidgetCategory::DataTable,
        config_fields: vec![
            FieldSpec::text("sender_field", "Sender Field").required(),
            FieldSpec::text("subject_field", "Subject Field").required(),
            FieldSpec::text("body_field", "Body Field"),
            FieldSpec::text("date_field", "Date Field"),
            FieldSpec::boolean("show_preview", "Show Body Preview").with_default(json!(true)),
            FieldSpec::boolean("group_threads", "Group by Thread").with_default(json!(true)),
        ],
        data_query: Some(DataQuery {
            object_type: Some("message".into()),
            sort: vec![QuerySort {
                field: "date".into(),
                descending: true,
            }],
            ..Default::default()
        }),
        data_key: Some("messages".into()),
        signals: vec![
            SignalSpec::new("message-clicked", "A message was clicked")
                .with_payload(vec![FieldSpec::text("message_id", "Message ID")]),
            SignalSpec::new("message-starred", "A message was starred/unstarred").with_payload(
                vec![
                    FieldSpec::text("message_id", "Message ID"),
                    FieldSpec::text("starred", "Starred"),
                ],
            ),
            SignalSpec::new("thread-expanded", "A thread was expanded")
                .with_payload(vec![FieldSpec::text("thread_id", "Thread ID")]),
        ],
        default_size: WidgetSize::new(3, 4),
        min_size: Some(WidgetSize::new(2, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: Some(0),
                padding: Some(0),
                children: vec![TemplateNode::Repeater {
                    source: "messages".into(),
                    item_template: Box::new(TemplateNode::Container {
                        direction: LayoutDirection::Vertical,
                        gap: Some(2),
                        padding: Some(12),
                        children: vec![
                            TemplateNode::Container {
                                direction: LayoutDirection::Horizontal,
                                gap: Some(8),
                                padding: None,
                                children: vec![
                                    TemplateNode::DataBinding {
                                        field: "sender".into(),
                                        component_id: "text".into(),
                                        prop_key: "body".into(),
                                    },
                                    TemplateNode::DataBinding {
                                        field: "date".into(),
                                        component_id: "text".into(),
                                        prop_key: "body".into(),
                                    },
                                ],
                            },
                            TemplateNode::DataBinding {
                                field: "subject".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            },
                            TemplateNode::Conditional {
                                field: "preview".into(),
                                child: Box::new(TemplateNode::DataBinding {
                                    field: "preview".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                }),
                                fallback: None,
                            },
                        ],
                    }),
                    empty_label: Some("No messages".into()),
                }],
            },
        },
        ..Default::default()
    }
}

/// Timeline view — chronological event stream.
pub fn timeline_view() -> WidgetContribution {
    WidgetContribution {
        id: "timeline-view".into(),
        label: "Timeline".into(),
        description: "Chronological stream of events and activities".into(),
        icon: Some("clock".into()),
        category: WidgetCategory::DataTable,
        config_fields: vec![
            FieldSpec::text("title_field", "Title Field").required(),
            FieldSpec::text("date_field", "Date Field").required(),
            FieldSpec::text("description_field", "Description Field"),
            FieldSpec::text("icon_field", "Icon Field"),
            FieldSpec::boolean("show_timestamps", "Show Timestamps").with_default(json!(true)),
            FieldSpec::boolean("group_by_date", "Group by Date").with_default(json!(true)),
        ],
        data_query: Some(DataQuery {
            object_type: Some("activity".into()),
            sort: vec![QuerySort {
                field: "date".into(),
                descending: true,
            }],
            ..Default::default()
        }),
        data_key: Some("events".into()),
        signals: vec![
            SignalSpec::new("event-clicked", "A timeline event was clicked")
                .with_payload(vec![FieldSpec::text("event_id", "Event ID")]),
        ],
        default_size: WidgetSize::new(3, 4),
        min_size: Some(WidgetSize::new(2, 2)),
        template: WidgetTemplate {
            root: TemplateNode::Container {
                direction: LayoutDirection::Vertical,
                gap: Some(0),
                padding: Some(8),
                children: vec![TemplateNode::Repeater {
                    source: "events".into(),
                    item_template: Box::new(TemplateNode::Container {
                        direction: LayoutDirection::Horizontal,
                        gap: Some(12),
                        padding: Some(8),
                        children: vec![
                            TemplateNode::Conditional {
                                field: "timestamp".into(),
                                child: Box::new(TemplateNode::DataBinding {
                                    field: "timestamp".into(),
                                    component_id: "text".into(),
                                    prop_key: "body".into(),
                                }),
                                fallback: None,
                            },
                            TemplateNode::Container {
                                direction: LayoutDirection::Vertical,
                                gap: Some(2),
                                padding: None,
                                children: vec![
                                    TemplateNode::DataBinding {
                                        field: "title".into(),
                                        component_id: "text".into(),
                                        prop_key: "body".into(),
                                    },
                                    TemplateNode::Conditional {
                                        field: "description".into(),
                                        child: Box::new(TemplateNode::DataBinding {
                                            field: "description".into(),
                                            component_id: "text".into(),
                                            prop_key: "body".into(),
                                        }),
                                        fallback: None,
                                    },
                                ],
                            },
                        ],
                    }),
                    empty_label: Some("No events".into()),
                }],
            },
        },
        ..Default::default()
    }
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
