//! Scanner — generic byte-level cursor for building tokenizers.
//!
//! Port of `language/syntax/scanner.ts`. Provides position
//! tracking, peek/advance/match, save/restore for backtracking,
//! and common scanning helpers (identifiers, numbers, quoted
//! strings). Language-specific tokenizers compose on top of this
//! rather than reimplementing character-level logic.

use std::fmt;

use super::ast_types::{Position, SourceRange};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScannerState {
    pub offset: usize,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanError {
    pub message: String,
    pub position: Position,
}

impl ScanError {
    pub fn new(message: impl Into<String>, position: Position) -> Self {
        Self {
            message: message.into(),
            position,
        }
    }
}

impl fmt::Display for ScanError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{}",
            self.message, self.position.line, self.position.column
        )
    }
}

impl std::error::Error for ScanError {}

/// Byte-indexed source scanner. The source is held as the original
/// UTF-8 `&str`, but `offset` is a byte index so slicing is O(1).
///
/// ASCII-only scanning helpers (`advance`, `peek`, `scan_while`)
/// treat one byte as one character — this matches the legacy TS
/// behavior for the identifier, number, and quote helpers used by
/// the expression scanner. Callers that need Unicode-aware slicing
/// should use `source()` plus their own walk.
pub struct Scanner<'s> {
    source: &'s str,
    offset: usize,
    line: usize,
    column: usize,
}

impl<'s> Scanner<'s> {
    pub fn new(source: &'s str) -> Self {
        Self {
            source,
            offset: 0,
            line: 1,
            column: 0,
        }
    }

