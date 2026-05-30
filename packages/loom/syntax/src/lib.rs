//! TextMate grammar generator for Loom v3.
//!
//! The grammar is **derived** from [`loom_parser::keywords`]: adding
//! a keyword or declaration kind in the parser automatically extends
//! the editor highlights on the next `prism codegen loom-tmgrammar`.
//!
//! The five-bracket discipline (spec §5) gives the grammar its top-
//! level scopes: `( … )`, `{ … }`, `[ … ]`, `< … >`, and
//! ` ``` … ``` ` each get their own scope name so themes can colour
//! them independently.
//!
//! Scopes follow VS Code / Zed conventions so existing dark/light
//! themes pick up Loom files without per-theme work:
//! `entity.name.type.*`, `keyword.control.*`, `string.quoted.*`,
//! `support.function.*`, `comment.block.*`, etc. — see
//! <https://macromates.com/manual/en/language_grammars#naming_conventions>.

use loom_parser::ast::DeclarationKind;
use loom_parser::keywords::{
    declarations_with_kind, BUILTIN_DIRECTIVES, CONTRACT_KEYS, MERIDIAN_KEYWORDS, RESERVED_INLINE,
    SIMULACRA_KEYWORDS, SYNTACTIC_DIRECTIVES,
};

/// Emit the canonical `loom.tmLanguage.json` as a serialised JSON
/// string.
///
/// Output structure: one `patterns` repository of named rules, with
/// the top-level `patterns` array choosing which rules apply in the
/// file's default context. Indented contexts (declaration bodies,
/// beat bodies) re-include the relevant subsets so highlighting stays
/// consistent regardless of nesting.
pub fn emit_tmgrammar() -> String {
    let value = serde_json::json!({
        "name": "Loom",
        "scopeName": "source.loom",
        "fileTypes": ["loom"],
        "patterns": top_level_patterns(),
        "repository": repository(),
    });
    serde_json::to_string_pretty(&value).expect("tmgrammar JSON is well-formed")
}

fn top_level_patterns() -> serde_json::Value {
    use serde_json::json;
    json!([
        // Fences win over comments: a stage manager's note inside a
        // `` ``` ... ``` `` block is allowed to contain `//`.
        { "include": "#metadata-fence" },
        { "include": "#line-comment" },
        { "include": "#block-comment" },
        { "include": "#header" },
        { "include": "#knot-marker" },
        { "include": "#declaration-opener" },
        { "include": "#let-binding" },
        { "include": "#scene-heading" },
        { "include": "#choice" },
        { "include": "#divert" },
        { "include": "#tunnel-return" },
        { "include": "#directive-line" },
        { "include": "#parenthetical" },
        { "include": "#speaker" },
        { "include": "#contract-key" },
        { "include": "#simulacra-keyword" },
        { "include": "#meridian-keyword" },
        { "include": "#property" },
        { "include": "#interpolation" },
    ])
}

