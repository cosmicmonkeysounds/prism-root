//! `LoomSyntaxProvider` — diagnostics + completion + hover for `.loom`
//! source.
//!
//! Today this is a **thin** wrapper around [`super::parser::parse`]:
//! parser diagnostics flow through, basic keyword completion is wired
//! to the [`super::keywords`] registry, and hover returns help text
//! for built-in sigils / keywords. The validator pass that emits the
//! full §21 catalog is the next deliverable; once it lands, this
//! provider becomes the LSP server's entry point.

use crate::language::syntax::{
    CompletionItem, CompletionKind, Diagnostic, DiagnosticSeverity, HoverInfo, SchemaContext,
    SyntaxProvider, TextRange,
};

use super::keywords::KEYWORD_CATEGORIES;
use super::parser::{parse, LoomDiagnostic, Severity as LoomSeverity};
use super::validator::validate;
use super::LOOM_ID;

#[derive(Debug, Clone, Default)]
pub struct LoomSyntaxProvider;

impl LoomSyntaxProvider {
    pub fn new() -> Self {
        Self
    }
}

impl SyntaxProvider for LoomSyntaxProvider {
    fn name(&self) -> &str {
        LOOM_ID
    }

    fn diagnose(&self, source: &str, _context: Option<&SchemaContext>) -> Vec<Diagnostic> {
        let result = parse(source);
        // Run the validator over the parsed tree even when the parser
        // produced diagnostics — validator passes are independent and
        // their messages tend to be the more actionable ones for
        // editor users.
        let validator_diags = validate(&result.root);

        let mut diagnostics = Vec::with_capacity(result.diagnostics.len() + validator_diags.len());
        diagnostics.extend(result.diagnostics.into_iter().map(convert_diagnostic));
        diagnostics.extend(validator_diags.into_iter().map(convert_diagnostic));
        diagnostics
    }

