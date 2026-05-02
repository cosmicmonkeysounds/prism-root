//! Spreadsheet engine — selection, virtual scrolling, clipboard,
//! serialization, and formula evaluation.
//!
//! Port of `@core/spreadsheet` engine logic. All functions are pure
//! (no I/O, no global state). The formula engine is a focused subset
//! covering SUM, AVERAGE, COUNT, MIN, MAX, IF, CONCATENATE, cell
//! references (A1 notation), ranges, and basic arithmetic.

use serde_json::Value;

use super::types::{
    CellAddress, CellRange, CellType, CellValue, ColumnDef, RowData, SpreadsheetData,
};

// ═══════════════════════════════════════════════════════════════════
// Selection Model
// ═══════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionState {
    pub anchor_row: usize,
    pub anchor_col: usize,
    pub active_row: usize,
    pub active_col: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectionRange {
    pub min_row: usize,
    pub max_row: usize,
    pub min_col: usize,
    pub max_col: usize,
}

/// Create a single-cell selection.
pub fn select_single(row: usize, col: usize) -> SelectionState {
    SelectionState {
        anchor_row: row,
        anchor_col: col,
        active_row: row,
        active_col: col,
    }
}

/// Extend selection from anchor to the given cell.
pub fn extend_to(state: &SelectionState, row: usize, col: usize) -> SelectionState {
    SelectionState {
        anchor_row: state.anchor_row,
        anchor_col: state.anchor_col,
        active_row: row,
        active_col: col,
    }
}

/// Move the entire selection by `(dr, dc)`, clamping to bounds.
pub fn move_selection(
    state: &SelectionState,
    dr: i32,
    dc: i32,
    row_count: usize,
    col_count: usize,
) -> SelectionState {
    let r = clamp_add(state.active_row, dr, row_count);
    let c = clamp_add(state.active_col, dc, col_count);
    select_single(r, c)
}

/// Extend the selection by `(dr, dc)`, clamping active cell to bounds.
pub fn extend_by(
    state: &SelectionState,
    dr: i32,
    dc: i32,
    row_count: usize,
    col_count: usize,
) -> SelectionState {
    let r = clamp_add(state.active_row, dr, row_count);
    let c = clamp_add(state.active_col, dc, col_count);
    extend_to(state, r, c)
}

/// Normalize the selection into a min/max range.
pub fn get_selection_range(state: &SelectionState) -> SelectionRange {
    SelectionRange {
        min_row: state.anchor_row.min(state.active_row),
        max_row: state.anchor_row.max(state.active_row),
        min_col: state.anchor_col.min(state.active_col),
        max_col: state.anchor_col.max(state.active_col),
    }
}

/// True when the selection spans more than one cell.
pub fn is_range_selection(state: &SelectionState) -> bool {
    state.anchor_row != state.active_row || state.anchor_col != state.active_col
}

/// True when `(row, col)` falls within the selection range.
pub fn is_cell_selected(state: &SelectionState, row: usize, col: usize) -> bool {
    let r = get_selection_range(state);
    row >= r.min_row && row <= r.max_row && col >= r.min_col && col <= r.max_col
}

/// True when `(row, col)` is the active (cursor) cell.
pub fn is_cell_active(state: &SelectionState, row: usize, col: usize) -> bool {
    state.active_row == row && state.active_col == col
}

/// Number of cells in the selection rectangle.
pub fn selection_size(state: &SelectionState) -> usize {
    let r = get_selection_range(state);
    (r.max_row - r.min_row + 1) * (r.max_col - r.min_col + 1)
}

fn clamp_add(val: usize, delta: i32, count: usize) -> usize {
    if count == 0 {
        return 0;
    }
    let max = count.saturating_sub(1);
    let result = val as i64 + delta as i64;
    result.clamp(0, max as i64) as usize
}

// ═══════════════════════════════════════════════════════════════════
// Virtual Scrolling Model
// ═══════════════════════════════════════════════════════════════════

pub struct VirtualConfig {
    pub row_count: usize,
    pub col_count: usize,
    pub row_height: f64,
    pub col_widths: Vec<f64>,
    pub container_height: f64,
    pub container_width: f64,
    pub scroll_top: f64,
    pub scroll_left: f64,
    pub overscan: usize,
}

pub struct VirtualWindow {
    pub start_row: usize,
    pub end_row: usize,
    pub start_col: usize,
    pub end_col: usize,
    pub offset_top: f64,
    pub offset_left: f64,
    pub total_height: f64,
    pub total_width: f64,
}

/// Compute cumulative column offsets from widths.
pub fn compute_col_offsets(col_widths: &[f64]) -> Vec<f64> {
    let mut offsets = Vec::with_capacity(col_widths.len() + 1);
    offsets.push(0.0);
    let mut acc = 0.0;
    for &w in col_widths {
        acc += w;
        offsets.push(acc);
    }
    offsets
}

/// Compute the visible window of rows and columns given scroll state.
pub fn compute_virtual_window(config: &VirtualConfig) -> VirtualWindow {
    let total_height = config.row_count as f64 * config.row_height;
    let col_offsets = compute_col_offsets(&config.col_widths);
    let total_width = col_offsets.last().copied().unwrap_or(0.0);

    // Rows
    let first_visible_row = if config.row_height > 0.0 {
        (config.scroll_top / config.row_height).floor() as usize
    } else {
        0
    };
    let visible_rows = if config.row_height > 0.0 {
        (config.container_height / config.row_height).ceil() as usize + 1
    } else {
        0
    };
    let start_row = first_visible_row.saturating_sub(config.overscan);
    let end_row = (first_visible_row + visible_rows + config.overscan).min(config.row_count);

    // Columns — binary search for first visible
    let start_col_raw = col_offsets
        .partition_point(|&o| o <= config.scroll_left)
        .saturating_sub(1);
    let end_col_raw =
        col_offsets.partition_point(|&o| o < config.scroll_left + config.container_width);
    let start_col = start_col_raw.saturating_sub(config.overscan);
    let end_col = (end_col_raw + config.overscan).min(config.col_count);

    let offset_top = start_row as f64 * config.row_height;
    let offset_left = if start_col < col_offsets.len() {
        col_offsets[start_col]
    } else {
        0.0
    };

    VirtualWindow {
        start_row,
        end_row,
        start_col,
        end_col,
        offset_top,
        offset_left,
        total_height,
        total_width,
    }
}

/// Return the scroll_top value that makes `row` visible.
pub fn scroll_row_into_view(
    row: usize,
    row_height: f64,
    scroll_top: f64,
    container_height: f64,
) -> f64 {
    let row_top = row as f64 * row_height;
    let row_bottom = row_top + row_height;
    if row_top < scroll_top {
        row_top
    } else if row_bottom > scroll_top + container_height {
        row_bottom - container_height
    } else {
        scroll_top
    }
}

// ═══════════════════════════════════════════════════════════════════
// Clipboard (TSV Interop)
// ═══════════════════════════════════════════════════════════════════

