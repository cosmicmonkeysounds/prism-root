//! `LoomDatabase` — the postcard-serializable bundle a runtime loads.
//!
//! Compiled from a parsed `prism_core::language::loom::parser::RootNode`
//! by [`compile`]. The bundle drops everything the playhead doesn't
//! need at run time (raw token text, source comments, layout details)
//! and keeps a flat, fast-to-walk shape: per-document section tables,
//! cast / cue / location / cohort registries, and a list of "items"
//! per section in source order.
//!
//! Phase 1 scope: dialogue, choices, diverts, returns, flavor / stage
//! lines, action lines (opaque payload), annotations (opaque payload).
//! Deferred to Phase 2+:
//!   - expression evaluation (guards always evaluate to true today)
//!   - inline text grammar (each TextContent is collapsed to a single
//!     `text` string; the parser tree is retained verbatim under
//!     `raw_text` for the eventual richer renderer)
//!   - generators / scenes / compose
//!   - faction simulator
//!   - reactivity (reactive `let`, generators, scheduler)

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use prism_core::language::loom::node_kinds as nk;
use prism_core::language::syntax::{RootNode, SyntaxNode};

// ─── Bundle shape ──────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LoomDatabase {
    pub documents: Vec<Document>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub id: String,
    pub title: Option<String>,
    pub tags: Vec<String>,
    pub casts: IndexMap<String, CastSlot>,
    pub cues: IndexMap<String, CueDef>,
    pub locations: IndexMap<String, LocationDef>,
    pub cohorts: IndexMap<String, CohortDef>,
    pub sections: IndexMap<String, Section>,
    /// Section ids in source order. The playhead falls through to the
    /// next section when one finishes without an explicit divert.
    pub section_order: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CastSlot {
    pub id: String,
    pub label: Option<String>,
    pub voice: Option<String>,
    /// Free-form additional properties from `.foo bar` lines, keyed
    /// by property name with the raw value tail as the value.
    pub properties: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CueDef {
    pub id: String,
    pub properties: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LocationDef {
    pub id: String,
    pub label: Option<String>,
    pub properties: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CohortDef {
    pub id: String,
    pub label: Option<String>,
    pub properties: IndexMap<String, String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Section {
    pub id: String,
    pub modifiers: Vec<String>,
    pub items: Vec<Item>,
}

/// Externally tagged so the bundle round-trips through postcard
/// (the design's bundle wire format). Internally-tagged enums are
/// not supported by non-self-describing serde formats.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Item {
    Dialogue {
        speaker: String,
        attrs: Vec<(String, String)>,
        lines: Vec<String>,
    },
    Flavor {
        text: String,
    },
    Stage {
        text: String,
    },
    Choice {
        once: bool,
        label: String,
        body: Vec<Item>,
    },
    Divert {
        target: String,
    },
    Return {
        thread: Option<String>,
    },
    Action {
        keyword: String,
        payload: String,
    },
    Annotation {
        name: String,
        body: String,
    },
    /// Catch-all for productions Phase 1 doesn't compile further. The
    /// payload's `node_kind` field carries the raw SyntaxNode kind so
    /// consumers can stringify-and-skip without breaking playback.
    /// (Named `node_kind`, not `kind`, to avoid colliding with the
    /// serde internal tag.)
    Other {
        node_kind: String,
        text: String,
    },
}

// ─── Compile ───────────────────────────────────────────────────────

/// Lower a parsed `RootNode` into a [`LoomDatabase`]. Returns the
/// compiled bundle alongside any compile-time diagnostics (today:
/// none — the parser+validator catch the issues that would matter
/// here; the slot exists so future shape checks slot in).
pub fn compile(root: &RootNode) -> LoomDatabase {
    let mut db = LoomDatabase::default();
    for doc_node in &root.children {
        if doc_node.kind == nk::DOCUMENT {
            db.documents.push(compile_document(doc_node));
        }
    }
    db
}

fn compile_document(node: &SyntaxNode) -> Document {
    let mut doc = Document {
        id: String::new(),
        title: None,
        tags: Vec::new(),
        casts: IndexMap::new(),
        cues: IndexMap::new(),
        locations: IndexMap::new(),
        cohorts: IndexMap::new(),
        sections: IndexMap::new(),
        section_order: Vec::new(),
    };

    let mut anonymous_count: usize = 0;
    for child in &node.children {
        match child.kind.as_str() {
            nk::HEADER => {
                if let Some(id) = first_ident_value(child) {
                    doc.id = id;
                }
                if let Some(s) = child.children.iter().find(|c| c.kind == nk::STRING) {
                    doc.title = unquote(s.value.as_deref());
                }
                for tag in child.children.iter().filter(|c| c.kind == nk::DOC_TAG) {
                    if let Some(name) = first_ident_value(tag) {
                        doc.tags.push(name);
                    }
                }
            }
            nk::CAST_DECL => {
                let slot = compile_cast(child);
                doc.casts.insert(slot.id.clone(), slot);
            }
            nk::CUE_DECL => {
                let cue = compile_cue(child);
                doc.cues.insert(cue.id.clone(), cue);
            }
            nk::LOCATION_DECL => {
                let loc = compile_location(child);
                doc.locations.insert(loc.id.clone(), loc);
            }
            nk::COHORT_DECL => {
                let coh = compile_cohort(child);
                doc.cohorts.insert(coh.id.clone(), coh);
            }
            nk::SECTION => {
                let mut section = compile_section(child);
                if section.id.is_empty() {
                    section.id = format!("__anon_{anonymous_count}");
                    anonymous_count += 1;
                }
                doc.section_order.push(section.id.clone());
                doc.sections.insert(section.id.clone(), section);
            }
            _ => {}
        }
    }

    doc
}

fn compile_cast(node: &SyntaxNode) -> CastSlot {
    let mut slot = CastSlot {
        id: speaker_or_static(node).unwrap_or_default(),
        ..Default::default()
    };
    // Optional inline title string.
    if let Some(s) = node.children.iter().find(|c| c.kind == nk::STRING) {
        slot.label = unquote(s.value.as_deref());
    }
    // Property block (we don't store its node kind explicitly — the
    // parser groups properties under a "property_block" node).
    for prop in find_properties(node) {
        let (key, value) = read_property(prop);
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "label" => slot.label = Some(unquote_str(&value)),
            "voice" => slot.voice = Some(value.clone()),
            _ => {}
        }
        slot.properties.insert(key, value);
    }
    slot
}

fn compile_cue(node: &SyntaxNode) -> CueDef {
    let mut cue = CueDef {
        id: first_ident_value(node).unwrap_or_default(),
        ..Default::default()
    };
    for prop in find_properties(node) {
        let (k, v) = read_property(prop);
        cue.properties.insert(k, v);
    }
    cue
}

fn compile_location(node: &SyntaxNode) -> LocationDef {
    let mut loc = LocationDef {
        id: speaker_or_static(node).unwrap_or_default(),
        ..Default::default()
    };
    if let Some(s) = node.children.iter().find(|c| c.kind == nk::STRING) {
        loc.label = unquote(s.value.as_deref());
    }
    for prop in find_properties(node) {
        let (k, v) = read_property(prop);
        if k == "label" {
            loc.label = Some(unquote_str(&v));
        }
        loc.properties.insert(k, v);
    }
    loc
}

fn compile_cohort(node: &SyntaxNode) -> CohortDef {
    let mut coh = CohortDef {
        id: first_ident_value(node).unwrap_or_default(),
        ..Default::default()
    };
    for prop in find_properties(node) {
        let (k, v) = read_property(prop);
        if k == "label" {
            coh.label = Some(unquote_str(&v));
        }
        coh.properties.insert(k, v);
    }
    coh
}

fn compile_section(node: &SyntaxNode) -> Section {
    let mut section = Section {
        id: first_ident_value(node).unwrap_or_default(),
        modifiers: Vec::new(),
        items: Vec::new(),
    };
    for child in &node.children {
        match child.kind.as_str() {
            nk::MODIFIER => {
                if let Some(name) = first_ident_value(child) {
                    section.modifiers.push(name);
                }
            }
            nk::IDENT | nk::GUARD | nk::PARTICIPANT_SCOPE | nk::DOCSTRING => {}
            _ => {
                if let Some(item) = compile_item(child) {
                    section.items.push(item);
                }
            }
        }
    }
    section
}

fn compile_item(node: &SyntaxNode) -> Option<Item> {
    Some(match node.kind.as_str() {
        nk::DIALOGUE => compile_dialogue(node),
        nk::FLAVOR_LINE => Item::Flavor {
            text: render_text_content(node),
        },
        nk::STAGE_DIRECTION => Item::Stage {
            text: render_text_content(node),
        },
        nk::CHOICE => compile_choice(node),
        nk::DIVERT => compile_divert(node),
        nk::RETURN_LINE => Item::Return {
            thread: first_ident_value(node),
        },
        nk::ACTION_LINE => compile_action(node),
        nk::ANNOTATION => compile_annotation(node),
        "blank" | "comment" => return None,
        // Block constructs (each-visit / after / when / match), text
        // variations, sexps, etc. — keep them visible as Other so the
        // tree round-trips and the consumer can decide how to skip.
        kind => Item::Other {
            node_kind: kind.to_string(),
            text: render_inline_text(node),
        },
    })
}

fn compile_dialogue(node: &SyntaxNode) -> Item {
    let speaker = node
        .children
        .iter()
        .find(|c| c.kind == nk::SPEAKER_REF)
        .and_then(|s| {
            // SPEAKER_REF wraps one of SPEAKER / RESOLVE_REF / STATIC_REF.
            s.children
                .first()
                .and_then(|inner| match inner.kind.as_str() {
                    nk::SPEAKER | nk::IDENT => inner.value.clone(),
                    nk::RESOLVE_REF => {
                        // `$name` → return "$name" so the host can route it.
                        first_ident_value(inner).map(|n| format!("${n}"))
                    }
                    nk::STATIC_REF => first_ident_value(inner).map(|n| format!("@{n}")),
                    _ => None,
                })
        })
        .unwrap_or_default();

    let mut attrs = Vec::new();
    if let Some(block) = node.children.iter().find(|c| c.kind == nk::CHAR_BLOCK) {
        for item in block.children.iter().filter(|c| c.kind == nk::CHAR_ITEM) {
            let idents: Vec<_> = item
                .children
                .iter()
                .filter(|c| c.kind == nk::IDENT)
                .collect();
            match idents.as_slice() {
                [a] => attrs.push((String::new(), a.value.clone().unwrap_or_default())),
                [k, v] => attrs.push((
                    k.value.clone().unwrap_or_default(),
                    v.value.clone().unwrap_or_default(),
                )),
                _ => {}
            }
        }
    }

    let mut lines = Vec::new();
    for line in node.children.iter().filter(|c| c.kind == nk::TEXT_LINE) {
        lines.push(render_text_content(line));
    }

    Item::Dialogue {
        speaker,
        attrs,
        lines,
    }
}

fn compile_choice(node: &SyntaxNode) -> Item {
    // ChoiceKind is encoded in the `choice_marker` leaf — `*` for once,
    // `+` for sticky.
    let once = node
        .children
        .iter()
        .find(|c| c.kind == "choice_marker")
        .and_then(|m| m.value.as_deref())
        .map(|m| m == "*")
        .unwrap_or(true);

    let label = node
        .children
        .iter()
        .find(|c| c.kind == nk::CHOICE_LABEL)
        .map(render_text_content)
        .unwrap_or_default();

    let mut body = Vec::new();
    // Inline `-> target` on the same line as the choice — compile it
    // as a Divert at the head of the body.
    if let Some(div) = node.children.iter().find(|c| c.kind == nk::DIVERT) {
        body.push(compile_divert(div));
    }
    // Nested content block.
    if let Some(block) = node.children.iter().find(|c| c.kind == "content_block") {
        for item_node in &block.children {
            if let Some(item) = compile_item(item_node) {
                body.push(item);
            }
        }
    }

    Item::Choice { once, label, body }
}

fn compile_divert(node: &SyntaxNode) -> Item {
    // First IDENT or STATIC_REF child is the target.
    let target = node
        .children
        .iter()
        .find_map(|c| match c.kind.as_str() {
            nk::IDENT => c.value.clone(),
            nk::STATIC_REF => first_ident_value(c).map(|n| format!("@{n}")),
            nk::TUNNEL_CALL => first_ident_value(c),
            _ => None,
        })
        .unwrap_or_default();
    Item::Divert { target }
}

fn compile_action(node: &SyntaxNode) -> Item {
    let keyword_action = node.children.iter().find(|c| c.kind == nk::KEYWORD_ACTION);
    let (keyword, payload) = match keyword_action {
        Some(ka) => {
            let kw = first_ident_value(ka).unwrap_or_default();
            let payload = ka
                .children
                .iter()
                .find(|c| c.kind == nk::PROPERTY_VALUE)
                .and_then(|c| c.value.clone())
                .unwrap_or_default();
            (kw, payload)
        }
        None => (String::new(), String::new()),
    };
    Item::Action { keyword, payload }
}

fn compile_annotation(node: &SyntaxNode) -> Item {
    let name = first_ident_value(node).unwrap_or_default();
    let body = node
        .children
        .iter()
        .find(|c| c.kind == nk::PROPERTY_VALUE)
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    Item::Annotation { name, body }
}

// ─── Helpers ───────────────────────────────────────────────────────

fn first_ident_value(node: &SyntaxNode) -> Option<String> {
    node.children
        .iter()
        .find(|c| matches!(c.kind.as_str(), nk::IDENT | nk::SPEAKER))
        .and_then(|c| c.value.clone())
}

fn speaker_or_static(node: &SyntaxNode) -> Option<String> {
    for c in &node.children {
        match c.kind.as_str() {
            nk::SPEAKER => return c.value.clone(),
            nk::STATIC_REF => return first_ident_value(c).map(|n| format!("@{n}")),
            _ => {}
        }
    }
    None
}

/// Locate all PROPERTY nodes under `node` — they nest inside a
/// "property_block" container the parser emits for indented `.name`
/// runs.
fn find_properties(node: &SyntaxNode) -> Vec<&SyntaxNode> {
    let mut out = Vec::new();
    fn walk<'a>(n: &'a SyntaxNode, out: &mut Vec<&'a SyntaxNode>) {
        if n.kind == nk::PROPERTY {
            out.push(n);
            return;
        }
        for c in &n.children {
            walk(c, out);
        }
    }
    walk(node, &mut out);
    out
}

fn read_property(node: &SyntaxNode) -> (String, String) {
    let key = first_ident_value(node).unwrap_or_default();
    let value = node
        .children
        .iter()
        .find(|c| c.kind == nk::PROPERTY_VALUE)
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    (key, value)
}

fn unquote(s: Option<&str>) -> Option<String> {
    s.map(unquote_str)
}

fn unquote_str(raw: &str) -> String {
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        raw[1..raw.len() - 1].to_string()
    } else {
        raw.to_string()
    }
}

