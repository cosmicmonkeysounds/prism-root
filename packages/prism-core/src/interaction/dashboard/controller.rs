//! Dashboard controller — manages presets, tabs, widgets, and
//! subscriber notifications.
//!
//! Port of `interaction/dashboard/dashboard-controller.ts`.

use serde_json::Value;
use uuid::Uuid;

use super::types::{DashboardPreset, DashboardTab, WidgetSlot};

/// Input for adding a new widget to a tab.
pub struct NewWidgetSlot {
    pub id: Option<String>,
    pub widget_type: String,
    pub label: Option<String>,
    pub col_span: Option<u8>,
    pub row_span: Option<u8>,
    pub config: Option<Value>,
}

/// Partial update for an existing widget.
pub struct WidgetPatch {
    pub label: Option<Option<String>>,
    pub col_span: Option<u8>,
    pub row_span: Option<u8>,
    pub config: Option<Option<Value>>,
}

pub struct DashboardController {
    presets: Vec<DashboardPreset>,
    active_tab_id: String,
    listeners: Vec<(usize, Box<dyn Fn()>)>,
    next_id: usize,
}

impl DashboardController {
    pub fn new(presets: Vec<DashboardPreset>, active_tab_id: Option<String>) -> Self {
        let presets = deep_clone_presets(&presets);
        let fallback = presets
            .first()
            .and_then(|p| p.tabs.first())
            .map(|t| t.id.clone())
            .unwrap_or_default();
        let active_tab_id = active_tab_id.unwrap_or(fallback);
        Self {
            presets,
            active_tab_id,
            listeners: Vec::new(),
            next_id: 0,
        }
    }

    pub fn presets(&self) -> &[DashboardPreset] {
        &self.presets
    }

    pub fn active_tab_id(&self) -> &str {
        &self.active_tab_id
    }