/// Serialize a rectangular region of the spreadsheet as TSV.
pub fn serialize_range(
    data: &SpreadsheetData,
    start_row: usize,
    end_row: usize,
    start_col: usize,
    end_col: usize,
) -> String {
    let mut out = String::new();
    for r in start_row..=end_row {
        if r > start_row {
            out.push('\n');
        }
        if let Some(row) = data.rows.get(r) {
            for c in start_col..=end_col {
                if c > start_col {
                    out.push('\t');
                }
                if let Some(col_def) = data.columns.get(c) {
                    if let Some(val) = row.cells.get(&col_def.id) {
                        out.push_str(&serialize_cell(val));
                    }
                }
            }
        }
    }
    out
}

/// Serialize a single JSON cell value to a clipboard-friendly string.
pub fn serialize_cell(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(b) => if *b { "true" } else { "false" }.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => s.clone(),
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(serialize_cell).collect();
            parts.join(", ")
        }
        Value::Object(_) => serde_json::to_string(value).unwrap_or_default(),
    }
}

/// Parse a TSV string into a 2D grid of strings.
/// Handles quoted fields (double-quote escaping).
pub fn parse_tsv(text: &str) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    for line in text.lines() {
        let mut cells = Vec::new();
        let mut chars = line.chars().peekable();
        while chars.peek().is_some() {
            if chars.peek() == Some(&'"') {
                // Quoted field
                chars.next(); // consume opening quote
                let mut field = String::new();
                loop {
                    match chars.next() {
                        Some('"') => {
                            if chars.peek() == Some(&'"') {
                                field.push('"');
                                chars.next();
                            } else {
                                break;
                            }
                        }
                        Some(c) => field.push(c),
                        None => break,
                    }
                }
                cells.push(field);
                // Skip the tab after the quoted field
                if chars.peek() == Some(&'\t') {
                    chars.next();
                }
            } else {
                // Unquoted field
                let mut field = String::new();
                loop {
                    match chars.peek() {
                        Some(&'\t') => {
                            chars.next();
                            break;
                        }
                        Some(_) => field.push(chars.next().unwrap()),
                        None => break,
                    }
                }
                cells.push(field);
            }
        }
        rows.push(cells);
    }
    rows
}

