//! `LoomDatabase` — the postcard-serializable bundle a runtime loads.
//!
//! Compiled from a parsed `prism_core::language::loom::parser::RootNode`
//! by [`compile`]. The bundle drops everything the playhead doesn't
//! need at run time (raw token text, source comments, layout details)
//! and keeps a flat, fast-to-walk shape: per-document section tables,
//! cast / cue / location / cohort registries, and a list of "items"
//! per section in source order.
//!
//! Phase 2 scope: every Phase 1 production plus —
//!   - parsed expressions lowered to [`super::resolver::Expr`] for
//!     guards (`if $trust > 50`), each-visit branches' "after" forms,
//!     and the `let name = <expr>` registry;
//!   - [`Mutation`] captures `$x := v`, `$x += v`, `$x -= v`, `$x++`,
//!     `~ var $x := v`, and `~ fire <event>`;
//!   - [`Item::EachVisit`], [`Item::After`], [`Item::Match`] block
//!     constructs;
//!   - inline `${expr}` / `$name` interpolation evaluated through the
//!     resolver context at frame render time.
//!
//! Deferred to Phase 3+:
//!   - generators / scenes / compose
//!   - faction simulator
//!   - reactive let → `Memo<T>` with dependency tracking (today the
//!     show eagerly re-evaluates every let on every mutation; correct
//!     but not yet incremental)

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use prism_core::language::loom::node_kinds as nk;
use prism_core::language::syntax::{RootNode, SyntaxNode};

use crate::expr::compile_expr;
use crate::resolver::Expr;
use crate::value::Value;

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
    /// Top-level `let name = <expr>` bindings, in source order. The
    /// show evaluates them at boot and after every mutation that
    /// touches the resolver-visible state.
    pub lets: Vec<LetBinding>,
}

/// One `let name = <expr>` declaration. Serialized as the source
/// position alone — the compiled [`Expr`] is rebuilt on load by
/// re-running [`compile_expr`] over the stored source. (Today the
/// runtime keeps the compiled form alongside; future LoroDoc-backed
/// hot reload will rebuild on patch.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LetBinding {
    pub name: String,
    /// The compiled body. `Expr` is `Clone` + serde-able through the
    /// existing tree, so the bundle round-trips through postcard.
    pub body: Expr,
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
    /// Section-level `-- foo if $expr` guard. `None` means
    /// unconditionally enterable.
    pub guard: Option<Expr>,
    pub items: Vec<Item>,
}

/// One assignment operator for [`Mutation`] / [`Item::Mutate`]. The
/// operator decides how to combine the existing value with the RHS;
/// `Set` overwrites, `PlusEq` / `MinusEq` are int-or-list aware,
/// `Inc` matches `$x++` (no RHS).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignOp {
    Set,
    PlusEq,
    MinusEq,
    Inc,
}

/// A captured action-line mutation. The runtime applies it through
/// `Show::apply_mutation` on encounter; serialisable so the bundle
/// can round-trip and savable replays land identical writes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mutation {
    /// `$x` or `$x.field.subfield`. The leading `$` is stripped.
    pub name: String,
    pub chain: Vec<String>,
    pub op: AssignOp,
    /// `None` only for [`AssignOp::Inc`].
    pub rhs: Option<Expr>,
}

// PartialEq via name + chain + op + serialized rhs — the rhs Expr tree
// isn't structurally PartialEq because of f64, but the wire form is.
impl PartialEq for Mutation {
    fn eq(&self, other: &Self) -> bool {
        if self.name != other.name || self.chain != other.chain || self.op != other.op {
            return false;
        }
        match (&self.rhs, &other.rhs) {
            (None, None) => true,
            (Some(a), Some(b)) => serde_json::to_value(a).ok() == serde_json::to_value(b).ok(),
            _ => false,
        }
    }
}

/// One branch of an `each visit` block. `kind` is `"first"` / `"then"`
/// / `"finally"`. `body` is the content the playhead enters when the
/// branch is chosen.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VisitBranch {
    pub kind: String,
    pub body: Vec<Item>,
}

