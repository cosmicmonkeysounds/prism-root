//! `domain::spreadsheet` — pure-data spreadsheet engine.
//!
//! Port of `@core/spreadsheet` TypeScript module. Splits the original
//! into a `types` data module (cell types, column defs, row data,
//! addressing, workbook/sheet containers) and an `engine` module
//! (selection, virtual scrolling, clipboard TSV interop, CSV/JSON
//! import-export, and a focused formula engine supporting SUM,
//! AVERAGE, COUNT, MIN, MAX, IF, CONCATENATE, cell references, and
//! basic arithmetic). Layer 1 only — no UI or rendering.

pub mod engine;
pub mod types;

pub use engine::{
    cell_ref_to_string, coerce_pasted_value, compute_col_offsets, compute_virtual_window,
    extend_by, extend_to, from_csv, from_json_records, get_selection_range, is_cell_active,
    is_cell_selected, is_range_selection, move_selection, parse_cell_range, parse_cell_ref,
    parse_tsv, scroll_row_into_view, select_single, selection_size, serialize_cell,
    serialize_range, to_csv, to_json_records, widget_contributions, BasicFormulaEngine,
    FormulaEngine, SelectionRange, SelectionState, VirtualConfig, VirtualWindow,
};
pub use types::{
    CellAddress, CellRange, CellType, CellValue, ColumnDef, RowData, SelectOption, Sheet,
    SpreadsheetData, Workbook,
};
