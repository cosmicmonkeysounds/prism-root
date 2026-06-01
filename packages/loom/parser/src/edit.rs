//! Span-preserving, minimal-diff structural edits over `.loom` source.
//!
//! The parser keeps byte-accurate [`Span`](crate::Span)s on every AST
//! node, and the original source string is the single source of truth:
//! the comment pre-pass blanks comments to spaces rather than dropping
//! them (see [`comments`](crate::comments)), so every byte offset still
//! maps to the real text. That lets us perform structural edits —
//! reorder a beat, change a beat's `cast:` line — as pure byte-range
//! splices that leave every untouched line byte-identical, so the
//! editor's collaborative Loro merges stay clean (the IDE redesign's
//! "edits round-trip to source" invariant, docs/dev/loom-ide-redesign.md
//! §10/§13).
//!
//! Edits are returned as [`TextEdit`]s (independent byte-range splices)
//! rather than a re-rendered file; [`apply_edits`] applies a batch.
//! This is the foundation the Editing-facet timeline drives; further
//! operations (insert/remove beat, reorder body items, retime clock
//! gates) layer on the same primitive.

use serde::{Deserialize, Serialize};

use crate::ast::{Beat, Item, LoomFile};
use crate::source::Span;

/// A single byte-range splice into the original source. `start..end`
/// is a half-open UTF-8 byte range into the source the edit was
/// computed against; `replacement` is the text to put there. A
/// zero-length range (`start == end`) is a pure insertion.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEdit {
    pub start: usize,
    pub end: usize,
    pub replacement: String,
}

/// Where to drop a beat when moving it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Anchor {
    /// Immediately before the beat named `.0`.
    Before(String),
    /// Immediately after the beat named `.0`.
    After(String),
    /// As the first top-level item (just after the header).
    Start,
    /// As the last top-level item.
    End,
}

/// Why a structural edit could not be produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditError {
    /// No beat with the requested name exists in the file.
    BeatNotFound(String),
    /// The move anchor referenced a beat that does not exist.
    AnchorNotFound(String),
    /// The edits handed to [`apply_edits`] overlap and cannot be applied.
    OverlappingEdits,
}

impl core::fmt::Display for EditError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            EditError::BeatNotFound(n) => write!(f, "beat not found: {n}"),
            EditError::AnchorNotFound(n) => write!(f, "anchor beat not found: {n}"),
            EditError::OverlappingEdits => write!(f, "overlapping text edits"),
        }
    }
}

impl std::error::Error for EditError {}

/// Apply a batch of non-overlapping [`TextEdit`]s to `source`, yielding
/// the new text. Edits are applied left-to-right over the original
/// offsets, so callers pass offsets relative to `source` (not to the
/// partially-edited result).
pub fn apply_edits(source: &str, edits: &[TextEdit]) -> Result<String, EditError> {
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by_key(|e| (e.start, e.end));

    let mut prev_end = 0usize;
    for e in &sorted {
        if e.start < prev_end {
            return Err(EditError::OverlappingEdits);
        }
        prev_end = e.end.max(e.start);
    }

    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    for e in &sorted {
        out.push_str(&source[cursor..e.start]);
        out.push_str(&e.replacement);
        cursor = e.end;
    }
    out.push_str(&source[cursor..]);
    Ok(out)
}

// ---------------------------------------------------------------------------
// Operations
// ---------------------------------------------------------------------------

/// Set a beat's contract property (`cast:`, `setting:`, …) to `value`.
///
/// If the property already exists, only its line is rewritten (the
/// indentation and key are preserved; every other byte is untouched).
/// If absent, a new contract line is inserted after the last existing
/// contract property, or directly under the `==` opener when the
/// contract is empty. Returns an empty edit list when the value is
/// already what was requested.
pub fn set_beat_property(
    source: &str,
    file: &LoomFile,
    beat: &str,
    key: &str,
    value: &str,
) -> Result<Vec<TextEdit>, EditError> {
    let b = beat_at(file, beat).ok_or_else(|| EditError::BeatNotFound(beat.to_string()))?;

    if let Some(pv) = b.contract.get(key) {
        let (ls, le) = line_bounds(source, pv.span.start.byte as usize);
        let indent = leading_ws(&source[ls..le]);
        let replacement = format!("{indent}{key}: {value}");
        if source.get(ls..le) == Some(replacement.as_str()) {
            return Ok(vec![]); // no-op
        }
        return Ok(vec![TextEdit { start: ls, end: le, replacement }]);
    }

    // Property absent — insert a new contract line.
    let (anchor_eol, indent) = match b.contract.iter().last() {
        Some((_, last)) => {
            let (ls, le) = line_bounds(source, last.span.start.byte as usize);
            (le, leading_ws(&source[ls..le]).to_string())
        }
        None => {
            let (_, le) = line_bounds(source, b.span.start.byte as usize);
            (le, "  ".to_string())
        }
    };

    let (start, replacement) = if anchor_eol < source.len() {
        // `anchor_eol` points at the line's trailing '\n'; insert a full
        // new line at the start of the following line.
        (anchor_eol + 1, format!("{indent}{key}: {value}\n"))
    } else {
        // Anchor line is the last line in the file (no trailing newline).
        (source.len(), format!("\n{indent}{key}: {value}"))
    };
    Ok(vec![TextEdit { start, end: start, replacement }])
}

