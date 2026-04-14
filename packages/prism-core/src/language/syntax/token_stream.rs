//! Generic typed token stream for recursive-descent parsers.
//!
//! Port of `language/syntax/token-stream.ts`. Works with any token
//! type that implements [`BaseToken`]. Language-specific
//! tokenizers supply their own `Kind` enum.

use std::fmt;

/// Minimum information a parser needs about a token: its kind,
/// raw source slice, and start position for error reporting.
///
/// Implementors typically derive `Clone` and keep the kind as a
/// small `Copy` enum so `TokenStream<T>` can cheaply advance.
pub trait BaseToken: Clone {
    type Kind: Copy + Eq + fmt::Debug;
    fn kind(&self) -> Self::Kind;
    fn raw(&self) -> &str;
    fn offset(&self) -> usize;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError {
    pub message: String,
    pub offset: usize,
    pub line: usize,
    pub column: usize,
    pub raw: String,
}

impl TokenError {
    pub fn new<T: BaseToken>(message: impl Into<String>, token: &T) -> Self {
        Self {
            message: message.into(),
            offset: token.offset(),
            line: token.line(),
            column: token.column(),
            raw: token.raw().to_string(),
        }
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} at {}:{} (got '{}')",
            self.message, self.line, self.column, self.raw
        )
    }
}

impl std::error::Error for TokenError {}

/// Cursor over a `Vec<T>` of tokens. The last token is assumed to
/// be an EOF sentinel so `peek` / `advance` are always safe.
pub struct TokenStream<T: BaseToken> {
    tokens: Vec<T>,
    pos: usize,
}

impl<T: BaseToken> TokenStream<T> {
    pub fn new(tokens: Vec<T>) -> Self {
        Self { tokens, pos: 0 }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn tokens(&self) -> &[T] {
        &self.tokens
    }

    pub fn peek(&self) -> &T {
        self.peek_ahead(0)
    }

    pub fn peek_ahead(&self, ahead: usize) -> &T {
        let idx = self.pos + ahead;
        if idx >= self.tokens.len() {
            &self.tokens[self.tokens.len() - 1]
        } else {
            &self.tokens[idx]
        }
    }

    pub fn advance(&mut self) -> T {
        let tok = self.tokens[self.pos].clone();
        if self.pos < self.tokens.len() - 1 {
            self.pos += 1;
        }
        tok
    }

    pub fn check(&self, kind: T::Kind) -> bool {
        self.peek().kind() == kind
    }

    pub fn eat(&mut self, kind: T::Kind) -> Option<T> {
        if self.check(kind) {
            Some(self.advance())
        } else {
            None
        }
    }

    pub fn expect(&mut self, kind: T::Kind) -> Result<T, TokenError> {
        self.expect_msg(kind, &format!("Expected '{kind:?}'"))
    }

    pub fn expect_msg(&mut self, kind: T::Kind, message: &str) -> Result<T, TokenError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(TokenError::new(message, self.peek()))
        }
    }

    pub fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len() - 1
    }

    pub fn save(&self) -> usize {
        self.pos
    }

    pub fn restore(&mut self, pos: usize) {
        self.pos = pos;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Kind {
        Num,
        Plus,
        Eof,
    }

    #[derive(Debug, Clone)]
    struct Tok {
        kind: Kind,
        raw: String,
        offset: usize,
    }

    impl BaseToken for Tok {
        type Kind = Kind;
        fn kind(&self) -> Kind {
            self.kind
        }
        fn raw(&self) -> &str {
            &self.raw
        }
        fn offset(&self) -> usize {
            self.offset
        }
        fn line(&self) -> usize {
            1
        }
        fn column(&self) -> usize {
            self.offset
        }
    }

    fn t(kind: Kind, raw: &str, offset: usize) -> Tok {
        Tok {
            kind,
            raw: raw.to_string(),
            offset,
        }
    }

    #[test]
    fn walks_tokens() {
        let mut s = TokenStream::new(vec![
            t(Kind::Num, "1", 0),
            t(Kind::Plus, "+", 1),
            t(Kind::Num, "2", 2),
            t(Kind::Eof, "", 3),
        ]);
        assert!(s.check(Kind::Num));
        s.advance();
        assert!(s.check(Kind::Plus));
        s.advance();
        assert_eq!(s.expect(Kind::Num).unwrap().raw, "2");
        assert!(s.is_at_end());
    }

    #[test]
    fn eat_returns_none_on_mismatch() {
        let mut s = TokenStream::new(vec![t(Kind::Num, "1", 0), t(Kind::Eof, "", 1)]);
        assert!(s.eat(Kind::Plus).is_none());
        assert!(s.eat(Kind::Num).is_some());
    }

    #[test]
    fn save_restore() {
        let mut s = TokenStream::new(vec![
            t(Kind::Num, "1", 0),
            t(Kind::Plus, "+", 1),
            t(Kind::Eof, "", 2),
        ]);
        let p = s.save();
        s.advance();
        s.restore(p);
        assert!(s.check(Kind::Num));
    }
}
