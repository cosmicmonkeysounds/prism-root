#![allow(unused_imports)]

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use prism_builder::app::PrismApp;
use prism_builder::layout::{GridCell, PageSize};
use prism_builder::{
    compile_slint_preview, compute_layout, preview_component_factory,
    render_document_slint_preview_with_assets_and_data, BuilderDocument, CellEdge,
    ComponentRegistry, FacetKind, FieldKind, Node, NodeId, ScriptLanguage,
};
use prism_core::design_tokens::DesignTokens;
use prism_core::editor::EditorState;
#[cfg(feature = "native")]
use prism_core::foundation::persistence::{CollectionStore, EdgeFilter, ObjectFilter};
use prism_core::foundation::vfs::VfsManager;
use prism_luau_derive::SlintBinding;
use slint::{ComponentHandle, Model, ModelRc, SharedString, TimerMode, VecModel};

/// Editor-cursor scalar properties pushed in lockstep at the end of
/// `push_editor_data`. The line model is replaced separately because it's
/// a `VecModel`, not a Slint property.
#[derive(SlintBinding)]
#[slint(global = "AppWindow", push_only)]
struct EditorCursorBindings {
    editor_cursor_line: i32,
    editor_cursor_col: i32,
    editor_cursor_visible: bool,
    editor_cursor_prefix: SharedString,
    editor_language: SharedString,
    editor_line_count: i32,
    editor_char_count: i32,
}

use super::super::commands::build_context_menu_items;
use super::super::{
    panel_id_for_slint, sync_model, AppState, PersistentModels, ShellInner, ShellView,
    TransformTool,
};
use super::{
    clear_panel_slots, clone_node_with_new_ids, collect_split_bounds, default_props_for_component,
    deserialize_addr, field_kind_for_key, field_row_data_to_slint, format_slider_value,
    format_value_for_source, mime_from_extension, panel_metadata_from_workspace, parse_hex_color,
    push_dock_layout, push_user_swatches, resolve_schema_id, serialize_addr,
    slint_source_key_for_edit, sync_ui_from_shared, sync_ui_impl,
};
use crate::panels::{editor::CodeEditorPanel, properties::PropertiesPanel, Panel};
use crate::search::SearchIndex;
use crate::selection::SelectionModel;
use crate::{
    AppCardItem, AppWindow, BreadcrumbItem, ColorPreset, CommandItem, ComponentPaletteItem,
    DockDividerRect, DockPanelRect, DockTabItem, EditorIndentGuide, EditorLine, EditorToken,
    ExplorerNodeItem, FieldRow, GridCellItem, GridEdgeHandle, InspectorNode, MenuDef, MenuItem,
    PageLayoutData, PreviewNode, SearchResultItem, TabItem, ToastItem, WidgetToolbarItem,
    WorkflowPageItem,
};

