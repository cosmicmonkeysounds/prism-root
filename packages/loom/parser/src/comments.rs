//! C-style comment pre-pass (spec §6.1).
//!
//! Runs over raw `.loom` source before line classification and rewrites
//! every comment region to ASCII spaces (newlines preserved). Because
//! byte offsets and line numbers are kept intact, every span produced
//! by the downstream lexer / parser still points at the right slice of
//! the *original* source.
//!
//! Two flavours:
//!
//! * `// …` to end of line.
//! * `/* … */`, may span any number of lines.
//!
//! The opener glyph (`//` or `/*`) is only recognised when it appears
//! at the start of the line or is preceded by whitespace. This keeps
//! `https://example.com` in dialogue intact — the `/` there is
//! preceded by `:`, not whitespace.
//!
//! Comments are **not** recognised inside a `` ``` … ``` `` production-
//! metadata fence (spec §15) — the fence is its own out-of-band region
//! and a stage manager's note may legitimately contain `//`.
//!
//! An unterminated `/*` produces a single
//! [`Code::L1007UnterminatedBlockComment`](crate::diagnostics::Code)
//! diagnostic spanning from the opener to end of input. The rest of
//! the file is treated as comment text and quietly dropped — the
//! diagnostic is the loud part.

use crate::diagnostics::{Code, Diagnostic};
use crate::source::{Position, Span};

/// Strip comments out of `source`. Returns the rewritten source plus
/// any diagnostics raised.
pub fn strip(source: &str) -> (String, Vec<Diagnostic>) {
    let mut diagnostics = Vec::new();
    let mut out: Vec<u8> = Vec::with_capacity(source.len());
    let mut in_block: Option<(u32, u32, u32)> = None; // (line, col, byte) of the opener
    let mut in_fence = false;
    let mut line_idx: u32 = 0;
    let mut line_start_byte: u32 = 0;

    for raw_line in source.split_inclusive('\n') {
        // Fence handling — toggle on lines whose first non-whitespace
        // tokens are ```. Skip this check when we are mid-block-comment;
        // a `/* */` that opens before a fence wins.
        if in_block.is_none() {
            let trimmed = raw_line.trim_start_matches([' ', '\t']);
            let trimmed_clean = trimmed.trim_end_matches('\n').trim_end_matches('\r');
            if trimmed_clean.starts_with("```") {
                if in_fence {
                    in_fence = false;
                } else {
                    let rest = &trimmed_clean[3..];
                    let inline_close = rest.ends_with("```") && rest.len() >= 3;
                    if !inline_close {
                        in_fence = true;
                    }
                }
                out.extend_from_slice(raw_line.as_bytes());
                line_idx += 1;
                line_start_byte += raw_line.len() as u32;
                continue;
            }
        }

        if in_fence {
            out.extend_from_slice(raw_line.as_bytes());
            line_idx += 1;
            line_start_byte += raw_line.len() as u32;
            continue;
        }

        let bytes = raw_line.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i];

            if in_block.is_some() {
                if c == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    in_block = None;
                } else if c == b'\n' || c == b'\r' {
                    out.push(c);
                    i += 1;
                } else {
                    // Preserve a single ASCII space per source byte; this
                    // keeps every downstream byte offset stable, even
                    // inside multi-byte UTF-8 sequences (which become
                    // runs of spaces but never split a code point —
                    // they're wholly inside the comment region).
                    out.push(b' ');
                    i += 1;
                }
                continue;
            }

            if c == b'/'
                && bytes.get(i + 1) == Some(&b'/')
                && opener_boundary_ok(bytes, i)
            {
                // Line comment — replace through end of line, keep \n.
                while i < bytes.len() && bytes[i] != b'\n' {
                    if bytes[i] == b'\r' {
                        out.push(b'\r');
                    } else {
                        out.push(b' ');
                    }
                    i += 1;
                }
                continue;
            }

            if c == b'/'
                && bytes.get(i + 1) == Some(&b'*')
                && opener_boundary_ok(bytes, i)
            {
                let col = i as u32;
                in_block = Some((line_idx, col, line_start_byte + col));
                out.push(b' ');
                out.push(b' ');
                i += 2;
                continue;
            }

            out.push(c);
            i += 1;
        }

        line_idx += 1;
        line_start_byte += raw_line.len() as u32;
    }

    if let Some((line, col, byte)) = in_block {
        let end_byte = out.len() as u32;
        let end_line = line_idx;
        let end_col = end_byte.saturating_sub(line_start_byte);
        diagnostics.push(Diagnostic::error(
            Code::L1007UnterminatedBlockComment,
            Span::new(
                Position::new(line, col, byte),
                Position::new(end_line, end_col, end_byte),
            ),
            "block comment opened with `/*` is never closed",
        ));
    }

    // SAFETY: we only replaced single ASCII bytes with single ASCII
    // bytes (` ` / `\r` / `\n`); the bytes we copied through were the
    // original UTF-8, so the buffer is still valid UTF-8.
    let stripped =
        String::from_utf8(out).expect("strip_comments preserves UTF-8 by construction");
    (stripped, diagnostics)
}

/// `//` and `/*` are only comment openers when they sit at the start
/// of a line or follow whitespace. This guards URLs in dialogue
/// (`https://example.com` — the slash is preceded by `:`).
fn opener_boundary_ok(bytes: &[u8], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_comment_is_stripped_to_eol() {
        let (out, diags) = strip("hello // tail\nworld\n");
        assert!(diags.is_empty());
        assert_eq!(out, "hello        \nworld\n");
    }

    #[test]
    fn block_comment_preserves_newlines() {
        let (out, diags) = strip("a /* one\ntwo */ b\n");
        assert!(diags.is_empty());
        assert_eq!(out, "a       \n       b\n");
    }

    #[test]
    fn unterminated_block_diagnoses() {
        let (_out, diags) = strip("a /* open and never closed\nstill open\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, Code::L1007UnterminatedBlockComment);
    }

    #[test]
    fn slashes_in_urls_are_safe() {
        let (out, diags) = strip("see https://example.com/path\n");
        assert!(diags.is_empty());
        assert_eq!(out, "see https://example.com/path\n");
    }

    #[test]
    fn comments_inside_fence_are_preserved() {
        let src = "```note\n// not a comment here\n/* also fine */\n```\n";
        let (out, diags) = strip(src);
        assert!(diags.is_empty());
        assert_eq!(out, src);
    }

    #[test]
    fn inline_fence_is_handled() {
        // The inline fence opens and closes on the same line; the next
        // line's `//` should still be treated as a comment.
        let (out, _) = strip("```warn lx14```\n// real comment\nWREN\n");
        assert_eq!(out, "```warn lx14```\n               \nWREN\n");
    }

    #[test]
    fn utf8_inside_comment_is_replaced_with_spaces() {
        // A multi-byte UTF-8 sequence ("é" = 0xC3 0xA9) inside a block
        // comment must not be left dangling — every byte is replaced
        // with an ASCII space so downstream byte offsets line up.
        let (out, _) = strip("/* café */ end\n");
        // 10 bytes of comment (the `é` contributes two bytes) + " end\n".
        assert_eq!(out.len(), "/* café */ end\n".len());
        assert!(out.ends_with(" end\n"));
        assert!(out.chars().all(|c| c == ' ' || c.is_ascii() && !c.is_control() || c == '\n'));
    }
}
