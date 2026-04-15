//! Prism's in-house markdown dialect — block and inline tokenizers.
//!
//! Port of `language/forms/markdown.ts`. This is **not** CommonMark:
//! it's the narrow set of block/inline constructs Prism notes and
//! debug panels actually use (headings, paragraphs, lists, code
//! fences, task items, wiki links, emphasis, inline code,
//! hyperlinks). The full-fat CommonMark + GFM path is `pulldown-cmark`
//! and lives behind the `language::markdown` contribution as a
//! future swap — see the migration plan §6.1.
//!
//! Parity target is byte-identical to the TS tokenizer: the list of
//! `BlockToken`s produced here must round-trip through a matching
//! fixture in the Rust test suite and in `8426588` — see
//! `docs/dev/clay-migration-plan.md` §10.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

use regex::Regex;

use super::wiki_link::parse_wiki_links;

// ── Block tokens ───────────────────────────────────────────────────

/// One block-level token from [`parse_markdown`]. The `kind`
/// discriminator mirrors the TS union verbatim so on-disk notes
/// round-trip unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum BlockToken {
    Empty,
    Hr,
    H1 {
        text: String,
    },
    H2 {
        text: String,
    },
    H3 {
        text: String,
    },
    P {
        text: String,
    },
    Blockquote {
        text: String,
    },
    Li {
        text: String,
    },
    Oli {
        text: String,
        n: u32,
    },
    Task {
        text: String,
        checked: bool,
    },
    Code {
        text: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lang: Option<String>,
    },
}

impl BlockToken {
    /// The inline text payload for blocks that carry one. `None` for
    /// `Empty` / `Hr`.
    pub fn text(&self) -> Option<&str> {
        match self {
            BlockToken::Empty | BlockToken::Hr => None,
            BlockToken::H1 { text }
            | BlockToken::H2 { text }
            | BlockToken::H3 { text }
            | BlockToken::P { text }
            | BlockToken::Blockquote { text }
            | BlockToken::Li { text }
            | BlockToken::Oli { text, .. }
            | BlockToken::Task { text, .. }
            | BlockToken::Code { text, .. } => Some(text.as_str()),
        }
    }
}

// ── Inline tokens ──────────────────────────────────────────────────

/// One inline-level token from [`parse_inline`]. `Bold` and `Italic`
/// wrap nested inline tokens recursively — same shape as the TS tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum InlineToken {
    Text { text: String },
    Bold { children: Vec<InlineToken> },
    Italic { children: Vec<InlineToken> },
    Code { text: String },
    Link { text: String, href: String },
    Wiki { id: String, display: String },
}

// ── Block parser ───────────────────────────────────────────────────

static TASK_RE: OnceLock<Regex> = OnceLock::new();
static OLI_RE: OnceLock<Regex> = OnceLock::new();
static INLINE_RE: OnceLock<Regex> = OnceLock::new();

fn task_re() -> &'static Regex {
    TASK_RE.get_or_init(|| Regex::new(r"^[-*] \[([ x])\] (.*)$").expect("task regex compiles"))
}

fn oli_re() -> &'static Regex {
    OLI_RE.get_or_init(|| Regex::new(r"^(\d+)\. (.*)$").expect("ordered-list regex compiles"))
}