/// Move the beat named `beat` to the position described by `anchor`,
/// rewriting the source as a delete + insert pair. Every other item's
/// bytes are preserved verbatim; only the separators at the removal and
/// insertion seams are normalised to a single blank line. Returns an
/// empty edit list when the move is a no-op.
pub fn move_beat(
    source: &str,
    file: &LoomFile,
    beat: &str,
    anchor: Anchor,
) -> Result<Vec<TextEdit>, EditError> {
    let i = beat_index(file, beat).ok_or_else(|| EditError::BeatNotFound(beat.to_string()))?;
    let n = file.items.len();

    let target = match &anchor {
        Anchor::Start => 0,
        Anchor::End => n,
        Anchor::Before(name) => {
            beat_index(file, name).ok_or_else(|| EditError::AnchorNotFound(name.clone()))?
        }
        Anchor::After(name) => {
            beat_index(file, name).ok_or_else(|| EditError::AnchorNotFound(name.clone()))? + 1
        }
    };

    // Moving a beat to where it already is changes nothing.
    if target == i || target == i + 1 {
        return Ok(vec![]);
    }

    let blocks = item_blocks(source, file);
    let (bs, be) = blocks[i];
    let content = source[bs..be].trim_end();
    let moved = format!("{content}\n\n");

    let del = TextEdit { start: bs, end: be, replacement: String::new() };

    let ins = if target < n { blocks[target].0 } else { source.len() };
    let preceding = &source[..ins];
    let trailing_newlines = preceding.bytes().rev().take_while(|&c| c == b'\n').count();
    let mut replacement = String::new();
    if !preceding.is_empty() {
        // Guarantee a blank line before the inserted beat.
        replacement.push_str(&"\n".repeat(2usize.saturating_sub(trailing_newlines)));
    }
    replacement.push_str(&moved);
    let insert = TextEdit { start: ins, end: ins, replacement };

    Ok(vec![del, insert])
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn beat_index(file: &LoomFile, name: &str) -> Option<usize> {
    file.items
        .iter()
        .position(|it| matches!(it, Item::Beat(b) if b.name == name))
}

fn beat_at<'a>(file: &'a LoomFile, name: &str) -> Option<&'a Beat> {
    file.items.iter().find_map(|it| match it {
        Item::Beat(b) if b.name == name => Some(b),
        _ => None,
    })
}

fn item_span(item: &Item) -> Span {
    match item {
        Item::Declaration(d) => d.span,
        Item::LetBinding(l) => l.span,
        Item::Beat(b) => b.span,
    }
}

/// Byte offset of the physical line that contains `byte`, snapped back
/// to the start of that line.
fn item_start(source: &str, item: &Item) -> usize {
    line_bounds(source, item_span(item).start.byte as usize).0
}

/// Contiguous `[start, end)` byte ranges, one per top-level item, that
/// partition the source from the first item to EOF. Each block carries
/// the trailing separator up to the next item.
fn item_blocks(source: &str, file: &LoomFile) -> Vec<(usize, usize)> {
    let starts: Vec<usize> = file.items.iter().map(|it| item_start(source, it)).collect();
    let n = starts.len();
    (0..n)
        .map(|k| (starts[k], if k + 1 < n { starts[k + 1] } else { source.len() }))
        .collect()
}

/// `(line_start, line_end)` byte range of the physical line containing
/// `byte`, where `line_end` excludes the trailing '\n'.
fn line_bounds(source: &str, byte: usize) -> (usize, usize) {
    let bytes = source.as_bytes();
    let clamped = byte.min(source.len());
    let mut start = clamped;
    while start > 0 && bytes[start - 1] != b'\n' {
        start -= 1;
    }
    let mut end = clamped;
    while end < source.len() && bytes[end] != b'\n' {
        end += 1;
    }
    (start, end)
}

/// Leading run of spaces/tabs on a single line (no trailing '\n').
fn leading_ws(line: &str) -> &str {
    let end = line.find(|c: char| c != ' ' && c != '\t').unwrap_or(line.len());
    &line[..end]
}
