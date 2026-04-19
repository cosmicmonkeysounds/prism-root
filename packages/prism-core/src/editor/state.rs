use serde::{Deserialize, Serialize};

use super::buffer::Buffer;
use super::cursor::{Cursor, Position};
use super::selection::Selection;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EditorState {
    pub buffer: Buffer,
    pub cursor: Cursor,
    pub selection: Option<Selection>,
    pub language: String,
    pub read_only: bool,
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            buffer: Buffer::new(),
            cursor: Cursor::new(),
            selection: None,
            language: String::new(),
            read_only: false,
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
    }

    pub fn insert_newline(&mut self) {
        self.insert_char('\n');
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

    pub fn select_all(&mut self) {
        let end = self.buffer.len_chars();
        self.selection = Some(Selection::new(
            Position::zero(),
            self.buffer.char_to_position(end),
        ));
        self.cursor.move_to_buffer_end(&self.buffer);
    }

    fn update_selection(&mut self, extend: bool) {
        if extend && self.selection.is_none() {
            self.selection = Some(Selection::caret(self.cursor.position));
        } else if !extend {
            // Cleared by the caller when appropriate.
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
                return true;
            }
        }
        false
    }
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
    fn newline() {
        let mut s = EditorState::with_text("ab");
        s.cursor.set_position(Position { line: 0, col: 1 });
        s.insert_newline();
        assert_eq!(s.text(), "a\nb");
        assert_eq!(s.cursor.position, Position { line: 1, col: 0 });
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
}
