use serde::{Deserialize, Serialize};

use super::buffer::Buffer;
use super::cursor::{Cursor, Position};
use super::fold::FoldState;
use super::selection::Selection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorState {
    pub buffer: Buffer,
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub language: String,
    pub read_only: bool,
    pub tab_width: usize,
    pub fold_state: FoldState,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Cursor::new(),
            selection: None,
            language: String::new(),
            read_only: false,
            tab_width: 4,
            fold_state: FoldState::new(),
        }
    }
}

impl EditorState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_text(text: &str) -> Self {
        Self {
            buffer: Buffer::from_text(text),
            ..Default::default()
        }
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn set_text(&mut self, text: &str) {
        self.buffer = Buffer::from_text(text);
        self.cursor = Cursor::new();
        self.selection = None;
        self.fold_state = FoldState::new();
    }

    pub fn toggle_fold_at_cursor(&mut self) {
        let line = self.cursor.position.line;
        self.fold_state
            .toggle_fold(line, &self.buffer, self.tab_width);
    }

    pub fn toggle_fold_at_line(&mut self, line: usize) {
        self.fold_state
            .toggle_fold(line, &self.buffer, self.tab_width);
    }

    fn invalidate_folds(&mut self) {
        let line = self.cursor.position.line;
        self.fold_state.invalidate_from(line);
    }

    pub fn insert_char(&mut self, ch: char) {
        if self.read_only {
            return;
        }
        self.delete_selection_if_any();
        let idx = self.cursor.char_index(&self.buffer);
        let mut s = String::new();
        s.push(ch);
        self.buffer.insert(idx, &s);
        self.cursor.set_char_index(&self.buffer, idx + 1);
        self.invalidate_folds();
    }

    pub fn insert_text(&mut self, text: &str) {
        if self.read_only || text.is_empty() {
            return;
        }
        self.delete_selection_if_any();
        let idx = self.cursor.char_index(&self.buffer);
        let len = text.chars().count();
        self.buffer.insert(idx, text);
        self.cursor.set_char_index(&self.buffer, idx + len);
        self.invalidate_folds();
    }

    pub fn insert_tab(&mut self) {
        if self.read_only {
            return;
        }
        self.delete_selection_if_any();
        let col = self.cursor.position.col;
        let spaces_to_next = self.tab_width - (col % self.tab_width);
        let spaces: String = " ".repeat(spaces_to_next);
        let idx = self.cursor.char_index(&self.buffer);
        self.buffer.insert(idx, &spaces);
        self.cursor
            .set_char_index(&self.buffer, idx + spaces_to_next);
        self.invalidate_folds();
    }

    pub fn dedent(&mut self) {
        if self.read_only {
            return;
        }
        let line = self.cursor.position.line;
        if let Some(line_text) = self.buffer.line(line) {
            let leading = line_text.chars().take_while(|c| *c == ' ').count();
            let to_remove = leading.min(self.tab_width);
            if to_remove > 0 {
                let line_start = self.buffer.position_to_char(&Position { line, col: 0 });
                self.buffer.delete(line_start, line_start + to_remove);
                let new_col = self.cursor.position.col.saturating_sub(to_remove);
                self.cursor.set_position(Position { line, col: new_col });
                self.invalidate_folds();
            }
        }
    }

    pub fn insert_newline(&mut self) {
        if self.read_only {
            return;
        }
        self.delete_selection_if_any();
        let current_line = self.cursor.position.line;
        let indent = self
            .buffer
            .line(current_line)
            .map(|lt| lt.chars().take_while(|c| *c == ' ').collect::<String>())
            .unwrap_or_default();
        let idx = self.cursor.char_index(&self.buffer);
        let insert = format!("\n{indent}");
        let insert_len = insert.chars().count();
        self.buffer.insert(idx, &insert);
        self.cursor.set_char_index(&self.buffer, idx + insert_len);
        self.invalidate_folds();
    }

    pub fn backspace(&mut self) {
        if self.read_only {
            return;
        }
        if self.delete_selection_if_any() {
            return;
        }
        let idx = self.cursor.char_index(&self.buffer);
        if idx > 0 {
            self.buffer.delete(idx - 1, idx);
            self.cursor.set_char_index(&self.buffer, idx - 1);
            self.invalidate_folds();
        }
    }