/// Concatenate the rendered inline text of a node carrying TEXT_CONTENT
/// children. For Phase 1 we collapse the full sequence of LITERAL_RUN /
/// RESOLVE_REF / STATIC_REF / BACKLINK / etc. into a single human-
/// readable string. Richer rendering (real interpolation, trigger
/// dispatch, backlink resolution) lands when the renderer arrives.
fn render_text_content(node: &SyntaxNode) -> String {
    let mut out = String::new();
    for child in &node.children {
        if child.kind == nk::TEXT_CONTENT {
            collect_text(child, &mut out);
        } else if child.kind == nk::LITERAL_RUN {
            push_with_space(&mut out, child.value.as_deref().unwrap_or(""));
        }
    }
    out.trim().to_string()
}

fn render_inline_text(node: &SyntaxNode) -> String {
    let mut out = String::new();
    collect_text(node, &mut out);
    out.trim().to_string()
}

fn collect_text(node: &SyntaxNode, out: &mut String) {
    match node.kind.as_str() {
        nk::LITERAL_RUN => push_with_space(out, node.value.as_deref().unwrap_or("")),
        nk::ESCAPED_CHAR => push_with_space(out, node.value.as_deref().unwrap_or("")),
        nk::BACKLINK => {
            // Render as the display text if present, else the target.
            let lit_runs: Vec<&str> = node
                .children
                .iter()
                .filter(|c| c.kind == nk::LITERAL_RUN)
                .filter_map(|c| c.value.as_deref())
                .collect();
            // The display child (after `|`) is the last LITERAL_RUN if
            // there are two; otherwise the target text serves as both.
            let display = lit_runs.last().copied().unwrap_or("");
            push_with_space(out, display);
        }
        nk::RESOLVE_REF => {
            // Render as `${name}` so the consumer can spot unresolved
            // references at preview time.
            if let Some(name) = first_ident_value(node) {
                push_with_space(out, &format!("${{{name}}}"));
            }
        }
        nk::STATIC_REF => {
            if let Some(name) = first_ident_value(node) {
                push_with_space(out, &format!("@{name}"));
            }
        }
        nk::INLINE_TRIGGER
        | nk::CHAIN_TRIGGER
        | nk::COND_TRIGGER
        | nk::RANGE_CLOSER
        | nk::INLINE_ASSIGN
        | nk::INLINE_EVAL => {
            // Triggers carry no renderable text — they're side-effect
            // markers. Skip silently.
        }
        nk::TEXT_VARIATION => {
            // For Phase 1, render the first variant; cycle / shuffle /
            // weighted modes need the ledger + RNG state, which we wire
            // up when the renderer lands.
            if let Some(first) = node
                .children
                .iter()
                .find(|c| c.kind == nk::VARIATION_VARIANT)
            {
                collect_text(first, out);
            }
        }
        _ => {
            for c in &node.children {
                collect_text(c, out);
            }
        }
    }
}

