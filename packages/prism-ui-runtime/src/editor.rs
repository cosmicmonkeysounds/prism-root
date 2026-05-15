//! The Prism text editor engine.
//!
//! One state machine, two consumers: inline string-property fields
//! (single-line, kind `text` / `color` / `file`) and the multi-line
//! code editor (kind `code` / `textarea`). Both share the same buffer,
//! caret, selection, command surface, and undo history — the only
//! difference is whether `\n` is a permissible character.
//!
//! The editor is *pure* — no I/O, no rendering. Hosts wrap it in a
//! focus session, route keyboard / pointer events through
//! [`TextEditor::apply_key`] + [`TextEditor::apply_text`] +
//! [`TextEditor::place_caret_at`], and read back `text()` /
//! `caret_byte()` / `selection()` to drive the next render.
//!
//! ## Buffer model
//!
//! Plain `String` storage. The legacy Slint editor used `ropey`; for
//! the Prism range (config / script files up to a few thousand lines)
//! the constant factors aren't worth the extra dependency yet — a
//! `String::insert_str` over a 100KB buffer is microseconds.
//! Promoting to a rope when a real workload demands it is one
//! field-swap.
//!
//! ## Indexing
//!
//! Carets and selection anchors are **byte indices** into the buffer.
//! Cosmic-text's per-glyph `start..end` byte ranges match this, so
//! pixel ↔ byte conversion in the paint pass is a glyph walk with no
//! UTF-16 detour. Every editor command validates the index lands on
//! a `char` boundary before mutating, so external callers (a pixel
//! hit-test that resolves to a byte) cannot corrupt the buffer.

use serde::{Deserialize, Serialize};

use crate::event::Modifiers;

/// The core editor state. `Clone` cheap (a `String` + a few
/// `usize`s), `Default`-constructible to the empty document.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextEditor {
    text: String,
    /// Byte index of the caret. Always on a char boundary. `0` means
    /// before the first char; `text.len()` means after the last.
    caret: usize,
    /// When `Some`, an anchor for an active selection. The selection
    /// is the byte range `min(anchor, caret) .. max(anchor, caret)`.
    /// `None` means the caret has no selection.
    anchor: Option<usize>,
    /// Sticky preferred column for vertical movement. Up/Down arrow
    /// remembers the column the user navigated *from* so a short line
    /// in the middle of a longer-line block doesn't permanently
    /// shorten the caret's home column. Reset by any horizontal move
    /// or edit.
    preferred_col: Option<usize>,
    /// `false` for single-line fields (string property rows). When
    /// false, `\n` is silently stripped on insert and Up/Down arrows
    /// fall back to Home/End behaviour. Set at construction by the
    /// host that owns the focus session.
    multiline: bool,
    /// Active IME preedit composition — non-empty while the user is
    /// in the middle of typing a multi-keystroke character (Japanese
    /// kana, Korean jamo, dead-key accents). When non-empty, the
    /// host paints this string *at the caret* with an underline; the
    /// real buffer / caret stay untouched until commit. On commit
    /// (`apply_ime_commit`), preedit is cleared and the committed
    /// text inserts at the caret as a normal edit.
    preedit: String,
    /// OS-reported caret position *within the preedit string*, in
    /// byte offsets. `None` means the OS didn't report one; the
    /// renderer falls back to the preedit's end.
    preedit_cursor: Option<usize>,
    history: History,
}

/// One snapshot in the undo stack. Captures the full buffer plus
/// caret position; replay is `set_text(...); caret = ...`. A more
/// space-efficient encoding (insert / delete deltas) is possible but
/// the simpler model is fast enough for the typical buffer sizes the
/// shell actually edits.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Snapshot {
    text: String,
    caret: usize,
    anchor: Option<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    /// Coalescing group key for the *next* mutation. Two consecutive
    /// mutations with the same group key share one undo snapshot —
    /// typing a word doesn't push 5 separate undo entries.
    last_group: Option<EditGroup>,
}

/// Coalescing key. Consecutive edits with the same key share an undo
/// entry; a different key (or `None`, set by selection / nav moves)
/// finalises the current group.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum EditGroup {
    /// Plain typing — printable characters, accumulated together
    /// across a contiguous run.
    Typing,
    /// Backspace pressed repeatedly — accumulated together.
    Backspace,
    /// Forward delete pressed repeatedly — accumulated together.
    Delete,
    /// Anything else (newlines, paste, indent, line ops) — one snapshot
    /// per call, no coalescing.
    Atomic,
}

/// Outcome of an `apply_*` call. `Mutated` means buffer or caret
/// changed and the host should re-render; `Inert` means nothing
/// changed (e.g. an arrow key that already had the caret at the
/// start of the buffer).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditOutcome {
    Mutated,
    Inert,
}

impl EditOutcome {
    pub fn mutated(self) -> bool {
        matches!(self, EditOutcome::Mutated)
    }
}

impl TextEditor {
    /// New empty single-line editor.
    pub fn new_single_line() -> Self {
        Self::default()
    }

    /// New empty multi-line editor.
    pub fn new_multi_line() -> Self {
        Self {
            multiline: true,
            ..Self::default()
        }
    }

    /// Seed from an existing string. Caret lands at the end so an
    /// inline field-edit session feels "continue typing where the
    /// value left off" instead of "select-all and replace".
    pub fn with_text(text: impl Into<String>) -> Self {
        let text = text.into();
        let caret = text.len();
        Self {
            text,
            caret,
            ..Self::default()
        }
    }

    pub fn multiline(mut self, on: bool) -> Self {
        self.multiline = on;
        self
    }

    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn caret_byte(&self) -> usize {
        self.caret
    }

    /// Active selection as `(start_byte, end_byte)` with `start < end`,
    /// or `None` when no selection is active.
    pub fn selection(&self) -> Option<(usize, usize)> {
        let a = self.anchor?;
        if a == self.caret {
            return None;
        }
        Some((a.min(self.caret), a.max(self.caret)))
    }

    pub fn has_selection(&self) -> bool {
        self.selection().is_some()
    }

    /// Replace the buffer entirely, e.g. on a fresh field-focus
    /// session. Clears selection, places caret at the end, resets
    /// undo history (the new value is the new baseline).
    pub fn set_text(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.caret = text.len();
        self.text = text;
        self.anchor = None;
        self.preferred_col = None;
        self.history = History::default();
    }