    pub fn delete(&mut self) {
        if self.read_only {
            return;
        }
        if self.delete_selection_if_any() {
            return;
        }
        let idx = self.cursor.char_index(&self.buffer);
        if idx < self.buffer.len_chars() {
            self.buffer.delete(idx, idx + 1);
            self.invalidate_folds();
        }
    }

    pub fn delete_word_left(&mut self) {
        if self.read_only {
            return;
        }
        if self.delete_selection_if_any() {
            return;
        }
        let idx = self.cursor.char_index(&self.buffer);
        let target = word_boundary_left(&self.buffer.text(), idx);
        if target < idx {
            self.buffer.delete(target, idx);
            self.cursor.set_char_index(&self.buffer, target);
            self.invalidate_folds();
        }
    }

    pub fn delete_word_right(&mut self) {
        if self.read_only {
            return;
        }
        if self.delete_selection_if_any() {
            return;
        }
        let idx = self.cursor.char_index(&self.buffer);
        let target = word_boundary_right(&self.buffer.text(), idx);
        if target > idx {
            self.buffer.delete(idx, target);
            self.invalidate_folds();
        }
    }

    pub fn delete_line(&mut self) {
        if self.read_only {
            return;
        }
        let line = self.cursor.position.line;
        let line_count = self.buffer.line_count();
        if line_count <= 1 && self.buffer.is_empty() {
            return;
        }
        let line_start = self.buffer.position_to_char(&Position { line, col: 0 });
        let line_end = if line + 1 < line_count {
            self.buffer.position_to_char(&Position {
                line: line + 1,
                col: 0,
            })
        } else if line > 0 {
            let prev_end = self.buffer.position_to_char(&Position { line, col: 0 });
            let total = self.buffer.len_chars();
            self.buffer.delete(prev_end.saturating_sub(1), total);
            let new_line = line.saturating_sub(1);
            let line_len = self.buffer.line_len_chars(new_line);
            self.cursor.set_position(Position {
                line: new_line,
                col: self.cursor.position.col.min(line_len),
            });
            self.selection = None;
            self.invalidate_folds();
            return;
        } else {
            self.buffer.len_chars()
        };

        self.buffer.delete(line_start, line_end);
        self.selection = None;
        let new_line = line.min(self.buffer.line_count().saturating_sub(1));
        let line_len = self.buffer.line_len_chars(new_line);
        self.cursor.set_position(Position {
            line: new_line,
            col: self.cursor.position.col.min(line_len),
        });
        self.invalidate_folds();
    }

    pub fn duplicate_line(&mut self) {
        if self.read_only {
            return;
        }
        let line = self.cursor.position.line;
        if let Some(line_text) = self.buffer.line(line) {
            let trimmed = line_text.trim_end_matches('\n');
            let insert = format!("\n{trimmed}");
            let line_end_col = self.buffer.line_len_chars(line);
            let insert_pos = self.buffer.position_to_char(&Position {
                line,
                col: line_end_col,
            });
            self.buffer.insert(insert_pos, &insert);
            self.cursor.set_position(Position {
                line: line + 1,
                col: self.cursor.position.col,
            });
            self.invalidate_folds();
        }
    }