/// One renderable text run inside dialogue / flavor / stage / choice
/// labels. Phase 2 keeps the parsed `RESOLVE_REF` / `STATIC_REF` /
/// `INLINE_ASSIGN` segments around so the playhead can interpolate
/// live, fire inline-assign mutations, and route static refs through
/// the registry. `Literal` text is preserved verbatim with whitespace
/// trimmed on the seams.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSeg {
    Literal(String),
    /// `$name` / `${expr}` — resolve at frame render.
    Resolve(Expr),
    /// `@name` — static ref; renders as `@name` or its registered
    /// label when one exists.
    StaticRef(String),
    /// `[[target|display]]` — backlink. Rendered as `display`.
    Backlink {
        target: String,
        display: String,
    },
    /// Inline mutation `<$x := v>` — fires when this segment is
    /// rendered. No visible text.
    Assign(Mutation),
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
        /// One entry per source-level dialogue line. Each entry is a
        /// sequence of segments the playhead concatenates at render.
        lines: Vec<Vec<TextSeg>>,
    },
    Flavor {
        text: Vec<TextSeg>,
    },
    Stage {
        text: Vec<TextSeg>,
    },
    Choice {
        once: bool,
        label: Vec<TextSeg>,
        /// `if <expr>` guard captured off the choice line. `None` is
        /// "always visible."
        guard: Option<Expr>,
        body: Vec<Item>,
    },
    Divert {
        target: String,
    },
    Return {
        thread: Option<String>,
    },
    /// `~ keyword payload` registry-driven action that does NOT mutate
    /// runtime state — the host dispatches it via [`Frame::Action`].
    /// State-mutating actions live in [`Item::Mutate`] / [`Item::Fire`].
    Action {
        keyword: String,
        payload: String,
    },
    /// `~ var $x := v` / `~ $x := v` / `~ $x += v` / `~ $x++`. The
    /// runtime applies the change and surfaces no frame.
    Mutate(Mutation),
    /// `~ fire <event>` — pushes a `Fired` ledger entry.
    Fire {
        event: String,
    },
    Annotation {
        name: String,
        body: String,
    },
    /// `each visit` → branches are tried in order; the playhead
    /// enters the first `kind` that matches its visit count
    /// (`first` on visit 1, `then` for the middle visits, `finally`
    /// on the last). `next` cycles through `then` branches in order
    /// after the first visit.
    EachVisit {
        branches: Vec<VisitBranch>,
    },
    /// `after $expr` followed by an optional `otherwise` arm. The
    /// playhead descends into `body_if` when `cond` evaluates truthy,
    /// otherwise into `body_else` (or skips when `body_else` is empty).
    After {
        cond: Expr,
        body_if: Vec<Item>,
        body_else: Vec<Item>,
    },
    /// `match $expr` with arms keyed by the rendered string form of
    /// each arm's discriminant.
    Match {
        scrutinee: Expr,
        arms: Vec<(String, Vec<Item>)>,
    },
    /// Section-level `if <expr>` placed before a content block. The
    /// playhead descends into `body` only when truthy.
    Conditional {
        cond: Expr,
        body: Vec<Item>,
    },
    /// Catch-all for productions Phase 2 doesn't compile further. The
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
        lets: Vec::new(),
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
            nk::LET_BINDING => {
                if let Some(binding) = compile_let_binding(child) {
                    doc.lets.push(binding);
                }
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

fn compile_let_binding(node: &SyntaxNode) -> Option<LetBinding> {
    let name = first_ident_value(node)?;
    // The let binding's children are [IDENT(name), <expr>]. Grab the
    // first non-ident child as the expression body.
    let expr_node = node.children.iter().find(|c| c.kind != nk::IDENT)?;
    Some(LetBinding {
        name,
        body: compile_expr(expr_node),
    })
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
        guard: None,
        items: Vec::new(),
    };
    for child in &node.children {
        match child.kind.as_str() {
            nk::MODIFIER => {
                if let Some(name) = first_ident_value(child) {
                    section.modifiers.push(name);
                }
            }
            nk::GUARD => {
                if let Some(expr_node) = child.children.first() {
                    section.guard = Some(compile_expr(expr_node));
                }
            }
            nk::IDENT | nk::PARTICIPANT_SCOPE | nk::DOCSTRING => {}
            _ => {}
        }
    }
    section.items = compile_item_sequence(&node.children);
    section
}

