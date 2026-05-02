//! Pure data types for the spreadsheet engine.
//!
//! Port of `@core/spreadsheet` types. Defines cell types, column
//! definitions, row data, addressing, and workbook/sheet containers.

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Cell Types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CellType {
    Text,
    Number,
    Currency,
    Date,
    DateTime,
    Boolean,
    Select,
    Url,
    Email,
    Formula,
}

// ── Column Definition ─────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
    pub color: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnDef {
    pub id: String,
    pub label: String,
    pub cell_type: CellType,
    pub width: Option<f64>,
    pub min_width: Option<f64>,
    pub max_width: Option<f64>,
    pub read_only: bool,
    pub required: bool,
    /// For `CellType::Select` — the available options.
    pub options: Vec<SelectOption>,
    pub format: Option<String>,
    pub hidden: bool,
    pub frozen: bool,
}

// ── Row Data ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowData {
    pub id: String,
    /// Column-id to cell value. `IndexMap` preserves insertion order.
    pub cells: indexmap::IndexMap<String, Value>,
}

// ── Spreadsheet Data ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpreadsheetData {
    pub columns: Vec<ColumnDef>,
    pub rows: Vec<RowData>,
}

// ── Cell Addressing ───────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellAddress {
    pub row: usize,
    pub col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub start: CellAddress,
    pub end: CellAddress,
}

// ── Cell Value ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellValue {
    Text(String),
    Number(f64),
    Boolean(bool),
    Null,
}

// ── Sheet / Workbook ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sheet {
    pub id: String,
    pub name: String,
    pub data: SpreadsheetData,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workbook {
    pub sheets: Vec<Sheet>,
    pub active_sheet_id: String,
}