    pub fn move_left(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            if let Some(sel) = self.selection.take() {
                let start = sel.start(&self.buffer);
                self.cursor.set_char_index(&self.buffer, start);
                return;
            }
        }
        self.cursor.move_left(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn move_right(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            if let Some(sel) = self.selection.take() {
                let end = sel.end(&self.buffer);
                self.cursor.set_char_index(&self.buffer, end);
                return;
            }
        }
        self.cursor.move_right(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn move_up(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_up(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn move_down(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_down(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn move_to_line_start(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_to_line_start();
        self.update_selection_head(extend_selection);
    }

    pub fn move_to_line_end(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_to_line_end(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn move_word_left(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        let idx = self.cursor.char_index(&self.buffer);
        let target = word_boundary_left(&self.buffer.text(), idx);
        self.cursor.set_char_index(&self.buffer, target);
        self.update_selection_head(extend_selection);
    }

    pub fn move_word_right(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        let idx = self.cursor.char_index(&self.buffer);
        let target = word_boundary_right(&self.buffer.text(), idx);
        self.cursor.set_char_index(&self.buffer, target);
        self.update_selection_head(extend_selection);
    }

    pub fn move_to_buffer_start(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_to_buffer_start();
        self.update_selection_head(extend_selection);
    }

    pub fn move_to_buffer_end(&mut self, extend_selection: bool) {
        self.update_selection(extend_selection);
        if !extend_selection {
            self.selection = None;
        }
        self.cursor.move_to_buffer_end(&self.buffer);
        self.update_selection_head(extend_selection);
    }

    pub fn select_all(&mut self) {
        let end = self.buffer.len_chars();
        self.selection = Some(Selection::new(
            Position::zero(),
            self.buffer.char_to_position(end),
        ));
        self.cursor.move_to_buffer_end(&self.buffer);
    }

    pub fn set_cursor_position(&mut self, line: usize, col: usize) {
        let line = line.min(self.buffer.line_count().saturating_sub(1));
        let col = col.min(self.buffer.line_len_chars(line));
        self.cursor.set_position(Position { line, col });
        self.selection = None;
    }

    pub fn extend_selection_to(&mut self, line: usize, col: usize) {
        let line = line.min(self.buffer.line_count().saturating_sub(1));
        let col = col.min(self.buffer.line_len_chars(line));
        let pos = Position { line, col };
        if self.selection.is_none() {
            self.selection = Some(Selection::new(self.cursor.position, pos));
        } else if let Some(sel) = &mut self.selection {
            sel.head = pos;
        }
        self.cursor.set_position(pos);
    }

    pub fn handle_action(&mut self, action: &str) {
        match action {
            "enter" => self.insert_newline(),
            "backspace" => self.backspace(),
            "ctrl-backspace" => self.delete_word_left(),
            "delete" => self.delete(),
            "ctrl-delete" => self.delete_word_right(),
            "tab" => self.insert_tab(),
            "shift-tab" => self.dedent(),
            "up" => self.move_up(false),
            "shift-up" => self.move_up(true),
            "down" => self.move_down(false),
            "shift-down" => self.move_down(true),
            "left" => self.move_left(false),
            "shift-left" => self.move_left(true),
            "ctrl-left" => self.move_word_left(false),
            "ctrl-shift-left" => self.move_word_left(true),
            "right" => self.move_right(false),
            "shift-right" => self.move_right(true),
            "ctrl-right" => self.move_word_right(false),
            "ctrl-shift-right" => self.move_word_right(true),
            "home" => self.move_to_line_start(false),
            "shift-home" => self.move_to_line_start(true),
            "ctrl-home" => self.move_to_buffer_start(false),
            "ctrl-shift-home" => self.move_to_buffer_start(true),
            "end" => self.move_to_line_end(false),
            "shift-end" => self.move_to_line_end(true),
            "ctrl-end" => self.move_to_buffer_end(false),
            "ctrl-shift-end" => self.move_to_buffer_end(true),
            "select-all" => self.select_all(),
            "duplicate-line" => self.duplicate_line(),
            "delete-line" => self.delete_line(),
            "toggle-fold" => self.toggle_fold_at_cursor(),
            _ => {}
        }
    }

    fn update_selection(&mut self, extend: bool) {
        if extend && self.selection.is_none() {
            self.selection = Some(Selection::caret(self.cursor.position));
        }
    }

    fn update_selection_head(&mut self, extend: bool) {
        if extend {
            if let Some(sel) = &mut self.selection {
                sel.head = self.cursor.position;
                if sel.is_empty() {
                    self.selection = None;
                }
            }
        }
    }

    fn delete_selection_if_any(&mut self) -> bool {
        if let Some(sel) = self.selection.take() {
            if !sel.is_empty() {
                let start = sel.start(&self.buffer);
                let end = sel.end(&self.buffer);
                self.buffer.delete(start, end);
                self.cursor.set_char_index(&self.buffer, start);
                self.invalidate_folds();
                return true;
            }
        }
        false
    }
}

fn word_boundary_left(text: &str, from: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let mut i = from.min(chars.len());
    if i == 0 {
        return 0;
    }
    i -= 1;
    while i > 0 && chars[i].is_whitespace() {
        i -= 1;
    }
    if chars[i].is_alphanumeric() || chars[i] == '_' {
        while i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
            i -= 1;
        }
    } else if !chars[i].is_whitespace() {
        while i > 0
            && !chars[i - 1].is_alphanumeric()
            && chars[i - 1] != '_'
            && !chars[i - 1].is_whitespace()
        {
            i -= 1;
        }
    }
    i
}

fn word_boundary_right(text: &str, from: usize) -> usize {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = from;
    if i >= len {
        return len;
    }
    if chars[i].is_alphanumeric() || chars[i] == '_' {
        while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
            i += 1;
        }
    } else if !chars[i].is_whitespace() {
        while i < len && !chars[i].is_alphanumeric() && chars[i] != '_' && !chars[i].is_whitespace()
        {
            i += 1;
        }
    }
    while i < len && chars[i].is_whitespace() {
        i += 1;
    }
    i
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_and_backspace() {
        let mut s = EditorState::new();
        s.insert_char('a');
        s.insert_char('b');
        assert_eq!(s.text(), "ab");
        s.backspace();
        assert_eq!(s.text(), "a");
    }

    #[test]
    fn insert_text() {
        let mut s = EditorState::new();
        s.insert_text("hello world");
        assert_eq!(s.text(), "hello world");
        assert_eq!(s.cursor.position, Position { line: 0, col: 11 });
    }

    #[test]
    fn newline_auto_indent() {
        let mut s = EditorState::with_text("    hello");
        s.cursor.set_position(Position { line: 0, col: 9 });
        s.insert_newline();
        assert_eq!(s.text(), "    hello\n    ");
        assert_eq!(s.cursor.position, Position { line: 1, col: 4 });
    }

    #[test]
    fn newline_mid_line_preserves_indent() {
        let mut s = EditorState::with_text("    ab");
        s.cursor.set_position(Position { line: 0, col: 5 });
        s.insert_newline();
        assert_eq!(s.text(), "    a\n    b");
        assert_eq!(s.cursor.position, Position { line: 1, col: 4 });
    }

    #[test]
    fn tab_inserts_spaces() {
        let mut s = EditorState::new();
        s.insert_tab();
        assert_eq!(s.text(), "    ");
        assert_eq!(s.cursor.position.col, 4);
    }

    #[test]
    fn tab_aligns_to_stop() {
        let mut s = EditorState::new();
        s.insert_char('a');
        s.insert_tab();
        assert_eq!(s.text(), "a   ");
        assert_eq!(s.cursor.position.col, 4);
    }

    #[test]
    fn dedent_removes_spaces() {
        let mut s = EditorState::with_text("    hello");
        s.cursor.set_position(Position { line: 0, col: 6 });
        s.dedent();
        assert_eq!(s.text(), "hello");
        assert_eq!(s.cursor.position.col, 2);
    }

    #[test]
    fn dedent_partial() {
        let mut s = EditorState::with_text("  hi");
        s.cursor.set_position(Position { line: 0, col: 3 });
        s.dedent();
        assert_eq!(s.text(), "hi");
        assert_eq!(s.cursor.position.col, 1);
    }

    #[test]
    fn delete_forward() {
        let mut s = EditorState::with_text("abc");
        s.cursor.set_position(Position { line: 0, col: 1 });
        s.delete();
        assert_eq!(s.text(), "ac");
    }

    #[test]
    fn select_and_delete() {
        let mut s = EditorState::with_text("hello world");
        s.selection = Some(Selection::new(
            Position { line: 0, col: 0 },
            Position { line: 0, col: 5 },
        ));
        s.backspace();
        assert_eq!(s.text(), " world");
    }

    #[test]
    fn select_all() {
        let mut s = EditorState::with_text("abc\ndef");
        s.select_all();
        let sel = s.selection.unwrap();
        assert_eq!(sel.selected_text(&s.buffer), "abc\ndef");
    }

    #[test]
    fn type_replaces_selection() {
        let mut s = EditorState::with_text("hello world");
        s.selection = Some(Selection::new(
            Position { line: 0, col: 0 },
            Position { line: 0, col: 5 },
        ));
        s.insert_text("goodbye");
        assert_eq!(s.text(), "goodbye world");
    }

    #[test]
    fn read_only_blocks_edits() {
        let mut s = EditorState::with_text("locked");
        s.read_only = true;
        s.insert_char('x');
        assert_eq!(s.text(), "locked");
        s.backspace();
        assert_eq!(s.text(), "locked");
    }

    #[test]
    fn shift_arrow_extends_selection() {
        let mut s = EditorState::with_text("abcdef");
        s.cursor.set_position(Position { line: 0, col: 2 });
        s.move_right(true);
        s.move_right(true);
        let sel = s.selection.unwrap();
        assert_eq!(sel.selected_text(&s.buffer), "cd");
    }

    #[test]
    fn arrow_collapses_selection() {
        let mut s = EditorState::with_text("abcdef");
        s.selection = Some(Selection::new(
            Position { line: 0, col: 1 },
            Position { line: 0, col: 4 },
        ));
        s.move_right(false);
        assert!(s.selection.is_none());
        assert_eq!(s.cursor.position, Position { line: 0, col: 4 });
    }

    #[test]
    fn set_text_resets_state() {
        let mut s = EditorState::with_text("old");
        s.cursor.set_position(Position { line: 0, col: 3 });
        s.set_text("new content");
        assert_eq!(s.text(), "new content");
        assert_eq!(s.cursor.position, Position::zero());
        assert!(s.selection.is_none());
    }

    #[test]
    fn word_left() {
        let mut s = EditorState::with_text("hello world");
        s.cursor.set_position(Position { line: 0, col: 11 });
        s.move_word_left(false);
        assert_eq!(s.cursor.position.col, 6);
        s.move_word_left(false);
        assert_eq!(s.cursor.position.col, 0);
    }

    #[test]
    fn word_right() {
        let mut s = EditorState::with_text("hello world");
        s.cursor.set_position(Position { line: 0, col: 0 });
        s.move_word_right(false);
        assert_eq!(s.cursor.position.col, 6);
        s.move_word_right(false);
        assert_eq!(s.cursor.position.col, 11);
    }

    #[test]
    fn delete_word_left() {
        let mut s = EditorState::with_text("hello world");
        s.cursor.set_position(Position { line: 0, col: 11 });
        s.delete_word_left();
        assert_eq!(s.text(), "hello ");
    }

    #[test]
    fn delete_word_right() {
        let mut s = EditorState::with_text("hello world");
        s.cursor.set_position(Position { line: 0, col: 0 });
        s.delete_word_right();
        assert_eq!(s.text(), "world");
    }

    #[test]
    fn duplicate_line() {
        let mut s = EditorState::with_text("hello\nworld");
        s.cursor.set_position(Position { line: 0, col: 2 });
        s.duplicate_line();
        assert_eq!(s.text(), "hello\nhello\nworld");
        assert_eq!(s.cursor.position, Position { line: 1, col: 2 });
    }

    #[test]
    fn delete_line_middle() {
        let mut s = EditorState::with_text("aaa\nbbb\nccc");
        s.cursor.set_position(Position { line: 1, col: 1 });
        s.delete_line();
        assert_eq!(s.text(), "aaa\nccc");
        assert_eq!(s.cursor.position.line, 1);
    }

    #[test]
    fn handle_action_dispatch() {
        let mut s = EditorState::with_text("abc");
        s.cursor.set_position(Position { line: 0, col: 3 });
        s.handle_action("enter");
        assert_eq!(s.text(), "abc\n");
        assert_eq!(s.cursor.position.line, 1);
    }

    #[test]
    fn set_cursor_position_clamps() {
        let mut s = EditorState::with_text("abc\ndef");
        s.set_cursor_position(99, 99);
        assert_eq!(s.cursor.position, Position { line: 1, col: 3 });
    }

    #[test]
    fn drag_selection_workflow() {
        let mut s = EditorState::with_text("hello world");
        s.set_cursor_position(0, 2);
        assert!(s.selection.is_none());
        s.extend_selection_to(0, 7);
        let sel = s.selection.unwrap();
        assert_eq!(sel.anchor, Position { line: 0, col: 2 });
        assert_eq!(sel.head, Position { line: 0, col: 7 });
        assert_eq!(sel.selected_text(&s.buffer), "llo w");
    }

    #[test]
    fn drag_selection_multiline() {
        let mut s = EditorState::with_text("abc\ndef\nghi");
        s.set_cursor_position(0, 1);
        s.extend_selection_to(2, 2);
        let sel = s.selection.unwrap();
        assert_eq!(sel.selected_text(&s.buffer), "bc\ndef\ngh");
        assert_eq!(s.cursor.position, Position { line: 2, col: 2 });
    }

    #[test]
    fn extend_selection_continues_from_anchor() {
        let mut s = EditorState::with_text("abcdef");
        s.set_cursor_position(0, 1);
        s.extend_selection_to(0, 3);
        s.extend_selection_to(0, 5);
        let sel = s.selection.unwrap();
        assert_eq!(sel.anchor, Position { line: 0, col: 1 });
        assert_eq!(sel.head, Position { line: 0, col: 5 });
        assert_eq!(sel.selected_text(&s.buffer), "bcde");
    }
}
