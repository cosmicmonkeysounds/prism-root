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
        /// shaped-text end position (i.e. *after* cosmic-text laid
        /// out the glyphs). Carries the colour the caret should
        /// take. Used by focused `TextInput` leaves so users see
        /// where the next keystroke will land. SSR backends ignore
        /// this field — caret rendering is a native-only concern.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caret: Option<Color>,
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