    /// Reset the buffer + caret without touching undo. Used by
    /// "cancel field-focus" (Esc) where we want to restore the pre-
    /// session value but keep no history at all (the session ended).
    pub fn reset_to(&mut self, text: impl Into<String>) {
        self.set_text(text);
    }

    /// Replace the contents of an existing field-focus session
    /// *without* losing the undo history. Used when an external
    /// mutation rewrites the bound prop (e.g. another tab edited the
    /// same value) but the user still has the field open.
    pub fn rebase(&mut self, text: impl Into<String>) {
        let text = text.into();
        let caret = self.caret.min(text.len());
        self.text = text;
        self.caret = clamp_to_char_boundary(&self.text, caret);
        self.anchor = None;
        self.preferred_col = None;
        self.history.last_group = None;
    }

    /// Position the caret at byte `idx`. Snaps to the nearest char
    /// boundary; clears the selection unless `extend` is true (in
    /// which case the existing caret becomes the anchor for a new
    /// selection). Used by pointer hits, palette nav, etc.
    pub fn place_caret_at(&mut self, idx: usize, extend: bool) {
        let idx = clamp_to_char_boundary(&self.text, idx);
        if extend {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
        self.caret = idx;
        self.preferred_col = None;
        self.history.last_group = None;
    }

    /// Select the entire buffer. Caret lands at end so a subsequent
    /// arrow / type behaves like users expect.
    pub fn select_all(&mut self) {
        if self.text.is_empty() {
            self.anchor = None;
            return;
        }
        self.anchor = Some(0);
        self.caret = self.text.len();
        self.preferred_col = None;
        self.history.last_group = None;
    }

    pub fn clear_selection(&mut self) {
        self.anchor = None;
    }

    /// Insert `text` at the caret, replacing any active selection.
    /// `\n` is stripped when the editor is single-line. Coalesces
    /// into the typing-run undo group unless `atomic`, which forces
    /// a snapshot (used by paste so it's one undoable unit).
    pub fn insert(&mut self, text: &str, atomic: bool) -> EditOutcome {
        let cleaned: String = if self.multiline {
            text.to_string()
        } else {
            text.chars().filter(|c| *c != '\n' && *c != '\r').collect()
        };
        if cleaned.is_empty() && !self.has_selection() {
            return EditOutcome::Inert;
        }
        let group = if atomic {
            EditGroup::Atomic
        } else {
            EditGroup::Typing
        };
        self.push_undo(group);
        if let Some((s, e)) = self.selection() {
            self.text.replace_range(s..e, &cleaned);
            self.caret = s + cleaned.len();
            self.anchor = None;
        } else {
            self.text.insert_str(self.caret, &cleaned);
            self.caret += cleaned.len();
        }
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Backspace: when a selection is active, delete it. Otherwise
    /// remove the char before the caret. Coalesces consecutive
    /// backspaces.
    pub fn delete_backward(&mut self) -> EditOutcome {
        if self.has_selection() {
            return self.delete_selection(EditGroup::Atomic);
        }
        if self.caret == 0 {
            return EditOutcome::Inert;
        }
        let prev = prev_char_boundary(&self.text, self.caret);
        self.push_undo(EditGroup::Backspace);
        self.text.replace_range(prev..self.caret, "");
        self.caret = prev;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Forward-delete: when a selection is active, delete it.
    /// Otherwise remove the char after the caret.
    pub fn delete_forward(&mut self) -> EditOutcome {
        if self.has_selection() {
            return self.delete_selection(EditGroup::Atomic);
        }
        if self.caret >= self.text.len() {
            return EditOutcome::Inert;
        }
        let next = next_char_boundary(&self.text, self.caret);
        self.push_undo(EditGroup::Delete);
        self.text.replace_range(self.caret..next, "");
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Word-wise backspace (Ctrl/Alt+Backspace). Deletes from the
    /// caret back to the start of the previous word.
    pub fn delete_word_backward(&mut self) -> EditOutcome {
        if self.has_selection() {
            return self.delete_selection(EditGroup::Atomic);
        }
        if self.caret == 0 {
            return EditOutcome::Inert;
        }
        let start = prev_word_boundary(&self.text, self.caret);
        self.push_undo(EditGroup::Atomic);
        self.text.replace_range(start..self.caret, "");
        self.caret = start;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    pub fn delete_word_forward(&mut self) -> EditOutcome {
        if self.has_selection() {
            return self.delete_selection(EditGroup::Atomic);
        }
        if self.caret >= self.text.len() {
            return EditOutcome::Inert;
        }
        let end = next_word_boundary(&self.text, self.caret);
        self.push_undo(EditGroup::Atomic);
        self.text.replace_range(self.caret..end, "");
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Delete the entire current line (Ctrl+K). If the line ends in
    /// a `\n`, the newline is consumed too so the next line moves
    /// up. Selection (if any) is collapsed first.
    pub fn delete_line(&mut self) -> EditOutcome {
        if !self.multiline {
            // single-line: just clear the buffer
            if self.text.is_empty() {
                return EditOutcome::Inert;
            }
            self.push_undo(EditGroup::Atomic);
            self.text.clear();
            self.caret = 0;
            self.anchor = None;
            return EditOutcome::Mutated;
        }
        let (line_start, line_end_incl) = current_line_range(&self.text, self.caret);
        if line_start == line_end_incl {
            return EditOutcome::Inert;
        }
        self.push_undo(EditGroup::Atomic);
        self.text.replace_range(line_start..line_end_incl, "");
        self.caret = line_start.min(self.text.len());
        self.anchor = None;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Duplicate the current line below the current line (Ctrl+D).
    /// Caret stays on the original line so the visual effect is
    /// "made a copy underneath".
    pub fn duplicate_line(&mut self) -> EditOutcome {
        if !self.multiline {
            return EditOutcome::Inert;
        }
        let (line_start, line_end_excl) = current_line_range_exclusive(&self.text, self.caret);
        if line_start == line_end_excl && self.text.is_empty() {
            return EditOutcome::Inert;
        }
        self.push_undo(EditGroup::Atomic);
        let line = self.text[line_start..line_end_excl].to_string();
        let insert_at = line_end_excl;
        let payload = format!("\n{line}");
        self.text.insert_str(insert_at, &payload);
        EditOutcome::Mutated
    }

    /// Move the caret one char left. Shift extends selection.
    pub fn move_left(&mut self, shift: bool) -> EditOutcome {
        self.preferred_col = None;
        if !shift {
            if let Some((s, _)) = self.selection() {
                self.caret = s;
                self.anchor = None;
                self.history.last_group = None;
                return EditOutcome::Mutated;
            }
        }
        if self.caret == 0 {
            return EditOutcome::Inert;
        }
        let prev = prev_char_boundary(&self.text, self.caret);
        self.update_anchor_for_extend(shift);
        self.caret = prev;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_right(&mut self, shift: bool) -> EditOutcome {
        self.preferred_col = None;
        if !shift {
            if let Some((_, e)) = self.selection() {
                self.caret = e;
                self.anchor = None;
                self.history.last_group = None;
                return EditOutcome::Mutated;
            }
        }
        if self.caret >= self.text.len() {
            return EditOutcome::Inert;
        }
        let next = next_char_boundary(&self.text, self.caret);
        self.update_anchor_for_extend(shift);
        self.caret = next;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_word_left(&mut self, shift: bool) -> EditOutcome {
        if self.caret == 0 {
            return EditOutcome::Inert;
        }
        let target = prev_word_boundary(&self.text, self.caret);
        self.update_anchor_for_extend(shift);
        self.caret = target;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_word_right(&mut self, shift: bool) -> EditOutcome {
        if self.caret >= self.text.len() {
            return EditOutcome::Inert;
        }
        let target = next_word_boundary(&self.text, self.caret);
        self.update_anchor_for_extend(shift);
        self.caret = target;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_line_start(&mut self, shift: bool) -> EditOutcome {
        let (line_start, _) = current_line_range_exclusive(&self.text, self.caret);
        if line_start == self.caret && !shift {
            // already there
            return EditOutcome::Inert;
        }
        self.update_anchor_for_extend(shift);
        self.caret = line_start;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_line_end(&mut self, shift: bool) -> EditOutcome {
        let (_, line_end) = current_line_range_exclusive(&self.text, self.caret);
        if line_end == self.caret && !shift {
            return EditOutcome::Inert;
        }
        self.update_anchor_for_extend(shift);
        self.caret = line_end;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_doc_start(&mut self, shift: bool) -> EditOutcome {
        if self.caret == 0 && !shift {
            return EditOutcome::Inert;
        }
        self.update_anchor_for_extend(shift);
        self.caret = 0;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_doc_end(&mut self, shift: bool) -> EditOutcome {
        let end = self.text.len();
        if self.caret == end && !shift {
            return EditOutcome::Inert;
        }
        self.update_anchor_for_extend(shift);
        self.caret = end;
        self.preferred_col = None;
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    /// Page-up / page-down — move the caret by `lines` rows while
    /// preserving the sticky preferred column. Single-line editors
    /// collapse to a doc-start / doc-end jump. Multi-line editors
    /// walk through the buffer's newlines, snapping to the same
    /// column on the destination row (clamping to its length).
    /// Shift extends the selection from the existing anchor.
    pub fn page_move(&mut self, lines: i32, shift: bool) -> EditOutcome {
        if !self.multiline || lines == 0 {
            if lines > 0 {
                return self.move_doc_end(shift);
            } else if lines < 0 {
                return self.move_doc_start(shift);
            } else {
                return EditOutcome::Inert;
            }
        }
        let col = self.preferred_col.unwrap_or(self.current_column());
        let (cur_line, _) = self.caret_line_col();
        // `caret_line_col` is 1-based; convert to 0-based for math.
        let target_line_0 = (cur_line as i32 - 1 + lines).max(0) as usize;
        let last_line_0 = self.line_count().saturating_sub(1);
        let target_line_0 = target_line_0.min(last_line_0);
        let target_line_start = if target_line_0 == 0 {
            0
        } else {
            let mut count = 0usize;
            let mut start = 0usize;
            for (i, b) in self.text.bytes().enumerate() {
                if b == b'\n' {
                    count += 1;
                    if count == target_line_0 {
                        start = i + 1;
                        break;
                    }
                }
            }
            start
        };
        let new_caret = clamp_column(&self.text, target_line_start, col);
        if new_caret == self.caret && !shift {
            return EditOutcome::Inert;
        }
        self.update_anchor_for_extend(shift);
        self.caret = new_caret;
        self.preferred_col = Some(col);
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    /// Up arrow — multi-line only. Falls back to `move_doc_start`
    /// on a single-line editor (matches the behaviour of most native
    /// text fields).
    pub fn move_up(&mut self, shift: bool) -> EditOutcome {
        if !self.multiline {
            return self.move_doc_start(shift);
        }
        let col = self.current_column();
        let (line_start, _) = current_line_range_exclusive(&self.text, self.caret);
        if line_start == 0 {
            return self.move_doc_start(shift);
        }
        let prev_line_end = line_start - 1;
        let (prev_line_start, _) = current_line_range_exclusive(&self.text, prev_line_end);
        let preferred = self.preferred_col.unwrap_or(col);
        let target = clamp_column(&self.text, prev_line_start, preferred);
        self.update_anchor_for_extend(shift);
        self.caret = target;
        self.preferred_col = Some(preferred);
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    pub fn move_down(&mut self, shift: bool) -> EditOutcome {
        if !self.multiline {
            return self.move_doc_end(shift);
        }
        let col = self.current_column();
        let (_, line_end) = current_line_range_exclusive(&self.text, self.caret);
        if line_end >= self.text.len() {
            return self.move_doc_end(shift);
        }
        // next line starts after the `\n`
        let next_line_start = line_end + 1;
        let preferred = self.preferred_col.unwrap_or(col);
        let target = clamp_column(&self.text, next_line_start, preferred);
        self.update_anchor_for_extend(shift);
        self.caret = target;
        self.preferred_col = Some(preferred);
        self.history.last_group = None;
        EditOutcome::Mutated
    }

    /// Insert a newline at the caret (or replace selection with `\n`).
    /// Auto-indents the new line to match the leading whitespace of
    /// the previous line. Single-line editors treat this as a no-op
    /// (the host should commit/blur instead — that's a host-policy
    /// call, not the editor's).
    pub fn insert_newline(&mut self) -> EditOutcome {
        if !self.multiline {
            return EditOutcome::Inert;
        }
        // Compute the indent from the line the caret sits on *before*
        // we touch the buffer. If a selection is active, `insert`'s
        // replace path puts the caret at `min(anchor, caret)`, but
        // we want the original line's indent — capture first.
        let indent = leading_whitespace_of_line(&self.text, self.caret);
        let to_insert = format!("\n{indent}");
        self.insert(&to_insert, true)
    }

    /// Insert a tab (or N spaces) at the caret. `tab_size` of `0`
    /// inserts a literal `\t`; otherwise inserts that many spaces.
    pub fn insert_tab(&mut self, tab_size: usize) -> EditOutcome {
        if self.multiline && self.has_selection() && self.selection_spans_lines() {
            return self.indent_selection(tab_size);
        }
        let payload = if tab_size == 0 {
            "\t".to_string()
        } else {
            " ".repeat(tab_size)
        };
        self.insert(&payload, true)
    }

    /// Shift-Tab — dedent. When a selection spans multiple lines,
    /// dedents each line; otherwise removes up to `tab_size` leading
    /// spaces (or one `\t`) from the current line.
    pub fn dedent(&mut self, tab_size: usize) -> EditOutcome {
        if !self.multiline {
            return EditOutcome::Inert;
        }
        if self.has_selection() && self.selection_spans_lines() {
            return self.dedent_selection(tab_size);
        }
        let (line_start, _) = current_line_range_exclusive(&self.text, self.caret);
        let bytes = self.text.as_bytes();
        let mut end = line_start;
        let mut removed = 0usize;
        let limit = if tab_size == 0 { 1 } else { tab_size };
        let mut tabbed_out = false;
        while end < bytes.len() && removed < limit && !tabbed_out {
            match bytes[end] {
                b'\t' => {
                    end += 1;
                    // A literal tab counts as a full dedent unit and
                    // also terminates the run — mixing tabs and spaces
                    // is too ambiguous to handle inside the same pass.
                    tabbed_out = true;
                }
                b' ' => {
                    end += 1;
                    removed += 1;
                }
                _ => break,
            }
        }
        if end == line_start {
            return EditOutcome::Inert;
        }
        self.push_undo(EditGroup::Atomic);
        let removed_bytes = end - line_start;
        self.text.replace_range(line_start..end, "");
        if self.caret >= end {
            self.caret -= removed_bytes;
        } else if self.caret > line_start {
            self.caret = line_start;
        }
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Indent every line in the current selection by `tab_size`
    /// spaces (or one `\t` when `tab_size == 0`).
    fn indent_selection(&mut self, tab_size: usize) -> EditOutcome {
        let (s, e) = self
            .selection()
            .expect("indent_selection called with no selection");
        self.push_undo(EditGroup::Atomic);
        let payload: String = if tab_size == 0 {
            "\t".into()
        } else {
            " ".repeat(tab_size)
        };
        let mut line_starts = Vec::new();
        line_starts.push(line_start_of(&self.text, s));
        let mut cur = line_starts[0];
        while cur < e {
            if let Some(nl) = self.text[cur..e].find('\n') {
                let next = cur + nl + 1;
                if next < e || next <= self.text.len() {
                    line_starts.push(next);
                }
                cur = next;
            } else {
                break;
            }
        }
        // Insert payload at each line start, in reverse so earlier
        // insertions don't shift later positions.
        let payload_len = payload.len();
        let mut shifted = 0usize;
        for start in line_starts.iter() {
            let at = start + shifted;
            self.text.insert_str(at, &payload);
            shifted += payload_len;
        }
        // Re-derive selection: anchor shifts by one payload (it's
        // before the first edited line) or stays put if on the same
        // line as the caret.
        let anchor = self.anchor.expect("selection requires anchor");
        if anchor <= self.caret {
            self.anchor = Some(anchor + payload_len);
            self.caret += shifted;
        } else {
            self.anchor = Some(anchor + shifted);
            self.caret += payload_len;
        }
        EditOutcome::Mutated
    }

    fn dedent_selection(&mut self, tab_size: usize) -> EditOutcome {
        let (s, e) = self
            .selection()
            .expect("dedent_selection called with no selection");
        let mut line_starts = vec![line_start_of(&self.text, s)];
        let mut cur = line_starts[0];
        while cur < e {
            if let Some(nl) = self.text[cur..e].find('\n') {
                let next = cur + nl + 1;
                if next <= self.text.len() {
                    line_starts.push(next);
                }
                cur = next;
            } else {
                break;
            }
        }
        let mut total_removed = 0usize;
        let mut removed_per_line = Vec::with_capacity(line_starts.len());
        let limit = if tab_size == 0 { 1 } else { tab_size };
        for &start in &line_starts {
            let bytes = self.text.as_bytes();
            let mut end = start;
            let mut removed = 0usize;
            let mut tabbed_out = false;
            while end < bytes.len() && removed < limit && !tabbed_out {
                match bytes[end] {
                    b'\t' => {
                        end += 1;
                        tabbed_out = true;
                    }
                    b' ' => {
                        end += 1;
                        removed += 1;
                    }
                    _ => break,
                }
            }
            removed_per_line.push(end - start);
            total_removed += end - start;
        }
        if total_removed == 0 {
            return EditOutcome::Inert;
        }
        self.push_undo(EditGroup::Atomic);
        // Remove in reverse so earlier indices stay stable.
        for (start, removed) in line_starts.iter().rev().zip(removed_per_line.iter().rev()) {
            if *removed > 0 {
                self.text.replace_range(*start..(*start + *removed), "");
            }
        }
        let anchor = self.anchor.expect("selection requires anchor");
        let first_removed = *removed_per_line.first().unwrap_or(&0);
        if anchor <= self.caret {
            self.anchor = Some(anchor.saturating_sub(first_removed));
            self.caret = self.caret.saturating_sub(total_removed);
        } else {
            self.anchor = Some(anchor.saturating_sub(total_removed));
            self.caret = self.caret.saturating_sub(first_removed);
        }
        EditOutcome::Mutated
    }

    fn selection_spans_lines(&self) -> bool {
        let Some((s, e)) = self.selection() else {
            return false;
        };
        self.text[s..e].contains('\n')
    }

    /// Pop a snapshot off undo, push the current state onto redo,
    /// restore. Closes any active typing group so the next mutation
    /// starts a fresh undo entry.
    pub fn undo(&mut self) -> EditOutcome {
        let Some(snap) = self.history.undo.pop() else {
            return EditOutcome::Inert;
        };
        let current = Snapshot {
            text: std::mem::take(&mut self.text),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.history.redo.push(current);
        self.text = snap.text;
        self.caret = snap.caret.min(self.text.len());
        self.anchor = snap.anchor;
        self.history.last_group = None;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    pub fn redo(&mut self) -> EditOutcome {
        let Some(snap) = self.history.redo.pop() else {
            return EditOutcome::Inert;
        };
        let current = Snapshot {
            text: std::mem::take(&mut self.text),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.history.undo.push(current);
        self.text = snap.text;
        self.caret = snap.caret.min(self.text.len());
        self.anchor = snap.anchor;
        self.history.last_group = None;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    /// Returned string is the selection's contents, or `None` when
    /// there's no selection (so the host can choose to copy the
    /// current line instead — common convention).
    pub fn selected_text(&self) -> Option<&str> {
        let (s, e) = self.selection()?;
        Some(&self.text[s..e])
    }

    fn delete_selection(&mut self, group: EditGroup) -> EditOutcome {
        let Some((s, e)) = self.selection() else {
            return EditOutcome::Inert;
        };
        self.push_undo(group);
        self.text.replace_range(s..e, "");
        self.caret = s;
        self.anchor = None;
        self.preferred_col = None;
        EditOutcome::Mutated
    }

    fn update_anchor_for_extend(&mut self, shift: bool) {
        if shift {
            if self.anchor.is_none() {
                self.anchor = Some(self.caret);
            }
        } else {
            self.anchor = None;
        }
    }

    /// Snapshot helper. Closes the previous group (different key) or
    /// merges (same key); the new state is the *pre-mutation* state.
    fn push_undo(&mut self, group: EditGroup) {
        // Always drain redo on a fresh mutation.
        self.history.redo.clear();
        let coalesce = matches!(
            group,
            EditGroup::Typing | EditGroup::Backspace | EditGroup::Delete
        ) && self.history.last_group == Some(group);
        self.history.last_group = Some(group);
        if coalesce {
            return;
        }
        let snap = Snapshot {
            text: self.text.clone(),
            caret: self.caret,
            anchor: self.anchor,
        };
        self.history.undo.push(snap);
        // Cap the stack so a 200k-byte buffer doesn't blow memory
        // after a few hundred edits.
        const UNDO_CAP: usize = 256;
        if self.history.undo.len() > UNDO_CAP {
            self.history.undo.remove(0);
        }
    }

    /// (1-based) line + column derived from the caret position.
    /// Matches the legacy `code_editor` status-bar formatter.
    pub fn caret_line_col(&self) -> (usize, usize) {
        byte_offset_to_line_col(&self.text, self.caret)
    }

    /// Column (0-based) of the caret on the current line. Used for
    /// the sticky preferred-column logic.
    fn current_column(&self) -> usize {
        let (line_start, _) = current_line_range_exclusive(&self.text, self.caret);
        self.text[line_start..self.caret].chars().count()
    }

    /// Look up the byte index of column `col` on the line that
    /// `byte_idx` falls in. Used by host pointer routing — for a
    /// pixel hit, the paint pass returns the row + sub-pixel column,
    /// the host rounds to a column, and this maps back to a byte for
    /// `place_caret_at`.
    pub fn line_col_to_byte(&self, line_1based: usize, col_0based: usize) -> usize {
        let mut current_line = 1usize;
        let mut line_start = 0usize;
        for (idx, b) in self.text.bytes().enumerate() {
            if current_line == line_1based {
                break;
            }
            if b == b'\n' {
                current_line += 1;
                line_start = idx + 1;
            }
        }
        if current_line < line_1based {
            return self.text.len();
        }
        clamp_column(&self.text, line_start, col_0based)
    }

    /// Number of lines in the buffer (always ≥ 1).
    pub fn line_count(&self) -> usize {
        1 + self.text.bytes().filter(|b| *b == b'\n').count()
    }

    // ── Key + text application ──────────────────────────────────

    /// Apply a `code` + modifier combo. Returns `Mutated` when state
    /// changed. Unknown codes fall through to `Inert` so the caller
    /// can route the same event to other handlers (global shortcuts).
    pub fn apply_key(&mut self, code: &str, mods: Modifiers) -> EditOutcome {
        let shift = mods.shift;
        let cmd = mods.ctrl || mods.meta;
        match code {
            "arrowleft" => {
                if cmd {
                    self.move_word_left(shift)
                } else {
                    self.move_left(shift)
                }
            }
            "arrowright" => {
                if cmd {
                    self.move_word_right(shift)
                } else {
                    self.move_right(shift)
                }
            }
            "arrowup" => self.move_up(shift),
            "arrowdown" => self.move_down(shift),
            // Page nav: 12 rows is the rough "single page" heuristic
            // — close enough for the editor's typical viewport height
            // without plumbing the real row count through the engine.
            // Hosts that need per-buffer page sizes can call
            // `page_move(N, shift)` directly.
            "pageup" => self.page_move(-12, shift),
            "pagedown" => self.page_move(12, shift),
            "home" => {
                if cmd {
                    self.move_doc_start(shift)
                } else {
                    self.move_line_start(shift)
                }
            }
            "end" => {
                if cmd {
                    self.move_doc_end(shift)
                } else {
                    self.move_line_end(shift)
                }
            }
            "backspace" => {
                if cmd {
                    self.delete_word_backward()
                } else {
                    self.delete_backward()
                }
            }
            "delete" => {
                if cmd {
                    self.delete_word_forward()
                } else {
                    self.delete_forward()
                }
            }
            "enter" | "return" => self.insert_newline(),
            "tab" => {
                if shift {
                    self.dedent(2)
                } else {
                    self.insert_tab(2)
                }
            }
            "a" if cmd => {
                self.select_all();
                EditOutcome::Mutated
            }
            "z" if cmd && !shift => self.undo(),
            "z" if cmd && shift => self.redo(),
            "y" if cmd => self.redo(),
            "d" if cmd => self.duplicate_line(),
            "k" if cmd => self.delete_line(),
            _ => EditOutcome::Inert,
        }
    }

    /// Apply a `Text` event — a chunk of typed / IME-committed text.
    pub fn apply_text(&mut self, text: &str) -> EditOutcome {
        self.insert(text, false)
    }

    /// Active IME preedit string. Empty when no composition is in
    /// progress. The host paints this *at* the caret position with
    /// an underline; the real buffer hasn't been mutated yet.
    pub fn preedit(&self) -> &str {
        &self.preedit
    }

    pub fn preedit_cursor(&self) -> Option<usize> {
        self.preedit_cursor
    }

    pub fn has_preedit(&self) -> bool {
        !self.preedit.is_empty()
    }

    /// Apply an `ImePreedit` event — replaces (or clears) the active
    /// composition. Doesn't mutate the buffer; the host re-renders
    /// with `display_text()` to show the preedit inline at the caret.
    pub fn apply_ime_preedit(&mut self, text: &str, cursor_byte: Option<usize>) -> EditOutcome {
        let was_empty = self.preedit.is_empty();
        let is_empty = text.is_empty();
        if was_empty && is_empty {
            return EditOutcome::Inert;
        }
        self.preedit.clear();
        self.preedit.push_str(text);
        self.preedit_cursor = cursor_byte;
        // Don't push undo — preedit isn't a real edit.
        EditOutcome::Mutated
    }

    /// Apply an `ImeCommit` event — the IME finalised a composition.
    /// Clears the preedit and inserts `text` at the caret as a
    /// regular edit (atomic, so it's one undo step).
    pub fn apply_ime_commit(&mut self, text: &str) -> EditOutcome {
        self.preedit.clear();
        self.preedit_cursor = None;
        if text.is_empty() {
            return EditOutcome::Mutated;
        }
        self.insert(text, true)
    }

    /// Clear any active IME composition. Called on focus-out and on
    /// the `ImeDisabled` event.
    pub fn clear_preedit(&mut self) -> EditOutcome {
        if self.preedit.is_empty() {
            return EditOutcome::Inert;
        }
        self.preedit.clear();
        self.preedit_cursor = None;
        EditOutcome::Mutated
    }

    /// `text` with any active preedit spliced in at the caret. Hosts
    /// pass this to the renderer when displaying the editor so users
    /// see the in-progress composition inline. Returns a borrowed
    /// `&str` when no preedit is active (the common case), or an
    /// owned `String` when one is. Use [`Self::preedit_range`] to
    /// find the byte range of the preedit inside the resulting
    /// string and paint an underline through it.
    pub fn display_text(&self) -> std::borrow::Cow<'_, str> {
        if self.preedit.is_empty() {
            std::borrow::Cow::Borrowed(&self.text)
        } else {
            let mut out = String::with_capacity(self.text.len() + self.preedit.len());
            out.push_str(&self.text[..self.caret]);
            out.push_str(&self.preedit);
            out.push_str(&self.text[self.caret..]);
            std::borrow::Cow::Owned(out)
        }
    }

    /// Byte range of the active preedit inside [`Self::display_text`].
    /// `None` when no preedit is active.
    pub fn preedit_range(&self) -> Option<(usize, usize)> {
        if self.preedit.is_empty() {
            None
        } else {
            Some((self.caret, self.caret + self.preedit.len()))
        }
    }

    /// Caret byte offset within [`Self::display_text`]. When a preedit
    /// is active, accounts for the OS-reported cursor *inside* the
    /// preedit; otherwise just the normal `caret_byte()`.
    pub fn display_caret_byte(&self) -> usize {
        if self.preedit.is_empty() {
            self.caret
        } else {
            let inner = self.preedit_cursor.unwrap_or(self.preedit.len());
            self.caret + inner.min(self.preedit.len())
        }
    }
}

// ── String-walking helpers ─────────────────────────────────────────

fn clamp_to_char_boundary(text: &str, idx: usize) -> usize {
    let len = text.len();
    if idx >= len {
        return len;
    }
    if text.is_char_boundary(idx) {
        return idx;
    }
    // Walk backwards to the nearest boundary.
    let mut i = idx;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn prev_char_boundary(text: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    let mut i = idx - 1;
    while i > 0 && !text.is_char_boundary(i) {
        i -= 1;
    }
    i
}

fn next_char_boundary(text: &str, idx: usize) -> usize {
    let len = text.len();
    if idx >= len {
        return len;
    }
    let mut i = idx + 1;
    while i < len && !text.is_char_boundary(i) {
        i += 1;
    }
    i
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Walk back from `idx` until the *start* of the word the caret sits
/// in (or the previous word's start, if `idx` is on whitespace). The
/// result is the byte index of the first char of that word.
fn prev_word_boundary(text: &str, idx: usize) -> usize {
    if idx == 0 {
        return 0;
    }
    // Convert to char positions for easier walking.
    let prefix = &text[..idx];
    let chars: Vec<(usize, char)> = prefix.char_indices().collect();
    let mut i = chars.len();
    // Skip trailing non-word chars
    while i > 0 && !is_word_char(chars[i - 1].1) {
        i -= 1;
    }
    while i > 0 && is_word_char(chars[i - 1].1) {
        i -= 1;
    }
    if i == 0 {
        0
    } else {
        chars[i].0
    }
}

fn next_word_boundary(text: &str, idx: usize) -> usize {
    let len = text.len();
    if idx >= len {
        return len;
    }
    let suffix = &text[idx..];
    let chars: Vec<(usize, char)> = suffix.char_indices().collect();
    let mut i = 0usize;
    // Skip leading non-word chars
    while i < chars.len() && !is_word_char(chars[i].1) {
        i += 1;
    }
    while i < chars.len() && is_word_char(chars[i].1) {
        i += 1;
    }
    if i >= chars.len() {
        len
    } else {
        idx + chars[i].0
    }
}

fn line_start_of(text: &str, idx: usize) -> usize {
    let idx = idx.min(text.len());
    text[..idx].rfind('\n').map(|n| n + 1).unwrap_or(0)
}

fn line_end_excl_of(text: &str, idx: usize) -> usize {
    text[idx..]
        .find('\n')
        .map(|n| idx + n)
        .unwrap_or(text.len())
}

/// `(line_start_byte, line_end_byte_excl)` — end is the position of
/// the `\n` (or `text.len()`).
pub(crate) fn current_line_range_exclusive(text: &str, idx: usize) -> (usize, usize) {
    let idx = idx.min(text.len());
    (line_start_of(text, idx), line_end_excl_of(text, idx))
}

/// `(line_start_byte, next_line_start_byte)` — INCLUSIVE of the
/// trailing `\n`. Used by `delete_line` so Ctrl+K consumes the
/// newline too.
fn current_line_range(text: &str, idx: usize) -> (usize, usize) {
    let (s, e) = current_line_range_exclusive(text, idx);
    let e_incl = if e < text.len() { e + 1 } else { e };
    (s, e_incl)
}

fn clamp_column(text: &str, line_start: usize, target_col: usize) -> usize {
    let (_, line_end) = current_line_range_exclusive(text, line_start);
    let line = &text[line_start..line_end];
    for (col, (ci, _)) in line.char_indices().enumerate() {
        if col == target_col {
            return line_start + ci;
        }
    }
    line_end
}

fn leading_whitespace_of_line(text: &str, idx: usize) -> String {
    let (start, end) = current_line_range_exclusive(text, idx);
    text[start..end]
        .chars()
        .take_while(|c| *c == ' ' || *c == '\t')
        .collect()
}

/// 1-based line and column from a byte offset. `(0, 0)` when the
/// buffer is empty.
pub fn byte_offset_to_line_col(text: &str, byte_offset: usize) -> (usize, usize) {
    if text.is_empty() {
        return (0, 0);
    }
    let byte_offset = byte_offset.min(text.len());
    let mut line = 1usize;
    let mut line_start = 0usize;
    for (idx, ch) in text.char_indices() {
        if idx >= byte_offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + 1;
        }
    }
    let col = text[line_start..byte_offset].chars().count() + 1;
    (line, col)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Modifiers;

    fn mods() -> Modifiers {
        Modifiers::default()
    }

    fn ctrl() -> Modifiers {
        Modifiers {
            ctrl: true,
            ..Default::default()
        }
    }

    fn shift() -> Modifiers {
        Modifiers {
            shift: true,
            ..Default::default()
        }
    }

    fn ctrl_shift() -> Modifiers {
        Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        }
    }

    #[test]
    fn empty_editor_is_sane() {
        let ed = TextEditor::new_single_line();
        assert_eq!(ed.text(), "");
        assert_eq!(ed.caret_byte(), 0);
        assert!(ed.selection().is_none());
    }

    #[test]
    fn typing_appends_at_caret() {
        let mut ed = TextEditor::with_text("ab");
        assert_eq!(ed.caret_byte(), 2);
        ed.apply_text("c");
        assert_eq!(ed.text(), "abc");
        assert_eq!(ed.caret_byte(), 3);
    }

    #[test]
    fn single_line_strips_newlines() {
        let mut ed = TextEditor::new_single_line();
        ed.apply_text("a\nb");
        assert_eq!(ed.text(), "ab");
    }

    #[test]
    fn multiline_keeps_newlines() {
        let mut ed = TextEditor::new_multi_line();
        ed.apply_text("a\nb");
        assert_eq!(ed.text(), "a\nb");
    }

    #[test]
    fn backspace_removes_one_char() {
        let mut ed = TextEditor::with_text("abc");
        ed.apply_key("backspace", mods());
        assert_eq!(ed.text(), "ab");
        assert_eq!(ed.caret_byte(), 2);
    }

    #[test]
    fn backspace_utf8_safe() {
        let mut ed = TextEditor::with_text("aπ");
        ed.apply_key("backspace", mods());
        assert_eq!(ed.text(), "a");
        assert_eq!(ed.caret_byte(), 1);
    }

    #[test]
    fn arrow_left_moves_caret() {
        let mut ed = TextEditor::with_text("abc");
        ed.apply_key("arrowleft", mods());
        assert_eq!(ed.caret_byte(), 2);
    }

    #[test]
    fn shift_arrow_extends_selection() {
        let mut ed = TextEditor::with_text("abc");
        ed.apply_key("arrowleft", shift());
        ed.apply_key("arrowleft", shift());
        assert_eq!(ed.selection(), Some((1, 3)));
    }

    #[test]
    fn ctrl_a_selects_all() {
        let mut ed = TextEditor::with_text("abc");
        ed.apply_key("a", ctrl());
        assert_eq!(ed.selection(), Some((0, 3)));
    }

    #[test]
    fn typing_replaces_selection() {
        let mut ed = TextEditor::with_text("abc");
        ed.select_all();
        ed.apply_text("X");
        assert_eq!(ed.text(), "X");
    }

    #[test]
    fn home_end_navigate_within_line() {
        let mut ed = TextEditor::with_text("hello world");
        ed.apply_key("home", mods());
        assert_eq!(ed.caret_byte(), 0);
        ed.apply_key("end", mods());
        assert_eq!(ed.caret_byte(), 11);
    }

    #[test]
    fn up_down_navigate_lines_with_preferred_column() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("abcdef\nab\nabcdef");
        // Caret at end (after final f)
        assert_eq!(ed.caret_line_col(), (3, 7));
        ed.apply_key("arrowup", mods()); // → line 2, end
        assert_eq!(ed.caret_line_col(), (2, 3));
        ed.apply_key("arrowup", mods()); // → line 1, col 6 (preferred)
        assert_eq!(ed.caret_line_col(), (1, 7));
    }

    #[test]
    fn undo_reverts_typing_run() {
        let mut ed = TextEditor::with_text("hi");
        ed.apply_text("a");
        ed.apply_text("b");
        ed.apply_text("c");
        assert_eq!(ed.text(), "hiabc");
        ed.apply_key("z", ctrl());
        assert_eq!(ed.text(), "hi");
    }

    #[test]
    fn redo_reapplies() {
        let mut ed = TextEditor::with_text("hi");
        ed.apply_text("a");
        ed.apply_key("z", ctrl());
        assert_eq!(ed.text(), "hi");
        ed.apply_key("z", ctrl_shift());
        assert_eq!(ed.text(), "hia");
    }

    #[test]
    fn ctrl_arrow_jumps_words() {
        let mut ed = TextEditor::with_text("hello world");
        ed.apply_key("arrowleft", ctrl());
        assert_eq!(ed.caret_byte(), 6); // start of "world"
        ed.apply_key("arrowleft", ctrl());
        assert_eq!(ed.caret_byte(), 0);
    }

    #[test]
    fn newline_auto_indents() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("    hello");
        ed.apply_key("end", mods());
        ed.apply_key("enter", mods());
        assert_eq!(ed.text(), "    hello\n    ");
    }

    #[test]
    fn tab_inserts_two_spaces() {
        let mut ed = TextEditor::new_multi_line();
        ed.apply_key("tab", mods());
        assert_eq!(ed.text(), "  ");
    }

    #[test]
    fn shift_tab_dedents() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("    x");
        ed.apply_key("home", mods());
        ed.apply_key("tab", shift());
        assert_eq!(ed.text(), "  x");
    }

    #[test]
    fn ctrl_k_deletes_line() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("one\ntwo\nthree");
        // place caret in middle of line 2 (byte 5, between "tw" and "o")
        ed.place_caret_at(5, false);
        ed.apply_key("k", ctrl());
        assert_eq!(ed.text(), "one\nthree");
    }

    #[test]
    fn ctrl_d_duplicates_line() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("hello");
        ed.apply_key("d", ctrl());
        assert_eq!(ed.text(), "hello\nhello");
    }

    #[test]
    fn place_caret_at_clamps_to_char_boundary() {
        let mut ed = TextEditor::with_text("aπb"); // π is two bytes (b1..b3)
        ed.place_caret_at(2, false); // middle of π
                                     // Should snap back to 1 (start of π).
        assert_eq!(ed.caret_byte(), 1);
    }

    #[test]
    fn caret_line_col_is_1_based() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("a\nbc");
        ed.place_caret_at(3, false);
        assert_eq!(ed.caret_line_col(), (2, 2));
    }

    #[test]
    fn selection_after_paste_is_collapsed() {
        let mut ed = TextEditor::with_text("ab");
        ed.select_all();
        ed.apply_text("xyz");
        assert_eq!(ed.text(), "xyz");
        assert!(!ed.has_selection());
        assert_eq!(ed.caret_byte(), 3);
    }

    #[test]
    fn delete_word_backward_eats_preceding_word() {
        let mut ed = TextEditor::with_text("hello world");
        ed.apply_key("backspace", ctrl());
        assert_eq!(ed.text(), "hello ");
    }

    #[test]
    fn multiline_indent_selection() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("a\nb\nc");
        ed.place_caret_at(0, false);
        ed.place_caret_at(5, true);
        ed.apply_key("tab", mods());
        assert_eq!(ed.text(), "  a\n  b\n  c");
    }

    #[test]
    fn ime_preedit_sets_display_text_and_range() {
        let mut ed = TextEditor::with_text("ab");
        ed.place_caret_at(2, false);
        ed.apply_ime_preedit("X", None);
        assert!(ed.has_preedit());
        assert_eq!(ed.display_text(), "abX");
        assert_eq!(ed.preedit_range(), Some((2, 3)));
        // The real buffer is untouched.
        assert_eq!(ed.text(), "ab");
        assert_eq!(ed.caret_byte(), 2);
    }

    #[test]
    fn ime_commit_clears_preedit_and_inserts() {
        let mut ed = TextEditor::with_text("ab");
        ed.place_caret_at(1, false);
        ed.apply_ime_preedit("X", None);
        ed.apply_ime_commit("漢字");
        assert!(!ed.has_preedit());
        assert_eq!(ed.text(), "a漢字b");
        // Caret lands at end of inserted text.
        assert_eq!(ed.caret_byte(), 1 + "漢字".len());
    }

    #[test]
    fn ime_preedit_replaces_previous_composition() {
        let mut ed = TextEditor::new_single_line();
        ed.apply_ime_preedit("か", None);
        ed.apply_ime_preedit("かん", None);
        assert_eq!(ed.preedit(), "かん");
        ed.apply_ime_preedit("", None);
        assert!(!ed.has_preedit());
    }

    #[test]
    fn display_caret_byte_inside_preedit() {
        let mut ed = TextEditor::with_text("abc");
        ed.place_caret_at(1, false); // caret between "a" and "bc"
        ed.apply_ime_preedit("XYZ", Some(2));
        // display = "aXYZbc"; caret sits at preedit byte 2 → 1 + 2 = 3
        assert_eq!(ed.display_caret_byte(), 3);
    }

    #[test]
    fn pagedown_moves_caret_down_several_rows() {
        let mut ed = TextEditor::new_multi_line();
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!("line {i}\n"));
        }
        ed.set_text(&text);
        ed.place_caret_at(0, false);
        ed.apply_key("pagedown", mods());
        // After page-down we should be on roughly line 13 (1-based:
        // page=12). The buffer is monotonic; rough check.
        let (line, _) = ed.caret_line_col();
        assert!(
            (10..=15).contains(&line),
            "expected line near 13, got {line}"
        );
    }

    #[test]
    fn pageup_clamps_at_doc_start() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("a\nb\nc");
        ed.place_caret_at(0, false);
        ed.apply_key("pageup", mods());
        assert_eq!(ed.caret_byte(), 0);
    }

    #[test]
    fn page_move_preserves_preferred_column() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("aaaaaa\nbb\ncccccc\ndddddd\neeeeee\nffffff\ngggggg\nhhhhhh\niiiiii\njjjjjj\nkkkkkk\nllllll\nmmmmmm\nnnnnnn\noooooo");
        // Caret on line 1, col 5 (after 5 a's).
        ed.place_caret_at(5, false);
        ed.apply_key("arrowdown", mods()); // line 2; only 2 chars, caret clamps to col 2
        let (line2, _col) = ed.caret_line_col();
        assert_eq!(line2, 2);
        // Page-down with preferred col preserved → land at col 5 on
        // the destination row.
        ed.apply_key("pagedown", mods());
        let (_line, col) = ed.caret_line_col();
        // Preferred column = 5 → on a 6-char row, lands at col 6
        // (1-based) which equals 5 chars in.
        assert_eq!(col, 6);
    }

    #[test]
    fn line_col_to_byte_resolves_to_expected_offset() {
        let mut ed = TextEditor::new_multi_line();
        ed.set_text("hello\nworld");
        assert_eq!(ed.line_col_to_byte(1, 0), 0);
        assert_eq!(ed.line_col_to_byte(1, 5), 5);
        assert_eq!(ed.line_col_to_byte(2, 0), 6);
        assert_eq!(ed.line_col_to_byte(2, 3), 9);
        // Out-of-range col clamps to end-of-line
        assert_eq!(ed.line_col_to_byte(1, 99), 5);
    }
}
