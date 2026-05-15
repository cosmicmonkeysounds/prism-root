//! Shared dashboard data shapes.
//!
//! Widget definitions themselves live as
//! [`crate::widget::WidgetContribution`]s declared in
//! [`super::controller::widget_contributions`] — see that function for
//! the unified registration path. These types describe how widgets get
//! laid out inside a dashboard tab (slots, tabs, presets); they don't
//! describe the widgets themselves.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetSlot {
    pub id: String,
    pub widget_type: String,
    pub label: Option<String>,
    pub col_span: u8,
    pub row_span: u8,
    pub config: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardTab {
    pub id: String,
    pub label: String,
    pub widgets: Vec<WidgetSlot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DashboardPreset {
    pub id: String,
    pub name: String,
    pub tabs: Vec<DashboardTab>,
}