fn repository() -> serde_json::Value {
    use serde_json::json;

    let declaration_alternation = declarations_with_kind()
        .map(|(word, _)| word)
        .collect::<Vec<_>>()
        .join("|");

    let syntactic_alternation = SYNTACTIC_DIRECTIVES.join("|").replace(' ', "\\\\s+");
    let builtin_alternation = BUILTIN_DIRECTIVES.join("|");
    let contract_alternation = CONTRACT_KEYS.join("|").replace(' ', "\\\\s+");
    let reserved_alternation = RESERVED_INLINE.join("|");
    let simulacra_alternation = SIMULACRA_KEYWORDS.join("|").replace(' ', "\\\\s+");
    let meridian_alternation = MERIDIAN_KEYWORDS.join("|");

    let mut rep = serde_json::Map::new();

    // # Title
    rep.insert(
        "header".into(),
        json!({
            "match": "^\\s*(#)(?!#)\\s*(.*)$",
            "captures": {
                "1": { "name": "punctuation.definition.heading.loom" },
                "2": { "name": "entity.name.section.loom" }
            }
        }),
    );

    // == knot_name
    rep.insert(
        "knot-marker".into(),
        json!({
            "match": "^\\s*(==)\\s*([A-Za-z_][A-Za-z0-9_]*)?\\s*$",
            "captures": {
                "1": { "name": "punctuation.definition.knot.loom" },
                "2": { "name": "entity.name.function.knot.loom" }
            }
        }),
    );

    // CHARACTER Name [is Mixin, Other]
    rep.insert(
        "declaration-opener".into(),
        json!({
            "match": format!(
                "^\\s*({})\\s+([A-Za-z_][A-Za-z0-9_]*)?(?:\\s+(is)\\s+(.+))?\\s*$",
                declaration_alternation
            ),
            "captures": {
                "1": { "name": "storage.type.declaration.loom" },
                "2": { "name": "entity.name.type.loom" },
                "3": { "name": "keyword.other.is.loom" },
                "4": { "name": "entity.other.inherited-class.mixin.loom" }
            }
        }),
    );

    // let name = expr
    rep.insert(
        "let-binding".into(),
        json!({
            "begin": "^\\s*(let)\\s+([A-Za-z_][A-Za-z0-9_]*)\\s*(=)",
            "beginCaptures": {
                "1": { "name": "storage.type.let.loom" },
                "2": { "name": "variable.other.reactive.loom" },
                "3": { "name": "keyword.operator.assignment.loom" }
            },
            "end": "$",
            "patterns": [
                { "include": "#expression" }
            ]
        }),
    );

    // INT. LIGHTHOUSE - DAWN
    rep.insert(
        "scene-heading".into(),
        json!({
            "match": "^\\s*((?:INT\\.|EXT\\.|INT/EXT|INT\\./EXT\\.|I/E\\s+).+)$",
            "captures": {
                "1": { "name": "markup.heading.scene.loom" }
            }
        }),
    );

    // * choice / + sticky choice (with optional [suppressed] tail)
    rep.insert(
        "choice".into(),
        json!({
            "begin": "^\\s*([*+])\\s+",
            "beginCaptures": {
                "1": { "name": "keyword.control.choice.loom" }
            },
            "end": "$",
            "patterns": [
                {
                    "match": "\\[([^\\]]*)\\]",
                    "captures": {
                        "0": { "name": "comment.block.suppression.loom" }
                    }
                },
                { "include": "#interpolation" },
                { "include": "#inline-directive" }
            ]
        }),
    );

    // -> target [with k: v, ...]
    rep.insert(
        "divert".into(),
        json!({
            "match": "(->)\\s*(END|[A-Za-z_][A-Za-z0-9_/#]*)?",
            "captures": {
                "1": { "name": "keyword.control.divert.loom" },
                "2": { "name": "entity.name.label.divert.loom" }
            }
        }),
    );

    rep.insert(
        "tunnel-return".into(),
        json!({
            "match": "^\\s*(<-)\\s*$",
            "captures": {
                "1": { "name": "keyword.control.tunnel-return.loom" }
            }
        }),
    );

    // <if: cond>, <else>, <broadcast: …>, <sfx: bell>
    rep.insert(
        "directive-line".into(),
        json!({
            "begin": "(<)",
            "beginCaptures": {
                "1": { "name": "punctuation.section.directive.begin.loom" }
            },
            "end": "(>)",
            "endCaptures": {
                "1": { "name": "punctuation.section.directive.end.loom" }
            },
            "patterns": [
                {
                    "match": format!(
                        "\\b({})\\b",
                        syntactic_alternation
                    ),
                    "name": "keyword.control.flow.loom"
                },
                {
                    "match": format!(
                        "\\b({})\\b",
                        builtin_alternation
                    ),
                    "name": "support.function.builtin.loom"
                },
                {
                    "match": "\\b([a-zA-Z_][a-zA-Z0-9_]*)\\s*(?=:)",
                    "name": "support.function.user.loom"
                },
                { "include": "#expression" }
            ]
        }),
    );

    rep.insert(
        "inline-directive".into(),
        json!({
            "begin": "<",
            "end": ">",
            "patterns": [
                { "include": "#directive-line" }
            ]
        }),
    );

    // (parenthetical) — performer-facing
    rep.insert(
        "parenthetical".into(),
        json!({
            "match": "^\\s*(\\([^)]*\\))\\s*$",
            "captures": {
                "1": { "name": "comment.line.parenthetical.loom" }
            }
        }),
    );

    // ALL CAPS speaker cue — WREN | FISHER
    rep.insert(
        "speaker".into(),
        json!({
            "match": "^\\s*([A-Z][A-Z0-9_ ]*(?:\\|\\s*[A-Z][A-Z0-9_ ]*)*)\\s*$",
            "captures": {
                "1": { "name": "entity.name.tag.speaker.loom" }
            }
        }),
    );

    // Reserved contract keys (cast:, setting:, with topic:, …)
    rep.insert(
        "contract-key".into(),
        json!({
            "match": format!(
                "^\\s*({})\\s*(:)",
                contract_alternation
            ),
            "captures": {
                "1": { "name": "keyword.other.contract.loom" },
                "2": { "name": "punctuation.separator.key-value.loom" }
            }
        }),
    );

    // Simulacra body keywords (spec §10): trusts/respects/fears,
    // reacts, knows, goal, on complete / on fail, generator, …
    rep.insert(
        "simulacra-keyword".into(),
        json!({
            "match": format!("\\b({})\\b", simulacra_alternation),
            "name": "keyword.other.simulacra.loom"
        }),
    );

    // Meridian primitives (spec §11): attribute / axis / pool / stat
    // / node / mode / curve / max / regen / cost / requires / effect.
    rep.insert(
        "meridian-keyword".into(),
        json!({
            "match": format!("\\b({})\\b", meridian_alternation),
            "name": "keyword.other.meridian.loom"
        }),
    );

    // Free-form property — key: value
    rep.insert(
        "property".into(),
        json!({
            "match": "^\\s*([A-Za-z_][A-Za-z0-9_-]*)\\s*(:)(?=\\s|$)",
            "captures": {
                "1": { "name": "variable.other.property.loom" },
                "2": { "name": "punctuation.separator.key-value.loom" }
            }
        }),
    );

    // {interpolation}
    rep.insert(
        "interpolation".into(),
        json!({
            "begin": "\\{",
            "end": "\\}",
            "beginCaptures": {
                "0": { "name": "punctuation.section.interpolation.begin.loom" }
            },
            "endCaptures": {
                "0": { "name": "punctuation.section.interpolation.end.loom" }
            },
            "patterns": [
                { "include": "#expression" }
            ]
        }),
    );

    // // line comment (spec §6.1). The `(?:^|\\s)` lookbehind keeps
    // URLs like `https://example.com` intact — the `//` there is
    // preceded by `:`, not whitespace.
    rep.insert(
        "line-comment".into(),
        json!({
            "match": "(?:^|(?<=\\s))(//.*)$",
            "captures": {
                "1": { "name": "comment.line.double-slash.loom" }
            }
        }),
    );

    // /* … */ block comment (spec §6.1). Same boundary rule.
    rep.insert(
        "block-comment".into(),
        json!({
            "begin": "(?:^|(?<=\\s))(/\\*)",
            "beginCaptures": {
                "1": { "name": "punctuation.definition.comment.begin.loom" }
            },
            "end": "(\\*/)",
            "endCaptures": {
                "1": { "name": "punctuation.definition.comment.end.loom" }
            },
            "name": "comment.block.loom"
        }),
    );

    // ```fence … ``` production-metadata sidecar
    rep.insert(
        "metadata-fence".into(),
        json!({
            "begin": "^\\s*(```)([a-zA-Z0-9_-]*)\\s*$",
            "beginCaptures": {
                "1": { "name": "punctuation.definition.fence.begin.loom" },
                "2": { "name": "entity.name.tag.fence.loom" }
            },
            "end": "^\\s*(```)\\s*$",
            "endCaptures": {
                "1": { "name": "punctuation.definition.fence.end.loom" }
            },
            "name": "meta.embedded.block.loom",
            "contentName": "comment.block.metadata.loom"
        }),
    );

    // Expression context — reserved words, numbers, strings.
    rep.insert(
        "expression".into(),
        json!({
            "patterns": [
                {
                    "match": format!("\\b({})\\b", reserved_alternation),
                    "name": "keyword.other.reserved.loom"
                },
                {
                    "match": "\\b-?\\d+(?:\\.\\d+)?\\b",
                    "name": "constant.numeric.loom"
                },
                {
                    "match": "\\b(true|false|nil)\\b",
                    "name": "constant.language.loom"
                },
                {
                    "begin": "\"",
                    "end": "\"",
                    "name": "string.quoted.double.loom",
                    "patterns": [
                        { "match": "\\\\.", "name": "constant.character.escape.loom" }
                    ]
                },
                {
                    "match": "[+\\-*/%=<>!]+",
                    "name": "keyword.operator.loom"
                }
            ]
        }),
    );

    serde_json::Value::Object(rep)
}

