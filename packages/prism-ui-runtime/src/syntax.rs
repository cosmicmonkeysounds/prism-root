//! Syntax highlighting — language-aware tokenizers that emit
//! [`TextSpan`] ranges the renderer can paint in distinct colours.
//!
//! Three languages ship: `luau`, `rust`, `javascript` (a.k.a.
//! `js` / `typescript` / `ts`). The same dispatch function backs
//! every language; switching is a `match` on the language tag at
//! the top, and the per-language work is a tight state machine
//! over the byte stream — no regex, no allocator pressure beyond
//! the output `Vec<TextSpan>`.
//!
//! ## Token vocabulary
//!
//! Eight kinds map to the palette below. Adding a kind is one
//! enum variant + one row in [`Palette::default_colour`]. The
//! palette is fixed at the runtime layer (the same colours work
//! in every code editor instance); user-theming is a follow-up
//! that plugs in a custom [`Palette`] at the call site.
//!
//! * **Keyword** — `function`, `local`, `if`, `return`, …
//! * **String** — `"…"`, `'…'`, `[[…]]` (Luau long strings).
//! * **Number** — `42`, `3.14`, `0xff`.
//! * **Comment** — `-- …`, `// …`, `/* … */`, `--[[ … ]]`.
//! * **Operator** — `+ - * / = == ~= < > <= >= and or not`.
//! * **Punctuation** — `( ) { } [ ] , ; :`.
//! * **Identifier** — anything else that's alphanumeric.
//! * **Type** — language-specific (uppercase first letter for
//!   Rust types; reserved tokens like `nil` / `true` / `false`
//!   that aren't quite keywords for Luau).

use crate::command::{Color, TextSpan};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Keyword,
    String,
    Number,
    Comment,
    Operator,
    Punctuation,
    Identifier,
    Type,
}

/// Colour palette for the eight token kinds. The default palette is
/// the One Dark-style tinted set the legacy Slint editor used —
/// it reads well against the editor's white background while
/// staying distinct from selection / caret accents.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub keyword: Color,
    pub string: Color,
    pub number: Color,
    pub comment: Color,
    pub operator: Color,
    pub punctuation: Color,
    pub identifier: Color,
    pub type_: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            keyword: Color {
                r: 0xc6,
                g: 0x78,
                b: 0xdd,
                a: 0xff,
            },
            string: Color {
                r: 0x49,
                g: 0x8a,
                b: 0x35,
                a: 0xff,
            },
            number: Color {
                r: 0xd1,
                g: 0x9a,
                b: 0x66,
                a: 0xff,
            },
            comment: Color {
                r: 0x80,
                g: 0x86,
                b: 0x90,
                a: 0xff,
            },
            operator: Color {
                r: 0x2f,
                g: 0x5c,
                b: 0xa8,
                a: 0xff,
            },
            punctuation: Color {
                r: 0x53,
                g: 0x59,
                b: 0x60,
                a: 0xff,
            },
            // `identifier` is `None` for plain identifiers — they
            // inherit the command's base text colour so themed
            // backgrounds (light / dark) don't need a palette
            // override per theme.
            identifier: Color {
                r: 0x10,
                g: 0x12,
                b: 0x18,
                a: 0xff,
            },
            type_: Color {
                r: 0xe0,
                g: 0x6c,
                b: 0x75,
                a: 0xff,
            },
        }
    }
}

impl Palette {
    fn colour_for(&self, kind: TokenKind) -> Color {
        match kind {
            TokenKind::Keyword => self.keyword,
            TokenKind::String => self.string,
            TokenKind::Number => self.number,
            TokenKind::Comment => self.comment,
            TokenKind::Operator => self.operator,
            TokenKind::Punctuation => self.punctuation,
            TokenKind::Identifier => self.identifier,
            TokenKind::Type => self.type_,
        }
    }
}