fn inline_re() -> &'static Regex {
    INLINE_RE.get_or_init(|| {
        // Matches the TS INLINE_RE_SOURCE exactly. Order matters: the
        // wiki-link alternative must come first so `[[` is never
        // matched as a malformed bold/italic run.
        Regex::new(
            r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]|\*\*(.+?)\*\*|\*(.+?)\*|`([^`]+)`|\[([^\]]+)\]\(([^)]+)\)",
        )
        .expect("inline regex compiles")
    })
}

/// Parse a markdown document into block tokens.
///
/// Line-oriented; identical scanning rules to the TS version:
/// code fences flip a sticky `in_code` flag; blank lines become
/// `Empty`; thematic breaks match `---`, `***`, `___`; headings
/// match `#`/`##`/`###`; blockquote matches `> `; task items match
/// before plain list items so `- [ ]` isn't swallowed as a
/// bullet; ordered list items (`1. `) match next; every other
/// non-empty line becomes a paragraph.
pub fn parse_markdown(md: &str) -> Vec<BlockToken> {
    let mut tokens: Vec<BlockToken> = Vec::new();
    let mut in_code = false;
    let mut code_lang = String::new();
    let mut code_lines: Vec<String> = Vec::new();

    for raw in md.split('\n') {
        if let Some(fence_rest) = raw.strip_prefix("```") {
            if in_code {
                tokens.push(BlockToken::Code {
                    text: code_lines.join("\n"),
                    lang: (!code_lang.is_empty()).then(|| code_lang.clone()),
                });
                code_lines.clear();
                code_lang.clear();
                in_code = false;
            } else {
                in_code = true;
                code_lang = fence_rest.trim().to_owned();
            }
            continue;
        }
        if in_code {
            code_lines.push(raw.to_owned());
            continue;
        }

        if raw.trim().is_empty() {
            tokens.push(BlockToken::Empty);
            continue;
        }

        if raw == "---" || raw == "***" || raw == "___" {
            tokens.push(BlockToken::Hr);
            continue;
        }

        if let Some(rest) = raw.strip_prefix("### ") {
            tokens.push(BlockToken::H3 {
                text: rest.to_owned(),
            });
            continue;
        }
        if let Some(rest) = raw.strip_prefix("## ") {
            tokens.push(BlockToken::H2 {
                text: rest.to_owned(),
            });
            continue;
        }
        if let Some(rest) = raw.strip_prefix("# ") {
            tokens.push(BlockToken::H1 {
                text: rest.to_owned(),
            });
            continue;
        }

        if let Some(rest) = raw.strip_prefix("> ") {
            tokens.push(BlockToken::Blockquote {
                text: rest.to_owned(),
            });
            continue;
        }

        if let Some(caps) = task_re().captures(raw) {
            let checked = caps.get(1).map(|c| c.as_str() == "x").unwrap_or(false);
            let text = caps
                .get(2)
                .map(|c| c.as_str().to_owned())
                .unwrap_or_default();
            tokens.push(BlockToken::Task { text, checked });
            continue;
        }

        if raw.starts_with("- ") || raw.starts_with("* ") {
            tokens.push(BlockToken::Li {
                text: raw[2..].to_owned(),
            });
            continue;
        }

        if let Some(caps) = oli_re().captures(raw) {
            let n = caps
                .get(1)
                .and_then(|c| c.as_str().parse::<u32>().ok())
                .unwrap_or(0);
            let text = caps
                .get(2)
                .map(|c| c.as_str().to_owned())
                .unwrap_or_default();
            tokens.push(BlockToken::Oli { text, n });
            continue;
        }

        tokens.push(BlockToken::P {
            text: raw.to_owned(),
        });
    }

    if in_code && !code_lines.is_empty() {
        tokens.push(BlockToken::Code {
            text: code_lines.join("\n"),
            lang: (!code_lang.is_empty()).then(|| code_lang.clone()),
        });
    }

    tokens
}

// ── Inline parser ──────────────────────────────────────────────────

/// Parse inline markdown into a token stream.
///
/// Recursive: `**bold**` / `*italic*` children are parsed again so
/// nested emphasis and wikilinks-inside-bold work.
pub fn parse_inline(text: &str) -> Vec<InlineToken> {
    let mut tokens = Vec::new();
    let mut last = 0usize;

    for caps in inline_re().captures_iter(text) {
        let m = caps.get(0).expect("match 0 always present");
        if m.start() > last {
            tokens.push(InlineToken::Text {
                text: text[last..m.start()].to_owned(),
            });
        }

        if let Some(id_match) = caps.get(1) {
            let id = id_match.as_str().trim().to_owned();
            let display = caps
                .get(2)
                .map(|c| c.as_str().trim().to_owned())
                .unwrap_or_else(|| id.clone());
            tokens.push(InlineToken::Wiki { id, display });
        } else if let Some(bold) = caps.get(3) {
            tokens.push(InlineToken::Bold {
                children: parse_inline(bold.as_str()),
            });
        } else if let Some(italic) = caps.get(4) {
            tokens.push(InlineToken::Italic {
                children: parse_inline(italic.as_str()),
            });
        } else if let Some(code) = caps.get(5) {
            tokens.push(InlineToken::Code {
                text: code.as_str().to_owned(),
            });
        } else if let (Some(link_text), Some(href)) = (caps.get(6), caps.get(7)) {
            tokens.push(InlineToken::Link {
                text: link_text.as_str().to_owned(),
                href: href.as_str().to_owned(),
            });
        }

        last = m.end();
    }

    if last < text.len() {
        tokens.push(InlineToken::Text {
            text: text[last..].to_owned(),
        });
    }

    tokens
}

/// Flatten an inline token stream back to plain text. Matches the TS
/// `inlineToPlainText` exactly: bold/italic recurse, code/link/text
/// use their raw payload, wiki emits the display value.
pub fn inline_to_plain_text(tokens: &[InlineToken]) -> String {
    let mut out = String::new();
    for t in tokens {
        match t {
            InlineToken::Text { text } => out.push_str(text),
            InlineToken::Bold { children } | InlineToken::Italic { children } => {
                out.push_str(&inline_to_plain_text(children));
            }
            InlineToken::Code { text } | InlineToken::Link { text, .. } => out.push_str(text),
            InlineToken::Wiki { display, .. } => out.push_str(display),
        }
    }
    out
}