pub(crate) fn push_editor_data(models: &PersistentModels, window: &AppWindow, es: &EditorState) {
    use prism_core::editor::{
        active_indent_depth, compute_line_indent_guides, highlight_line, is_foldable, TokenKind,
    };

    let line_count = es.buffer.line_count();
    let cursor_line = es.cursor.position.line;
    let cursor_col = es.cursor.position.col;
    let active_depth = active_indent_depth(&es.buffer, cursor_line, es.tab_width);

    let (sel_start, sel_end) = es
        .selection
        .as_ref()
        .map(|s| s.ordered_positions(&es.buffer))
        .unzip();

    let mut lines: Vec<EditorLine> = Vec::with_capacity(line_count);

    let mut i = 0;
    while i < line_count {
        if es.fold_state.is_hidden(i) {
            i += 1;
            continue;
        }

        let raw = es.buffer.line(i).unwrap_or_default();
        let trimmed = raw.trim_end_matches('\n');
        let tokens_raw = highlight_line(trimmed, &es.language);

        let tokens: Vec<EditorToken> = tokens_raw
            .into_iter()
            .map(|t| {
                let c = match t.kind {
                    TokenKind::Keyword => slint::Color::from_rgb_u8(0xc6, 0x78, 0xdd),
                    TokenKind::String => slint::Color::from_rgb_u8(0x98, 0xc3, 0x79),
                    TokenKind::Comment => slint::Color::from_rgb_u8(0x5c, 0x63, 0x70),
                    TokenKind::Number => slint::Color::from_rgb_u8(0xd1, 0x9a, 0x66),
                    TokenKind::Operator => slint::Color::from_rgb_u8(0x56, 0xb6, 0xc2),
                    TokenKind::Punctuation => slint::Color::from_rgb_u8(0xab, 0xb2, 0xbf),
                    TokenKind::Identifier => slint::Color::from_rgb_u8(0xe0, 0x6c, 0x75),
                    TokenKind::Whitespace => slint::Color::from_argb_u8(0, 0, 0, 0),
                    TokenKind::Plain => slint::Color::from_rgb_u8(0xab, 0xb2, 0xbf),
                };
                EditorToken {
                    text: SharedString::from(t.text),
                    token_color: c,
                    col_offset: 0,
                }
            })
            .collect();
        let token_model = Rc::new(VecModel::from(tokens));

        let is_current = i == cursor_line;
        let (sf, st) = compute_line_selection(i, trimmed.len(), &sel_start, &sel_end);

        let guides_raw = compute_line_indent_guides(&es.buffer, i, es.tab_width, active_depth);
        let guides: Vec<EditorIndentGuide> = guides_raw
            .into_iter()
            .map(|g| EditorIndentGuide {
                depth: g.depth as i32,
                active: g.active,
            })
            .collect();
        let guide_model = Rc::new(VecModel::from(guides));

        let folded = es.fold_state.is_fold_start(i);
        let foldable = folded || is_foldable(&es.buffer, i, es.tab_width);
        let fold_preview = if folded {
            es.fold_state
                .get_fold(i)
                .map(|f| SharedString::from(&f.preview))
                .unwrap_or_default()
        } else {
            SharedString::default()
        };

        lines.push(EditorLine {
            number: (i + 1) as i32,
            buffer_line: i as i32,
            tokens: ModelRc::from(token_model as Rc<dyn Model<Data = EditorToken>>),
            indent_guides: ModelRc::from(guide_model as Rc<dyn Model<Data = EditorIndentGuide>>),
            is_current,
            sel_from: sf,
            sel_to: st,
            is_foldable: foldable,
            is_folded: folded,
            fold_preview,
        });

        i += 1;
    }

    sync_model(&models.editor_lines, &lines, |c| {
        window.set_editor_lines_count(c)
    });

    let cursor_prefix: String = es
        .buffer
        .line(cursor_line)
        .unwrap_or_default()
        .trim_end_matches('\n')
        .chars()
        .take(cursor_col)
        .collect();

    EditorCursorBindings {
        editor_cursor_line: cursor_line as i32,
        editor_cursor_col: cursor_col as i32,
        editor_cursor_visible: true,
        editor_cursor_prefix: SharedString::from(cursor_prefix),
        editor_language: SharedString::from(&es.language),
        editor_line_count: line_count as i32,
        editor_char_count: es.buffer.len_chars() as i32,
    }
    .bind_to(window);
}

pub(crate) fn display_row_to_buffer_line(es: &EditorState, display_row: usize) -> usize {
    let mut display = 0;
    for i in 0..es.buffer.line_count() {
        if es.fold_state.is_hidden(i) {
            continue;
        }
        if display == display_row {
            return i;
        }
        display += 1;
    }
    es.buffer.line_count().saturating_sub(1)
}

pub(crate) fn compute_line_selection(
    line: usize,
    line_len: usize,
    sel_start: &Option<prism_core::editor::Position>,
    sel_end: &Option<prism_core::editor::Position>,
) -> (i32, i32) {
    let (start, end) = match (sel_start, sel_end) {
        (Some(s), Some(e)) => (s, e),
        _ => return (-1, -1),
    };
    if line < start.line || line > end.line {
        return (-1, -1);
    }
    let from = if line == start.line { start.col } else { 0 };
    let to = if line == end.line {
        end.col
    } else {
        line_len + 1
    };
    if from == to {
        return (-1, -1);
    }
    (from as i32, to as i32)
}
