use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenKind {
    Keyword,
    String,
    Comment,
    Number,
    Operator,
    Punctuation,
    Identifier,
    Whitespace,
    Plain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub kind: TokenKind,
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "self", "Self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while", "yield",
];

const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

const JS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "debugger",
    "default",
    "delete",
    "do",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "null",
    "of",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "typeof",
    "undefined",
    "var",
    "void",
    "while",
    "with",
    "yield",
];

fn keywords_for(lang: &str) -> &'static [&'static str] {
    match lang {
        "rust" | "rs" => RUST_KEYWORDS,
        "lua" | "luau" => LUA_KEYWORDS,
        "javascript" | "js" | "typescript" | "ts" => JS_KEYWORDS,
        _ => &[],
    }
}

fn comment_prefix(lang: &str) -> &'static str {
    match lang {
        "lua" | "luau" => "--",
        _ => "//",
    }
}

pub fn highlight_line(line: &str, language: &str) -> Vec<Token> {
    let keywords = keywords_for(language);
    let cpfx = comment_prefix(language);
    let bytes = line.as_bytes();
    let len = bytes.len();
    let mut tokens = Vec::new();
    let mut i = 0;

    while i < len {
        if line[i..].starts_with(cpfx) {
            tokens.push(Token {
                text: line[i..].to_string(),
                kind: TokenKind::Comment,
            });
            return tokens;
        }

        let b = bytes[i];

        if b == b' ' || b == b'\t' {
            let start = i;
            while i < len && (bytes[i] == b' ' || bytes[i] == b'\t') {
                i += 1;
            }
            tokens.push(Token {
                text: line[start..i].to_string(),
                kind: TokenKind::Whitespace,
            });
            continue;
        }

        if b == b'"' || b == b'\'' || b == b'`' {
            let quote = b;
            let start = i;
            i += 1;
            while i < len && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < len {
                    i += 1;
                }
                i += 1;
            }
            if i < len {
                i += 1;
            }
            tokens.push(Token {
                text: line[start..i].to_string(),
                kind: TokenKind::String,
            });
            continue;
        }

        if b.is_ascii_digit() {
            let start = i;
            if b == b'0' && i + 1 < len && (bytes[i + 1] == b'x' || bytes[i + 1] == b'b') {
                i += 2;
            }
            while i < len
                && (bytes[i].is_ascii_digit()
                    || bytes[i] == b'.'
                    || bytes[i] == b'_'
                    || bytes[i].is_ascii_hexdigit())
            {
                i += 1;
            }
            while i < len && bytes[i].is_ascii_alphanumeric() {
                i += 1;
            }
            tokens.push(Token {
                text: line[start..i].to_string(),
                kind: TokenKind::Number,
            });
            continue;
        }

        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < len && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &line[start..i];
            let kind = if keywords.contains(&word) {
                TokenKind::Keyword
            } else {
                TokenKind::Identifier
            };
            tokens.push(Token {
                text: word.to_string(),
                kind,
            });
            continue;
        }

        if b"+-*/%=<>!&|^~?".contains(&b) {
            let start = i;
            while i < len && b"+-*/%=<>!&|^~?".contains(&bytes[i]) {
                i += 1;
            }
            tokens.push(Token {
                text: line[start..i].to_string(),
                kind: TokenKind::Operator,
            });
            continue;
        }

        if b"(){}[];:,.#@$\\".contains(&b) {
            tokens.push(Token {
                text: line[i..i + 1].to_string(),
                kind: TokenKind::Punctuation,
            });
            i += 1;
            continue;
        }

        let ch = line[i..].chars().next().unwrap();
        let clen = ch.len_utf8();
        tokens.push(Token {
            text: line[i..i + clen].to_string(),
            kind: TokenKind::Plain,
        });
        i += clen;
    }

    if tokens.is_empty() {
        tokens.push(Token {
            text: String::new(),
            kind: TokenKind::Plain,
        });
    }

    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_fn_declaration() {
        let tokens = highlight_line("fn main() {", "rust");
        assert_eq!(tokens[0].kind, TokenKind::Keyword);
        assert_eq!(tokens[0].text, "fn");
        assert_eq!(tokens[1].kind, TokenKind::Whitespace);
        assert_eq!(tokens[2].kind, TokenKind::Identifier);
        assert_eq!(tokens[2].text, "main");
        assert_eq!(tokens[3].kind, TokenKind::Punctuation);
        assert_eq!(tokens[3].text, "(");
    }

    #[test]
    fn rust_let_binding() {
        let tokens = highlight_line("    let x = 42;", "rust");
        assert_eq!(tokens[0].kind, TokenKind::Whitespace);
        assert_eq!(tokens[1].kind, TokenKind::Keyword);
        assert_eq!(tokens[1].text, "let");
        assert_eq!(tokens[3].kind, TokenKind::Identifier);
        assert_eq!(tokens[3].text, "x");
        assert_eq!(tokens[5].kind, TokenKind::Operator);
        assert_eq!(tokens[5].text, "=");
        assert_eq!(tokens[7].kind, TokenKind::Number);
        assert_eq!(tokens[7].text, "42");
    }

    #[test]
    fn string_literal() {
        let tokens = highlight_line("let s = \"hello world\";", "rust");
        let str_tok = tokens.iter().find(|t| t.kind == TokenKind::String).unwrap();
        assert_eq!(str_tok.text, "\"hello world\"");
    }

    #[test]
    fn escaped_string() {
        let tokens = highlight_line(r#"let s = "he\"llo";"#, "rust");
        let str_tok = tokens.iter().find(|t| t.kind == TokenKind::String).unwrap();
        assert_eq!(str_tok.text, r#""he\"llo""#);
    }

    #[test]
    fn line_comment() {
        let tokens = highlight_line("// this is a comment", "rust");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Comment);
        assert_eq!(tokens[0].text, "// this is a comment");
    }

    #[test]
    fn lua_comment() {
        let tokens = highlight_line("-- lua comment", "lua");
        assert_eq!(tokens[0].kind, TokenKind::Comment);
    }

    #[test]
    fn code_then_comment() {
        let tokens = highlight_line("let x = 1; // assign", "rust");
        let comment = tokens.last().unwrap();
        assert_eq!(comment.kind, TokenKind::Comment);
        assert_eq!(comment.text, "// assign");
    }

    #[test]
    fn hex_number() {
        let tokens = highlight_line("0xff00", "rust");
        assert_eq!(tokens[0].kind, TokenKind::Number);
        assert_eq!(tokens[0].text, "0xff00");
    }

    #[test]
    fn empty_line() {
        let tokens = highlight_line("", "rust");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].kind, TokenKind::Plain);
        assert_eq!(tokens[0].text, "");
    }

    #[test]
    fn js_keywords() {
        let tokens = highlight_line("const x = function() {}", "js");
        assert_eq!(tokens[0].kind, TokenKind::Keyword);
        assert_eq!(tokens[0].text, "const");
        let func = tokens.iter().find(|t| t.text == "function").unwrap();
        assert_eq!(func.kind, TokenKind::Keyword);
    }

    #[test]
    fn operators_grouped() {
        let tokens = highlight_line("a >= b && c != d", "rust");
        let ops: Vec<&Token> = tokens
            .iter()
            .filter(|t| t.kind == TokenKind::Operator)
            .collect();
        assert_eq!(ops[0].text, ">=");
        assert_eq!(ops[1].text, "&&");
        assert_eq!(ops[2].text, "!=");
    }

    #[test]
    fn unknown_language_no_keywords() {
        let tokens = highlight_line("fn let const", "txt");
        assert!(tokens.iter().all(|t| t.kind != TokenKind::Keyword));
    }
}
