//! TextMate grammar emitter for the Loom language.
//!
//! Reads the keyword / sigil / operator tables in [`super::keywords`] and
//! produces a TextMate-format JSON grammar (`loom.tmLanguage.json`) that
//! VSCode, Sublime Text, IntelliJ, and GitHub Linguist all consume.
//!
//! The output is **derived** — every keyword that lives in
//! [`super::keywords`] is reflected here; adding a new keyword to a
//! category automatically adds it to the editor highlights on the next
//! `prism codegen loom-tmgrammar`.
//!
//! ## What this is and isn't
//!
//! TextMate grammars are regex-based and approximate. They do well at
//! lexical classification (this token is a keyword, that span is a
//! string) and badly at structural classification (is this `node`
//! identifier the declaration or the reference?). For the structural
//! tier we ship an LSP server backed by the canonical Loom parser; see
//! the LSP follow-up note in `tools/loom-syntax/README.md`.
//!
//! ## Scope-name convention
//!
//! All scopes end in `.loom`. The prefix follows the standard TextMate
//! taxonomy (`keyword.control`, `string.quoted.double`,
//! `variable.other`, `constant.numeric`, …) so editor themes that ship
//! defaults colour Loom out of the box.

use serde_json::{json, Map, Value};

use super::keywords::{
    Operator, Sigil, ATOMIC_SIGILS, BUILTIN_TRIGGER_TYPES, KEYWORD_CATEGORIES, OPERATORS,
};
use super::{LOOM_ID, LOOM_MIME_TYPE};

/// Emit the TextMate grammar as a pretty-printed JSON string.
///
/// This is the public entry point the `prism codegen loom-tmgrammar`
/// CLI command (and tests) invoke.
pub fn emit_tmgrammar() -> String {
    let value = build_grammar();
    serde_json::to_string_pretty(&value).expect("tmgrammar JSON value should always serialise")
}

