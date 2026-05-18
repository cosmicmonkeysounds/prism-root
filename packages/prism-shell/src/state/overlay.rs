//! OverlaySlot + its modal/picker/toast sub-types.
//! Split out of `state/mod.rs` (Phase B.4). `use super::*`
//! inherits intra-`state` types + crate imports; cross-module
//! callers reach widened `pub(crate)` items.

use super::*;

// ── overlay ───────────────────────────────────────────────────────

/// Floating chrome — toasts, the command palette, and the help
/// tooltip. None of these own a dock panel; they paint on top of
/// the app-window via the parsed skeleton's overlay siblings.
///
/// Three bindings consume this slot:
/// `shell.toast-stack`, `shell.command-palette`, `shell.help-tooltip`.
/// Each method emits the JSON shape its block already speaks (see
/// `components/{toast,command_palette,help_tooltip}.rs`).
#[derive(Clone, Debug, Default)]
pub struct OverlaySlot {
    pub toasts: Vec<Toast>,
    pub command_palette: CommandPalette,
    pub help_tooltip: Option<HelpTooltip>,
    /// Wave 1.6 of `docs/dev/composable-builder-plan.md` — modifier
    /// picker overlay. `open = true` after the inspector's "+ Add
    /// Behaviour" footer is clicked; selecting a behaviour or pressing
    /// Esc closes it.
    pub modifier_picker: ModifierPicker,
    /// Wave 4.3 of `docs/dev/composable-builder-plan.md` — connection
    /// picker overlay. `open = true` after the signals panel's "+
    /// Add Connection" footer is clicked; the three form fields
    /// drive a new `SignalConnection` on confirm.
    pub connection_picker: ConnectionPicker,
    /// Wave 2.4 of `docs/dev/composable-builder-plan.md` — color
    /// picker overlay. `open = true` after a color swatch is clicked;
    /// the picker hex input + preset palette commit through
    /// `set_node_prop` against `(target_id, key)`.
    pub color_picker: ColorPicker,
    /// Wave 2.3 of `docs/dev/composable-builder-plan.md` — select
    /// dropdown overlay. `open = true` after a `select`-kind
    /// field-edit row is clicked; option rows commit through
    /// `set_node_prop` against `(target_id, key)` and close the
    /// overlay. Replaces the legacy click-to-cycle behaviour for
    /// the select kind.
    pub select_dropdown: SelectDropdown,
}

/// Wave 2.3 — open/closed state of `shell.select-dropdown`. The
/// dropdown is anchored under the select-kind field-edit row that
/// opened it; `target_id` + `key` are seeded at open time and
/// consumed by the option-click commit path. `options` carries
/// the `{value, label}` row list verbatim from the field-editor
/// row so the DSL block can iterate via `for=`.
#[derive(Clone, Debug, Default)]
pub struct SelectDropdown {
    pub open: bool,
    pub target_id: String,
    pub key: String,
    pub value: String,
    pub options: Vec<Value>,
}

/// Wave 2.4 — open/closed state of `shell.color-picker`. The picker
/// is anchored under the swatch that opened it; the target_id +
/// key are seeded at open time and consumed by the hex commit /
/// preset-click mutators.
#[derive(Clone, Debug, Default)]
pub struct ColorPicker {
    pub open: bool,
    pub target_id: String,
    pub key: String,
    pub value: String,
    /// Wave 2.4 HSL — when a slider press captures, stash the
    /// (channel, track_x, track_width) triple so subsequent
    /// pointer-moves rewrite the same channel without re-reading
    /// the hit attrs. Cleared on pointer-up or when the picker
    /// closes.
    pub slider_drag: Option<ColorSliderDrag>,
}

/// Wave 2.4 HSL slider drag state — captured at pointer-down so
/// pointer-move recomputes the channel fraction against a stable
/// track rect (the hit cache rebuilds across re-renders, so we
/// can't rely on re-hit-testing mid-drag).
#[derive(Clone, Debug)]
pub struct ColorSliderDrag {
    pub channel: ColorChannel,
    pub track_x: f32,
    pub track_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorChannel {
    Hue,
    Saturation,
    Lightness,
}

impl ColorChannel {
    pub fn from_attr(s: &str) -> Option<Self> {
        match s {
            "h" => Some(Self::Hue),
            "s" => Some(Self::Saturation),
            "l" => Some(Self::Lightness),
            _ => None,
        }
    }
}

impl ColorPicker {
    /// Wave 2.4 — eight preset swatches the picker exposes as a
    /// click-to-commit row. Tuned for inspector workflows: high-
    /// contrast neutrals + the project accent + a few cools / warms
    /// for quick mock-ups. Authors who want a different palette
    /// override this list via the `presets` prop on `shell.color-picker`
    /// when authoring against a custom skeleton.
    pub const PRESETS: &'static [&'static str] = &[
        "#ffffff", "#cccccc", "#666666", "#000000", "#0060c0", "#ff5050", "#ffb020", "#22aa66",
    ];
}

