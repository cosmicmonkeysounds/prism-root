//! Wiki-link parser: `[[id]]` and `[[id|display]]`.
//!
//! Port of `language/forms/wiki-link.ts`. This lives under `forms`
//! because the TS tree re-exported it from `@prism/core/forms`, and
//! the markdown block parser in the same subtree depends on it.
//!
//! Byte-for-byte parity with the TS regex: `/\[\[([^\]|]+)(?:\|([^\]]+))?\]\]/g`.
//!
//! NOTE: the registry also publishes a `wikilink_token` inline-token
//! definition via [`crate::language::registry::wikilink_token`]. That
//! variant is aimed at CodeMirror / Slint chip rendering and uses a
//! slightly different capture-group layout. Both coexist on purpose:
//! this module is used by the markdown block tokenizer, the registry
//! variant is used by the language surface.

use std::sync::OnceLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

fn wiki_link_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]").expect("wiki link regex compiles")
    })
}

/// Token emitted by [`parse_wiki_links`]. `Text` is the unchanged
/// slice between / around link tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WikiToken {
    Text {
        text: String,
    },
    Link {
        id: String,
        display: String,
        raw: String,
    },
}

/// Parse a text string into wiki-link tokens. Non-matching slices
/// become `Text` tokens in order.
pub fn parse_wiki_links(text: &str) -> Vec<WikiToken> {
    let mut tokens = Vec::new();
    let mut last = 0usize;

    for m in wiki_link_re().captures_iter(text) {
        let mat = m.get(0).expect("match 0 always present");
        let start = mat.start();
        if start > last {
            tokens.push(WikiToken::Text {
                text: text[last..start].to_owned(),
            });
        }
        let id = m
            .get(1)
            .map(|c| c.as_str().trim().to_owned())
            .unwrap_or_default();
        let display = m
            .get(2)
            .map(|c| c.as_str().trim().to_owned())
            .unwrap_or_else(|| id.clone());
        tokens.push(WikiToken::Link {
            id,
            display,
            raw: mat.as_str().to_owned(),
        });
        last = mat.end();
    }

    if last < text.len() {
        tokens.push(WikiToken::Text {
            text: text[last..].to_owned(),
        });
    }

    tokens
}

/// Just the ids from every wiki link in `text`, in source order.
pub fn extract_linked_ids(text: &str) -> Vec<String> {
    wiki_link_re()
        .captures_iter(text)
        .map(|m| {
            m.get(1)
                .map(|c| c.as_str().trim().to_owned())
                .unwrap_or_default()
        })
        .collect()
}

/// Rewrite every wiki link using a display resolver: the resolver
/// receives the target id and returns the replacement string.
///
/// Matches the TS semantics: if a link has an explicit display
/// override (`[[id|Pretty]]`), the override wins and the resolver is
/// ignored for that link.
pub fn render_wiki_links(text: &str, mut resolver: impl FnMut(&str) -> String) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0usize;

    for m in wiki_link_re().captures_iter(text) {
        let mat = m.get(0).expect("match 0 always present");
        out.push_str(&text[last..mat.start()]);
        let id = m.get(1).map(|c| c.as_str().trim()).unwrap_or("");
        let display_override = m.get(2).map(|c| c.as_str().trim());
        let replacement = match display_override {
            Some(d) => d.to_owned(),
            None => resolver(id),
        };
        out.push_str(&replacement);
        last = mat.end();
    }

    out.push_str(&text[last..]);
    out
}

/// Build a wiki link string. Emits the short form `[[id]]` when
/// `display` is `None` or equal to `id`.
pub fn build_wiki_link(id: &str, display: Option<&str>) -> String {
    match display {
        Some(d) if d != id => format!("[[{id}|{d}]]"),
        _ => format!("[[{id}]]"),
    }
}

/// If the cursor is *inside* an unclosed `[[` at `cursor_pos`, return
/// the substring between `[[` and the cursor. Returns `None` if the
/// cursor is not inside an open link.
///
/// Matches the TS `detectInlineLink` autocomplete helper: it looks
/// for the *last* `[[` before `cursor_pos` and bails if the
/// intervening slice already closed with `]]`.
pub fn detect_inline_link(text: &str, cursor_pos: usize) -> Option<String> {
    let clamped = cursor_pos.min(text.len());
    let before = &text[..clamped];
    let open_idx = before.rfind("[[")?;
    let between = &before[open_idx + 2..];
    if between.contains("]]") {
        return None;
    }
    Some(between.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_returns_plain_text_when_no_links() {
        assert_eq!(
            parse_wiki_links("hello world"),
            vec![WikiToken::Text {
                text: "hello world".into()
            }]
        );
    }

    #[test]
    fn parse_returns_short_form_link() {
        let tokens = parse_wiki_links("see [[page]] here");
        assert_eq!(tokens.len(), 3);
        assert_eq!(
            tokens[1],
            WikiToken::Link {
                id: "page".into(),
                display: "page".into(),
                raw: "[[page]]".into(),
            }
        );
    }

    #[test]
    fn parse_returns_pipe_override() {
        let tokens = parse_wiki_links("[[page|Pretty Name]]");
        assert_eq!(
            tokens[0],
            WikiToken::Link {
                id: "page".into(),
                display: "Pretty Name".into(),
                raw: "[[page|Pretty Name]]".into(),
            }
        );
    }

    #[test]
    fn extract_linked_ids_returns_ids_in_order() {
        let ids = extract_linked_ids("[[a]] then [[b|Beta]] and [[c]]");
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn render_wiki_links_uses_resolver_without_override() {
        let rendered = render_wiki_links("see [[page]] done", |id| format!("<{id}>"));
        assert_eq!(rendered, "see <page> done");
    }

    #[test]
    fn render_wiki_links_keeps_explicit_override() {
        let rendered = render_wiki_links("[[page|Custom]]", |_| "ignored".into());
        assert_eq!(rendered, "Custom");
    }

    #[test]
    fn build_wiki_link_short_form_when_display_matches() {
        assert_eq!(build_wiki_link("page", None), "[[page]]");
        assert_eq!(build_wiki_link("page", Some("page")), "[[page]]");
        assert_eq!(build_wiki_link("page", Some("Pretty")), "[[page|Pretty]]");
    }

    #[test]
    fn detect_inline_link_inside_open_link_returns_prefix() {
        assert_eq!(
            detect_inline_link("some [[partial", 14),
            Some("partial".into())
        );
    }

    #[test]
    fn detect_inline_link_returns_none_when_closed() {
        assert_eq!(detect_inline_link("some [[page]] done", 18), None);
    }

    #[test]
    fn detect_inline_link_returns_none_without_open_marker() {
        assert_eq!(detect_inline_link("plain text", 5), None);
    }
}