/// Walk a slice of child nodes producing `Item`s, joining `after` with
/// any immediately-following `otherwise` into a single [`Item::After`].
/// Skips structural / metadata nodes that don't lower to items
/// (modifiers, guards, idents, participant scopes, docstrings).
fn compile_item_sequence(children: &[SyntaxNode]) -> Vec<Item> {
    let mut items = Vec::new();
    let mut i = 0;
    while i < children.len() {
        let child = &children[i];
        match child.kind.as_str() {
            nk::MODIFIER
            | nk::GUARD
            | nk::IDENT
            | nk::PARTICIPANT_SCOPE
            | nk::DOCSTRING
            | nk::HEADER
            | nk::DOC_TAG => {
                i += 1;
                continue;
            }
            nk::AFTER_BLOCK => {
                // Look ahead past blank/comment for an otherwise.
                let mut j = i + 1;
                while j < children.len() && matches!(children[j].kind.as_str(), "blank" | "comment")
                {
                    j += 1;
                }
                let otherwise = children.get(j).filter(|n| n.kind == nk::OTHERWISE_BLOCK);
                items.push(compile_after_block(child, otherwise));
                i = if otherwise.is_some() { j + 1 } else { i + 1 };
                continue;
            }
            nk::OTHERWISE_BLOCK => {
                // A stray `otherwise` with no preceding `after` — emit
                // as Other so it doesn't silently vanish.
                items.push(Item::Other {
                    node_kind: child.kind.clone(),
                    text: render_inline_text(child),
                });
                i += 1;
                continue;
            }
            _ => {
                if let Some(item) = compile_item(child) {
                    items.push(item);
                }
                i += 1;
            }
        }
    }
    items
}