/// Wave 1.6 — open/closed state of the `shell.modifier-picker`
/// overlay. The owning-node id seeds the attach mutator on selection;
/// `attached` filters the registry list so users can't double-attach.
#[derive(Clone, Debug, Default)]
pub struct ModifierPicker {
    pub open: bool,
    pub target_id: String,
    pub attached: Vec<String>,
}

/// Wave 4.3 — open/closed state of the `shell.connection-picker`
/// overlay plus the three form fields the user fills in to build
/// a new `SignalConnection`. Default state is "closed with empty
/// fields"; `open_connection_picker` flips `open` true with sensible
/// defaults so the user starts on a usable shape.
#[derive(Clone, Debug, Default)]
pub struct ConnectionPicker {
    pub open: bool,
    pub source_signal: String,
    pub action_kind: String,
    pub target_label: String,
}

impl ConnectionPicker {
    /// Wave 4.3 — ordered list of `ActionKind` variant labels the
    /// picker cycles through when the user clicks the action-kind
    /// row. The order mirrors `prism_builder::signal::ActionKind`'s
    /// declaration so adding a variant there is the only edit a
    /// future grammar change needs.
    pub const ACTION_KINDS: &'static [&'static str] = &[
        "SetProperty",
        "ToggleVisibility",
        "NavigateTo",
        "PlayAnimation",
        "EmitSignal",
        "Custom",
        "Bind",
    ];

    /// Next variant after `current` in `ACTION_KINDS`, wrapping at
    /// the end. Used by the picker's click-to-cycle row.
    pub fn cycle_action_kind(current: &str) -> &'static str {
        let len = Self::ACTION_KINDS.len();
        let idx = Self::ACTION_KINDS
            .iter()
            .position(|k| *k == current)
            .map(|i| (i + 1) % len)
            .unwrap_or(0);
        Self::ACTION_KINDS[idx]
    }
}

#[derive(Clone, Debug)]
pub struct Toast {
    pub title: String,
    pub body: String,
    pub kind: ToastKind,
}

#[derive(Clone, Copy, Debug, Default)]
pub enum ToastKind {
    #[default]
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Success => "success",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct CommandPalette {
    pub open: bool,
    /// Query buffer — a full [`TextEditor`] (single-line) so the
    /// palette inherits caret, selection, arrow nav, Ctrl+A, IME, and
    /// clipboard from the shared text-input engine. Reads project
    /// through [`Self::query_text`].
    pub query: prism_ui_runtime::editor::TextEditor,
    pub results: Vec<CommandResult>,
    pub selected_index: usize,
}

impl CommandPalette {
    /// Current query text — projected from the underlying
    /// [`TextEditor`] buffer.
    pub fn query_text(&self) -> &str {
        self.query.text()
    }
}

#[derive(Clone, Debug)]
pub struct CommandResult {
    pub id: String,
    pub label: String,
    pub shortcut: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HelpTooltip {
    pub title: String,
    pub summary: String,
}

impl OverlaySlot {
    /// JSON for `shell.toast-stack`. Empty list is valid — the block
    /// renders the empty container.
    pub fn toast_stack_props(&self) -> Value {
        json!({ "toasts": self.toasts_json() })
    }

    /// JSON for `shell.command-palette`. The block reads `query`,
    /// `results`, and `selected-index`; visibility is gated by `open`
    /// (skeleton-side `visible="…"` author attr binds against it).
    pub fn command_palette_props(&self) -> Value {
        let mut props = json!({
            "open": self.command_palette.open,
            "query": self.command_palette.query_text(),
            "caret": self.command_palette.query.caret_byte(),
            "results": self.results_json(),
            "selected-index": self.command_palette.selected_index,
        });
        if let Some((a, b)) = self.command_palette.query.selection() {
            props["selection"] = json!(format!("{a},{b}"));
        }
        props
    }

    /// JSON for `shell.help-tooltip`. When no tooltip is showing, all
    /// fields are empty strings — the block paints nothing.
    pub fn help_tooltip_props(&self) -> Value {
        match &self.help_tooltip {
            Some(t) => json!({ "title": t.title, "summary": t.summary, "visible": true }),
            None => json!({ "title": "", "summary": "", "visible": false }),
        }
    }

    /// JSON for `shell.modifier-picker`. Wave 1.6 of
    /// `docs/dev/composable-builder-plan.md`. Pulls the registered
    /// behaviours from the shared `ModifierRegistry`, filters out the
    /// already-attached ids (the `add-modifier-open` route stashed
    /// them on `self.modifier_picker.attached`), and emits the
    /// picker's open/target-id state.
    pub fn modifier_picker_props(
        &self,
        registry: Option<&prism_builder::ModifierRegistry>,
    ) -> Value {
        let options = match (self.modifier_picker.open, registry) {
            (true, Some(reg)) => {
                let attached: std::collections::HashSet<&str> = self
                    .modifier_picker
                    .attached
                    .iter()
                    .map(String::as_str)
                    .collect();
                let entries: Vec<Value> = reg
                    .list()
                    .into_iter()
                    .filter(|d| !attached.contains(d.id.as_str()))
                    .map(|d| {
                        json!({
                            "id": d.id,
                            "label": d.label,
                            "description": d.description,
                        })
                    })
                    .collect();
                Value::Array(entries)
            }
            _ => Value::Array(Vec::new()),
        };
        json!({
            "open": self.modifier_picker.open,
            "target-id": self.modifier_picker.target_id,
            "options": options,
        })
    }