/// Per-declaration scope helper, exposed so the LSP can surface the
/// same scope name when reporting symbol references.
pub fn declaration_scope(kind: DeclarationKind) -> &'static str {
    match kind {
        DeclarationKind::Character => "entity.name.type.character.loom",
        DeclarationKind::Trait => "entity.name.type.trait.loom",
        DeclarationKind::Item => "entity.name.type.item.loom",
        DeclarationKind::Location => "entity.name.type.location.loom",
        DeclarationKind::Faction => "entity.name.type.faction.loom",
        DeclarationKind::Stats => "entity.name.type.stats.loom",
        DeclarationKind::Tree => "entity.name.type.tree.loom",
        DeclarationKind::Generator => "entity.name.type.generator.loom",
        DeclarationKind::Scene => "entity.name.type.scene.loom",
        DeclarationKind::Cohort => "entity.name.type.cohort.loom",
        DeclarationKind::Role => "entity.name.type.role.loom",
        DeclarationKind::Person => "entity.name.type.person.loom",
        DeclarationKind::Roster => "entity.name.type.roster.loom",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed_grammar() -> serde_json::Value {
        serde_json::from_str(&emit_tmgrammar()).expect("emit_tmgrammar produces valid JSON")
    }

    #[test]
    fn grammar_metadata_is_set() {
        let g = parsed_grammar();
        assert_eq!(g["name"], "Loom");
        assert_eq!(g["scopeName"], "source.loom");
        assert_eq!(g["fileTypes"][0], "loom");
    }

    #[test]
    fn every_repository_entry_is_referenced_or_referenced_by_another() {
        // Trip-wire for orphan rules — the repository should not grow
        // dead patterns. Builds a transitive include set from the
        // top-level patterns and asserts every key is reachable.
        let g = parsed_grammar();
        let repo = g["repository"].as_object().unwrap();
        let mut reachable: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut stack: Vec<String> = g["patterns"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|p| p["include"].as_str())
            .map(|s| s.trim_start_matches('#').to_string())
            .collect();
        while let Some(name) = stack.pop() {
            if !reachable.insert(name.clone()) {
                continue;
            }
            if let Some(rule) = repo.get(&name) {
                walk(rule, &mut stack);
            }
        }
        let extra: Vec<_> = repo.keys().filter(|k| !reachable.contains(*k)).collect();
        assert!(extra.is_empty(), "orphan repository entries: {extra:?}");
    }

    fn walk(rule: &serde_json::Value, out: &mut Vec<String>) {
        if let Some(s) = rule["include"].as_str() {
            out.push(s.trim_start_matches('#').to_string());
        }
        if let Some(arr) = rule["patterns"].as_array() {
            for sub in arr {
                walk(sub, out);
            }
        }
    }

    #[test]
    fn comment_scopes_are_emitted() {
        let g = parsed_grammar();
        let repo = g["repository"].as_object().unwrap();
        let line = &repo["line-comment"];
        assert!(line["match"].as_str().unwrap().contains("//"));
        assert_eq!(
            line["captures"]["1"]["name"],
            "comment.line.double-slash.loom"
        );
        let block = &repo["block-comment"];
        assert!(block["begin"].as_str().unwrap().contains("/\\*"));
        assert!(block["end"].as_str().unwrap().contains("\\*/"));
        assert_eq!(block["name"], "comment.block.loom");
    }

    #[test]
    fn every_declaration_word_appears_in_grammar() {
        let json = emit_tmgrammar();
        for word in loom_parser::keywords::DECLARATIONS {
            assert!(
                json.contains(word),
                "declaration keyword `{word}` missing from grammar"
            );
        }
    }

    #[test]
    fn declaration_scope_distinguishes_kinds() {
        // No two declaration kinds should map to the same scope.
        use std::collections::HashSet;
        let mut seen: HashSet<&'static str> = HashSet::new();
        for (_, kind) in declarations_with_kind() {
            assert!(seen.insert(declaration_scope(kind)));
        }
    }
}