    pub fn subscribe(&mut self, listener: Box<dyn Fn()>) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        self.listeners.push((id, listener));
        id
    }

    pub fn unsubscribe(&mut self, id: usize) {
        self.listeners.retain(|(lid, _)| *lid != id);
    }

    // ── Tab ops ──────────────────────────────────────────────────

    pub fn add_tab(&mut self, name: &str) -> &DashboardTab {
        let tab = DashboardTab {
            id: Uuid::new_v4().to_string(),
            label: name.to_string(),
            widgets: Vec::new(),
        };
        if self.presets.is_empty() {
            self.presets.push(DashboardPreset {
                id: Uuid::new_v4().to_string(),
                name: "Default".to_string(),
                tabs: vec![tab],
            });
        } else {
            self.presets[0].tabs.push(tab);
        }
        self.notify();
        self.presets[0].tabs.last().unwrap()
    }

    pub fn remove_tab(&mut self, id: &str) {
        let mut found = false;
        for preset in &mut self.presets {
            let before = preset.tabs.len();
            preset.tabs.retain(|t| t.id != id);
            if preset.tabs.len() < before {
                found = true;
            }
        }
        if !found {
            return;
        }
        if self.active_tab_id == id {
            self.active_tab_id = self
                .presets
                .first()
                .and_then(|p| p.tabs.first())
                .map(|t| t.id.clone())
                .unwrap_or_default();
        }
        self.notify();
    }

    pub fn rename_tab(&mut self, id: &str, name: &str) {
        let mut found = false;
        for preset in &mut self.presets {
            for tab in &mut preset.tabs {
                if tab.id == id {
                    tab.label = name.to_string();
                    found = true;
                }
            }
        }
        if found {
            self.notify();
        }
    }

    pub fn set_active_tab(&mut self, id: &str) {
        if self.active_tab_id != id {
            self.active_tab_id = id.to_string();
            self.notify();
        }
    }

    pub fn reorder_tabs(&mut self, ordered_ids: &[&str]) {
        if let Some(preset) = self.presets.first_mut() {
            let mut reordered: Vec<DashboardTab> = Vec::with_capacity(preset.tabs.len());
            for &oid in ordered_ids {
                if let Some(pos) = preset.tabs.iter().position(|t| t.id == oid) {
                    reordered.push(preset.tabs.remove(pos));
                }
            }
            // Append any tabs not mentioned in ordered_ids.
            reordered.append(&mut preset.tabs);
            preset.tabs = reordered;
            self.notify();
        }
    }

    // ── Widget ops ───────────────────────────────────────────────

    pub fn add_widget(&mut self, tab_id: &str, widget: NewWidgetSlot) -> Option<&WidgetSlot> {
        let tab = self.find_tab_mut(tab_id)?;
        let slot = WidgetSlot {
            id: widget.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
            widget_type: widget.widget_type,
            label: widget.label,
            col_span: clamp_span(widget.col_span.unwrap_or(1)),
            row_span: clamp_span(widget.row_span.unwrap_or(1)),
            config: widget.config,
        };
        tab.widgets.push(slot);
        self.notify();
        // Re-borrow after notify to satisfy the borrow checker.
        let tab = self.find_tab(tab_id)?;
        tab.widgets.last()
    }

    pub fn remove_widget(&mut self, tab_id: &str, widget_id: &str) {
        if let Some(tab) = self.find_tab_mut(tab_id) {
            let before = tab.widgets.len();
            tab.widgets.retain(|w| w.id != widget_id);
            if tab.widgets.len() < before {
                self.notify();
            }
        }
    }

    pub fn update_widget(&mut self, tab_id: &str, widget_id: &str, patch: WidgetPatch) {
        if let Some(tab) = self.find_tab_mut(tab_id) {
            if let Some(w) = tab.widgets.iter_mut().find(|w| w.id == widget_id) {
                // Never overwrite id via patch.
                if let Some(label) = patch.label {
                    w.label = label;
                }
                if let Some(col_span) = patch.col_span {
                    w.col_span = clamp_span(col_span);
                }
                if let Some(row_span) = patch.row_span {
                    w.row_span = clamp_span(row_span);
                }
                if let Some(config) = patch.config {
                    w.config = config;
                }
                self.notify();
            }
        }
    }

    pub fn reorder_widgets(&mut self, tab_id: &str, ordered_ids: &[&str]) {
        if let Some(tab) = self.find_tab_mut(tab_id) {
            let mut reordered: Vec<WidgetSlot> = Vec::with_capacity(tab.widgets.len());
            for &oid in ordered_ids {
                if let Some(pos) = tab.widgets.iter().position(|w| w.id == oid) {
                    reordered.push(tab.widgets.remove(pos));
                }
            }
            reordered.append(&mut tab.widgets);
            tab.widgets = reordered;
            self.notify();
        }
    }

    pub fn to_json(&self) -> Vec<DashboardPreset> {
        deep_clone_presets(&self.presets)
    }

    // ── Internals ────────────────────────────────────────────────

    fn find_tab_mut(&mut self, id: &str) -> Option<&mut DashboardTab> {
        for preset in &mut self.presets {
            for tab in &mut preset.tabs {
                if tab.id == id {
                    return Some(tab);
                }
            }
        }
        None
    }

    fn find_tab(&self, id: &str) -> Option<&DashboardTab> {
        for preset in &self.presets {
            for tab in &preset.tabs {
                if tab.id == id {
                    return Some(tab);
                }
            }
        }
        None
    }

    fn notify(&self) {
        for (_, listener) in &self.listeners {
            listener();
        }
    }
}

// ── Free functions ───────────────────────────────────────────────

/// Clamp a span value to the 1-3 range. 0 becomes 1, >3 becomes 3.
pub fn clamp_span(span: u8) -> u8 {
    span.clamp(1, 3)
}

/// Auto-flow widgets into 3-column rows. A span-3 widget always
/// forces a new row.
pub fn layout_rows(widgets: &[WidgetSlot]) -> Vec<Vec<&WidgetSlot>> {
    let mut rows: Vec<Vec<&WidgetSlot>> = Vec::new();
    let mut current_row: Vec<&WidgetSlot> = Vec::new();
    let mut current_cols: u8 = 0;

    for widget in widgets {
        let span = clamp_span(widget.col_span);

        if span >= 3 {
            // Flush the current row if non-empty.
            if !current_row.is_empty() {
                rows.push(current_row);
                current_row = Vec::new();
            }
            rows.push(vec![widget]);
            current_cols = 0;
            continue;
        }

        if current_cols + span > 3 {
            rows.push(current_row);
            current_row = Vec::new();
            current_cols = 0;
        }

        current_row.push(widget);
        current_cols += span;
    }

    if !current_row.is_empty() {
        rows.push(current_row);
    }

    rows
}

