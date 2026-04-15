//! Surface type primitives for the unified [`LanguageRegistry`].
//!
//! Port of `language/registry/surface-types.ts`. ADR-002 §A2 collapsed
//! the parser registry and the surface registry into a single
//! `LanguageContribution` record, so the primitive building blocks
//! live next to the registry rather than the syntax engine.

use indexmap::IndexMap;
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;

// ── Surface Mode ───────────────────────────────────────────────────

/// Editing modes available in the document surface. Prism uses
/// CodeMirror 6 exclusively — no richtext / TipTap mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceMode {
    /// CodeMirror raw syntax editing.
    Code,
    /// CodeMirror live-preview (rendered on inactive lines).
    Preview,
    /// Schema-driven field inputs.
    Form,
    /// Grid editing for tabular data.
    Spreadsheet,
    /// Full HTML layout engine.
    Report,
}

// ── Inline Token Definition ────────────────────────────────────────

/// Result of running an [`InlineTokenDef::extract`] callback against
/// a regex match.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InlineTokenExtract {
    pub display: String,
    pub data: IndexMap<String, JsonValue>,
}

/// Callback type for `InlineTokenDef::extract`. Boxed so the def is
/// movable across the registry API.
pub type InlineTokenExtractFn =
    Arc<dyn Fn(&regex::Captures<'_>) -> InlineTokenExtract + Send + Sync>;

/// Pattern-matched inline token that renders identically across all
/// surface modes (code marks, preview chips, form chips). Languages
/// contribute these via `LanguageSurface::inline_tokens`.
#[derive(Clone)]
pub struct InlineTokenDef {
    /// Unique token id: `"wikilink"`, `"operand"`, `"resolve-ref"`.
    pub id: String,
    /// Compiled pattern. Must be a global regex (used with
    /// `captures_iter`); capture groups feed `extract`.
    pub pattern: Regex,
    /// Extract display text and structured data from a regex match.
    pub extract: InlineTokenExtractFn,
    /// CSS class applied in code-mode surfaces (CodeMirror mark
    /// decoration in the TS tree; Slint class lookup in the Rust tree).
    pub css_class: Option<String>,
    /// Semantic color hint for chip renderers. Palette matches the
    /// legacy TS palette: `"teal"`, `"amber"`, `"violet"`, `"emerald"`,
    /// `"rose"`, `"blue"`, `"zinc"`.
    pub chip_color: Option<String>,
    /// In preview modes, replace raw syntax with a chip widget.
    pub replace_in_preview: bool,
}

impl std::fmt::Debug for InlineTokenDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InlineTokenDef")
            .field("id", &self.id)
            .field("pattern", &self.pattern.as_str())
            .field("css_class", &self.css_class)
            .field("chip_color", &self.chip_color)
            .field("replace_in_preview", &self.replace_in_preview)
            .finish_non_exhaustive()
    }
}

// ── Inline Token Builder ───────────────────────────────────────────

/// Fluent builder for concise [`InlineTokenDef`] creation.
pub struct InlineTokenBuilder {
    id: String,
    pattern: Regex,
    extract: Option<InlineTokenExtractFn>,
    css_class: Option<String>,
    chip_color: Option<String>,
    replace_in_preview: bool,
}

impl InlineTokenBuilder {
    pub fn new(id: impl Into<String>, pattern: Regex) -> Self {
        Self {
            id: id.into(),
            pattern,
            extract: None,
            css_class: None,
            chip_color: None,
            replace_in_preview: false,
        }
    }

    pub fn extract<F>(mut self, f: F) -> Self
    where
        F: Fn(&regex::Captures<'_>) -> InlineTokenExtract + Send + Sync + 'static,
    {
        self.extract = Some(Arc::new(f));
        self
    }

    pub fn css(mut self, class_name: impl Into<String>) -> Self {
        self.css_class = Some(class_name.into());
        self
    }

    pub fn chip(mut self, color: impl Into<String>) -> Self {
        self.chip_color = Some(color.into());
        self
    }

    pub fn replace_in_preview(mut self, replace: bool) -> Self {
        self.replace_in_preview = replace;
        self
    }

    pub fn build(self) -> Result<InlineTokenDef, InlineTokenBuildError> {
        let extract = self
            .extract
            .ok_or_else(|| InlineTokenBuildError::MissingExtract(self.id.clone()))?;
        Ok(InlineTokenDef {
            id: self.id,
            pattern: self.pattern,
            extract,
            css_class: self.css_class,
            chip_color: self.chip_color,
            replace_in_preview: self.replace_in_preview,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InlineTokenBuildError {
    #[error("InlineTokenBuilder({0}): extract is required")]
    MissingExtract(String),
}

/// Convenience constructor mirroring the TS `inlineToken(id, pattern)`
/// factory.
pub fn inline_token(id: impl Into<String>, pattern: Regex) -> InlineTokenBuilder {
    InlineTokenBuilder::new(id, pattern)
}

// ── Built-in inline tokens ─────────────────────────────────────────

/// Build the wiki-link inline token: `[[id|display]]` or `[[id]]`.
///
/// Constructed eagerly because `regex::Regex` compilation can fail
/// at runtime; exposing a constructor rather than a `static` keeps
/// the failure mode explicit. Callers typically call this once at
/// boot and cache the result on the registry.
pub fn wikilink_token() -> InlineTokenDef {
    let pattern =
        Regex::new(r"\[\[([^\]\|]+?)(?:\|([^\]]+?))?\]\]").expect("wikilink regex is valid");
    inline_token("wikilink", pattern)
        .extract(|caps| {
            let raw = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            let display_override = caps.get(2).map(|m| m.as_str());
            let display = display_override
                .map(str::to_owned)
                .unwrap_or_else(|| raw.rsplit(':').next().unwrap_or(raw).to_owned());
            let mut data = IndexMap::new();
            data.insert("raw".to_owned(), JsonValue::String(raw.to_owned()));
            data.insert(
                "display".to_owned(),
                JsonValue::String(display_override.unwrap_or("").to_owned()),
            );
            InlineTokenExtract { display, data }
        })
        .css("pt-token-wikilink")
        .chip("teal")
        .replace_in_preview(true)
        .build()
        .expect("wikilink token has extract")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wikilink_extracts_display_override() {
        let tok = wikilink_token();
        let caps = tok.pattern.captures("[[page-id|Pretty Name]]").unwrap();
        let extracted = (tok.extract)(&caps);
        assert_eq!(extracted.display, "Pretty Name");
        assert_eq!(extracted.data["raw"], JsonValue::String("page-id".into()));
    }

    #[test]
    fn wikilink_falls_back_to_last_segment() {
        let tok = wikilink_token();
        let caps = tok.pattern.captures("[[ns:page-id]]").unwrap();
        let extracted = (tok.extract)(&caps);
        assert_eq!(extracted.display, "page-id");
    }

    #[test]
    fn builder_rejects_missing_extract() {
        let pattern = Regex::new(r"#(\w+)").unwrap();
        let err = InlineTokenBuilder::new("tag", pattern).build().unwrap_err();
        assert!(matches!(err, InlineTokenBuildError::MissingExtract(_)));
    }

    #[test]
    fn surface_mode_serializes_lowercase() {
        let s = serde_json::to_string(&SurfaceMode::Spreadsheet).unwrap();
        assert_eq!(s, "\"spreadsheet\"");
    }
}
