use ropey::Rope;
use serde::{Deserialize, Serialize};

use super::cursor::Position;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Buffer {
    #[serde(
        serialize_with = "serialize_rope",
        deserialize_with = "deserialize_rope"
    )]
    rope: Rope,
}

fn serialize_rope<S: serde::Serializer>(rope: &Rope, ser: S) -> Result<S::Ok, S::Error> {
    ser.serialize_str(&rope.to_string())
}

fn deserialize_rope<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Rope, D::Error> {
    let s = String::deserialize(de)?;
    Ok(Rope::from_str(&s))
}

impl Default for Buffer {
    fn default() -> Self {
        Self { rope: Rope::new() }
    }
}

impl Buffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_text(s: &str) -> Self {
        Self {
            rope: Rope::from_str(s),
        }
    }

    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    pub fn len_chars(&self) -> usize {
        self.rope.len_chars()
    }

    pub fn is_empty(&self) -> bool {
        self.rope.len_chars() == 0
    }

    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    pub fn line(&self, idx: usize) -> Option<String> {
        if idx >= self.rope.len_lines() {
            return None;
        }
        let slice = self.rope.line(idx);
        Some(slice.to_string())
    }

    pub fn line_len_chars(&self, idx: usize) -> usize {
        if idx >= self.rope.len_lines() {
            return 0;
        }
        let line = self.rope.line(idx);
        let len = line.len_chars();
        // Exclude trailing newline from the "visible" length.
        if len > 0 && line.char(len - 1) == '\n' {
            len - 1
        } else {
            len
        }
    }

    pub fn insert(&mut self, char_idx: usize, text: &str) {
        let idx = char_idx.min(self.rope.len_chars());
        self.rope.insert(idx, text);
    }

    pub fn delete(&mut self, start: usize, end: usize) {
        let s = start.min(self.rope.len_chars());
        let e = end.min(self.rope.len_chars());
        if s < e {
            self.rope.remove(s..e);
        }
    }

    pub fn char_to_position(&self, char_idx: usize) -> Position {
        let idx = char_idx.min(self.rope.len_chars());
        let line = self.rope.char_to_line(idx);
        let line_start = self.rope.line_to_char(line);
        Position {
            line,
            col: idx - line_start,
        }
    }

    pub fn position_to_char(&self, pos: &Position) -> usize {
        if pos.line >= self.rope.len_lines() {
            return self.rope.len_chars();
        }
        let line_start = self.rope.line_to_char(pos.line);
        let line_len = self.line_len_chars(pos.line);
        line_start + pos.col.min(line_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer() {
        let buf = Buffer::new();
        assert!(buf.is_empty());
        assert_eq!(buf.len_chars(), 0);
        assert_eq!(buf.line_count(), 1);
        assert_eq!(buf.text(), "");
    }

    #[test]
    fn from_text_and_lines() {
        let buf = Buffer::from_text("hello\nworld\n");
        assert_eq!(buf.len_chars(), 12);
        assert_eq!(buf.line_count(), 3);
        assert_eq!(buf.line(0).unwrap(), "hello\n");
        assert_eq!(buf.line(1).unwrap(), "world\n");
        assert_eq!(buf.line(2).unwrap(), "");
    }

    #[test]
    fn insert_and_delete() {
        let mut buf = Buffer::from_text("hello");
        buf.insert(5, " world");
        assert_eq!(buf.text(), "hello world");
        buf.delete(5, 11);
        assert_eq!(buf.text(), "hello");
    }

    #[test]
    fn line_len_chars_excludes_newline() {
        let buf = Buffer::from_text("abc\ndef\n");
        assert_eq!(buf.line_len_chars(0), 3);
        assert_eq!(buf.line_len_chars(1), 3);
        assert_eq!(buf.line_len_chars(2), 0);
    }

    #[test]
    fn position_round_trip() {
        let buf = Buffer::from_text("abc\ndef\nghi");
        let pos = buf.char_to_position(5);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.col, 1);
        assert_eq!(buf.position_to_char(&pos), 5);
    }

    #[test]
    fn position_clamping() {
        let buf = Buffer::from_text("ab\ncd");
        let pos = Position { line: 0, col: 99 };
        assert_eq!(buf.position_to_char(&pos), 2);
        let pos = Position { line: 99, col: 0 };
        assert_eq!(buf.position_to_char(&pos), buf.len_chars());
    }

    #[test]
    fn serde_round_trip() {
        let buf = Buffer::from_text("line1\nline2");
        let json = serde_json::to_string(&buf).unwrap();
        let buf2: Buffer = serde_json::from_str(&json).unwrap();
        assert_eq!(buf.text(), buf2.text());
    }
}