    /// Wave 2.4 — JSON for `shell.color-picker`. Closed → collapses
    /// to a 0×0 hidden div; open → renders preview swatch + hex echo
    /// + preset row keyed off `ColorPicker::PRESETS`. The hex value
    ///   rides through the existing field-focus pipeline so paste /
    ///   type / Enter commits land via `set_node_prop`.
    pub fn color_picker_props(&self) -> Value {
        let presets: Vec<Value> = ColorPicker::PRESETS
            .iter()
            .map(|hex| {
                json!({
                    "value": *hex,
                    "selected": self.color_picker.value.eq_ignore_ascii_case(hex),
                })
            })
            .collect();
        let parsed = prism_builder::color::parse_hex(&self.color_picker.value)
            .unwrap_or(prism_builder::color::Rgba::BLACK);
        let (h, s, l) = prism_builder::color::rgb_to_hsl(parsed);
        json!({
            "open": self.color_picker.open,
            "target-id": self.color_picker.target_id,
            "key": self.color_picker.key,
            "value": self.color_picker.value,
            "presets": presets,
            "h": h,
            "s": s,
            "l": l,
            "h-pct": (h / 360.0) * 100.0,
            "s-pct": s,
            "l-pct": l,
        })
    }

    /// Wave 2.3 — JSON for `shell.select-dropdown`. Closed →
    /// collapses to a 0×0 hidden overlay (Wave 11.2 batch-5 pattern).
    /// Open → renders one option row per entry in `options`, each
    /// marked `selected` when its value matches the current bound
    /// value. The DSL block iterates via `for=` and emits a
    /// `data-role="select-dropdown-option"` row per entry that the
    /// pointer router commits through `commit_select_dropdown_value`.
    pub fn select_dropdown_props(&self) -> Value {
        let dropdown = &self.select_dropdown;
        let options: Vec<Value> = dropdown
            .options
            .iter()
            .map(|o| {
                let val = o
                    .get("value")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let label = o
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or(val.as_str())
                    .to_string();
                json!({
                    "value": val,
                    "label": label,
                    "selected": dropdown.value == val,
                })
            })
            .collect();
        json!({
            "open": dropdown.open,
            "target-id": dropdown.target_id,
            "key": dropdown.key,
            "value": dropdown.value,
            "options": options,
        })
    }

    /// Wave 4.3 — JSON for `shell.connection-picker`. The block
    /// reads `open` to gate visibility and the three form fields
    /// for the row labels; the routing attrs the user clicks come
    /// from the block itself, not the prop bag.
    pub fn connection_picker_props(&self) -> Value {
        json!({
            "open": self.connection_picker.open,
            "source-signal": self.connection_picker.source_signal,
            "action-kind": self.connection_picker.action_kind,
            "target-label": self.connection_picker.target_label,
        })
    }

    pub(crate) fn toasts_json(&self) -> Value {
        Value::Array(
            self.toasts
                .iter()
                .map(|t| {
                    json!({
                        "title": t.title,
                        "body": t.body,
                        "kind": t.kind.as_str(),
                    })
                })
                .collect(),
        )
    }

    pub(crate) fn results_json(&self) -> Value {
        Value::Array(
            self.command_palette
                .results
                .iter()
                .map(|r| {
                    let mut o = json!({ "id": r.id, "label": r.label });
                    if let Some(sc) = &r.shortcut {
                        o["shortcut"] = json!(sc);
                    }
                    o
                })
                .collect(),
        )
    }

    /// Fuzzy-filter a list of `(id, label)` command rows against the
    /// palette's current query. Returns the indices of `rows` that
    /// match, ordered best-score first. (§25 — single matcher, single
    /// caller. No service rebuilds the command list; no service
    /// reimplements the matcher.)
    pub fn filter_commands(&self, rows: &[(&str, &str)]) -> Vec<usize> {
        let q = self.command_palette.query_text().to_lowercase();
        if q.is_empty() {
            return (0..rows.len()).collect();
        }
        let mut scored: Vec<(usize, i32)> = rows
            .iter()
            .enumerate()
            .filter_map(|(i, (id, label))| {
                let id_lc = id.to_lowercase();
                let lab_lc = label.to_lowercase();
                let score = if lab_lc.contains(&q) {
                    100 - lab_lc.find(&q).unwrap_or(0) as i32
                } else if id_lc.contains(&q) {
                    50 - id_lc.find(&q).unwrap_or(0) as i32
                } else {
                    return None;
                };
                Some((i, score))
            })
            .collect();
        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(i, _)| i).collect()
    }
}