/// Highlight `source` for `language` against the default palette.
/// Returns a sorted, non-overlapping list of [`TextSpan`]s the paint
/// pass can use directly. Unknown languages return an empty span
/// list (the editor paints in the command's base colour).
///
/// Memoised through a single-slot per-thread cache keyed by
/// `(text_hash, language)`. Idle redraws of the same buffer
/// (caret blink, scroll, hover) hit the cache; mutations bust it
/// and re-tokenize on the next call. Single slot rather than an
/// LRU because real workloads only edit one buffer at a time —
/// the eviction churn of a multi-slot LRU would defeat the
/// hit-rate at the call sites that actually matter.
pub fn highlight(source: &str, language: &str) -> Vec<TextSpan> {
    use std::cell::RefCell;
    use std::hash::{Hash, Hasher};
    thread_local! {
        static CACHE: RefCell<Option<(u64, String, Vec<TextSpan>)>> = const { RefCell::new(None) };
    }
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    source.hash(&mut hasher);
    let key = hasher.finish();
    let hit: Option<Vec<TextSpan>> = CACHE.with(|c| {
        let borrow = c.borrow();
        borrow.as_ref().and_then(|(k, lang, spans)| {
            if *k == key && lang == language {
                Some(spans.clone())
            } else {
                None
            }
        })
    });
    if let Some(spans) = hit {
        return spans;
    }
    let spans = highlight_with_palette(source, language, &Palette::default());
    CACHE.with(|c| {
        *c.borrow_mut() = Some((key, language.to_string(), spans.clone()));
    });
    spans
}

/// Variant of [`highlight`] that accepts a custom palette — used by
/// tests and any host that wants theme-specific colours.
pub fn highlight_with_palette(source: &str, language: &str, palette: &Palette) -> Vec<TextSpan> {
    let tokens = match normalise_language(language) {
        Language::Luau => tokenize_luau(source),
        Language::Rust => tokenize_rust(source),
        Language::JavaScript => tokenize_js(source),
        Language::Unknown => return Vec::new(),
    };
    tokens
        .into_iter()
        .map(|(kind, start, end)| TextSpan {
            start_byte: start,
            end_byte: end,
            color: palette.colour_for(kind),
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Language {
    Luau,
    Rust,
    JavaScript,
    Unknown,
}

fn normalise_language(s: &str) -> Language {
    match s.trim().to_ascii_lowercase().as_str() {
        "luau" | "lua" => Language::Luau,
        "rust" | "rs" => Language::Rust,
        "javascript" | "js" | "typescript" | "ts" | "jsx" | "tsx" => Language::JavaScript,
        _ => Language::Unknown,
    }
}

const LUAU_KEYWORDS: &[&str] = &[
    "and", "break", "continue", "do", "else", "elseif", "end", "for", "function", "if", "in",
    "local", "not", "or", "repeat", "return", "then", "until", "while",
];

const LUAU_TYPES: &[&str] = &["nil", "true", "false", "self"];

fn tokenize_luau(source: &str) -> Vec<(TokenKind, usize, usize)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Block comment `--[[ … ]]` — matched before line comment.
        if b == b'-' && i + 3 < bytes.len() && &bytes[i..i + 4] == b"--[[" {
            let start = i;
            i += 4;
            while i + 1 < bytes.len() && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        // Line comment `-- …`
        if b == b'-' && i + 1 < bytes.len() && bytes[i + 1] == b'-' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        // String — both single + double quotes, with \-escape.
        if b == b'"' || b == b'\'' {
            let q = b;
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' {
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            out.push((TokenKind::String, start, i));
            continue;
        }
        // Long string `[[ … ]]` — Luau-only.
        if b == b'[' && i + 1 < bytes.len() && bytes[i + 1] == b'[' {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b']' && bytes[i + 1] == b']') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push((TokenKind::String, start, i));
            continue;
        }
        // Number — integer / float / hex.
        if b.is_ascii_digit() {
            let start = i;
            if b == b'0' && i + 1 < bytes.len() && (bytes[i + 1] == b'x' || bytes[i + 1] == b'X') {
                i += 2;
                while i < bytes.len() && bytes[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                    i += 1;
                }
            }
            out.push((TokenKind::Number, start, i));
            continue;
        }
        // Identifier / keyword / type.
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &source[start..i];
            let kind = if LUAU_KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else if LUAU_TYPES.contains(&word) {
                TokenKind::Type
            } else {
                TokenKind::Identifier
            };
            out.push((kind, start, i));
            continue;
        }
        // Operator / punctuation.
        if let Some(span) = match_operator(bytes, i) {
            out.push((TokenKind::Operator, i, i + span));
            i += span;
            continue;
        }
        if is_punctuation(b) {
            out.push((TokenKind::Punctuation, i, i + 1));
            i += 1;
            continue;
        }
        // Anything else (whitespace, unknown) — skip without a span.
        i += 1;
    }
    out
}

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "type", "unsafe", "use",
    "where", "while", "yield",
];