/// Extract every wiki-link id referenced by any block's inline text,
/// in source order. Goes through [`parse_wiki_links`] so the exact
/// regex from `wiki_link` is the single source of truth.
pub fn extract_wiki_ids(blocks: &[BlockToken]) -> Vec<String> {
    let mut ids = Vec::new();
    for block in blocks {
        if let Some(text) = block.text() {
            if text.is_empty() {
                continue;
            }
            for token in parse_wiki_links(text) {
                if let super::wiki_link::WikiToken::Link { id, .. } = token {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_parse_empty_input() {
        assert_eq!(parse_markdown(""), vec![BlockToken::Empty]);
    }

    #[test]
    fn block_parse_headings_and_paragraphs() {
        let md = "# Title\n\nParagraph text.\n## Sub\n### Tiny";
        let tokens = parse_markdown(md);
        assert_eq!(
            tokens,
            vec![
                BlockToken::H1 {
                    text: "Title".into()
                },
                BlockToken::Empty,
                BlockToken::P {
                    text: "Paragraph text.".into()
                },
                BlockToken::H2 { text: "Sub".into() },
                BlockToken::H3 {
                    text: "Tiny".into()
                },
            ]
        );
    }

    #[test]
    fn block_parse_task_before_list_item() {
        let tokens = parse_markdown("- [ ] todo\n- [x] done\n- bullet");
        assert_eq!(
            tokens,
            vec![
                BlockToken::Task {
                    text: "todo".into(),
                    checked: false
                },
                BlockToken::Task {
                    text: "done".into(),
                    checked: true
                },
                BlockToken::Li {
                    text: "bullet".into()
                },
            ]
        );
    }

    #[test]
    fn block_parse_ordered_list() {
        let tokens = parse_markdown("1. first\n2. second");
        assert_eq!(
            tokens,
            vec![
                BlockToken::Oli {
                    text: "first".into(),
                    n: 1
                },
                BlockToken::Oli {
                    text: "second".into(),
                    n: 2
                },
            ]
        );
    }

    #[test]
    fn block_parse_code_fence_preserves_lang_and_lines() {
        let md = "```rust\nfn main() {}\n```";
        let tokens = parse_markdown(md);
        assert_eq!(
            tokens,
            vec![BlockToken::Code {
                text: "fn main() {}".into(),
                lang: Some("rust".into()),
            }]
        );
    }

    #[test]
    fn block_parse_unterminated_code_fence_closes_at_eof() {
        let md = "```\nabc\ndef";
        let tokens = parse_markdown(md);
        assert_eq!(
            tokens,
            vec![BlockToken::Code {
                text: "abc\ndef".into(),
                lang: None,
            }]
        );
    }

    #[test]
    fn block_parse_thematic_break() {
        assert_eq!(parse_markdown("---"), vec![BlockToken::Hr]);
        assert_eq!(parse_markdown("***"), vec![BlockToken::Hr]);
        assert_eq!(parse_markdown("___"), vec![BlockToken::Hr]);
    }

    #[test]
    fn block_parse_blockquote() {
        let tokens = parse_markdown("> quoted");
        assert_eq!(
            tokens,
            vec![BlockToken::Blockquote {
                text: "quoted".into()
            }]
        );
    }

    #[test]
    fn inline_parse_wiki_link() {
        let tokens = parse_inline("see [[page|Pretty]]!");
        assert_eq!(
            tokens,
            vec![
                InlineToken::Text {
                    text: "see ".into()
                },
                InlineToken::Wiki {
                    id: "page".into(),
                    display: "Pretty".into()
                },
                InlineToken::Text { text: "!".into() },
            ]
        );
    }

    #[test]
    fn inline_parse_bold_italic_nesting() {
        let tokens = parse_inline("**bold *and italic***");
        assert!(matches!(tokens.first(), Some(InlineToken::Bold { .. })));
    }

    #[test]
    fn inline_parse_code_and_link() {
        let tokens = parse_inline("use `map` and see [docs](https://e.com)");
        assert!(tokens
            .iter()
            .any(|t| matches!(t, InlineToken::Code { text } if text == "map")));
        assert!(tokens
            .iter()
            .any(|t| matches!(t, InlineToken::Link { href, .. } if href == "https://e.com")));
    }

    #[test]
    fn inline_to_plain_text_drops_formatting() {
        let tokens = parse_inline("**bold** and `code` and [link](u)");
        assert_eq!(inline_to_plain_text(&tokens), "bold and code and link");
    }

    #[test]
    fn extract_wiki_ids_walks_all_blocks() {
        let tokens = parse_markdown("# [[title]]\n- see [[a]] and [[b|Beta]]\n> cf [[c]]");
        let ids = extract_wiki_ids(&tokens);
        assert_eq!(ids, vec!["title", "a", "b", "c"]);
    }

    #[test]
    fn round_trip_block_json() {
        let tokens = parse_markdown("# Hi\n\n- [x] done");
        let json = serde_json::to_string(&tokens).unwrap();
        let back: Vec<BlockToken> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, tokens);
    }
}
