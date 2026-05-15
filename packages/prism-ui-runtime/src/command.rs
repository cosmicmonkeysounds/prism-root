//! Render-command stream — backend-neutral output of the Clay layout
//! pass. Every backend (femtovg / web / html) consumes this same
//! `Vec<RenderCommand>` and lowers it to its target surface.
//!
//! The command set intentionally mirrors HTML+CSS box-model
//! primitives so the HTML lowering is a near-direct serialisation.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct CornerRadius {
    pub tl: f32,
    pub tr: f32,
    pub br: f32,
    pub bl: f32,
}

/// Byte range within a `Text` command's `content` that the renderer
/// should highlight. Carried on the command so a single text leaf can
/// paint its own selection without needing a separate `Rectangle`
/// command per glyph row (which would force the layout pass to know
/// about per-glyph metrics).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub start_byte: usize,
    pub end_byte: usize,
}

/// Per-byte-range colour override carried alongside the base text
/// colour. Used for syntax highlighting: the editor's tokenizer
/// emits one of these per token, and the paint pass picks the
/// matching span's colour for each glyph (falling back to the
/// command's base `color` outside any span).
///
/// Spans are interpreted as half-open `[start_byte, end_byte)` and
/// must be **non-overlapping**; the paint pass picks the *first*
/// span whose range covers a given byte, so authoring overlapping
/// ranges is non-deterministic. The author is responsible for
/// sort-and-merge before emission — Prism's `editor::syntax` module
/// does this.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TextSpan {
    pub start_byte: usize,
    pub end_byte: usize,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RenderCommand {
    Rectangle {
        bounds: Rect,
        color: Color,
        radius: CornerRadius,
    },
    Border {
        bounds: Rect,
        color: Color,
        width: f32,
        radius: CornerRadius,
    },
    Text {
        bounds: Rect,
        content: String,
        color: Color,
        font_size: f32,
        /// When set, the renderer paints a 1.5-px caret bar at the
        /// byte offset given by `caret_byte` (or, when that's `None`,
        /// at the shaped-text end). Carries the colour the caret
        /// should take. Used by focused `TextInput` leaves so users
        /// see where the next keystroke will land. SSR backends
        /// ignore this field — caret rendering is a native-only
        /// concern.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caret: Option<Color>,
        /// Byte offset within `content` where the caret should sit.
        /// `None` falls back to the shaped-text end (back-compat
        /// with the original single-line "caret-at-end" behaviour).
        /// `Some(0)` means "before the first glyph"; `Some(content.len())`
        /// means "after the last".
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caret_byte: Option<usize>,
        /// Active text selection as a half-open byte range
        /// `start..end` within `content`, with `start < end`. The
        /// renderer paints a translucent highlight rectangle per
        /// laid-out glyph row that the range covers. `None` (or
        /// `start == end`) paints no highlight.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection: Option<TextSelection>,
        /// Colour of the selection highlight. Required when
        /// `selection` is `Some`; ignored otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        selection_color: Option<Color>,
        /// Per-byte-range colour overrides (syntax highlighting). The
        /// paint pass picks each glyph's colour by finding the span
        /// whose `[start, end)` range covers the glyph's byte
        /// offset; glyphs not covered by any span paint in the
        /// command's base `color`. Empty / absent → uniform colour.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        spans: Vec<TextSpan>,
        /// Byte range to underline. The paint pass draws a thin bar
        /// under every glyph whose byte range overlaps `[start, end)`.
        /// Used for IME preedit decoration — the in-progress
        /// composition reads as "tentative" without overloading the
        /// selection highlight channel. `None` paints no underline.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        underline: Option<TextSelection>,
        /// Underline colour. Required when `underline` is `Some`;
        /// ignored otherwise.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        underline_color: Option<Color>,
    },
    Image {
        bounds: Rect,
        source: String,
        #[serde(default)]
        radius: CornerRadius,
        /// Optional colour tint. When `Some`, the renderer treats the
        /// `source` as a mask and paints `tint` through it (the
        /// canonical icon-tinting pattern). When `None`, the image is
        /// painted as-is.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tint: Option<Color>,
    },
    ScissorStart {
        bounds: Rect,
    },
    ScissorEnd,
    /// Pass-through for arbitrary backend hints (CSS classes, ARIA
    /// attributes, data attributes). Native backends ignore these;
    /// the HTML backend emits them on the surrounding element.
    Hint {
        key: String,
        value: String,
    },
}