fn compile_item(node: &SyntaxNode) -> Option<Item> {
    Some(match node.kind.as_str() {
        nk::DIALOGUE => compile_dialogue(node),
        nk::FLAVOR_LINE => Item::Flavor {
            text: collect_text_segments(node),
        },
        nk::STAGE_DIRECTION => Item::Stage {
            text: collect_text_segments(node),
        },
        nk::CHOICE => compile_choice(node),
        nk::DIVERT => compile_divert(node),
        nk::RETURN_LINE => Item::Return {
            thread: first_ident_value(node),
        },
        nk::ACTION_LINE => compile_action_line(node)?,
        nk::ANNOTATION => compile_annotation(node),
        nk::EACH_VISIT_BLOCK => compile_each_visit(node),
        nk::AFTER_BLOCK => compile_after_block(node, None),
        nk::MATCH_BLOCK => compile_match(node),
        "blank" | "comment" => return None,
        // Catch-all for productions we don't recognize yet.
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

    let mut lines: Vec<Vec<TextSeg>> = Vec::new();
    for line in node.children.iter().filter(|c| c.kind == nk::TEXT_LINE) {
        lines.push(collect_text_segments(line));
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
        .map(collect_text_segments)
        .unwrap_or_default();

    let guard = node
        .children
        .iter()
        .find(|c| c.kind == nk::GUARD)
        .and_then(|g| g.children.first())
        .map(compile_expr);

    let mut body = Vec::new();
    // Inline `-> target` on the same line as the choice — compile it
    // as a Divert at the head of the body.
    if let Some(div) = node.children.iter().find(|c| c.kind == nk::DIVERT) {
        body.push(compile_divert(div));
    }
    // Nested content block.
    if let Some(block) = node.children.iter().find(|c| c.kind == "content_block") {
        body.extend(compile_item_sequence(&block.children));
    }

    Item::Choice {
        once,
        label,
        guard,
        body,
    }
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

/// Compile an `action_line` into the appropriate [`Item`] flavour.
/// Returns `None` for the (rare) empty action line.
fn compile_action_line(node: &SyntaxNode) -> Option<Item> {
    // Each action_line wraps either a `mutation_expr`, a
    // `keyword_action`, a `namespace_call`, or an error.
    if let Some(mut_expr) = node.children.iter().find(|c| c.kind == nk::MUTATION_EXPR) {
        return Some(Item::Mutate(compile_mutation(mut_expr)?));
    }
    let ka = node
        .children
        .iter()
        .find(|c| c.kind == nk::KEYWORD_ACTION)?;
    let kw = first_ident_value(ka).unwrap_or_default();
    let payload = ka
        .children
        .iter()
        .find(|c| c.kind == nk::PROPERTY_VALUE)
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    // Special-case the keywords that *do* mutate runtime state: `fire`
    // and `var $x := v`. Everything else stays opaque as `Item::Action`
    // for the host to dispatch.
    match kw.as_str() {
        "fire" => Some(Item::Fire {
            event: payload.split_whitespace().next().unwrap_or("").to_string(),
        }),
        "var" => parse_var_payload(&payload)
            .map(Item::Mutate)
            .or(Some(Item::Action {
                keyword: kw,
                payload,
            })),
        _ => Some(Item::Action {
            keyword: kw,
            payload,
        }),
    }
}

fn compile_mutation(node: &SyntaxNode) -> Option<Mutation> {
    // Children: [RESOLVE_REF, assign_op, <expr>?]
    let lhs = node.children.iter().find(|c| c.kind == nk::RESOLVE_REF)?;
    let (name, chain) = resolve_ref_path(lhs);
    let op_text = node
        .children
        .iter()
        .find(|c| c.kind == "assign_op")
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let op = match op_text.as_str() {
        ":=" => AssignOp::Set,
        "+=" => AssignOp::PlusEq,
        "-=" => AssignOp::MinusEq,
        "++" => AssignOp::Inc,
        _ => return None,
    };
    let rhs = node
        .children
        .iter()
        .rfind(|c| !matches!(c.kind.as_str(), nk::RESOLVE_REF) && c.kind != "assign_op")
        .map(compile_expr);
    Some(Mutation {
        name,
        chain,
        op,
        rhs,
    })
}

fn resolve_ref_path(node: &SyntaxNode) -> (String, Vec<String>) {
    let name = node
        .children
        .iter()
        .find(|c| c.kind == nk::IDENT)
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let mut chain = Vec::new();
    if let Some(fc) = node.children.iter().find(|c| c.kind == nk::FIELD_CHAIN) {
        for seg in &fc.children {
            if matches!(seg.kind.as_str(), nk::FIELD_ACCESS | nk::SAFE_NAV) {
                if let Some(id) = seg
                    .children
                    .iter()
                    .find(|c| c.kind == nk::IDENT)
                    .and_then(|c| c.value.clone())
                {
                    chain.push(id);
                }
            }
        }
    }
    (name, chain)
}

/// `~ var $x := value` falls through the registry as
/// `KeywordAction("var", "$x := value")`. The parser doesn't crack
/// the tail any further than a `property_value` blob, so we have to
/// re-parse it here. The acceptable forms are intentionally narrow:
/// `$ident(.field)* := <literal>` — anything richer (`$x := $y + 1`)
/// stays as a generic [`Item::Action`] and the host can dispatch it
/// manually.
fn parse_var_payload(payload: &str) -> Option<Mutation> {
    let payload = payload.trim();
    let payload = payload.strip_prefix('$')?;
    let assign_pos = payload.find(":=")?;
    let (lhs, rhs) = payload.split_at(assign_pos);
    let rhs = rhs[2..].trim();
    let lhs = lhs.trim();
    let mut parts = lhs.split('.');
    let name = parts.next()?.to_string();
    let chain: Vec<String> = parts.map(|p| p.to_string()).collect();
    let rhs_value = parse_literal_value(rhs)?;
    Some(Mutation {
        name,
        chain,
        op: AssignOp::Set,
        rhs: Some(Expr::Lit(rhs_value)),
    })
}

fn parse_literal_value(raw: &str) -> Option<Value> {
    let raw = raw.trim();
    if raw == "true" {
        return Some(Value::Bool(true));
    }
    if raw == "false" {
        return Some(Value::Bool(false));
    }
    if raw == "nil" {
        return Some(Value::Nil);
    }
    if let Ok(i) = raw.parse::<i64>() {
        return Some(Value::Int(i));
    }
    if let Ok(f) = raw.parse::<f64>() {
        return Some(Value::Float(f));
    }
    if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
        return Some(Value::Str(raw[1..raw.len() - 1].to_string()));
    }
    None
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

/// Walk the inline-text children of `node` and produce a flat
/// [`Vec<TextSeg>`] preserving every renderable atom (literal,
/// resolve, static ref, backlink) plus the side-effect markers the
/// playhead needs to fire (inline assigns). Phase 2 keeps things
/// simple: text variations render their first variant, triggers /
/// conditional triggers / range closers are dropped (their semantics
/// land with the live-performance pipeline in Phase 3+).
fn collect_text_segments(node: &SyntaxNode) -> Vec<TextSeg> {
    let mut out = Vec::new();
    for child in &node.children {
        if child.kind == nk::TEXT_CONTENT {
            collect_segments_into(child, &mut out);
        } else if child.kind == nk::LITERAL_RUN {
            push_literal(&mut out, child.value.as_deref().unwrap_or(""));
        }
    }
    coalesce_literals(out)
}

fn render_inline_text(node: &SyntaxNode) -> String {
    // For unmapped node kinds in `Item::Other` — flatten everything
    // we can find into a debug-friendly string.
    fn walk(n: &SyntaxNode, buf: &mut String) {
        if let Some(v) = n.value.as_deref() {
            if !v.is_empty() {
                if !buf.is_empty() && !buf.ends_with(' ') {
                    buf.push(' ');
                }
                buf.push_str(v.trim());
            }
        }
        for c in &n.children {
            walk(c, buf);
        }
    }
    let mut buf = String::new();
    walk(node, &mut buf);
    buf.trim().to_string()
}

fn collect_segments_into(node: &SyntaxNode, out: &mut Vec<TextSeg>) {
    match node.kind.as_str() {
        nk::LITERAL_RUN | nk::ESCAPED_CHAR => {
            push_literal(out, node.value.as_deref().unwrap_or(""));
        }
        nk::BACKLINK => {
            let lit_runs: Vec<&str> = node
                .children
                .iter()
                .filter(|c| c.kind == nk::LITERAL_RUN)
                .filter_map(|c| c.value.as_deref())
                .collect();
            let target = lit_runs.first().copied().unwrap_or("");
            let display = lit_runs.last().copied().unwrap_or(target);
            out.push(TextSeg::Backlink {
                target: target.to_string(),
                display: display.to_string(),
            });
        }
        nk::RESOLVE_REF | nk::INLINE_EVAL => {
            out.push(TextSeg::Resolve(compile_expr(node)));
        }
        nk::STATIC_REF => {
            let parts: Vec<String> = node
                .children
                .iter()
                .filter(|c| c.kind == nk::IDENT)
                .filter_map(|c| c.value.clone())
                .collect();
            out.push(TextSeg::StaticRef(parts.join(".")));
        }
        nk::INLINE_ASSIGN => {
            if let Some(m) = compile_inline_assign(node) {
                out.push(TextSeg::Assign(m));
            }
        }
        nk::INLINE_TRIGGER | nk::CHAIN_TRIGGER | nk::COND_TRIGGER | nk::RANGE_CLOSER => {
            // Side-effect markers — they belong to the live-performance
            // pipeline, not the text stream.
        }
        nk::TEXT_VARIATION => {
            // Render the first variant — cycle / shuffle / weighted
            // pickers land in Phase 3+ with the RNG + per-section state.
            if let Some(first) = node
                .children
                .iter()
                .find(|c| c.kind == nk::VARIATION_VARIANT)
            {
                collect_segments_into(first, out);
            }
        }
        _ => {
            for c in &node.children {
                collect_segments_into(c, out);
            }
        }
    }
}

fn compile_inline_assign(node: &SyntaxNode) -> Option<Mutation> {
    let lhs = node.children.iter().find(|c| c.kind == nk::RESOLVE_REF)?;
    let (name, chain) = resolve_ref_path(lhs);
    let op_text = node
        .children
        .iter()
        .find(|c| c.kind == "assign_op")
        .and_then(|c| c.value.clone())
        .unwrap_or_default();
    let op = match op_text.as_str() {
        ":=" => AssignOp::Set,
        "+=" => AssignOp::PlusEq,
        "-=" => AssignOp::MinusEq,
        "++" => AssignOp::Inc,
        _ => return None,
    };
    let rhs = node
        .children
        .iter()
        .rfind(|c| !matches!(c.kind.as_str(), nk::RESOLVE_REF) && c.kind != "assign_op")
        .map(compile_expr);
    Some(Mutation {
        name,
        chain,
        op,
        rhs,
    })
}

fn push_literal(out: &mut Vec<TextSeg>, s: &str) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return;
    }
    out.push(TextSeg::Literal(trimmed.to_string()));
}

/// Glue adjacent `Literal` segments into one so renderers don't have
/// to special-case the space-between-runs case.
fn coalesce_literals(segs: Vec<TextSeg>) -> Vec<TextSeg> {
    let mut out: Vec<TextSeg> = Vec::with_capacity(segs.len());
    for seg in segs {
        if let (Some(TextSeg::Literal(prev)), TextSeg::Literal(next)) = (out.last_mut(), &seg) {
            if !prev.is_empty() && !next.is_empty() {
                prev.push(' ');
            }
            prev.push_str(next);
            continue;
        }
        out.push(seg);
    }
    out
}

// ─── Block constructs (each visit / after / otherwise / match) ─────

fn compile_each_visit(node: &SyntaxNode) -> Item {
    let mut branches = Vec::new();
    for child in &node.children {
        if child.kind != nk::VISIT_BRANCH {
            continue;
        }
        let kind = child
            .children
            .iter()
            .find(|c| c.kind == nk::IDENT)
            .and_then(|c| c.value.clone())
            .unwrap_or_else(|| "then".to_string());
        let body = compile_content_block(child);
        branches.push(VisitBranch { kind, body });
    }
    Item::EachVisit { branches }
}

/// `after $expr` may be followed by an `otherwise` sibling in the
/// parent's child list. The caller threads that sibling in via
/// `following_otherwise` — `compile_section` and `compile_content_block`
/// pick it up by index. Here we only handle the local form (no
/// `otherwise`); the section-level joining logic lives in
/// [`compile_content_block`] / [`compile_section`].
fn compile_after_block(node: &SyntaxNode, otherwise: Option<&SyntaxNode>) -> Item {
    let cond = node
        .children
        .first()
        .map(compile_expr)
        .unwrap_or(Expr::Lit(Value::Nil));
    let body_if = compile_content_block(node);
    let body_else = otherwise.map(compile_content_block).unwrap_or_default();
    Item::After {
        cond,
        body_if,
        body_else,
    }
}

fn compile_match(node: &SyntaxNode) -> Item {
    let scrutinee = node
        .children
        .iter()
        .find(|c| !matches!(c.kind.as_str(), nk::MATCH_ARM))
        .map(compile_expr)
        .unwrap_or(Expr::Lit(Value::Nil));
    let mut arms = Vec::new();
    for arm in node.children.iter().filter(|c| c.kind == nk::MATCH_ARM) {
        let head = arm
            .children
            .iter()
            .find(|c| matches!(c.kind.as_str(), nk::IDENT | nk::STRING | nk::NUMBER))
            .and_then(|c| c.value.clone())
            .unwrap_or_default();
        let body = compile_content_block(arm);
        arms.push((head, body));
    }
    Item::Match { scrutinee, arms }
}

/// Pull the `content_block` child off a block-style node (each-visit
/// branch, after / otherwise, match arm) and compile its items.
fn compile_content_block(node: &SyntaxNode) -> Vec<Item> {
    for c in &node.children {
        if c.kind == "content_block" {
            return compile_item_sequence(&c.children);
        }
    }
    Vec::new()
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

    fn render_segs(segs: &[TextSeg]) -> String {
        segs.iter()
            .filter_map(|s| match s {
                TextSeg::Literal(s) => Some(s.clone()),
                TextSeg::Backlink { display, .. } => Some(display.clone()),
                TextSeg::StaticRef(s) => Some(format!("@{s}")),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(" ")
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
                assert!(render_segs(&lines[0]).contains("Hello"));
            }
            other => panic!("expected dialogue, got {other:?}"),
        }
        match &start.items[1] {
            Item::Choice {
                once, label, body, ..
            } => {
                assert!(*once, "`*` choices should compile as once");
                assert!(render_segs(label).contains("help"));
                assert!(
                    matches!(body.first(), Some(Item::Divert { target, .. }) if target == "next"),
                    "inline divert should land at the head of the choice body",
                );
            }
            other => panic!("expected choice, got {other:?}"),
        }
    }

    #[test]
    fn captures_choice_guard_expression() {
        let src = "# d\ncast WREN\n  .label x\n-- s\nWREN\n  hi\n  * leave if not $trusted\n    -> done\n-- done\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let choice = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::Choice { guard, .. } => Some(guard.clone()),
                _ => None,
            })
            .expect("choice present");
        assert!(choice.is_some(), "choice should carry compiled guard");
        assert!(matches!(choice.as_ref().unwrap(), Expr::Not(_)));
    }

    #[test]
    fn captures_section_guard() {
        let src = "# d\n-- s if played(intro)\n  > body\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        assert!(s.guard.is_some(), "section should carry compiled guard");
    }

    #[test]
    fn compiles_var_action_to_mutation() {
        let src = "# d\n-- s\n~ var $trust := 30\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let m = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::Mutate(m) => Some(m.clone()),
                _ => None,
            })
            .expect("mutation");
        assert_eq!(m.name, "trust");
        assert_eq!(m.op, AssignOp::Set);
        assert!(matches!(m.rhs, Some(Expr::Lit(Value::Int(30)))));
    }

    #[test]
    fn compiles_inline_mutation_action() {
        let src = "# d\n-- s\n~ $trust := 30\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let m = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::Mutate(m) => Some(m.clone()),
                _ => None,
            })
            .expect("mutation");
        assert_eq!(m.name, "trust");
    }

    #[test]
    fn compiles_fire_action() {
        let src = "# d\n-- s\n~ fire bell_solved\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let fired = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::Fire { event } => Some(event.clone()),
                _ => None,
            })
            .expect("fire");
        assert_eq!(fired, "bell_solved");
    }

    #[test]
    fn compiles_let_binding() {
        let src = "# d\nlet trusted = $trust > 50\n";
        let db = compile_src(src);
        assert_eq!(db.documents[0].lets.len(), 1);
        assert_eq!(db.documents[0].lets[0].name, "trusted");
        assert!(matches!(db.documents[0].lets[0].body, Expr::Gt(_, _)));
    }

    #[test]
    fn compiles_after_otherwise_pair() {
        let src = "# d\n-- s\nafter $trusted\n  > yes\notherwise\n  > no\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let pair = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::After {
                    cond,
                    body_if,
                    body_else,
                } => Some((cond.clone(), body_if.clone(), body_else.clone())),
                _ => None,
            })
            .expect("after");
        assert!(matches!(pair.0, Expr::Resolve { .. }));
        assert!(!pair.1.is_empty(), "if-body should hold the `> yes` flavor");
        assert!(
            !pair.2.is_empty(),
            "else-body should hold the `> no` flavor"
        );
    }

    #[test]
    fn compiles_each_visit_block() {
        let src =
            "# d\n-- s\neach visit\n  first\n    > hi\n  then\n    > again\n  finally\n    > done\n";
        let db = compile_src(src);
        let s = &db.documents[0].sections["s"];
        let branches = s
            .items
            .iter()
            .find_map(|i| match i {
                Item::EachVisit { branches } => Some(branches.clone()),
                _ => None,
            })
            .expect("each visit");
        let kinds: Vec<&str> = branches.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, vec!["first", "then", "finally"]);
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
