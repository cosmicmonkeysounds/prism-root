use serde::{Deserialize, Serialize};

use super::buffer::Buffer;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

impl Position {
    pub fn zero() -> Self {
        Self { line: 0, col: 0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cursor {
    pub position: Position,
    preferred_col: Option<usize>,
}

impl Default for Cursor {
    fn default() -> Self {
        Self {
            position: Position::zero(),
            preferred_col: None,
        }
    }
}

impl Cursor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn char_index(&self, buffer: &Buffer) -> usize {
        buffer.position_to_char(&self.position)
    }

    pub fn set_position(&mut self, pos: Position) {
        self.position = pos;
        self.preferred_col = None;
    }

    pub fn set_char_index(&mut self, buffer: &Buffer, idx: usize) {
        self.position = buffer.char_to_position(idx);
        self.preferred_col = None;
    }

    pub fn move_left(&mut self, buffer: &Buffer) {
        let idx = self.char_index(buffer);
        if idx > 0 {
            self.set_char_index(buffer, idx - 1);
        }
    }

    pub fn move_right(&mut self, buffer: &Buffer) {
        let idx = self.char_index(buffer);
        if idx < buffer.len_chars() {
            self.set_char_index(buffer, idx + 1);
        }
    }

    pub fn move_up(&mut self, buffer: &Buffer) {
        if self.position.line == 0 {
            return;
        }
        let target_col = self.preferred_col.unwrap_or(self.position.col);
        let new_line = self.position.line - 1;
        let line_len = buffer.line_len_chars(new_line);
        self.position = Position {
            line: new_line,
            col: target_col.min(line_len),
        };
        self.preferred_col = Some(target_col);
    }

    pub fn move_down(&mut self, buffer: &Buffer) {
        let last_line = buffer.line_count().saturating_sub(1);
        if self.position.line >= last_line {
            return;
        }
        let target_col = self.preferred_col.unwrap_or(self.position.col);
        let new_line = self.position.line + 1;
        let line_len = buffer.line_len_chars(new_line);
        self.position = Position {
            line: new_line,
            col: target_col.min(line_len),
        };
        self.preferred_col = Some(target_col);
    }

    pub fn move_to_line_start(&mut self) {
        self.position.col = 0;
        self.preferred_col = None;
    }

    pub fn move_to_line_end(&mut self, buffer: &Buffer) {
        self.position.col = buffer.line_len_chars(self.position.line);
        self.preferred_col = None;
    }

    pub fn move_to_buffer_start(&mut self) {
        self.position = Position::zero();
        self.preferred_col = None;
    }

    pub fn move_to_buffer_end(&mut self, buffer: &Buffer) {
        let idx = buffer.len_chars();
        self.set_char_index(buffer, idx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buf(s: &str) -> Buffer {
        Buffer::from_text(s)
    }

    #[test]
    fn basic_movement() {
        let b = buf("abc\ndef");
        let mut c = Cursor::new();
        c.move_right(&b);
        assert_eq!(c.position, Position { line: 0, col: 1 });
        c.move_right(&b);
        c.move_right(&b);
        c.move_right(&b);
        assert_eq!(c.position, Position { line: 1, col: 0 });
        c.move_left(&b);
        assert_eq!(c.position, Position { line: 0, col: 3 });
    }

    #[test]
    fn vertical_movement_preserves_preferred_col() {
        let b = buf("abcdef\nab\nabcdef");
        let mut c = Cursor::new();
        c.set_position(Position { line: 0, col: 5 });
        c.move_down(&b);
        assert_eq!(c.position, Position { line: 1, col: 2 });
        c.move_down(&b);
        assert_eq!(c.position, Position { line: 2, col: 5 });
    }

    #[test]
    fn home_end() {
        let b = buf("hello world");
        let mut c = Cursor::new();
        c.set_position(Position { line: 0, col: 5 });
        c.move_to_line_start();
        assert_eq!(c.position.col, 0);
        c.move_to_line_end(&b);
        assert_eq!(c.position.col, 11);
    }

    #[test]
    fn buffer_start_end() {
        let b = buf("aaa\nbbb\nccc");
        let mut c = Cursor::new();
        c.move_to_buffer_end(&b);
        assert_eq!(c.position, Position { line: 2, col: 3 });
        c.move_to_buffer_start();
        assert_eq!(c.position, Position::zero());
    }

    #[test]
    fn boundary_no_panic() {
        let b = buf("");
        let mut c = Cursor::new();
        c.move_left(&b);
        assert_eq!(c.position, Position::zero());
        c.move_up(&b);
        assert_eq!(c.position, Position::zero());
        c.move_down(&b);
        assert_eq!(c.position, Position::zero());
    }
}