    pub fn source(&self) -> &'s str {
        self.source
    }

    pub fn offset(&self) -> usize {
        self.offset
    }

    pub fn line(&self) -> usize {
        self.line
    }

    pub fn column(&self) -> usize {
        self.column
    }

    pub fn position(&self) -> Position {
        Position {
            offset: self.offset,
            line: self.line,
            column: self.column,
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.offset >= self.source.len()
    }

    pub fn save(&self) -> ScannerState {
        ScannerState {
            offset: self.offset,
            line: self.line,
            column: self.column,
        }
    }

    pub fn restore(&mut self, state: ScannerState) {
        self.offset = state.offset;
        self.line = state.line;
        self.column = state.column;
    }

    /// Peek the ASCII byte at `offset + ahead`, or `None` if past
    /// the end. Non-ASCII bytes round-trip correctly through the
    /// scanner but the helpers below only classify ASCII.
    pub fn peek(&self) -> Option<char> {
        self.peek_ahead(0)
    }

    pub fn peek_ahead(&self, ahead: usize) -> Option<char> {
        let idx = self.offset + ahead;
        self.source.as_bytes().get(idx).map(|b| *b as char)
    }

    /// Consume and return the current character (as a single
    /// byte). Returns `None` at EOF.
    pub fn advance(&mut self) -> Option<char> {
        let byte = *self.source.as_bytes().get(self.offset)?;
        self.offset += 1;
        let ch = byte as char;
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    pub fn slice(&self, start: usize, end: usize) -> &'s str {
        &self.source[start..end]
    }

    /// If the upcoming bytes match `expected`, consume them and
    /// return `true`.
    pub fn match_str(&mut self, expected: &str) -> bool {
        if !self.source[self.offset..].starts_with(expected) {
            return false;
        }
        for _ in 0..expected.len() {
            self.advance();
        }
        true
    }

    pub fn expect(&mut self, expected: &str) -> Result<(), ScanError> {
        self.expect_msg(expected, &format!("Expected '{expected}'"))
    }

    pub fn expect_msg(&mut self, expected: &str, message: &str) -> Result<(), ScanError> {
        if self.match_str(expected) {
            Ok(())
        } else {
            Err(self.error(message))
        }
    }

    pub fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' {
                self.advance();
            } else {
                break;
            }
        }
    }

    pub fn skip_whitespace_and_newlines(&mut self) {
        while let Some(ch) = self.peek() {
            if ch == ' ' || ch == '\t' || ch == '\n' || ch == '\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    pub fn scan_while<F>(&mut self, mut predicate: F) -> &'s str
    where
        F: FnMut(char) -> bool,
    {
        let start = self.offset;
        while let Some(ch) = self.peek() {
            if predicate(ch) {
                self.advance();
            } else {
                break;
            }
        }
        &self.source[start..self.offset]
    }

    /// Scan a quoted string with escape handling. Assumes the
    /// scanner is positioned ON the opening quote.
    pub fn scan_string(&mut self, quote: char) -> Result<String, ScanError> {
        let q = quote.to_string();
        self.expect_msg(&q, &format!("Expected '{quote}'"))?;

        let mut out = String::new();
        while let Some(ch) = self.peek() {
            if ch == quote {
                break;
            }
            if ch == '\\' && self.offset + 1 < self.source.len() {
                self.advance();
                let esc = self.advance().unwrap_or('\0');
                match esc {
                    'n' => out.push('\n'),
                    't' => out.push('\t'),
                    'r' => out.push('\r'),
                    '\\' => out.push('\\'),
                    '0' => out.push('\0'),
                    c if c == quote => out.push(quote),
                    c => {
                        out.push('\\');
                        out.push(c);
                    }
                }
            } else {
                out.push(self.advance().unwrap_or('\0'));
            }
        }
        self.expect_msg(&q, "Unterminated string literal")?;
        Ok(out)
    }

    /// Scan a numeric literal (integer or float, optional
    /// exponent) and return the parsed `f64`.
    pub fn scan_number(&mut self) -> Result<f64, ScanError> {
        let start = self.offset;

        if self.peek() == Some('-') {
            self.advance();
        }

        if self.peek() == Some('.') {
            self.advance();
            self.scan_digits();
        } else {
            self.scan_digits();
            if self.peek() == Some('.') && self.peek_ahead(1).is_some_and(is_digit) {
                self.advance();
                self.scan_digits();
            }
        }

        if matches!(self.peek(), Some('e') | Some('E')) {
            self.advance();
            if matches!(self.peek(), Some('+') | Some('-')) {
                self.advance();
            }
            self.scan_digits();
        }

        let raw = &self.source[start..self.offset];
        raw.parse::<f64>()
            .map_err(|_| self.error(&format!("Invalid number '{raw}'")))
    }

    fn scan_digits(&mut self) {
        while let Some(ch) = self.peek() {
            if is_digit(ch) {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Scan an identifier: `[a-zA-Z_][a-zA-Z0-9_]*`. Returns the
    /// identifier substring.
    pub fn scan_identifier(&mut self) -> Result<&'s str, ScanError> {
        match self.peek() {
            Some(ch) if is_ident_start(ch) => {}
            Some(ch) => return Err(self.error(&format!("Expected identifier, got '{ch}'"))),
            None => return Err(self.error("Expected identifier, got 'EOF'")),
        }
        Ok(self.scan_while(is_ident_char))
    }

    pub fn range_from(&self, start_offset: usize) -> SourceRange {
        SourceRange {
            start: self.pos_at(start_offset),
            end: self.position(),
        }
    }

    pub fn pos_at(&self, offset: usize) -> Position {
        super::ast_types::pos_at(self.source, offset)
    }

    pub fn error(&self, message: &str) -> ScanError {
        ScanError::new(message, self.position())
    }
}

pub fn is_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

pub fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

pub fn is_ident_char(ch: char) -> bool {
    is_ident_start(ch) || is_digit(ch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scans_identifier_and_number() {
        let mut s = Scanner::new("hello + 42");
        assert_eq!(s.scan_identifier().unwrap(), "hello");
        s.skip_whitespace();
        assert!(s.match_str("+"));
        s.skip_whitespace();
        assert_eq!(s.scan_number().unwrap(), 42.0);
        assert!(s.is_at_end());
    }

    #[test]
    fn save_restore_backtracks() {
        let mut s = Scanner::new("abc");
        let state = s.save();
        s.advance();
        s.advance();
        s.restore(state);
        assert_eq!(s.offset(), 0);
    }

    #[test]
    fn scans_quoted_string_with_escapes() {
        let mut s = Scanner::new("\"hi\\nthere\"");
        assert_eq!(s.scan_string('"').unwrap(), "hi\nthere");
    }

    #[test]
    fn position_tracks_lines() {
        let mut s = Scanner::new("a\nbc");
        s.advance();
        s.advance();
        s.advance();
        let p = s.position();
        assert_eq!(p.line, 2);
        assert_eq!(p.column, 1);
    }

    #[test]
    fn scan_number_float_and_exp() {
        let mut s = Scanner::new("3.14e2");
        assert_eq!(s.scan_number().unwrap(), 314.0);
    }
}