    fn complete(
        &self,
        source: &str,
        offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Vec<CompletionItem> {
        let prefix = extract_word_prefix(source, offset);
        if prefix.is_empty() {
            return Vec::new();
        }

        let mut items = Vec::new();

        for cat in KEYWORD_CATEGORIES {
            for word in cat.words {
                if word.starts_with(&prefix) {
                    items.push(CompletionItem {
                        label: word.to_string(),
                        kind: CompletionKind::Keyword,
                        detail: Some(cat.name.to_string()),
                        documentation: None,
                        sort_order: Some(200),
                        replace_range: None,
                        insert_text: None,
                    });
                }
            }
        }

        items.sort_by(|a, b| a.sort_order.cmp(&b.sort_order).then(a.label.cmp(&b.label)));
        items.dedup_by(|a, b| a.label == b.label);
        items
    }

    fn hover(
        &self,
        source: &str,
        offset: usize,
        _context: Option<&SchemaContext>,
    ) -> Option<HoverInfo> {
        let (word, start, end) = extract_word_at(source, offset)?;
        let detail = sigil_hover(&word).or_else(|| keyword_hover(&word))?;
        Some(HoverInfo {
            contents: detail,
            range: TextRange { start, end },
        })
    }
}

fn convert_diagnostic(d: LoomDiagnostic) -> Diagnostic {
    Diagnostic {
        message: d.message,
        severity: match d.severity {
            LoomSeverity::Error => DiagnosticSeverity::Error,
            LoomSeverity::Warning => DiagnosticSeverity::Warning,
            LoomSeverity::Info => DiagnosticSeverity::Info,
        },
        range: TextRange {
            start: d.range.start.offset,
            end: d.range.end.offset.max(d.range.start.offset),
        },
        code: Some(d.id.to_string()),
    }
}

// ─── Word extraction (mirrors the Luau provider's helpers) ──────────

fn extract_word_prefix(source: &str, offset: usize) -> String {
    if offset > source.len() {
        return String::new();
    }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    source[start..offset].to_string()
}

fn extract_word_at(source: &str, offset: usize) -> Option<(String, usize, usize)> {
    if offset > source.len() {
        return None;
    }
    let bytes = source.as_bytes();
    let mut start = offset;
    while start > 0 {
        let b = bytes[start - 1];
        if b.is_ascii_alphanumeric() || b == b'_' {
            start -= 1;
        } else {
            break;
        }
    }
    let mut end = offset;
    while end < bytes.len() {
        let b = bytes[end];
        if b.is_ascii_alphanumeric() || b == b'_' {
            end += 1;
        } else {
            break;
        }
    }
    if start == end {
        return None;
    }
    Some((source[start..end].to_string(), start, end))
}

// ─── Hover docs ────────────────────────────────────────────────────

fn keyword_hover(word: &str) -> Option<String> {
    for cat in KEYWORD_CATEGORIES {
        for w in cat.words {
            if *w == word {
                return Some(format!("**keyword** ({}) — `{}`", cat.name, word));
            }
        }
    }
    None
}

fn sigil_hover(word: &str) -> Option<String> {
    // Word-level sigil hover is rare since most sigils are single
    // punctuation chars (`$`, `@`, `[[`). The exception is the
    // `$ROLE` family — uppercase resolve targets — which can be
    // surfaced here when they ever become text-level constants.
    let _ = word;
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnose_returns_empty_on_valid_input() {
        let p = LoomSyntaxProvider::new();
        let src = "# story \"Title\"\n  .actors a\n";
        let d = p.diagnose(src, None);
        assert!(d.is_empty(), "expected no diagnostics, got {:?}", d);
    }

    #[test]
    fn diagnose_surfaces_unterminated_string() {
        let p = LoomSyntaxProvider::new();
        let src = "# d\nlet x = \"oops\n";
        let d = p.diagnose(src, None);
        assert!(d.iter().any(|x| x.code.as_deref() == Some("lex-error")));
    }

    #[test]
    fn diagnose_carries_diagnostic_codes() {
        let p = LoomSyntaxProvider::new();
        let src = "# d\n-- s\nlet x = (1 + 2\n";
        let d = p.diagnose(src, None);
        assert!(
            d.iter()
                .any(|x| x.code.as_deref() == Some("bracket-unbalanced")),
            "expected bracket-unbalanced code in {:?}",
            d
        );
    }

    #[test]
    fn diagnose_surfaces_validator_unknown_cast() {
        let p = LoomSyntaxProvider::new();
        // Speaker `WREN` used in dialogue with no `cast` declaration —
        // a registry-driven diagnostic the parser alone can't catch.
        let src = "# story\n-- start\nWREN\n  hi\n";
        let d = p.diagnose(src, None);
        assert!(
            d.iter().any(|x| x.code.as_deref() == Some("unknown-cast")),
            "expected unknown-cast from validator in {:?}",
            d
        );
    }

    #[test]
    fn diagnose_surfaces_validator_divert_target_unknown() {
        let p = LoomSyntaxProvider::new();
        let src = "# story\n-- start\n-> nowhere\n";
        let d = p.diagnose(src, None);
        assert!(
            d.iter()
                .any(|x| x.code.as_deref() == Some("divert-target-unknown")),
            "expected divert-target-unknown in {:?}",
            d
        );
    }

    #[test]
    fn diagnose_surfaces_validator_shape_check() {
        let p = LoomSyntaxProvider::new();
        // `goal` with no `priority` knob — pure shape check from the
        // validator's pass 3.
        let src = "# d\ngoal investigate\n  active_when = $x\n";
        let d = p.diagnose(src, None);
        assert!(
            d.iter()
                .any(|x| x.code.as_deref() == Some("goal-no-priority")),
            "expected goal-no-priority in {:?}",
            d
        );
    }

    #[test]
    fn complete_returns_keywords_with_prefix() {
        let p = LoomSyntaxProvider::new();
        let src = "# d\n-- s\ngo";
        let offset = src.len();
        let items = p.complete(src, offset, None);
        assert!(items.iter().any(|i| i.label == "goal"));
    }

    #[test]
    fn hover_resolves_keyword() {
        let p = LoomSyntaxProvider::new();
        let src = "knowledge";
        let h = p.hover(src, 0, None).expect("hover should return info");
        assert!(h.contents.contains("knowledge"));
    }
}