/// Coerce a pasted string to a typed `CellValue` based on the target
/// column's cell type.
pub fn coerce_pasted_value(raw: &str, col_type: CellType) -> CellValue {
    if raw.is_empty() {
        return CellValue::Null;
    }
    match col_type {
        CellType::Number | CellType::Currency => {
            // Strip currency symbols and commas
            let cleaned: String = raw
                .chars()
                .filter(|c| *c != '$' && *c != ',' && *c != '€' && *c != '£')
                .collect();
            match cleaned.trim().parse::<f64>() {
                Ok(n) => CellValue::Number(n),
                Err(_) => CellValue::Text(raw.to_string()),
            }
        }
        CellType::Boolean => match raw.to_lowercase().as_str() {
            "true" | "yes" | "1" => CellValue::Boolean(true),
            "false" | "no" | "0" => CellValue::Boolean(false),
            _ => CellValue::Text(raw.to_string()),
        },
        CellType::Date | CellType::DateTime => CellValue::Text(raw.to_string()),
        _ => CellValue::Text(raw.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Serialization (CSV / JSON Import-Export)
// ═══════════════════════════════════════════════════════════════════

/// Export spreadsheet data to CSV.
pub fn to_csv(data: &SpreadsheetData, include_header: bool) -> String {
    let mut out = String::new();
    if include_header {
        let headers: Vec<String> = data.columns.iter().map(|c| csv_escape(&c.label)).collect();
        out.push_str(&headers.join(","));
        out.push('\n');
    }
    for (i, row) in data.rows.iter().enumerate() {
        if i > 0 || include_header {
            // newline already added after header or between rows
        }
        if i > 0 {
            out.push('\n');
        }
        let cells: Vec<String> = data
            .columns
            .iter()
            .map(|col| {
                row.cells
                    .get(&col.id)
                    .map(|v| csv_escape(&serialize_cell(v)))
                    .unwrap_or_default()
            })
            .collect();
        out.push_str(&cells.join(","));
    }
    out
}

/// Export spreadsheet data as a vector of JSON record objects.
pub fn to_json_records(data: &SpreadsheetData) -> Vec<serde_json::Map<String, Value>> {
    data.rows
        .iter()
        .map(|row| {
            let mut map = serde_json::Map::new();
            for col in &data.columns {
                let val = row.cells.get(&col.id).cloned().unwrap_or(Value::Null);
                map.insert(col.label.clone(), val);
            }
            map
        })
        .collect()
}

/// Import CSV into spreadsheet data. If `column_ids` is provided, use
/// those as column IDs; otherwise auto-generate from headers.
pub fn from_csv(csv: &str, column_ids: Option<&[&str]>) -> SpreadsheetData {
    let mut lines = csv.lines();
    let header_line = match lines.next() {
        Some(l) => l,
        None => {
            return SpreadsheetData {
                columns: Vec::new(),
                rows: Vec::new(),
            };
        }
    };

    let headers = parse_csv_line(header_line);
    let columns: Vec<ColumnDef> = headers
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let id = column_ids
                .and_then(|ids| ids.get(i).copied())
                .map(String::from)
                .unwrap_or_else(|| format!("col_{i}"));
            ColumnDef {
                id,
                label: h.clone(),
                cell_type: CellType::Text,
                width: None,
                min_width: None,
                max_width: None,
                read_only: false,
                required: false,
                options: Vec::new(),
                format: None,
                hidden: false,
                frozen: false,
            }
        })
        .collect();

    let mut rows = Vec::new();
    for (i, line) in lines.enumerate() {
        let values = parse_csv_line(line);
        let mut cells = indexmap::IndexMap::new();
        for (j, col) in columns.iter().enumerate() {
            let raw = values.get(j).cloned().unwrap_or_default();
            cells.insert(col.id.clone(), infer_json_value(&raw));
        }
        rows.push(RowData {
            id: format!("row_{i}"),
            cells,
        });
    }

    SpreadsheetData { columns, rows }
}

/// Import JSON records into spreadsheet data.
pub fn from_json_records(
    records: &[serde_json::Map<String, Value>],
    column_overrides: Option<&[ColumnDef]>,
) -> SpreadsheetData {
    // Collect all keys from records in order
    let mut all_keys = indexmap::IndexMap::<String, ()>::new();
    for rec in records {
        for key in rec.keys() {
            all_keys.entry(key.clone()).or_default();
        }
    }

    let columns: Vec<ColumnDef> = if let Some(overrides) = column_overrides {
        overrides.to_vec()
    } else {
        all_keys
            .keys()
            .enumerate()
            .map(|(i, key)| ColumnDef {
                id: format!("col_{i}"),
                label: key.clone(),
                cell_type: CellType::Text,
                width: None,
                min_width: None,
                max_width: None,
                read_only: false,
                required: false,
                options: Vec::new(),
                format: None,
                hidden: false,
                frozen: false,
            })
            .collect()
    };

    let label_to_id: indexmap::IndexMap<String, String> = columns
        .iter()
        .map(|c| (c.label.clone(), c.id.clone()))
        .collect();

    let rows: Vec<RowData> = records
        .iter()
        .enumerate()
        .map(|(i, rec)| {
            let mut cells = indexmap::IndexMap::new();
            for (key, val) in rec {
                if let Some(col_id) = label_to_id.get(key) {
                    cells.insert(col_id.clone(), val.clone());
                }
            }
            RowData {
                id: format!("row_{i}"),
                cells,
            }
        })
        .collect();

    SpreadsheetData { columns, rows }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        let escaped = s.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        s.to_string()
    }
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        if chars.peek().is_none() {
            break;
        }
        if chars.peek() == Some(&'"') {
            chars.next(); // opening quote
            let mut field = String::new();
            loop {
                match chars.next() {
                    Some('"') => {
                        if chars.peek() == Some(&'"') {
                            field.push('"');
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    Some(c) => field.push(c),
                    None => break,
                }
            }
            fields.push(field);
            if chars.peek() == Some(&',') {
                chars.next();
            }
        } else {
            let mut field = String::new();
            loop {
                match chars.peek() {
                    Some(&',') => {
                        chars.next();
                        break;
                    }
                    Some(_) => field.push(chars.next().unwrap()),
                    None => break,
                }
            }
            fields.push(field);
        }
    }
    fields
}

fn infer_json_value(raw: &str) -> Value {
    if raw.is_empty() {
        return Value::Null;
    }
    if let Ok(n) = raw.parse::<f64>() {
        if let Some(num) = serde_json::Number::from_f64(n) {
            return Value::Number(num);
        }
    }
    match raw.to_lowercase().as_str() {
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => Value::String(raw.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Formula Engine
// ═══════════════════════════════════════════════════════════════════

/// Trait for pluggable formula evaluation.
pub trait FormulaEngine {
    fn evaluate(
        &self,
        formula: &str,
        data: &SpreadsheetData,
        sheet_row: usize,
        sheet_col: usize,
    ) -> CellValue;

    fn is_formula(value: &str) -> bool
    where
        Self: Sized,
    {
        value.starts_with('=')
    }
}

/// A focused formula engine supporting SUM, AVERAGE, COUNT, MIN, MAX,
/// IF, CONCATENATE, cell references (A1), ranges (A1:B3), basic
/// arithmetic (+, -, *, /), and comparisons (=, <>, <, >, <=, >=).
pub struct BasicFormulaEngine;

impl FormulaEngine for BasicFormulaEngine {
    fn evaluate(
        &self,
        formula: &str,
        data: &SpreadsheetData,
        _sheet_row: usize,
        _sheet_col: usize,
    ) -> CellValue {
        if !formula.starts_with('=') {
            return CellValue::Text(formula.to_string());
        }
        let expr = &formula[1..];
        let mut parser = FormulaParser::new(expr, data);
        parser.parse_expression()
    }
}

// ── Cell Reference Helpers ────────────────────────────────────────

/// Parse "A1" notation to a `CellAddress`. Column letters are
/// case-insensitive. Returns `None` on invalid input.
pub fn parse_cell_ref(cell_ref: &str) -> Option<CellAddress> {
    let cell_ref = cell_ref.trim();
    if cell_ref.is_empty() {
        return None;
    }
    let mut col_part = String::new();
    let mut row_part = String::new();
    for c in cell_ref.chars() {
        if c.is_ascii_alphabetic() && row_part.is_empty() {
            col_part.push(c.to_ascii_uppercase());
        } else if c.is_ascii_digit() {
            row_part.push(c);
        } else {
            return None;
        }
    }
    if col_part.is_empty() || row_part.is_empty() {
        return None;
    }
    let col = col_letters_to_index(&col_part)?;
    let row: usize = row_part.parse::<usize>().ok()?.checked_sub(1)?;
    Some(CellAddress { row, col })
}

/// Parse "A1:B3" notation to a `CellRange`.
pub fn parse_cell_range(range_ref: &str) -> Option<CellRange> {
    let parts: Vec<&str> = range_ref.split(':').collect();
    if parts.len() != 2 {
        return None;
    }
    let start = parse_cell_ref(parts[0])?;
    let end = parse_cell_ref(parts[1])?;
    Some(CellRange { start, end })
}

/// Convert a `CellAddress` back to A1 notation.
pub fn cell_ref_to_string(addr: &CellAddress) -> String {
    let col_str = col_index_to_letters(addr.col);
    format!("{}{}", col_str, addr.row + 1)
}

fn col_letters_to_index(letters: &str) -> Option<usize> {
    let mut result: usize = 0;
    for c in letters.chars() {
        if !c.is_ascii_uppercase() {
            return None;
        }
        result = result
            .checked_mul(26)?
            .checked_add((c as usize) - ('A' as usize) + 1)?;
    }
    result.checked_sub(1)
}

fn col_index_to_letters(mut index: usize) -> String {
    let mut letters = String::new();
    loop {
        letters.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters
}

// ── Formula Parser (Recursive Descent) ────────────────────────────

struct FormulaParser<'a> {
    input: &'a str,
    pos: usize,
    data: &'a SpreadsheetData,
}

impl<'a> FormulaParser<'a> {
    fn new(input: &'a str, data: &'a SpreadsheetData) -> Self {
        Self {
            input,
            pos: 0,
            data,
        }
    }

    fn parse_expression(&mut self) -> CellValue {
        self.skip_whitespace();
        self.parse_comparison()
    }

    fn parse_comparison(&mut self) -> CellValue {
        let left = self.parse_additive();
        self.skip_whitespace();

        let op = if self.try_consume("<=") {
            Some("<=")
        } else if self.try_consume(">=") {
            Some(">=")
        } else if self.try_consume("<>") {
            Some("<>")
        } else if self.try_consume("<") {
            Some("<")
        } else if self.try_consume(">") {
            Some(">")
        } else if self.try_consume("=") {
            Some("=")
        } else {
            None
        };

        if let Some(op) = op {
            let right = self.parse_additive();
            let result = match (to_number(&left), to_number(&right)) {
                (Some(l), Some(r)) => match op {
                    "<" => l < r,
                    ">" => l > r,
                    "<=" => l <= r,
                    ">=" => l >= r,
                    "=" => (l - r).abs() < f64::EPSILON,
                    "<>" => (l - r).abs() >= f64::EPSILON,
                    _ => false,
                },
                _ => {
                    let ls = to_text(&left);
                    let rs = to_text(&right);
                    match op {
                        "=" => ls == rs,
                        "<>" => ls != rs,
                        "<" => ls < rs,
                        ">" => ls > rs,
                        "<=" => ls <= rs,
                        ">=" => ls >= rs,
                        _ => false,
                    }
                }
            };
            CellValue::Boolean(result)
        } else {
            left
        }
    }

    fn parse_additive(&mut self) -> CellValue {
        let mut left = self.parse_multiplicative();
        loop {
            self.skip_whitespace();
            if self.try_consume("+") {
                let right = self.parse_multiplicative();
                left = match (to_number(&left), to_number(&right)) {
                    (Some(l), Some(r)) => CellValue::Number(l + r),
                    _ => {
                        // String concatenation fallback
                        CellValue::Text(format!("{}{}", to_text(&left), to_text(&right)))
                    }
                };
            } else if self.peek_char() == Some('-') && !self.at_end() {
                self.pos += 1;
                let right = self.parse_multiplicative();
                left = match (to_number(&left), to_number(&right)) {
                    (Some(l), Some(r)) => CellValue::Number(l - r),
                    _ => CellValue::Null,
                };
            } else {
                break;
            }
        }
        left
    }

    fn parse_multiplicative(&mut self) -> CellValue {
        let mut left = self.parse_unary();
        loop {
            self.skip_whitespace();
            if self.try_consume("*") {
                let right = self.parse_unary();
                left = match (to_number(&left), to_number(&right)) {
                    (Some(l), Some(r)) => CellValue::Number(l * r),
                    _ => CellValue::Null,
                };
            } else if self.try_consume("/") {
                let right = self.parse_unary();
                left = match (to_number(&left), to_number(&right)) {
                    (Some(l), Some(r)) => {
                        if r == 0.0 {
                            CellValue::Text("#DIV/0!".to_string())
                        } else {
                            CellValue::Number(l / r)
                        }
                    }
                    _ => CellValue::Null,
                };
            } else {
                break;
            }
        }
        left
    }

    fn parse_unary(&mut self) -> CellValue {
        self.skip_whitespace();
        if self.try_consume("-") {
            let val = self.parse_primary();
            match to_number(&val) {
                Some(n) => CellValue::Number(-n),
                None => CellValue::Null,
            }
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> CellValue {
        self.skip_whitespace();

        // Parenthesized expression
        if self.try_consume("(") {
            let val = self.parse_expression();
            self.skip_whitespace();
            self.try_consume(")");
            return val;
        }

        // String literal
        if self.peek_char() == Some('"') {
            return self.parse_string_literal();
        }

        // Number literal
        if let Some(c) = self.peek_char() {
            if c.is_ascii_digit() || c == '.' {
                return self.parse_number_literal();
            }
        }

        // Function or cell reference
        if let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() {
                let ident = self.parse_identifier();
                self.skip_whitespace();

                // Function call
                if self.try_consume("(") {
                    let result = self.evaluate_function(&ident);
                    self.skip_whitespace();
                    self.try_consume(")");
                    return result;
                }

                // Cell reference — could be a range like A1:B3 (handled
                // by the caller as a function argument) or a single cell.
                if let Some(addr) = parse_cell_ref(&ident) {
                    return self.resolve_cell(addr);
                }

                // Boolean literals
                match ident.to_uppercase().as_str() {
                    "TRUE" => return CellValue::Boolean(true),
                    "FALSE" => return CellValue::Boolean(false),
                    _ => {}
                }

                return CellValue::Text(ident);
            }
        }

        CellValue::Null
    }

    fn parse_string_literal(&mut self) -> CellValue {
        self.pos += 1; // skip opening quote
        let mut s = String::new();
        while let Some(c) = self.peek_char() {
            self.pos += 1;
            if c == '"' {
                if self.peek_char() == Some('"') {
                    s.push('"');
                    self.pos += 1;
                } else {
                    break;
                }
            } else {
                s.push(c);
            }
        }
        CellValue::Text(s)
    }

    fn parse_number_literal(&mut self) -> CellValue {
        let start = self.pos;
        let mut has_dot = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.pos += 1;
            } else if c == '.' && !has_dot {
                has_dot = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        let num_str = &self.input[start..self.pos];
        match num_str.parse::<f64>() {
            Ok(n) => CellValue::Number(n),
            Err(_) => CellValue::Null,
        }
    }

    fn parse_identifier(&mut self) -> String {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.pos += 1;
            } else {
                break;
            }
        }
        self.input[start..self.pos].to_string()
    }

    fn evaluate_function(&mut self, name: &str) -> CellValue {
        match name.to_uppercase().as_str() {
            "SUM" => self.eval_aggregate(|vals| vals.iter().sum()),
            "AVERAGE" => self.eval_aggregate(|vals| {
                if vals.is_empty() {
                    0.0
                } else {
                    vals.iter().sum::<f64>() / vals.len() as f64
                }
            }),
            "COUNT" => {
                let vals = self.collect_numeric_args();
                CellValue::Number(vals.len() as f64)
            }
            "MIN" => self.eval_aggregate(|vals| vals.iter().copied().fold(f64::INFINITY, f64::min)),
            "MAX" => {
                self.eval_aggregate(|vals| vals.iter().copied().fold(f64::NEG_INFINITY, f64::max))
            }
            "IF" => self.eval_if(),
            "CONCATENATE" => self.eval_concatenate(),
            _ => CellValue::Text(format!("#NAME? {name}")),
        }
    }

    fn eval_aggregate(&mut self, f: impl FnOnce(&[f64]) -> f64) -> CellValue {
        let vals = self.collect_numeric_args();
        if vals.is_empty() {
            CellValue::Number(0.0)
        } else {
            CellValue::Number(f(&vals))
        }
    }

    fn collect_numeric_args(&mut self) -> Vec<f64> {
        let mut vals = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek_char() == Some(')') || self.at_end() {
                break;
            }

            // Try to parse a range reference like A1:B3
            let saved = self.pos;
            let ident = self.parse_identifier();
            self.skip_whitespace();
            if self.try_consume(":") {
                let ident2 = self.parse_identifier();
                let range_str = format!("{ident}:{ident2}");
                if let Some(range) = parse_cell_range(&range_str) {
                    let range_vals = self.resolve_range(&range);
                    for v in range_vals {
                        if let Some(n) = to_number(&v) {
                            vals.push(n);
                        }
                    }
                    self.skip_whitespace();
                    self.try_consume(",");
                    continue;
                }
            }

            // Not a range — backtrack and parse as expression
            self.pos = saved;
            let val = self.parse_expression();
            if let Some(n) = to_number(&val) {
                vals.push(n);
            }
            self.skip_whitespace();
            self.try_consume(",");
        }
        vals
    }

    fn eval_if(&mut self) -> CellValue {
        self.skip_whitespace();
        let condition = self.parse_expression();
        self.skip_whitespace();
        self.try_consume(",");
        self.skip_whitespace();
        let then_val = self.parse_expression();
        self.skip_whitespace();
        let else_val = if self.try_consume(",") {
            self.skip_whitespace();
            self.parse_expression()
        } else {
            CellValue::Boolean(false)
        };

        let is_true = match &condition {
            CellValue::Boolean(b) => *b,
            CellValue::Number(n) => *n != 0.0,
            CellValue::Text(s) => !s.is_empty(),
            CellValue::Null => false,
        };

        if is_true {
            then_val
        } else {
            else_val
        }
    }

    fn eval_concatenate(&mut self) -> CellValue {
        let mut parts = Vec::new();
        loop {
            self.skip_whitespace();
            if self.peek_char() == Some(')') || self.at_end() {
                break;
            }
            let val = self.parse_expression();
            parts.push(to_text(&val));
            self.skip_whitespace();
            self.try_consume(",");
        }
        CellValue::Text(parts.join(""))
    }

    fn resolve_cell(&self, addr: CellAddress) -> CellValue {
        if let Some(row) = self.data.rows.get(addr.row) {
            if let Some(col) = self.data.columns.get(addr.col) {
                if let Some(val) = row.cells.get(&col.id) {
                    return json_to_cell_value(val);
                }
            }
        }
        CellValue::Null
    }

    fn resolve_range(&self, range: &CellRange) -> Vec<CellValue> {
        let min_row = range.start.row.min(range.end.row);
        let max_row = range.start.row.max(range.end.row);
        let min_col = range.start.col.min(range.end.col);
        let max_col = range.start.col.max(range.end.col);
        let mut vals = Vec::new();
        for r in min_row..=max_row {
            for c in min_col..=max_col {
                vals.push(self.resolve_cell(CellAddress { row: r, col: c }));
            }
        }
        vals
    }

    // ── Scanner Helpers ───────────────────────────────────────────

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn at_end(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn try_consume(&mut self, s: &str) -> bool {
        if self.input[self.pos..].starts_with(s) {
            self.pos += s.len();
            true
        } else {
            false
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_ascii_whitespace() {
                self.pos += 1;
            } else {
                break;
            }
        }
    }
}

fn to_number(val: &CellValue) -> Option<f64> {
    match val {
        CellValue::Number(n) => Some(*n),
        CellValue::Boolean(b) => Some(if *b { 1.0 } else { 0.0 }),
        CellValue::Text(s) => s.trim().parse::<f64>().ok(),
        CellValue::Null => None,
    }
}

fn to_text(val: &CellValue) -> String {
    match val {
        CellValue::Text(s) => s.clone(),
        CellValue::Number(n) => {
            if *n == n.floor() && n.abs() < 1e15 {
                format!("{}", *n as i64)
            } else {
                n.to_string()
            }
        }
        CellValue::Boolean(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        CellValue::Null => String::new(),
    }
}

fn json_to_cell_value(val: &Value) -> CellValue {
    match val {
        Value::Null => CellValue::Null,
        Value::Bool(b) => CellValue::Boolean(*b),
        Value::Number(n) => CellValue::Number(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => CellValue::Text(s.clone()),
        _ => CellValue::Text(val.to_string()),
    }
}

// ═══════════════════════════════════════════════════════════════════
// Widget Contributions
// ═══════════════════════════════════════════════════════════════════

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, NumericBounds, SelectOption, SignalSpec, TemplateNode,
        ToolbarAction, VariantOptionSpec, VariantSpec, WidgetCategory, WidgetContribution,
        WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "spreadsheet-data-table".into(),
            label: "Data Table".into(),
            description: "Interactive spreadsheet grid".into(),
            category: WidgetCategory::DataTable,
            config_fields: vec![
                FieldSpec::text("sheet_id", "Sheet ID"),
                FieldSpec::number(
                    "frozen_columns",
                    "Frozen Columns",
                    NumericBounds::min_max(0.0, 10.0),
                )
                .with_default(json!(0)),
                FieldSpec::boolean("show_row_numbers", "Show Row Numbers")
                    .with_default(json!(true)),
            ],
            signals: vec![
                SignalSpec::new("cell-selected", "A cell was selected").with_payload(vec![
                    FieldSpec::text("row_id", "Row ID"),
                    FieldSpec::text("col_id", "Column ID"),
                ]),
                SignalSpec::new("cell-changed", "A cell value changed").with_payload(vec![
                    FieldSpec::text("row_id", "Row ID"),
                    FieldSpec::text("col_id", "Column ID"),
                    FieldSpec::text("value", "Value"),
                ]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("add-row", "Add Row", "add"),
                ToolbarAction::signal("add-column", "Add Column", "add"),
                ToolbarAction::signal("delete-row", "Delete Row", "delete"),
                ToolbarAction::signal("export", "Export", "export"),
            ],
            variants: vec![VariantSpec {
                key: "density".into(),
                label: "Density".into(),
                options: vec![
                    VariantOptionSpec {
                        value: "compact".into(),
                        label: "Compact".into(),
                        overrides: json!({"row_height": 24}),
                    },
                    VariantOptionSpec {
                        value: "default".into(),
                        label: "Default".into(),
                        overrides: json!({"row_height": 32}),
                    },
                    VariantOptionSpec {
                        value: "comfortable".into(),
                        label: "Comfortable".into(),
                        overrides: json!({"row_height": 48}),
                    },
                ],
            }],
            default_size: WidgetSize::new(3, 2),
            data_query: Some(DataQuery {
                object_type: Some("row".into()),
                ..Default::default()
            }),
            data_key: Some("rows".into()),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::Repeater {
                            source: "rows".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "row"}),
                            }),
                            empty_label: Some("No data".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "spreadsheet-pivot-table".into(),
            label: "Pivot Table".into(),
            description: "Grouped summary view".into(),
            category: WidgetCategory::DataTable,
            config_fields: vec![
                FieldSpec::text("group_by", "Group By"),
                FieldSpec::select(
                    "aggregate",
                    "Aggregate",
                    vec![
                        SelectOption::new("count", "Count"),
                        SelectOption::new("sum", "Sum"),
                        SelectOption::new("avg", "Average"),
                        SelectOption::new("min", "Min"),
                        SelectOption::new("max", "Max"),
                    ],
                ),
                FieldSpec::text("value_column", "Value Column"),
            ],
            signals: vec![SignalSpec::new("group-selected", "A group was selected")
                .with_payload(vec![FieldSpec::text("group_key", "Group Key")])],
            toolbar_actions: vec![ToolbarAction::signal("refresh", "Refresh", "refresh")],
            default_size: WidgetSize::new(2, 2),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Pivot Table", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "groups".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "group"}),
                            }),
                            empty_label: Some("No groups".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "spreadsheet-chart".into(),
            label: "Chart".into(),
            description: "Data visualization".into(),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::select(
                    "chart_type",
                    "Chart Type",
                    vec![
                        SelectOption::new("bar", "Bar"),
                        SelectOption::new("line", "Line"),
                        SelectOption::new("pie", "Pie"),
                        SelectOption::new("scatter", "Scatter"),
                    ],
                ),
                FieldSpec::text("x_column", "X Column"),
                FieldSpec::text("y_column", "Y Column"),
                FieldSpec::text("title", "Title"),
            ],
            signals: vec![
                SignalSpec::new("point-selected", "A data point was selected").with_payload(vec![
                    FieldSpec::number("data_index", "Data Index", NumericBounds::unbounded()),
                ]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
                ToolbarAction::signal("export", "Export", "export"),
            ],
            default_size: WidgetSize::new(2, 2),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::DataBinding {
                            field: "title".into(),
                            component_id: "heading".into(),
                            prop_key: "body".into(),
                        },
                        TemplateNode::DataBinding {
                            field: "chart_data".into(),
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

// ═══════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── Test Helpers ──────────────────────────────────────────────

    fn make_test_data() -> SpreadsheetData {
        SpreadsheetData {
            columns: vec![
                ColumnDef {
                    id: "a".into(),
                    label: "Name".into(),
                    cell_type: CellType::Text,
                    width: Some(100.0),
                    min_width: None,
                    max_width: None,
                    read_only: false,
                    required: false,
                    options: Vec::new(),
                    format: None,
                    hidden: false,
                    frozen: false,
                },
                ColumnDef {
                    id: "b".into(),
                    label: "Score".into(),
                    cell_type: CellType::Number,
                    width: Some(80.0),
                    min_width: None,
                    max_width: None,
                    read_only: false,
                    required: false,
                    options: Vec::new(),
                    format: None,
                    hidden: false,
                    frozen: false,
                },
                ColumnDef {
                    id: "c".into(),
                    label: "Active".into(),
                    cell_type: CellType::Boolean,
                    width: Some(60.0),
                    min_width: None,
                    max_width: None,
                    read_only: false,
                    required: false,
                    options: Vec::new(),
                    format: None,
                    hidden: false,
                    frozen: false,
                },
            ],
            rows: vec![
                RowData {
                    id: "r0".into(),
                    cells: indexmap::IndexMap::from([
                        ("a".into(), json!("Alice")),
                        ("b".into(), json!(10)),
                        ("c".into(), json!(true)),
                    ]),
                },
                RowData {
                    id: "r1".into(),
                    cells: indexmap::IndexMap::from([
                        ("a".into(), json!("Bob")),
                        ("b".into(), json!(20)),
                        ("c".into(), json!(false)),
                    ]),
                },
                RowData {
                    id: "r2".into(),
                    cells: indexmap::IndexMap::from([
                        ("a".into(), json!("Carol")),
                        ("b".into(), json!(30)),
                        ("c".into(), json!(true)),
                    ]),
                },
            ],
        }
    }

    // ── Selection Tests ──────────────────────────────────────────

    #[test]
    fn select_single_sets_anchor_and_active() {
        let s = select_single(2, 3);
        assert_eq!(s.anchor_row, 2);
        assert_eq!(s.anchor_col, 3);
        assert_eq!(s.active_row, 2);
        assert_eq!(s.active_col, 3);
    }

    #[test]
    fn extend_to_keeps_anchor_moves_active() {
        let s = select_single(1, 1);
        let s2 = extend_to(&s, 3, 4);
        assert_eq!(s2.anchor_row, 1);
        assert_eq!(s2.anchor_col, 1);
        assert_eq!(s2.active_row, 3);
        assert_eq!(s2.active_col, 4);
    }

    #[test]
    fn move_selection_with_clamping() {
        let s = select_single(0, 0);
        // Move up from top-left — should clamp to (0,0)
        let s2 = move_selection(&s, -1, -1, 10, 10);
        assert_eq!(s2.active_row, 0);
        assert_eq!(s2.active_col, 0);
        // Move to bottom-right boundary
        let s3 = move_selection(&select_single(9, 9), 1, 1, 10, 10);
        assert_eq!(s3.active_row, 9);
        assert_eq!(s3.active_col, 9);
        // Normal move
        let s4 = move_selection(&select_single(5, 5), -2, 3, 10, 10);
        assert_eq!(s4.active_row, 3);
        assert_eq!(s4.active_col, 8);
    }

    #[test]
    fn get_selection_range_normalizes_min_max() {
        // active < anchor
        let s = SelectionState {
            anchor_row: 5,
            anchor_col: 8,
            active_row: 2,
            active_col: 3,
        };
        let r = get_selection_range(&s);
        assert_eq!(r.min_row, 2);
        assert_eq!(r.max_row, 5);
        assert_eq!(r.min_col, 3);
        assert_eq!(r.max_col, 8);
    }

    #[test]
    fn is_range_selection_detects_multi_cell() {
        assert!(!is_range_selection(&select_single(1, 1)));
        assert!(is_range_selection(&extend_to(&select_single(1, 1), 2, 2)));
    }

    #[test]
    fn is_cell_selected_within_range() {
        let s = extend_to(&select_single(1, 1), 3, 3);
        assert!(is_cell_selected(&s, 2, 2));
        assert!(is_cell_selected(&s, 1, 1));
        assert!(is_cell_selected(&s, 3, 3));
        assert!(!is_cell_selected(&s, 0, 0));
        assert!(!is_cell_selected(&s, 4, 4));
    }

    #[test]
    fn is_cell_active_checks_cursor() {
        let s = extend_to(&select_single(1, 1), 3, 3);
        assert!(is_cell_active(&s, 3, 3));
        assert!(!is_cell_active(&s, 1, 1));
    }

    #[test]
    fn selection_size_counts_cells() {
        let s = select_single(0, 0);
        assert_eq!(selection_size(&s), 1);
        let s2 = extend_to(&select_single(0, 0), 2, 3);
        assert_eq!(selection_size(&s2), 12); // 3 rows * 4 cols
    }

    #[test]
    fn extend_by_clamps_and_extends() {
        let s = select_single(5, 5);
        let s2 = extend_by(&s, -10, 2, 10, 10);
        assert_eq!(s2.anchor_row, 5);
        assert_eq!(s2.anchor_col, 5);
        assert_eq!(s2.active_row, 0);
        assert_eq!(s2.active_col, 7);
    }

    // ── Virtual Scrolling Tests ──────────────────────────────────

    #[test]
    fn compute_virtual_window_basic() {
        let config = VirtualConfig {
            row_count: 100,
            col_count: 5,
            row_height: 30.0,
            col_widths: vec![100.0; 5],
            container_height: 300.0,
            container_width: 400.0,
            scroll_top: 0.0,
            scroll_left: 0.0,
            overscan: 0,
        };
        let w = compute_virtual_window(&config);
        assert_eq!(w.start_row, 0);
        assert_eq!(w.end_row, 11); // ceil(300/30) + 1 = 11
        assert_eq!(w.start_col, 0);
        assert_eq!(w.total_height, 3000.0);
        assert_eq!(w.total_width, 500.0);
    }

    #[test]
    fn compute_virtual_window_with_overscan() {
        let config = VirtualConfig {
            row_count: 100,
            col_count: 10,
            row_height: 25.0,
            col_widths: vec![80.0; 10],
            container_height: 200.0,
            container_width: 300.0,
            scroll_top: 250.0, // row 10
            scroll_left: 0.0,
            overscan: 3,
        };
        let w = compute_virtual_window(&config);
        assert_eq!(w.start_row, 7); // 10 - 3
                                    // visible = ceil(200/25)+1 = 9, end = 10+9+3 = 22
        assert_eq!(w.end_row, 22);
    }

    #[test]
    fn compute_virtual_window_scroll_at_top() {
        let config = VirtualConfig {
            row_count: 50,
            col_count: 3,
            row_height: 20.0,
            col_widths: vec![100.0, 150.0, 200.0],
            container_height: 200.0,
            container_width: 300.0,
            scroll_top: 0.0,
            scroll_left: 0.0,
            overscan: 2,
        };
        let w = compute_virtual_window(&config);
        assert_eq!(w.start_row, 0);
        assert_eq!(w.offset_top, 0.0);
    }

    #[test]
    fn compute_virtual_window_scroll_at_bottom() {
        let config = VirtualConfig {
            row_count: 20,
            col_count: 2,
            row_height: 30.0,
            col_widths: vec![100.0, 100.0],
            container_height: 300.0,
            container_width: 200.0,
            scroll_top: 300.0, // row 10
            scroll_left: 0.0,
            overscan: 3,
        };
        let w = compute_virtual_window(&config);
        assert_eq!(w.end_row, 20); // clamped to row_count
    }

    #[test]
    fn scroll_row_into_view_already_visible() {
        let st = scroll_row_into_view(5, 30.0, 100.0, 300.0);
        // Row 5 is at 150..180, visible in 100..400
        assert_eq!(st, 100.0);
    }

    #[test]
    fn scroll_row_into_view_above() {
        let st = scroll_row_into_view(2, 30.0, 120.0, 300.0);
        // Row 2 is at 60..90, above 120
        assert_eq!(st, 60.0);
    }

    #[test]
    fn scroll_row_into_view_below() {
        let st = scroll_row_into_view(15, 30.0, 0.0, 300.0);
        // Row 15 is at 450..480, need to scroll so 480 is at bottom
        assert_eq!(st, 180.0);
    }

    #[test]
    fn compute_col_offsets_cumulative() {
        let offsets = compute_col_offsets(&[100.0, 50.0, 200.0]);
        assert_eq!(offsets, vec![0.0, 100.0, 150.0, 350.0]);
    }

    // ── Clipboard Tests ──────────────────────────────────────────

    #[test]
    fn serialize_range_produces_tsv() {
        let data = make_test_data();
        let tsv = serialize_range(&data, 0, 1, 0, 1);
        assert_eq!(tsv, "Alice\t10\nBob\t20");
    }

    #[test]
    fn parse_tsv_splits_correctly() {
        let rows = parse_tsv("a\tb\tc\n1\t2\t3");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], vec!["a", "b", "c"]);
        assert_eq!(rows[1], vec!["1", "2", "3"]);
    }

    #[test]
    fn parse_tsv_handles_quoted_fields() {
        let rows = parse_tsv("\"hello\tworld\"\tplain");
        assert_eq!(rows[0][0], "hello\tworld");
        assert_eq!(rows[0][1], "plain");
    }

    #[test]
    fn parse_tsv_handles_escaped_quotes() {
        let rows = parse_tsv("\"say \"\"hello\"\"\"\tother");
        assert_eq!(rows[0][0], "say \"hello\"");
    }

    #[test]
    fn coerce_pasted_value_number() {
        assert_eq!(
            coerce_pasted_value("42.5", CellType::Number),
            CellValue::Number(42.5)
        );
        assert_eq!(
            coerce_pasted_value("$1,234.56", CellType::Currency),
            CellValue::Number(1234.56)
        );
    }

    #[test]
    fn coerce_pasted_value_boolean() {
        assert_eq!(
            coerce_pasted_value("yes", CellType::Boolean),
            CellValue::Boolean(true)
        );
        assert_eq!(
            coerce_pasted_value("false", CellType::Boolean),
            CellValue::Boolean(false)
        );
    }

    #[test]
    fn coerce_pasted_value_date() {
        assert_eq!(
            coerce_pasted_value("2024-01-15", CellType::Date),
            CellValue::Text("2024-01-15".into())
        );
    }

    #[test]
    fn coerce_pasted_value_empty_is_null() {
        assert_eq!(coerce_pasted_value("", CellType::Text), CellValue::Null);
    }

    #[test]
    fn clipboard_round_trip() {
        let data = make_test_data();
        let tsv = serialize_range(&data, 0, 2, 0, 1);
        let parsed = parse_tsv(&tsv);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0][0], "Alice");
        assert_eq!(parsed[0][1], "10");
        assert_eq!(parsed[1][0], "Bob");
        assert_eq!(parsed[2][0], "Carol");
        assert_eq!(parsed[2][1], "30");
    }

    // ── Serialization Tests ──────────────────────────────────────

    #[test]
    fn to_csv_with_header() {
        let data = make_test_data();
        let csv = to_csv(&data, true);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Name,Score,Active");
        assert_eq!(lines[1], "Alice,10,true");
        assert_eq!(lines[2], "Bob,20,false");
        assert_eq!(lines[3], "Carol,30,true");
    }

    #[test]
    fn to_csv_without_header() {
        let data = make_test_data();
        let csv = to_csv(&data, false);
        let lines: Vec<&str> = csv.lines().collect();
        assert_eq!(lines[0], "Alice,10,true");
    }

    #[test]
    fn from_csv_auto_detects_columns() {
        let csv = "Name,Age\nAlice,30\nBob,25";
        let data = from_csv(csv, None);
        assert_eq!(data.columns.len(), 2);
        assert_eq!(data.columns[0].label, "Name");
        assert_eq!(data.columns[1].label, "Age");
        assert_eq!(data.rows.len(), 2);
        assert_eq!(data.rows[0].cells[&data.columns[0].id], json!("Alice"));
        assert_eq!(data.rows[1].cells[&data.columns[1].id], json!(25.0));
    }

    #[test]
    fn from_csv_with_custom_column_ids() {
        let csv = "X,Y\n1,2";
        let data = from_csv(csv, Some(&["col_x", "col_y"]));
        assert_eq!(data.columns[0].id, "col_x");
        assert_eq!(data.columns[1].id, "col_y");
    }

    #[test]
    fn to_json_records_structure() {
        let data = make_test_data();
        let records = to_json_records(&data);
        assert_eq!(records.len(), 3);
        assert_eq!(records[0]["Name"], json!("Alice"));
        assert_eq!(records[0]["Score"], json!(10));
        assert_eq!(records[1]["Active"], json!(false));
    }

    #[test]
    fn from_json_records_without_overrides() {
        let records = vec![
            serde_json::Map::from_iter([
                ("Name".into(), json!("Alice")),
                ("Age".into(), json!(30)),
            ]),
            serde_json::Map::from_iter([("Name".into(), json!("Bob")), ("Age".into(), json!(25))]),
        ];
        let data = from_json_records(&records, None);
        assert_eq!(data.columns.len(), 2);
        assert_eq!(data.rows.len(), 2);
        // Find the Name column by label (serde_json::Map order is not guaranteed)
        let name_col = data.columns.iter().find(|c| c.label == "Name").unwrap();
        assert_eq!(data.rows[0].cells[&name_col.id], json!("Alice"));
    }

    #[test]
    fn from_json_records_with_overrides() {
        let records = vec![serde_json::Map::from_iter([(
            "Name".into(),
            json!("Alice"),
        )])];
        let overrides = vec![ColumnDef {
            id: "name_col".into(),
            label: "Name".into(),
            cell_type: CellType::Text,
            width: Some(200.0),
            min_width: None,
            max_width: None,
            read_only: true,
            required: true,
            options: Vec::new(),
            format: None,
            hidden: false,
            frozen: false,
        }];
        let data = from_json_records(&records, Some(&overrides));
        assert_eq!(data.columns[0].id, "name_col");
        assert!(data.columns[0].read_only);
        assert_eq!(data.rows[0].cells["name_col"], json!("Alice"));
    }

    #[test]
    fn csv_round_trip_stability() {
        let original = "Name,Score\nAlice,10\nBob,20";
        let data = from_csv(original, None);
        let csv_out = to_csv(&data, true);
        let data2 = from_csv(&csv_out, None);
        let csv_out2 = to_csv(&data2, true);
        assert_eq!(csv_out, csv_out2);
    }

    #[test]
    fn csv_escape_commas_and_quotes() {
        let data = SpreadsheetData {
            columns: vec![ColumnDef {
                id: "a".into(),
                label: "Val".into(),
                cell_type: CellType::Text,
                width: None,
                min_width: None,
                max_width: None,
                read_only: false,
                required: false,
                options: Vec::new(),
                format: None,
                hidden: false,
                frozen: false,
            }],
            rows: vec![RowData {
                id: "r0".into(),
                cells: indexmap::IndexMap::from([("a".into(), json!("hello, \"world\""))]),
            }],
        };
        let csv = to_csv(&data, true);
        assert!(csv.contains("\"hello, \"\"world\"\"\""));
    }

    // ── Formula Engine Tests ─────────────────────────────────────

    #[test]
    fn is_formula_detects_equals_prefix() {
        assert!(BasicFormulaEngine::is_formula("=SUM(A1:A3)"));
        assert!(BasicFormulaEngine::is_formula("=1+2"));
        assert!(!BasicFormulaEngine::is_formula("hello"));
        assert!(!BasicFormulaEngine::is_formula(""));
    }

    #[test]
    fn formula_sum() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=SUM(B1:B3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(60.0));
    }

    #[test]
    fn formula_average() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=AVERAGE(B1:B3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(20.0));
    }

    #[test]
    fn formula_count() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=COUNT(B1:B3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(3.0));
    }

    #[test]
    fn formula_min() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=MIN(B1:B3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(10.0));
    }

    #[test]
    fn formula_max() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=MAX(B1:B3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(30.0));
    }

    #[test]
    fn formula_if_true() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        // B1 = 10, 10 > 5 is true
        let result = engine.evaluate("=IF(B1>5, \"yes\", \"no\")", &data, 0, 0);
        assert_eq!(result, CellValue::Text("yes".into()));
    }

    #[test]
    fn formula_if_false() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        // B1 = 10, 10 > 50 is false
        let result = engine.evaluate("=IF(B1>50, \"yes\", \"no\")", &data, 0, 0);
        assert_eq!(result, CellValue::Text("no".into()));
    }

    #[test]
    fn formula_concatenate() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=CONCATENATE(A1, \" \", A2)", &data, 0, 0);
        assert_eq!(result, CellValue::Text("Alice Bob".into()));
    }

    #[test]
    fn formula_cell_reference() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=A1", &data, 0, 0);
        assert_eq!(result, CellValue::Text("Alice".into()));
    }

    #[test]
    fn formula_arithmetic() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=1+2*3", &data, 0, 0);
        // 2*3 = 6, then 1+6 = 7 (precedence)
        assert_eq!(result, CellValue::Number(7.0));
    }

    #[test]
    fn formula_arithmetic_with_parens() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=(1+2)*3", &data, 0, 0);
        assert_eq!(result, CellValue::Number(9.0));
    }

    #[test]
    fn formula_division_by_zero() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=10/0", &data, 0, 0);
        assert_eq!(result, CellValue::Text("#DIV/0!".into()));
    }

    #[test]
    fn formula_negation() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=-5+3", &data, 0, 0);
        assert_eq!(result, CellValue::Number(-2.0));
    }

    // ── Cell Reference Tests ─────────────────────────────────────

    #[test]
    fn parse_cell_ref_a1() {
        let addr = parse_cell_ref("A1").unwrap();
        assert_eq!(addr, CellAddress { row: 0, col: 0 });
    }

    #[test]
    fn parse_cell_ref_z26() {
        let addr = parse_cell_ref("Z26").unwrap();
        assert_eq!(addr, CellAddress { row: 25, col: 25 });
    }

    #[test]
    fn parse_cell_ref_aa1() {
        let addr = parse_cell_ref("AA1").unwrap();
        assert_eq!(addr, CellAddress { row: 0, col: 26 });
    }

    #[test]
    fn parse_cell_ref_case_insensitive() {
        let addr = parse_cell_ref("b3").unwrap();
        assert_eq!(addr, CellAddress { row: 2, col: 1 });
    }

    #[test]
    fn parse_cell_ref_invalid() {
        assert!(parse_cell_ref("").is_none());
        assert!(parse_cell_ref("123").is_none());
        assert!(parse_cell_ref("A").is_none());
        assert!(parse_cell_ref("1A").is_none());
    }

    #[test]
    fn parse_cell_range_valid() {
        let range = parse_cell_range("A1:B3").unwrap();
        assert_eq!(range.start, CellAddress { row: 0, col: 0 });
        assert_eq!(range.end, CellAddress { row: 2, col: 1 });
    }

    #[test]
    fn cell_ref_to_string_round_trip() {
        let addr = CellAddress { row: 0, col: 0 };
        assert_eq!(cell_ref_to_string(&addr), "A1");
        let addr2 = CellAddress { row: 25, col: 25 };
        assert_eq!(cell_ref_to_string(&addr2), "Z26");
        let addr3 = CellAddress { row: 0, col: 26 };
        assert_eq!(cell_ref_to_string(&addr3), "AA1");
    }

    #[test]
    fn cell_ref_round_trip() {
        for col in 0..100 {
            for row in 0..10 {
                let addr = CellAddress { row, col };
                let s = cell_ref_to_string(&addr);
                let parsed = parse_cell_ref(&s).unwrap();
                assert_eq!(parsed, addr, "failed round-trip for {s}");
            }
        }
    }

    #[test]
    fn formula_cell_ref_arithmetic() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        // B1=10, B2=20 → 30
        let result = engine.evaluate("=B1+B2", &data, 0, 0);
        assert_eq!(result, CellValue::Number(30.0));
    }

    #[test]
    fn formula_comparison() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=B1<B2", &data, 0, 0);
        assert_eq!(result, CellValue::Boolean(true));
        let result2 = engine.evaluate("=B1=B2", &data, 0, 0);
        assert_eq!(result2, CellValue::Boolean(false));
    }

    #[test]
    fn formula_boolean_literal() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=TRUE", &data, 0, 0);
        assert_eq!(result, CellValue::Boolean(true));
    }

    #[test]
    fn formula_string_literal() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=\"hello\"", &data, 0, 0);
        assert_eq!(result, CellValue::Text("hello".into()));
    }

    #[test]
    fn formula_sum_with_literal_args() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=SUM(1, 2, 3)", &data, 0, 0);
        assert_eq!(result, CellValue::Number(6.0));
    }

    #[test]
    fn formula_unknown_function() {
        let data = make_test_data();
        let engine = BasicFormulaEngine;
        let result = engine.evaluate("=UNKNOWN(1)", &data, 0, 0);
        assert_eq!(result, CellValue::Text("#NAME? UNKNOWN".into()));
    }

    #[test]
    fn serialize_cell_variants() {
        assert_eq!(serialize_cell(&json!(null)), "");
        assert_eq!(serialize_cell(&json!(true)), "true");
        assert_eq!(serialize_cell(&json!(42)), "42");
        assert_eq!(serialize_cell(&json!("hello")), "hello");
        assert_eq!(serialize_cell(&json!([1, 2, 3])), "1, 2, 3");
    }

    #[test]
    fn from_csv_empty_input() {
        let data = from_csv("", None);
        assert!(data.columns.is_empty());
        assert!(data.rows.is_empty());
    }

    #[test]
    fn from_csv_header_only() {
        let data = from_csv("A,B,C", None);
        assert_eq!(data.columns.len(), 3);
        assert!(data.rows.is_empty());
    }

    #[test]
    fn widget_contributions_returns_3_widgets() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 3);
        let ids: Vec<&str> = widgets.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"spreadsheet-data-table"));
        assert!(ids.contains(&"spreadsheet-pivot-table"));
        assert!(ids.contains(&"spreadsheet-chart"));
    }
}