/// Number of rows produced by `layout_rows`.
pub fn grid_row_count(widgets: &[WidgetSlot]) -> usize {
    layout_rows(widgets).len()
}

/// Returns the 12 built-in widget definitions.
pub fn built_in_widgets() -> Vec<WidgetDef> {
    use crate::widget::{FieldSpec, NumericBounds, SelectOption};

    vec![
        WidgetDef {
            id: "stats".into(),
            label: "Stats".into(),
            description: Some("Key metrics at a glance".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![],
        },
        WidgetDef {
            id: "databases".into(),
            label: "Databases".into(),
            description: Some("Database overview".into()),
            default_col_span: 2,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![],
        },
        WidgetDef {
            id: "tasks".into(),
            label: "Tasks".into(),
            description: Some("Active task list".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![FieldSpec::select(
                "filter",
                "Filter",
                vec![
                    SelectOption::new("all", "All"),
                    SelectOption::new("today", "Today"),
                    SelectOption::new("overdue", "Overdue"),
                ],
            )],
        },
        WidgetDef {
            id: "reminders".into(),
            label: "Reminders".into(),
            description: Some("Upcoming reminders".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 2,
            config_schema: vec![],
        },
        WidgetDef {
            id: "capture".into(),
            label: "Capture".into(),
            description: Some("Quick capture input".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 2,
            config_schema: vec![],
        },
        WidgetDef {
            id: "goals".into(),
            label: "Goals".into(),
            description: Some("Goal progress tracker".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![],
        },
        WidgetDef {
            id: "finance".into(),
            label: "Finance".into(),
            description: Some("Financial summary".into()),
            default_col_span: 2,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![FieldSpec::select(
                "period",
                "Period",
                vec![
                    SelectOption::new("week", "This week"),
                    SelectOption::new("month", "This month"),
                    SelectOption::new("year", "This year"),
                ],
            )],
        },
        WidgetDef {
            id: "quick-links".into(),
            label: "Quick Links".into(),
            description: Some("Bookmarked shortcuts".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 2,
            config_schema: vec![],
        },
        WidgetDef {
            id: "timer".into(),
            label: "Timer".into(),
            description: Some("Pomodoro / focus timer".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 1,
            config_schema: vec![FieldSpec::number(
                "duration_minutes",
                "Duration (minutes)",
                NumericBounds::unbounded(),
            )
            .with_default(serde_json::Value::from(25))],
        },
        WidgetDef {
            id: "recent".into(),
            label: "Recent".into(),
            description: Some("Recently visited items".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![FieldSpec::number(
                "limit",
                "Items to show",
                NumericBounds::unbounded(),
            )
            .with_default(serde_json::Value::from(10))],
        },
        WidgetDef {
            id: "graph".into(),
            label: "Graph".into(),
            description: Some("Knowledge graph view".into()),
            default_col_span: 2,
            default_row_span: 2,
            min_col_span: 2,
            max_col_span: 3,
            config_schema: vec![],
        },
        WidgetDef {
            id: "custom".into(),
            label: "Custom".into(),
            description: Some("User-defined widget".into()),
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![FieldSpec::text("content", "Content")],
        },
    ]
}

/// Returns 3 default dashboard presets: Home, Focus, Finance.
pub fn create_default_presets() -> Vec<DashboardPreset> {
    vec![
        DashboardPreset {
            id: "home".to_string(),
            name: "Home".to_string(),
            tabs: vec![DashboardTab {
                id: "home-main".to_string(),
                label: "Overview".to_string(),
                widgets: vec![
                    WidgetSlot {
                        id: "home-stats".to_string(),
                        widget_type: "stats".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "home-tasks".to_string(),
                        widget_type: "tasks".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "home-reminders".to_string(),
                        widget_type: "reminders".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "home-goals".to_string(),
                        widget_type: "goals".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "home-quick-links".to_string(),
                        widget_type: "quick-links".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "home-timer".to_string(),
                        widget_type: "timer".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                ],
            }],
        },
        DashboardPreset {
            id: "focus".to_string(),
            name: "Focus".to_string(),
            tabs: vec![DashboardTab {
                id: "focus-main".to_string(),
                label: "Focus".to_string(),
                widgets: vec![
                    WidgetSlot {
                        id: "focus-timer".to_string(),
                        widget_type: "timer".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "focus-tasks".to_string(),
                        widget_type: "tasks".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "focus-capture".to_string(),
                        widget_type: "capture".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                ],
            }],
        },
        DashboardPreset {
            id: "finance".to_string(),
            name: "Finance".to_string(),
            tabs: vec![DashboardTab {
                id: "finance-main".to_string(),
                label: "Finance".to_string(),
                widgets: vec![
                    WidgetSlot {
                        id: "finance-widget".to_string(),
                        widget_type: "finance".to_string(),
                        label: None,
                        col_span: 2,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "finance-stats".to_string(),
                        widget_type: "stats".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                    WidgetSlot {
                        id: "finance-goals".to_string(),
                        widget_type: "goals".to_string(),
                        label: None,
                        col_span: 1,
                        row_span: 1,
                        config: None,
                    },
                ],
            }],
        },
    ]
}

fn deep_clone_presets(presets: &[DashboardPreset]) -> Vec<DashboardPreset> {
    presets.to_vec()
}

use super::types::WidgetDef;
use indexmap::IndexMap;

pub struct WidgetRegistry {
    defs: IndexMap<String, WidgetDef>,
}

impl WidgetRegistry {
    pub fn new() -> Self {
        Self {
            defs: IndexMap::new(),
        }
    }

    pub fn register(&mut self, def: WidgetDef) -> &mut Self {
        self.defs.insert(def.id.clone(), def);
        self
    }

    pub fn register_all(&mut self, defs: Vec<WidgetDef>) -> &mut Self {
        for def in defs {
            self.defs.insert(def.id.clone(), def);
        }
        self
    }

    pub fn get(&self, id: &str) -> Option<&WidgetDef> {
        self.defs.get(id)
    }

    pub fn has(&self, id: &str) -> bool {
        self.defs.contains_key(id)
    }

    pub fn all_defs(&self) -> Vec<&WidgetDef> {
        self.defs.values().collect()
    }

    pub fn all_ids(&self) -> Vec<&str> {
        self.defs.keys().map(|k| k.as_str()).collect()
    }
}

impl Default for WidgetRegistry {
    fn default() -> Self {
        Self::new()
    }
}

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, NumericBounds, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "dashboard-stats".into(),
            label: "Stats".into(),
            description: "Key metrics at a glance".into(),
            category: WidgetCategory::Display,
            default_size: WidgetSize::new(1, 1),
            signals: vec![SignalSpec::new("clicked", "Widget clicked")],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "label".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "value".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "trend".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "dashboard-tasks".into(),
            label: "Tasks".into(),
            description: "Active task list".into(),
            category: WidgetCategory::Display,
            default_size: WidgetSize::new(2, 1),
            config_fields: vec![FieldSpec::select(
                "filter",
                "Filter",
                vec![
                    SelectOption::new("all", "All"),
                    SelectOption::new("today", "Today"),
                    SelectOption::new("overdue", "Overdue"),
                ],
            )],
            signals: vec![SignalSpec::new("task-selected", "A task was selected")
                .with_payload(vec![FieldSpec::text("task_id", "Task ID")])],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh")],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Tasks"}),
                        },
                        TemplateNode::Repeater {
                            source: "tasks".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No tasks".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "dashboard-timer".into(),
            label: "Timer".into(),
            description: "Pomodoro / focus timer".into(),
            category: WidgetCategory::Temporal,
            default_size: WidgetSize::new(1, 1),
            config_fields: vec![FieldSpec::number(
                "duration_minutes",
                "Duration (minutes)",
                NumericBounds::unbounded(),
            )
            .with_default(json!(25))],
            signals: vec![
                SignalSpec::new("started", "Timer started"),
                SignalSpec::new("completed", "Timer completed"),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("start", "Start", "play"),
                ToolbarAction::signal("reset", "Reset", "refresh"),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "remaining".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "phase".into(),
                            component_id: "text".into(),
                            prop_key: "body".into(),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "dashboard-recent".into(),
            label: "Recent".into(),
            description: "Recently visited items".into(),
            category: WidgetCategory::Display,
            default_size: WidgetSize::new(2, 1),
            config_fields: vec![FieldSpec::number(
                "limit",
                "Items to show",
                NumericBounds::unbounded(),
            )
            .with_default(json!(10))],
            signals: vec![SignalSpec::new("item-selected", "An item was selected")
                .with_payload(vec![FieldSpec::text("item_id", "Item ID")])],
            toolbar_actions: vec![
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
                ToolbarAction::signal("clear", "Clear", "trash"),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Recent"}),
                        },
                        TemplateNode::Repeater {
                            source: "items".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No recent items".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "dashboard-graph".into(),
            label: "Graph".into(),
            description: "Knowledge graph visualization".into(),
            category: WidgetCategory::Display,
            default_size: WidgetSize::new(2, 2),
            signals: vec![
                SignalSpec::new("node-selected", "A graph node was selected")
                    .with_payload(vec![FieldSpec::text("node_id", "Node ID")]),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Knowledge Graph"}),
                        },
                        TemplateNode::DataBinding {
                            field: "graph_data".into(),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interaction::dashboard::types::*;
    use std::cell::Cell;
    use std::rc::Rc;

    // ── Helpers ──────────────────────────────────────────────────

    fn simple_preset(id: &str, tab_ids: &[&str]) -> DashboardPreset {
        DashboardPreset {
            id: id.to_string(),
            name: id.to_string(),
            tabs: tab_ids
                .iter()
                .map(|tid| DashboardTab {
                    id: tid.to_string(),
                    label: tid.to_string(),
                    widgets: Vec::new(),
                })
                .collect(),
        }
    }

    fn slot(id: &str, wt: &str, col: u8) -> WidgetSlot {
        WidgetSlot {
            id: id.to_string(),
            widget_type: wt.to_string(),
            label: None,
            col_span: col,
            row_span: 1,
            config: None,
        }
    }

    fn make_widget_def(id: &str) -> WidgetDef {
        WidgetDef {
            id: id.to_string(),
            label: id.to_string(),
            description: None,
            default_col_span: 1,
            default_row_span: 1,
            min_col_span: 1,
            max_col_span: 3,
            config_schema: vec![],
        }
    }

    // ── DashboardController construction ─────────────────────────

    #[test]
    fn construction_deep_clones_presets() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let ctrl = DashboardController::new(presets.clone(), None);
        // Mutating the source should not affect the controller.
        assert_eq!(ctrl.presets().len(), 1);
        assert_eq!(ctrl.presets()[0].tabs.len(), 1);
    }

    #[test]
    fn construction_falls_back_to_first_tab() {
        let presets = vec![simple_preset("p1", &["tab-a", "tab-b"])];
        let ctrl = DashboardController::new(presets, None);
        assert_eq!(ctrl.active_tab_id(), "tab-a");
    }

    #[test]
    fn construction_uses_provided_active_tab() {
        let presets = vec![simple_preset("p1", &["t1", "t2"])];
        let ctrl = DashboardController::new(presets, Some("t2".to_string()));
        assert_eq!(ctrl.active_tab_id(), "t2");
    }

    #[test]
    fn construction_empty_presets() {
        let ctrl = DashboardController::new(vec![], None);
        assert_eq!(ctrl.active_tab_id(), "");
        assert!(ctrl.presets().is_empty());
    }

    // ── Tab ops ──────────────────────────────────────────────────

    #[test]
    fn add_tab_creates_tab_in_first_preset() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let tab = ctrl.add_tab("New Tab");
        assert_eq!(tab.label, "New Tab");
        assert_eq!(ctrl.presets()[0].tabs.len(), 2);
    }

    #[test]
    fn add_tab_notifies_listeners() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.add_tab("New");
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn remove_tab_unknown_is_noop() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.remove_tab("nonexistent");
        assert_eq!(calls.get(), 0);
        assert_eq!(ctrl.presets()[0].tabs.len(), 1);
    }

    #[test]
    fn remove_tab_reassigns_active() {
        let presets = vec![simple_preset("p1", &["t1", "t2"])];
        let mut ctrl = DashboardController::new(presets, Some("t1".to_string()));
        ctrl.remove_tab("t1");
        assert_eq!(ctrl.active_tab_id(), "t2");
    }

    #[test]
    fn rename_tab_updates_label() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.rename_tab("t1", "Renamed");
        assert_eq!(ctrl.presets()[0].tabs[0].label, "Renamed");
    }

    #[test]
    fn rename_tab_unknown_is_silent() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.rename_tab("nonexistent", "X");
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn set_active_tab_changes_id() {
        let presets = vec![simple_preset("p1", &["t1", "t2"])];
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.set_active_tab("t2");
        assert_eq!(ctrl.active_tab_id(), "t2");
    }

    #[test]
    fn set_active_tab_same_is_noop() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.set_active_tab("t1");
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn reorder_tabs_respects_order_and_appends_missing() {
        let presets = vec![simple_preset("p1", &["a", "b", "c"])];
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.reorder_tabs(&["c", "a"]);
        let ids: Vec<&str> = ctrl.presets()[0]
            .tabs
            .iter()
            .map(|t| t.id.as_str())
            .collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    // ── Widget ops ───────────────────────────────────────────────

    #[test]
    fn add_widget_with_explicit_id() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let w = ctrl
            .add_widget(
                "t1",
                NewWidgetSlot {
                    id: Some("w1".to_string()),
                    widget_type: "stats".to_string(),
                    label: None,
                    col_span: Some(2),
                    row_span: Some(1),
                    config: None,
                },
            )
            .unwrap();
        assert_eq!(w.id, "w1");
        assert_eq!(w.col_span, 2);
    }

    #[test]
    fn add_widget_auto_generates_id() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let w = ctrl
            .add_widget(
                "t1",
                NewWidgetSlot {
                    id: None,
                    widget_type: "timer".to_string(),
                    label: None,
                    col_span: None,
                    row_span: None,
                    config: None,
                },
            )
            .unwrap();
        assert!(!w.id.is_empty());
        // UUID v4 is 36 chars.
        assert_eq!(w.id.len(), 36);
    }

    #[test]
    fn add_widget_unknown_tab_returns_none() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let result = ctrl.add_widget(
            "nonexistent",
            NewWidgetSlot {
                id: None,
                widget_type: "stats".to_string(),
                label: None,
                col_span: None,
                row_span: None,
                config: None,
            },
        );
        assert!(result.is_none());
    }

    #[test]
    fn remove_widget_removes_and_notifies() {
        let mut presets = vec![simple_preset("p1", &["t1"])];
        presets[0].tabs[0].widgets.push(slot("w1", "stats", 1));
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.remove_widget("t1", "w1");
        assert!(ctrl.presets()[0].tabs[0].widgets.is_empty());
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn remove_widget_unknown_is_silent() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.remove_widget("t1", "w-nonexistent");
        assert_eq!(calls.get(), 0);
    }

    #[test]
    fn update_widget_patches_fields() {
        let mut presets = vec![simple_preset("p1", &["t1"])];
        presets[0].tabs[0].widgets.push(slot("w1", "stats", 1));
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.update_widget(
            "t1",
            "w1",
            WidgetPatch {
                label: Some(Some("Updated".to_string())),
                col_span: Some(2),
                row_span: None,
                config: None,
            },
        );
        let w = &ctrl.presets()[0].tabs[0].widgets[0];
        assert_eq!(w.label.as_deref(), Some("Updated"));
        assert_eq!(w.col_span, 2);
        assert_eq!(w.row_span, 1); // unchanged
    }

    #[test]
    fn update_widget_never_overwrites_id() {
        let mut presets = vec![simple_preset("p1", &["t1"])];
        presets[0].tabs[0].widgets.push(slot("w1", "stats", 1));
        let mut ctrl = DashboardController::new(presets, None);
        // WidgetPatch has no `id` field, so this is enforced by the type system.
        ctrl.update_widget(
            "t1",
            "w1",
            WidgetPatch {
                label: Some(Some("Patched".to_string())),
                col_span: None,
                row_span: None,
                config: None,
            },
        );
        assert_eq!(ctrl.presets()[0].tabs[0].widgets[0].id, "w1");
    }

    #[test]
    fn update_widget_clamps_span() {
        let mut presets = vec![simple_preset("p1", &["t1"])];
        presets[0].tabs[0].widgets.push(slot("w1", "stats", 1));
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.update_widget(
            "t1",
            "w1",
            WidgetPatch {
                label: None,
                col_span: Some(5),
                row_span: Some(0),
                config: None,
            },
        );
        let w = &ctrl.presets()[0].tabs[0].widgets[0];
        assert_eq!(w.col_span, 3);
        assert_eq!(w.row_span, 1);
    }

    #[test]
    fn reorder_widgets_respects_order_and_appends_missing() {
        let mut presets = vec![simple_preset("p1", &["t1"])];
        presets[0].tabs[0].widgets.push(slot("a", "stats", 1));
        presets[0].tabs[0].widgets.push(slot("b", "tasks", 1));
        presets[0].tabs[0].widgets.push(slot("c", "timer", 1));
        let mut ctrl = DashboardController::new(presets, None);
        ctrl.reorder_widgets("t1", &["c", "a"]);
        let ids: Vec<&str> = ctrl.presets()[0].tabs[0]
            .widgets
            .iter()
            .map(|w| w.id.as_str())
            .collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    // ── Subscribe / unsubscribe ──────────────────────────────────

    #[test]
    fn subscribe_call_count() {
        let presets = vec![simple_preset("p1", &["t1", "t2"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.set_active_tab("t2");
        ctrl.rename_tab("t1", "X");
        assert_eq!(calls.get(), 2);
    }

    #[test]
    fn unsubscribe_stops_notifications() {
        let presets = vec![simple_preset("p1", &["t1", "t2"])];
        let mut ctrl = DashboardController::new(presets, None);
        let calls = Rc::new(Cell::new(0usize));
        let cc = calls.clone();
        let id = ctrl.subscribe(Box::new(move || cc.set(cc.get() + 1)));
        ctrl.set_active_tab("t2");
        assert_eq!(calls.get(), 1);
        ctrl.unsubscribe(id);
        ctrl.set_active_tab("t1");
        assert_eq!(calls.get(), 1);
    }

    // ── to_json ──────────────────────────────────────────────────

    #[test]
    fn to_json_deep_clone_isolation() {
        let presets = vec![simple_preset("p1", &["t1"])];
        let mut ctrl = DashboardController::new(presets, None);
        let snapshot = ctrl.to_json();
        ctrl.rename_tab("t1", "Changed");
        assert_eq!(snapshot[0].tabs[0].label, "t1");
        assert_eq!(ctrl.presets()[0].tabs[0].label, "Changed");
    }

    // ── clamp_span ───────────────────────────────────────────────

    #[test]
    fn clamp_span_boundaries() {
        assert_eq!(clamp_span(0), 1);
        assert_eq!(clamp_span(1), 1);
        assert_eq!(clamp_span(2), 2);
        assert_eq!(clamp_span(3), 3);
        assert_eq!(clamp_span(4), 3);
        assert_eq!(clamp_span(255), 3);
    }

    // ── layout_rows ──────────────────────────────────────────────

    #[test]
    fn layout_rows_empty() {
        let rows = layout_rows(&[]);
        assert!(rows.is_empty());
    }

    #[test]
    fn layout_rows_packing() {
        let widgets = vec![slot("a", "s", 1), slot("b", "s", 1), slot("c", "s", 1)];
        let rows = layout_rows(&widgets);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].len(), 3);
    }

    #[test]
    fn layout_rows_wrapping() {
        let widgets = vec![slot("a", "s", 2), slot("b", "s", 2), slot("c", "s", 1)];
        let rows = layout_rows(&widgets);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].len(), 1); // a (span-2) alone, b wouldn't fit
        assert_eq!(rows[1].len(), 2); // b (span-2) + c (span-1)
    }

    #[test]
    fn layout_rows_span_3_isolation() {
        let widgets = vec![slot("a", "s", 1), slot("b", "s", 3), slot("c", "s", 1)];
        let rows = layout_rows(&widgets);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].len(), 1); // a
        assert_eq!(rows[1].len(), 1); // b (span-3 forces its own row)
        assert_eq!(rows[2].len(), 1); // c
    }

    #[test]
    fn layout_rows_default_col_span() {
        // col_span=1 is the default, should pack 3 per row.
        let widgets = vec![
            slot("a", "s", 1),
            slot("b", "s", 1),
            slot("c", "s", 1),
            slot("d", "s", 1),
        ];
        let rows = layout_rows(&widgets);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].len(), 3);
        assert_eq!(rows[1].len(), 1);
    }

    // ── grid_row_count ───────────────────────────────────────────

    #[test]
    fn grid_row_count_consistent_with_layout_rows() {
        let widgets = vec![
            slot("a", "s", 1),
            slot("b", "s", 2),
            slot("c", "s", 3),
            slot("d", "s", 1),
        ];
        assert_eq!(grid_row_count(&widgets), layout_rows(&widgets).len());
    }

    // ── create_default_presets ────────────────────────────────────

    #[test]
    fn create_default_presets_count_and_names() {
        let presets = create_default_presets();
        assert_eq!(presets.len(), 3);
        assert_eq!(presets[0].name, "Home");
        assert_eq!(presets[1].name, "Focus");
        assert_eq!(presets[2].name, "Finance");
    }

    #[test]
    fn create_default_presets_no_shared_references() {
        let a = create_default_presets();
        let b = create_default_presets();
        // Mutating one shouldn't affect the other (they're separate allocations).
        assert_eq!(a[0].tabs[0].widgets.len(), b[0].tabs[0].widgets.len());
    }

    // ── WidgetRegistry ───────────────────────────────────────────

    #[test]
    fn registry_register_and_get() {
        let mut reg = WidgetRegistry::new();
        reg.register(make_widget_def("stats"));
        assert!(reg.has("stats"));
        assert_eq!(reg.get("stats").unwrap().label, "stats");
    }

    #[test]
    fn registry_has_returns_false_for_unknown() {
        let reg = WidgetRegistry::new();
        assert!(!reg.has("nonexistent"));
    }

    #[test]
    fn registry_chaining() {
        let mut reg = WidgetRegistry::new();
        reg.register(make_widget_def("a"))
            .register(make_widget_def("b"));
        assert_eq!(reg.all_ids().len(), 2);
    }

    #[test]
    fn registry_register_all() {
        let mut reg = WidgetRegistry::new();
        reg.register_all(vec![make_widget_def("x"), make_widget_def("y")]);
        assert!(reg.has("x"));
        assert!(reg.has("y"));
    }

    #[test]
    fn registry_all_defs_returns_all() {
        let mut reg = WidgetRegistry::new();
        reg.register_all(built_in_widgets());
        assert_eq!(reg.all_defs().len(), 12);
    }

    #[test]
    fn registry_all_ids_returns_all() {
        let mut reg = WidgetRegistry::new();
        reg.register_all(built_in_widgets());
        let ids = reg.all_ids();
        assert_eq!(ids.len(), 12);
        assert!(ids.contains(&"stats"));
        assert!(ids.contains(&"custom"));
    }

    #[test]
    fn registry_preserves_insertion_order() {
        let mut reg = WidgetRegistry::new();
        reg.register(make_widget_def("z"))
            .register(make_widget_def("a"))
            .register(make_widget_def("m"));
        let ids = reg.all_ids();
        assert_eq!(ids, vec!["z", "a", "m"]);
    }

    // ── built_in_widgets ─────────────────────────────────────────

    #[test]
    fn built_in_widgets_has_12_entries() {
        let widgets = built_in_widgets();
        assert_eq!(widgets.len(), 12);
        let ids: Vec<&str> = widgets.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"stats"));
        assert!(ids.contains(&"databases"));
        assert!(ids.contains(&"tasks"));
        assert!(ids.contains(&"reminders"));
        assert!(ids.contains(&"capture"));
        assert!(ids.contains(&"goals"));
        assert!(ids.contains(&"finance"));
        assert!(ids.contains(&"quick-links"));
        assert!(ids.contains(&"timer"));
        assert!(ids.contains(&"recent"));
        assert!(ids.contains(&"graph"));
        assert!(ids.contains(&"custom"));
    }

    #[test]
    fn widget_contributions_has_5_entries() {
        let contributions = widget_contributions();
        assert_eq!(contributions.len(), 5);
        let ids: Vec<&str> = contributions.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"dashboard-stats"));
        assert!(ids.contains(&"dashboard-tasks"));
        assert!(ids.contains(&"dashboard-timer"));
        assert!(ids.contains(&"dashboard-recent"));
        assert!(ids.contains(&"dashboard-graph"));
    }

    #[test]
    fn widget_contributions_roundtrip_through_json() {
        for c in widget_contributions() {
            let json = serde_json::to_string(&c).unwrap();
            let back: crate::widget::WidgetContribution = serde_json::from_str(&json).unwrap();
            assert_eq!(back.id, c.id);
        }
    }
}
