//! Shared dashboard data shapes.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::widget::FieldSpec;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WidgetDef {
    pub id: String,
    pub label: String,
    pub description: Option<String>,
    pub default_col_span: u8,
    pub default_row_span: u8,
    pub min_col_span: u8,
    pub max_col_span: u8,
    pub config_schema: Vec<FieldSpec>,
}

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