const RUST_TYPE_LITERALS: &[&str] = &["true", "false", "None", "Some", "Ok", "Err"];

fn tokenize_rust(source: &str) -> Vec<(TokenKind, usize, usize)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        // Block comment `/* … */`.
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        // Line comment `// …`.
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        // String — `"…"` with \-escape. Rust also has `r"…"` raw
        // strings; lex them as a normal string here (the colouring
        // ends up the same).
        if b == b'"' {
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != b'"' {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            out.push((TokenKind::String, start, i));
            continue;
        }
        // Char literal — `'x'` or `'\n'`. Lifetime annotations
        // (`'a`) look the same; if the third byte isn't a closing
        // quote we treat the run as a lifetime / identifier.
        if b == b'\'' && i + 2 < bytes.len() {
            let close = if bytes[i + 1] == b'\\' { i + 3 } else { i + 2 };
            if close < bytes.len() && bytes[close] == b'\'' {
                out.push((TokenKind::String, i, close + 1));
                i = close + 1;
                continue;
            }
        }
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.' || bytes[i] == b'_')
            {
                i += 1;
            }
            out.push((TokenKind::Number, start, i));
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                i += 1;
            }
            let word = &source[start..i];
            let kind = if RUST_KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else if RUST_TYPE_LITERALS.contains(&word)
                || word.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            {
                TokenKind::Type
            } else {
                TokenKind::Identifier
            };
            out.push((kind, start, i));
            continue;
        }
        if let Some(span) = match_operator(bytes, i) {
            out.push((TokenKind::Operator, i, i + span));
            i += span;
            continue;
        }
        if is_punctuation(b) {
            out.push((TokenKind::Punctuation, i, i + 1));
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

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
    "finally",
    "for",
    "function",
    "if",
    "import",
    "in",
    "instanceof",
    "let",
    "new",
    "of",
    "return",
    "super",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "interface",
    "type",
    "enum",
    "implements",
    "private",
    "protected",
    "public",
    "readonly",
    "static",
];

const JS_TYPE_LITERALS: &[&str] = &["true", "false", "null", "undefined", "NaN", "Infinity"];

fn tokenize_js(source: &str) -> Vec<(TokenKind, usize, usize)> {
    let bytes = source.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'*' {
            let start = i;
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            let start = i;
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            out.push((TokenKind::Comment, start, i));
            continue;
        }
        if b == b'"' || b == b'\'' || b == b'`' {
            let q = b;
            let start = i;
            i += 1;
            while i < bytes.len() && bytes[i] != q {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 2;
                    continue;
                }
                if bytes[i] == b'\n' && q != b'`' {
                    break;
                }
                i += 1;
            }
            i = (i + 1).min(bytes.len());
            out.push((TokenKind::String, start, i));
            continue;
        }
        if b.is_ascii_digit() {
            let start = i;
            while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'.') {
                i += 1;
            }
            out.push((TokenKind::Number, start, i));
            continue;
        }
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            let word = &source[start..i];
            let kind = if JS_KEYWORDS.contains(&word) {
                TokenKind::Keyword
            } else if JS_TYPE_LITERALS.contains(&word) {
                TokenKind::Type
            } else {
                TokenKind::Identifier
            };
            out.push((kind, start, i));
            continue;
        }
        if let Some(span) = match_operator(bytes, i) {
            out.push((TokenKind::Operator, i, i + span));
            i += span;
            continue;
        }
        if is_punctuation(b) {
            out.push((TokenKind::Punctuation, i, i + 1));
            i += 1;
            continue;
        }
        i += 1;
    }
    out
}

fn match_operator(bytes: &[u8], i: usize) -> Option<usize> {
    // Try the three-byte operators first, then two, then one.
    const THREE: &[&[u8]] = &[b"===", b"!==", b"..."];
    const TWO: &[&[u8]] = &[
        b"==", b"!=", b"<=", b">=", b"&&", b"||", b"->", b"=>", b"::", b"..", b"++", b"--", b"+=",
        b"-=", b"*=", b"/=", b"%=", b"^=", b"&=", b"|=", b"<<", b">>", b"~=",
    ];
    const ONE: &[u8] = b"+-*/%=<>!&|^~?";
    for op in THREE {
        if i + op.len() <= bytes.len() && &bytes[i..i + op.len()] == *op {
            return Some(op.len());
        }
    }
    for op in TWO {
        if i + op.len() <= bytes.len() && &bytes[i..i + op.len()] == *op {
            return Some(op.len());
        }
    }
    if ONE.contains(&bytes[i]) {
        return Some(1);
    }
    None
}

