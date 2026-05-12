//! Expression tokenizer.
//!
//! Port of `language/expression/scanner.ts`. Byte-oriented: every
//! significant character in the expression grammar is ASCII, so
//! byte indices round-trip through the legacy JSON offsets the
//! parser stores in [`ExprError::offset`].

use super::super::syntax::scanner::{is_digit, is_ident_char, is_ident_start};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TokenKind {
    Number,
    String,
    Bool,
    Operand,
    Ident,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Percent,
    Eq,
    Neq,
    Lt,
    Lte,
    Gt,
    Gte,
    And,
    Or,
    Not,
    LParen,
    RParen,
    Comma,
    /// `.` — field-access separator in dotted-path operands
    /// (`item.label`, `tabs.0.name`).
    Dot,
    /// `?` — ternary head (`cond ? a : b`).
    Question,
    /// `:` — ternary middle and `else` branch separator.
    Colon,
    Eof,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperandData {
    pub operand_type: String,
    pub id: String,
    pub subfield: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub raw: String,
    pub offset: usize,
    pub number_value: Option<f64>,
    pub string_value: Option<String>,
    pub bool_value: Option<bool>,
    pub operand_data: Option<OperandData>,
}

impl Token {
    fn plain(kind: TokenKind, raw: impl Into<String>, offset: usize) -> Self {
        Self {
            kind,
            raw: raw.into(),
            offset,
            number_value: None,
            string_value: None,
            bool_value: None,
            operand_data: None,
        }
    }
}

pub fn tokenize(source: &str) -> Vec<Token> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0usize;

    while i < bytes.len() {
        let ch = bytes[i] as char;

        if matches!(ch, ' ' | '\t' | '\n' | '\r') {
            i += 1;
            continue;
        }

        let start = i;

        // Operand: [type:id] or [type:id.subfield]
        if ch == '[' {
            if let Some(close_rel) = source[i + 1..].find(']') {
                let close = i + 1 + close_rel;
                let inner = &source[i + 1..close];
                if let Some(colon_idx) = inner.find(':') {
                    let operand_type = inner[..colon_idx].trim().to_string();
                    let rest = inner[colon_idx + 1..].trim();
                    let (id, subfield) = match rest.find('.') {
                        Some(dot) => (rest[..dot].to_string(), Some(rest[dot + 1..].to_string())),
                        None => (rest.to_string(), None),
                    };
                    i = close + 1;
                    let raw = source[start..i].to_string();
                    tokens.push(Token {
                        kind: TokenKind::Operand,
                        raw,
                        offset: start,
                        number_value: None,
                        string_value: None,
                        bool_value: None,
                        operand_data: Some(OperandData {
                            operand_type,
                            id,
                            subfield,
                        }),
                    });
                    continue;
                }
            }
        }

        // Number: digits or leading '.' followed by digit. The
        // leading-'.' form is suppressed when the previous token is an
        // expression terminal (Ident / Number / RParen / Operand) so
        // dotted-path operands like `tabs.0.name` tokenize as
        // `Ident Dot Number Dot Ident` instead of swallowing the `.0`
        // segment into a `.0` float. `.5` at the start of a primary
        // (after no token, an operator, an opener, etc.) still parses
        // as the numeric literal `0.5`.
        let is_path_dot = ch == '.'
            && matches!(
                tokens.last().map(|t| t.kind),
                Some(TokenKind::Ident | TokenKind::Number | TokenKind::RParen | TokenKind::Operand)
            );
        if !is_path_dot
            && (is_digit(ch)
                || (ch == '.' && i + 1 < bytes.len() && is_digit(bytes[i + 1] as char)))
        {
            let mut num = String::new();
            while i < bytes.len() && is_digit(bytes[i] as char) {
                num.push(bytes[i] as char);
                i += 1;
            }
            // Fractional part: only consume '.' when followed by a
            // digit, so `0.name` tokenizes as `Number(0) Dot Ident`
            // rather than `Number("0.") Ident` (which would lose the
            // dot needed to chain dotted-path operands).
            if i + 1 < bytes.len() && bytes[i] as char == '.' && is_digit(bytes[i + 1] as char) {
                num.push('.');
                i += 1;
                while i < bytes.len() && is_digit(bytes[i] as char) {
                    num.push(bytes[i] as char);
                    i += 1;
                }
            }
            let value = num.parse::<f64>().unwrap_or(0.0);
            tokens.push(Token {
                kind: TokenKind::Number,
                raw: num,
                offset: start,
                number_value: Some(value),
                string_value: None,
                bool_value: None,
                operand_data: None,
            });
            continue;
        }

        // String: '...' or "..." with simple escapes
        if ch == '"' || ch == '\'' {
            let quote = ch;
            i += 1;
            let mut str_val = String::new();
            while i < bytes.len() && bytes[i] as char != quote {
                if bytes[i] as char == '\\' && i + 1 < bytes.len() {
                    i += 1;
                    let esc = bytes[i] as char;
                    match esc {
                        'n' => str_val.push('\n'),
                        't' => str_val.push('\t'),
                        'r' => str_val.push('\r'),
                        '\\' => str_val.push('\\'),
                        c if c == quote => str_val.push(quote),
                        c => str_val.push(c),
                    }
                } else {
                    str_val.push(bytes[i] as char);
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // skip closing quote
            }
            let raw = source[start..i].to_string();
            tokens.push(Token {
                kind: TokenKind::String,
                raw,
                offset: start,
                number_value: None,
                string_value: Some(str_val),
                bool_value: None,
                operand_data: None,
            });
            continue;
        }

        // Identifier or keyword
        if is_ident_start(ch) {
            let id_start = i;
            while i < bytes.len() && is_ident_char(bytes[i] as char) {
                i += 1;
            }
            let ident = source[id_start..i].to_string();
            let lower = ident.to_ascii_lowercase();
            let token = if lower == "true" || lower == "false" {
                Token {
                    kind: TokenKind::Bool,
                    raw: ident,
                    offset: start,
                    number_value: None,
                    string_value: None,
                    bool_value: Some(lower == "true"),
                    operand_data: None,
                }
            } else if lower == "and" {
                Token::plain(TokenKind::And, ident, start)
            } else if lower == "or" {
                Token::plain(TokenKind::Or, ident, start)
            } else if lower == "not" {
                Token::plain(TokenKind::Not, ident, start)
            } else {
                Token::plain(TokenKind::Ident, ident, start)
            };
            tokens.push(token);
            continue;
        }

        // Two-char operators
        if i + 1 < bytes.len() {
            let two = &source[i..i + 2];
            if let Some(kind) = match two {
                "==" => Some(TokenKind::Eq),
                "!=" => Some(TokenKind::Neq),
                "<=" => Some(TokenKind::Lte),
                ">=" => Some(TokenKind::Gte),
                "&&" => Some(TokenKind::And),
                "||" => Some(TokenKind::Or),
                _ => None,
            } {
                tokens.push(Token::plain(kind, two, start));
                i += 2;
                continue;
            }
        }

        // Single-char operators. `!` (without trailing `=` — that was
        // already consumed as `!=` above) is the C-style logical
        // negation, equivalent to the `not` keyword. `.` `?` `:` round
        // out the dotted-path / ternary grammar (Wave 11.2 substrate).
        if let Some(kind) = match ch {
            '+' => Some(TokenKind::Plus),
            '-' => Some(TokenKind::Minus),
            '*' => Some(TokenKind::Star),
            '/' => Some(TokenKind::Slash),
            '^' => Some(TokenKind::Caret),
            '%' => Some(TokenKind::Percent),
            '<' => Some(TokenKind::Lt),
            '>' => Some(TokenKind::Gt),
            '(' => Some(TokenKind::LParen),
            ')' => Some(TokenKind::RParen),
            ',' => Some(TokenKind::Comma),
            '!' => Some(TokenKind::Not),
            '.' => Some(TokenKind::Dot),
            '?' => Some(TokenKind::Question),
            ':' => Some(TokenKind::Colon),
            _ => None,
        } {
            tokens.push(Token::plain(kind, ch.to_string(), start));
            i += 1;
            continue;
        }

        tokens.push(Token::plain(TokenKind::Unknown, ch.to_string(), start));
        i += 1;
    }

    tokens.push(Token::plain(TokenKind::Eof, "", i));
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(tokens: &[Token]) -> Vec<TokenKind> {
        tokens.iter().map(|t| t.kind).collect()
    }

    #[test]
    fn tokenizes_arithmetic() {
        let toks = tokenize("1 + 2 * 3");
        assert_eq!(
            kinds(&toks),
            vec![
                TokenKind::Number,
                TokenKind::Plus,
                TokenKind::Number,
                TokenKind::Star,
                TokenKind::Number,
                TokenKind::Eof,
            ]
        );
        assert_eq!(toks[0].number_value, Some(1.0));
        assert_eq!(toks[4].number_value, Some(3.0));
    }

    #[test]
    fn tokenizes_operand_with_subfield() {
        let toks = tokenize("[field:foo.bar]");
        assert_eq!(toks[0].kind, TokenKind::Operand);
        let data = toks[0].operand_data.as_ref().unwrap();
        assert_eq!(data.operand_type, "field");
        assert_eq!(data.id, "foo");
        assert_eq!(data.subfield.as_deref(), Some("bar"));
    }

    #[test]
    fn tokenizes_keywords_case_insensitive() {
        let toks = tokenize("TRUE AND False");
        assert_eq!(toks[0].kind, TokenKind::Bool);
        assert_eq!(toks[0].bool_value, Some(true));
        assert_eq!(toks[1].kind, TokenKind::And);
        assert_eq!(toks[2].kind, TokenKind::Bool);
        assert_eq!(toks[2].bool_value, Some(false));
    }

    #[test]
    fn tokenizes_string_with_escapes() {
        let toks = tokenize("\"hi\\nthere\"");
        assert_eq!(toks[0].kind, TokenKind::String);
        assert_eq!(toks[0].string_value.as_deref(), Some("hi\nthere"));
    }

    #[test]
    fn tokenizes_two_char_ops() {
        let toks = tokenize("a == b != c <= d >= e");
        assert_eq!(toks[1].kind, TokenKind::Eq);
        assert_eq!(toks[3].kind, TokenKind::Neq);
        assert_eq!(toks[5].kind, TokenKind::Lte);
        assert_eq!(toks[7].kind, TokenKind::Gte);
    }

    #[test]
    fn tokenizes_c_style_logical_operators() {
        let toks = tokenize("a && b || !c");
        // Ident And Ident Or Not Ident Eof — six meaningful + Eof
        assert_eq!(toks[1].kind, TokenKind::And);
        assert_eq!(toks[3].kind, TokenKind::Or);
        assert_eq!(toks[4].kind, TokenKind::Not);
        assert_eq!(toks[5].kind, TokenKind::Ident);
    }

    #[test]
    fn tokenizes_ternary_and_dot_path() {
        let toks = tokenize("item.label ? 'a' : 'b'");
        assert_eq!(toks[0].kind, TokenKind::Ident);
        assert_eq!(toks[1].kind, TokenKind::Dot);
        assert_eq!(toks[2].kind, TokenKind::Ident);
        assert_eq!(toks[3].kind, TokenKind::Question);
        assert_eq!(toks[4].kind, TokenKind::String);
        assert_eq!(toks[5].kind, TokenKind::Colon);
        assert_eq!(toks[6].kind, TokenKind::String);
    }
}