fn push_with_space(out: &mut String, s: &str) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with(|c: char| c.is_whitespace()) {
        out.push(' ');
    }
    out.push_str(trimmed);
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use prism_core::language::loom::parser::parse;

    fn compile_src(src: &str) -> LoomDatabase {
        compile(&parse(src).root)
    }

    #[test]
    fn compiles_minimal_document() {
        let db = compile_src("# tiny \"Title\"\n");
        assert_eq!(db.documents.len(), 1);
        assert_eq!(db.documents[0].id, "tiny");
        assert_eq!(db.documents[0].title.as_deref(), Some("Title"));
    }

    #[test]
    fn compiles_cast_with_label() {
        let db = compile_src("# d\ncast WREN\n  .label \"Wren\"\n  .voice female_mezzo\n");
        let cast = db.documents[0].casts.get("WREN").expect("cast");
        assert_eq!(cast.label.as_deref(), Some("Wren"));
        assert_eq!(cast.voice.as_deref(), Some("female_mezzo"));
    }

    #[test]
    fn compiles_section_with_dialogue_and_choice() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hello.\n  * I'll help. -> next\n-- next\nWREN\n  Thanks.\n";
        let db = compile_src(src);
        let start = db.documents[0]
            .sections
            .get("start")
            .expect("section start");
        assert_eq!(start.items.len(), 2);
        match &start.items[0] {
            Item::Dialogue { speaker, lines, .. } => {
                assert_eq!(speaker, "WREN");
                assert_eq!(lines.len(), 1);
                assert!(lines[0].contains("Hello"));
            }
            other => panic!("expected dialogue, got {other:?}"),
        }
        match &start.items[1] {
            Item::Choice { once, label, body } => {
                assert!(*once, "`*` choices should compile as once");
                assert!(label.contains("help"));
                assert!(
                    matches!(body.first(), Some(Item::Divert { target, .. }) if target == "next"),
                    "inline divert should land at the head of the choice body",
                );
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn section_order_is_source_order() {
        let src = "# d\n-- a\n-- b\n-- c\n";
        let db = compile_src(src);
        assert_eq!(db.documents[0].section_order, vec!["a", "b", "c"]);
    }

    #[test]
    fn sticky_choice_marker_round_trips() {
        let src = "# d\n-- s\nWREN\n  hi\n+ keep open\n  -> s\ncast WREN\n  .label x\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let any_sticky = s
            .items
            .iter()
            .any(|i| matches!(i, Item::Choice { once: false, .. }));
        assert!(any_sticky);
    }

    #[test]
    fn anonymous_sections_get_synthetic_ids() {
        let src = "# d\n--\n  WREN\n    body\n--\n  WREN\n    again\ncast WREN\n  .label x\n";
        let db = compile_src(src);
        let ids: Vec<&str> = db.documents[0]
            .section_order
            .iter()
            .map(String::as_str)
            .collect();
        assert_eq!(ids.len(), 2);
        assert!(ids.iter().all(|id| id.starts_with("__anon_")));
    }

    #[test]
    fn bundle_serializes_round_trip_via_postcard() {
        let db = compile_src("# d\ncast WREN\n  .label x\n-- s\nWREN\n  hi.\n");
        let bytes = postcard::to_allocvec(&db).expect("encode");
        let back: LoomDatabase = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(back.documents.len(), 1);
        assert_eq!(back.documents[0].id, "d");
        assert_eq!(back.documents[0].sections.len(), 1);
    }
}