fn build_grammar() -> Value {
    let mut repo = Map::new();

    // Order matters only insofar as ergonomic grouping — the registry is
    // resolved by name lookup, not array position.
    repo.insert("comment".into(), comment_patterns());
    repo.insert("docstring".into(), docstring_pattern());
    repo.insert("string".into(), string_pattern());
    repo.insert("number".into(), number_pattern());

    repo.insert("header".into(), header_pattern());
    repo.insert("doc_tag".into(), doc_tag_pattern());
    repo.insert("section_line".into(), section_line_pattern());
    repo.insert("slugline".into(), slugline_pattern());
    repo.insert("speaker_line".into(), speaker_line_pattern());
    repo.insert("flavor_line".into(), flavor_line_pattern());
    repo.insert("choice_marker".into(), choice_marker_pattern());
    repo.insert("divert".into(), divert_pattern());
    repo.insert("return_line".into(), return_line_pattern());
    repo.insert("action_prefix".into(), action_prefix_pattern());
    repo.insert("property".into(), property_pattern());
    repo.insert("modifier".into(), modifier_pattern());

    repo.insert("backlink".into(), backlink_pattern());
    repo.insert("inline_trigger".into(), inline_trigger_pattern());
    repo.insert("range_closer".into(), range_closer_pattern());
    repo.insert("inline_assign".into(), inline_assign_pattern());
    repo.insert("inline_eval_brace".into(), inline_eval_brace_pattern());
    repo.insert("inline_eval_paren".into(), inline_eval_paren_pattern());
    repo.insert("resolve_ref".into(), resolve_ref_pattern());
    repo.insert("static_ref".into(), static_ref_pattern());

    repo.insert("speaker_id".into(), speaker_id_pattern());
    repo.insert("atomic_sigil".into(), atomic_sigil_patterns());
    repo.insert("operator".into(), operator_patterns());
    repo.insert("keyword".into(), keyword_patterns());
    repo.insert("identifier".into(), identifier_pattern());

    // Each top-level entry references a repository pattern by name. The
    // resolution order here is the priority order — comments first so
    // they don't get swallowed by anything else, then docstrings, then
    // structural line classes (header, section, dialogue, …), then
    // inline atoms.
    let top_level = json!([
        { "include": "#comment" },
        { "include": "#docstring" },
        { "include": "#string" },
        { "include": "#header" },
        { "include": "#slugline" },
        { "include": "#section_line" },
        { "include": "#speaker_line" },
        { "include": "#choice_marker" },
        { "include": "#divert" },
        { "include": "#return_line" },
        { "include": "#flavor_line" },
        { "include": "#action_prefix" },
        { "include": "#property" },
        { "include": "#modifier" },
        { "include": "#backlink" },
        { "include": "#range_closer" },
        { "include": "#inline_trigger" },
        { "include": "#inline_assign" },
        { "include": "#inline_eval_brace" },
        { "include": "#inline_eval_paren" },
        { "include": "#resolve_ref" },
        { "include": "#static_ref" },
        { "include": "#doc_tag" },
        { "include": "#number" },
        { "include": "#atomic_sigil" },
        { "include": "#operator" },
        { "include": "#keyword" },
        { "include": "#identifier" },
    ]);

    json!({
        "name": "Loom",
        "scopeName": "source.loom",
        "fileTypes": ["loom"],
        "uuid": "5b9b8a8e-1f1c-4b1c-9a1e-c8b6b9d7e9f1",
        "information_for_contributors": [
            "GENERATED FILE — DO NOT EDIT BY HAND.",
            format!(
                "Regenerate with `prism codegen loom-tmgrammar`. Source: \
                 packages/prism-core/src/language/loom/ ({}).",
                LOOM_ID
            ),
            format!("Mime type: {}.", LOOM_MIME_TYPE),
            "Spec: docs/dev/loom-design.md + docs/dev/loom-grammar.md.",
        ],
        "patterns": top_level,
        "repository": repo,
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Comments & block-style literals
// ─────────────────────────────────────────────────────────────────────────

fn comment_patterns() -> Value {
    json!({
        "patterns": [
            {
                "name": "comment.line.double-slash.loom",
                "match": "//.*$"
            },
            {
                "name": "comment.block.loom",
                "begin": "/\\*",
                "end": "\\*/",
                "captures": {
                    "0": { "name": "punctuation.definition.comment.loom" }
                }
            }
        ]
    })
}

fn docstring_pattern() -> Value {
    json!({
        "name": "string.quoted.triple.loom",
        "begin": "'''",
        "end": "'''",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.string.begin.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.string.end.loom" }
        }
    })
}

fn string_pattern() -> Value {
    json!({
        "name": "string.quoted.double.loom",
        "begin": "\"",
        "end": "\"",
        "patterns": [
            {
                "name": "constant.character.escape.loom",
                "match": "\\\\(?:[\"\\\\nrt]|u\\{[0-9A-Fa-f]+\\})"
            }
        ]
    })
}

// Numbers and duration literals. The grammar (§3) admits an optional
// duration suffix (`s`, `ms`, `m`, `h`). We tag the suffix as a unit so
// themes can give it a faint contrast.
fn number_pattern() -> Value {
    json!({
        "patterns": [
            {
                "name": "constant.numeric.duration.loom",
                "match": "(?<![\\w])(-?[0-9]+(?:\\.[0-9]+)?)(ms|s|m|h)\\b",
                "captures": {
                    "1": { "name": "constant.numeric.loom" },
                    "2": { "name": "keyword.other.unit.loom" }
                }
            },
            {
                "name": "constant.numeric.loom",
                "match": "(?<![\\w])-?[0-9]+(?:\\.[0-9]+)?"
            }
        ]
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Line-class openers
// ─────────────────────────────────────────────────────────────────────────

// Document header: `# name "title" :tag :tag2`
fn header_pattern() -> Value {
    json!({
        "match": "^(#)\\s+([A-Za-z_][A-Za-z0-9_]*)",
        "captures": {
            "1": { "name": "punctuation.definition.heading.loom" },
            "2": { "name": "entity.name.section.document.loom" }
        }
    })
}

// Doc-tag (`:conversation`, `:script`, etc.). Tagged with the same scope
// regardless of recognised-ness; theme can colour known ones via the
// keyword `doc_tags` category.
fn doc_tag_pattern() -> Value {
    json!({
        "match": "(?<![\\w:])(:)([A-Za-z_][A-Za-z0-9_-]*)\\b",
        "captures": {
            "1": { "name": "punctuation.definition.tag.loom" },
            "2": { "name": "entity.name.tag.loom" }
        }
    })
}

// Section: `-- section_id`
fn section_line_pattern() -> Value {
    json!({
        "match": "^\\s*(--)\\s*([A-Za-z_][A-Za-z0-9_]*)?",
        "captures": {
            "1": { "name": "punctuation.section.section.loom" },
            "2": { "name": "entity.name.section.loom" }
        }
    })
}

// Slugline scene: `## act.scene "INT. HARBOR — LATE AFTERNOON"`
fn slugline_pattern() -> Value {
    json!({
        "match": "^\\s*(##)\\s*([A-Za-z_][A-Za-z0-9_]*(?:\\.[A-Za-z_][A-Za-z0-9_]*)?)?",
        "captures": {
            "1": { "name": "punctuation.section.slugline.loom" },
            "2": { "name": "entity.name.section.slugline.loom" }
        }
    })
}

// Speaker line: `WREN { worried }^` — ALL_CAPS at start of indented line.
// Lookahead requires `{`, `^`, or end-of-line to disambiguate from any
// other ALL_CAPS identifier in expression position.
fn speaker_line_pattern() -> Value {
    json!({
        "match": "^(\\s*)([A-Z][A-Z0-9_]*)(?=\\s*(?:\\{|\\^|$))",
        "captures": {
            "2": { "name": "entity.name.function.speaker.loom" }
        }
    })
}

// Flavor line: `> prose...`
fn flavor_line_pattern() -> Value {
    json!({
        "match": "^\\s*(>)\\s",
        "captures": {
            "1": { "name": "punctuation.section.flavor.loom" }
        }
    })
}

// Choice marker: `*` (once) or `+` (sticky) at line start.
fn choice_marker_pattern() -> Value {
    json!({
        "match": "^\\s*([*+])(?=\\s)",
        "captures": {
            "1": { "name": "punctuation.section.choice.loom" }
        }
    })
}

// Divert: `-> target` or `-> @target`
fn divert_pattern() -> Value {
    json!({
        "match": "(->)\\s*(@?[A-Za-z_][A-Za-z0-9_]*(?:\\.[A-Za-z_][A-Za-z0-9_]*)?)?",
        "captures": {
            "1": { "name": "keyword.control.divert.loom" },
            "2": { "name": "entity.name.section.divert-target.loom" }
        }
    })
}

// Return: `<-` or `<- name`
fn return_line_pattern() -> Value {
    json!({
        "match": "(<-)\\s*([A-Za-z_][A-Za-z0-9_]*)?",
        "captures": {
            "1": { "name": "keyword.control.return.loom" },
            "2": { "name": "entity.name.section.return-target.loom" }
        }
    })
}

// Action prefix: `~` at start of an action line.
fn action_prefix_pattern() -> Value {
    json!({
        "match": "^\\s*(~)",
        "captures": {
            "1": { "name": "punctuation.section.action.loom" }
        }
    })
}

// Property: `.key value`
fn property_pattern() -> Value {
    json!({
        "match": "^\\s*(\\.)([A-Za-z_][A-Za-z0-9_-]*)",
        "captures": {
            "1": { "name": "punctuation.definition.property.loom" },
            "2": { "name": "variable.other.property.loom" }
        }
    })
}

// Inline modifier: `.once`, `.sticky`, `.show("...")` — only inside a
// section / choice / divert line, but TextMate has no positional context
// so we match by shape: a `.name` *not* at column 0 (column-0 belongs to
// `property`).
fn modifier_pattern() -> Value {
    json!({
        "match": "(?<=\\s)(\\.)([A-Za-z_][A-Za-z0-9_-]*)",
        "captures": {
            "1": { "name": "punctuation.accessor.modifier.loom" },
            "2": { "name": "support.type.modifier.loom" }
        }
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Inline atoms
// ─────────────────────────────────────────────────────────────────────────

// Backlink: `[[ ... ]]` with optional `type:` prefix and `|display` tail.
fn backlink_pattern() -> Value {
    json!({
        "name": "markup.underline.link.loom",
        "begin": "\\[\\[",
        "end": "\\]\\]",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.link.begin.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.link.end.loom" }
        },
        "patterns": [
            {
                "name": "entity.name.tag.backlink-type.loom",
                "match": "([A-Za-z_][A-Za-z0-9_-]*)(:)",
                "captures": {
                    "1": { "name": "entity.name.tag.backlink-type.loom" },
                    "2": { "name": "punctuation.separator.loom" }
                }
            },
            {
                "match": "(\\|)([^\\]]+)",
                "captures": {
                    "1": { "name": "punctuation.separator.display.loom" },
                    "2": { "name": "string.unquoted.backlink-display.loom" }
                }
            }
        ]
    })
}

// Inline trigger: `<type:args [attr:val ...] [for:N] [%name]>`
//
// The plain "begin/end" form must reject `</...` (range closer) and
// `<-` (return) and `<$...` (inline assign) by negative lookahead in the
// begin pattern.
fn inline_trigger_pattern() -> Value {
    let triggers = BUILTIN_TRIGGER_TYPES.join("|");
    json!({
        "name": "meta.trigger.loom",
        "begin": format!(
            "<(?!/|\\?|-|\\$)([A-Za-z_][A-Za-z0-9_]*)(:)",
        ),
        "end": ">",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.trigger.begin.loom" },
            "1": {
                "patterns": [
                    {
                        "name": "support.function.trigger.builtin.loom",
                        "match": format!("\\b({})\\b", triggers)
                    },
                    {
                        "name": "support.function.trigger.loom",
                        "match": "[A-Za-z_][A-Za-z0-9_]*"
                    }
                ]
            },
            "2": { "name": "punctuation.separator.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.trigger.end.loom" }
        },
        "patterns": [
            { "include": "#string" },
            { "include": "#number" },
            { "include": "#resolve_ref" },
            { "include": "#static_ref" },
            {
                "name": "variable.parameter.trigger-attr.loom",
                "match": "\\b([A-Za-z_][A-Za-z0-9_]*)(:)",
                "captures": {
                    "1": { "name": "variable.parameter.trigger-attr.loom" },
                    "2": { "name": "punctuation.separator.loom" }
                }
            }
        ]
    })
}

// Range closer: `</>` or `</%name>`
fn range_closer_pattern() -> Value {
    json!({
        "patterns": [
            {
                "name": "punctuation.definition.range.end.loom",
                "match": "</>"
            },
            {
                "match": "(</%)([A-Za-z_][A-Za-z0-9_]*)(>)",
                "captures": {
                    "1": { "name": "punctuation.definition.range.end.loom" },
                    "2": { "name": "entity.name.tag.anchor.loom" },
                    "3": { "name": "punctuation.definition.range.end.loom" }
                }
            }
        ]
    })
}

// Inline assign: `<$x := expr>`
fn inline_assign_pattern() -> Value {
    json!({
        "name": "meta.inline-assign.loom",
        "begin": "<(?=\\$)",
        "end": ">",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.inline-assign.begin.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.inline-assign.end.loom" }
        },
        "patterns": [
            { "include": "#resolve_ref" },
            { "include": "#operator" },
            { "include": "#number" },
            { "include": "#string" },
            { "include": "#static_ref" },
            { "include": "#keyword" }
        ]
    })
}

// `${expr}` inline eval.
fn inline_eval_brace_pattern() -> Value {
    json!({
        "name": "meta.inline-eval.loom",
        "begin": "\\$\\{",
        "end": "\\}",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.inline-eval.begin.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.inline-eval.end.loom" }
        },
        "patterns": [
            { "include": "#string" },
            { "include": "#number" },
            { "include": "#resolve_ref" },
            { "include": "#static_ref" },
            { "include": "#operator" },
            { "include": "#keyword" },
            { "include": "#identifier" }
        ]
    })
}

// `$(expr ...)` s-expression inline eval.
fn inline_eval_paren_pattern() -> Value {
    json!({
        "name": "meta.inline-eval-sexp.loom",
        "begin": "\\$\\(",
        "end": "\\)",
        "beginCaptures": {
            "0": { "name": "punctuation.definition.inline-eval.begin.loom" }
        },
        "endCaptures": {
            "0": { "name": "punctuation.definition.inline-eval.end.loom" }
        },
        "patterns": [
            { "include": "#string" },
            { "include": "#number" },
            { "include": "#resolve_ref" },
            { "include": "#static_ref" },
            { "include": "#operator" },
            { "include": "#keyword" },
            { "include": "#identifier" }
        ]
    })
}

// Resolve ref: `$name`, `$name.field`, `$name?`, `$name[idx]`.
// (The brace / paren forms are handled separately above.)
fn resolve_ref_pattern() -> Value {
    json!({
        "match": "(\\$)([A-Za-z_][A-Za-z0-9_]*)((?:\\.[A-Za-z_][A-Za-z0-9_]*|\\?\\.[A-Za-z_][A-Za-z0-9_]*)*)(\\?)?",
        "captures": {
            "1": { "name": "punctuation.definition.variable.resolve.loom" },
            "2": { "name": "variable.other.resolve.loom" },
            "3": { "name": "variable.other.resolve.chain.loom" },
            "4": { "name": "punctuation.definition.presence-check.loom" }
        }
    })
}

// Static ref: `@name` or `@name.subname`.
fn static_ref_pattern() -> Value {
    json!({
        "match": "(@)([A-Za-z_][A-Za-z0-9_]*(?:\\.[A-Za-z_][A-Za-z0-9_]*)*)",
        "captures": {
            "1": { "name": "punctuation.definition.variable.static.loom" },
            "2": { "name": "variable.other.static.loom" }
        }
    })
}

fn speaker_id_pattern() -> Value {
    json!({
        "match": "\\b[A-Z][A-Z0-9_]+\\b",
        "name": "entity.name.function.speaker.loom"
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Atomic sigils, operators, keywords, identifier (generated)
// ─────────────────────────────────────────────────────────────────────────

fn atomic_sigil_patterns() -> Value {
    let patterns: Vec<Value> = ATOMIC_SIGILS
        .iter()
        .map(|s: &Sigil| {
            json!({
                "name": format!("{}.loom", s.scope),
                "match": format!("(?<![\\w]){}(?![\\w])", regex_escape(s.literal))
            })
        })
        .collect();
    json!({ "patterns": patterns })
}

fn operator_patterns() -> Value {
    // Sort by literal length (descending) so longer ops match before
    // shorter prefixes (e.g. `<=` wins over `<`).
    let mut ops: Vec<&Operator> = OPERATORS.iter().collect();
    ops.sort_by_key(|o| std::cmp::Reverse(o.literal.len()));
    let patterns: Vec<Value> = ops
        .iter()
        .map(|o| {
            json!({
                "name": format!("{}.loom", o.scope),
                "match": regex_escape(o.literal)
            })
        })
        .collect();
    json!({ "patterns": patterns })
}

fn keyword_patterns() -> Value {
    let patterns: Vec<Value> = KEYWORD_CATEGORIES
        .iter()
        .map(|cat| {
            let alternatives = cat
                .words
                .iter()
                .map(|w| regex_escape(w))
                .collect::<Vec<_>>()
                .join("|");
            json!({
                "name": format!("{}.loom", cat.scope),
                "match": format!("\\b({})\\b", alternatives)
            })
        })
        .collect();
    json!({ "patterns": patterns })
}

fn identifier_pattern() -> Value {
    json!({
        "match": "\\b[A-Za-z_][A-Za-z0-9_]*\\b",
        "name": "variable.other.loom"
    })
}

// ─────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────

fn regex_escape(s: &str) -> String {
    // Conservative escape for the literals we actually emit (operators,
    // keywords, single-character sigils). Covers every metacharacter
    // POSIX ERE / PCRE / Oniguruma agree on.
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '\\' | '.' | '+' | '*' | '?' | '(' | ')' | '[' | ']' | '{' | '}' | '^' | '$' | '|'
            | '/' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_produces_valid_json() {
        let text = emit_tmgrammar();
        let parsed: Value = serde_json::from_str(&text).expect("emitted JSON should parse");
        assert_eq!(parsed["scopeName"], "source.loom");
        assert_eq!(parsed["fileTypes"][0], "loom");
        assert!(parsed["patterns"].is_array());
        assert!(parsed["repository"].is_object());
    }

    #[test]
    fn emit_contains_every_category_scope() {
        let text = emit_tmgrammar();
        for cat in KEYWORD_CATEGORIES {
            let expected = format!("{}.loom", cat.scope);
            assert!(
                text.contains(&expected),
                "TextMate grammar should reference scope `{}` from category `{}`",
                expected,
                cat.name
            );
        }
    }

    #[test]
    fn emit_contains_every_keyword() {
        let text = emit_tmgrammar();
        for cat in KEYWORD_CATEGORIES {
            for word in cat.words {
                // The keyword appears inside an alternation regex, possibly
                // escaped (e.g. `one-to-one` survives unescaped because `-`
                // is not a metachar). Substring match is sufficient.
                assert!(
                    text.contains(word),
                    "TextMate grammar should reference keyword `{}` from category `{}`",
                    word,
                    cat.name
                );
            }
        }
    }

    #[test]
    fn header_pattern_captures_name_and_title_separately() {
        let value = header_pattern();
        let captures = &value["captures"];
        assert_eq!(
            captures["1"]["name"], "punctuation.definition.heading.loom",
            "the `#` should be punctuation"
        );
        assert_eq!(
            captures["2"]["name"], "entity.name.section.document.loom",
            "the document id should be the section entity name"
        );
    }

    #[test]
    fn operators_emit_longest_first() {
        let value = operator_patterns();
        let arr = value["patterns"].as_array().unwrap();
        let first = arr[0]["match"].as_str().unwrap();
        // The very first emitted operator regex must NOT be a single
        // character — otherwise it would shadow `:=`, `==`, etc.
        assert!(
            first.len() > 1,
            "operators should emit longest-first, got `{}` first",
            first
        );
    }

    #[test]
    fn regex_escape_handles_metacharacters() {
        assert_eq!(regex_escape("a"), "a");
        assert_eq!(regex_escape("--"), "--");
        assert_eq!(regex_escape("?."), "\\?\\.");
        assert_eq!(regex_escape("[["), "\\[\\[");
        assert_eq!(regex_escape("|"), "\\|");
        assert_eq!(regex_escape("one-to-one"), "one-to-one");
    }
}