fn is_punctuation(b: u8) -> bool {
    matches!(
        b,
        b'(' | b')' | b'{' | b'}' | b'[' | b']' | b',' | b';' | b':' | b'.'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn highlight_kinds<'a>(source: &'a str, language: &str) -> Vec<(TokenKind, &'a str)> {
        let palette = Palette::default();
        let tokens = match normalise_language(language) {
            Language::Luau => tokenize_luau(source),
            Language::Rust => tokenize_rust(source),
            Language::JavaScript => tokenize_js(source),
            Language::Unknown => Vec::new(),
        };
        let _ = palette;
        tokens
            .into_iter()
            .map(|(k, s, e)| (k, &source[s..e]))
            .collect()
    }

    #[test]
    fn luau_keyword_string_number() {
        let out = highlight_kinds("local x = 42", "luau");
        assert_eq!(out.len(), 4);
        assert_eq!(out[0].0, TokenKind::Keyword);
        assert_eq!(out[0].1, "local");
        assert_eq!(out[1].0, TokenKind::Identifier);
        assert_eq!(out[2].0, TokenKind::Operator);
        assert_eq!(out[3].0, TokenKind::Number);
        assert_eq!(out[3].1, "42");
    }

    #[test]
    fn luau_string_literal() {
        let out = highlight_kinds("print(\"hi\")", "luau");
        let kinds: Vec<_> = out.iter().map(|(k, _)| *k).collect();
        assert!(kinds.contains(&TokenKind::String));
    }

    #[test]
    fn luau_line_comment_covers_full_line() {
        let src = "-- a comment\nlocal x = 1";
        let out = highlight_kinds(src, "luau");
        assert_eq!(out[0].0, TokenKind::Comment);
        assert_eq!(out[0].1, "-- a comment");
    }

    #[test]
    fn luau_block_comment() {
        let src = "--[[ block ]] local x = 1";
        let out = highlight_kinds(src, "luau");
        assert_eq!(out[0].0, TokenKind::Comment);
        assert_eq!(out[0].1, "--[[ block ]]");
    }

    #[test]
    fn rust_keyword_and_type() {
        let out = highlight_kinds("fn foo() -> String { String::new() }", "rust");
        let kinds: Vec<_> = out.iter().map(|(k, w)| (*k, *w)).collect();
        assert!(kinds.contains(&(TokenKind::Keyword, "fn")));
        assert!(kinds.contains(&(TokenKind::Type, "String")));
    }

    #[test]
    fn rust_block_comment() {
        let src = "/* hi */ let x = 1;";
        let out = highlight_kinds(src, "rust");
        assert_eq!(out[0].0, TokenKind::Comment);
        assert_eq!(out[0].1, "/* hi */");
    }

    #[test]
    fn js_template_literal() {
        let src = "const x = `hello`;";
        let out = highlight_kinds(src, "js");
        let kinds: Vec<_> = out.iter().map(|(k, _)| *k).collect();
        assert!(kinds.contains(&TokenKind::String));
        assert!(kinds.contains(&TokenKind::Keyword)); // `const`
    }

    #[test]
    fn unknown_language_emits_no_spans() {
        let spans = highlight("hello world", "klingon");
        assert!(spans.is_empty());
    }

    /// Cache hit verification: the *cached* return value of
    /// `highlight` is byte-identical to a fresh tokenization for the
    /// same input. Doubles as a regression check that a stale cache
    /// can't silently leak spans from a previous call.
    #[test]
    fn highlight_cache_returns_consistent_spans() {
        let src = "local function f() return 42 end";
        let first = highlight(src, "luau");
        let second = highlight(src, "luau");
        assert_eq!(first, second);
        // Switching the language busts the cache and re-runs against
        // the other tokenizer.
        let rust = highlight(src, "rust");
        // Same source, different language → may differ (it does:
        // "local" is a Luau keyword, not a Rust one). Confirms cache
        // is *language-aware*, not just text-aware.
        assert_ne!(first, rust);
    }

    #[test]
    fn highlight_returns_sorted_non_overlapping_spans() {
        let spans = highlight("local a = 1", "luau");
        for win in spans.windows(2) {
            assert!(win[0].end_byte <= win[1].start_byte);
        }
    }
}
