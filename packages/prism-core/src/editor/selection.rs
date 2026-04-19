use serde::{Deserialize, Serialize};

use super::buffer::Buffer;
use super::cursor::Position;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Selection {
    pub anchor: Position,
    pub head: Position,
}

impl Selection {
    pub fn new(anchor: Position, head: Position) -> Self {
        Self { anchor, head }
    }

    pub fn caret(pos: Position) -> Self {
        Self {
            anchor: pos,
            head: pos,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn start(&self, buffer: &Buffer) -> usize {
        let a = buffer.position_to_char(&self.anchor);
        let h = buffer.position_to_char(&self.head);
        a.min(h)
    }

    pub fn end(&self, buffer: &Buffer) -> usize {
        let a = buffer.position_to_char(&self.anchor);
        let h = buffer.position_to_char(&self.head);
        a.max(h)
    }

    pub fn ordered_positions(&self, buffer: &Buffer) -> (Position, Position) {
        let a = buffer.position_to_char(&self.anchor);
        let h = buffer.position_to_char(&self.head);
        if a <= h {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    pub fn selected_text(&self, buffer: &Buffer) -> String {
        let start = self.start(buffer);
        let end = self.end(buffer);
        if start == end {
            return String::new();
        }
        let text = buffer.text();
        text.chars().skip(start).take(end - start).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caret_is_empty() {
        let sel = Selection::caret(Position { line: 1, col: 3 });
        assert!(sel.is_empty());
    }

    #[test]
    fn selected_text_forward() {
        let buf = Buffer::from_text("hello world");
        let sel = Selection::new(Position { line: 0, col: 0 }, Position { line: 0, col: 5 });
        assert_eq!(sel.selected_text(&buf), "hello");
    }

    #[test]
    fn selected_text_backward() {
        let buf = Buffer::from_text("hello world");
        let sel = Selection::new(Position { line: 0, col: 5 }, Position { line: 0, col: 0 });
        assert_eq!(sel.selected_text(&buf), "hello");
    }

    #[test]
    fn multi_line_selection() {
        let buf = Buffer::from_text("abc\ndef\nghi");
        let sel = Selection::new(Position { line: 0, col: 1 }, Position { line: 2, col: 2 });
        assert_eq!(sel.selected_text(&buf), "bc\ndef\ngh");
    }

    #[test]
    fn ordered_positions() {
        let buf = Buffer::from_text("abcdef");
        let sel = Selection::new(Position { line: 0, col: 5 }, Position { line: 0, col: 1 });
        let (start, end) = sel.ordered_positions(&buf);
        assert_eq!(start, Position { line: 0, col: 1 });
        assert_eq!(end, Position { line: 0, col: 5 });
    }
}
