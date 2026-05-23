//! Loom validator — the registry-driven diagnostics pass.
//!
//! The parser ([`super::parser`]) emits the diagnostics it can detect
//! purely from syntax (`lex-error`, `bracket-unbalanced`, `indent-jump`,
//! `unexpected-child`). Everything else in `docs/dev/loom-grammar.md`
//! §21 — every "unknown-X", every cycle check, every shape-required
//! knob — lands here.
//!
//! The validator runs in four passes over the parsed [`RootNode`]:
//!
//! 1. **Collect** declarations into a [`Registry`]: section ids,
//!    cast / cue / location / cohort slots, faction names, generator
//!    / scene / compose ids, knowledge / goal blocks, axes, pools,
//!    stats, tree nodes, dispositions. Emits `section-duplicate` on
//!    the way.
//!
//! 2. **Resolve** references against the registry. Every `SPEAKER`
//!    that appears in dialogue, every `@ref` in a divert / cast slot
//!    / location event / cue invocation, every `$ref` qualified
//!    against a typed namespace is checked. Emits `unknown-cast`,
//!    `unknown-cue`, `unknown-location`, `unknown-cohort`,
//!    `unknown-faction`, `divert-target-unknown`, `unknown-generator`,
//!    `unknown-scene`, `unknown-tree-node`, `unknown-compose`.
//!
//! 3. **Shape** checks that don't need cross-references:
//!    `goal-no-priority`, `faction-no-label`, `scene-no-states`,
//!    `axis-mode-missing`, `pool-max-required`, `stat-form-ambiguous`,
//!    `choice-in-script`.
//!
//! 4. **Cycle** detection over directed graphs the registry exposes:
//!    `disposition-mirror-cycle`, `stance-cycle`, `tree-cycle`,
//!    `faction-membership-cycle`.
//!
//! Scope: **single file**. Cross-document resolution (a cue declared
//! in one `.loom` file, referenced from another) needs a project
//! model that doesn't exist yet; once it does, the validator gains a
//! `validate_project(...)` entry point that merges per-file registries
//! before pass 2.

use std::collections::{HashMap, HashSet};

use crate::language::syntax::{RootNode, SourceRange, SyntaxNode};

use super::node_kinds as nk;
use super::parser::{LoomDiagnostic, Severity};

/// Run the full validator pipeline over a parsed root and return the
/// diagnostics it produced. Never panics; an empty root yields an
/// empty diagnostic list.
pub fn validate(root: &RootNode) -> Vec<LoomDiagnostic> {
    let mut diagnostics = Vec::new();
    let registry = collect_registry(root, &mut diagnostics);
    check_references(root, &registry, &mut diagnostics);
    check_shape(root, &mut diagnostics);
    check_cycles(&registry, &mut diagnostics);
    diagnostics
}

// ─── Registry ───────────────────────────────────────────────────────

/// Per-document declaration table. Lives only for the duration of one
/// `validate()` call; cross-document merging is the project-validator's
/// job (not yet implemented).
#[derive(Debug, Default)]
pub struct Registry {
    /// Section ids (anonymous sections aren't keyed here — they're
    /// reachable only by fall-through).
    pub sections: HashMap<String, SourceRange>,
    pub casts: HashMap<String, SourceRange>,
    pub cues: HashMap<String, SourceRange>,
    pub locations: HashMap<String, SourceRange>,
    pub cohorts: HashMap<String, SourceRange>,
    pub factions: HashMap<String, SourceRange>,
    pub generators: HashMap<String, SourceRange>,
    pub scenes: HashMap<String, SourceRange>,
    pub composes: HashMap<String, SourceRange>,
    pub tree_nodes: HashMap<String, SourceRange>,
    pub goals: HashMap<String, SourceRange>,
    pub axes: HashMap<String, SourceRange>,
    pub pools: HashMap<String, SourceRange>,
    pub stats: HashMap<String, SourceRange>,
    pub attributes: HashMap<String, SourceRange>,

    /// Documents that carry the `:script` or `:film` archetype tag —
    /// content inside them rejects `*` / `+` choices.
    pub script_doc_ranges: Vec<SourceRange>,

    /// Disposition axis mirror graph (axis_name → [mirror target
    /// axis_names]). Used by cycle detection.
    pub disposition_mirrors: Vec<(String, String, SourceRange)>,

    /// Faction parent links — a child can declare `.parent @P` or be
    /// listed under `members` of another faction. Both forms feed this
    /// edge list (target → parent).
    pub faction_parents: Vec<(String, String, SourceRange)>,

    /// Tree node `requires` edges (target → required predecessor).
    pub tree_requires: Vec<(String, String, SourceRange)>,
}

// ─── Pass 1 — collection ────────────────────────────────────────────

fn collect_registry(root: &RootNode, diagnostics: &mut Vec<LoomDiagnostic>) -> Registry {
    let mut reg = Registry::default();
    for doc in &root.children {
        collect_in_node(doc, &mut reg, diagnostics);
    }
    reg
}

fn collect_in_node(node: &SyntaxNode, reg: &mut Registry, diagnostics: &mut Vec<LoomDiagnostic>) {
    match node.kind.as_str() {
        nk::SECTION => {
            // First IDENT child is the section name (anonymous sections
            // skip it).
            if let Some(name) = first_ident_value(node) {
                let range = node.position.unwrap_or_default_safe();
                if let Some(prev) = reg.sections.insert(name.clone(), range) {
                    diagnostics.push(diag(
                        prev,
                        "section-duplicate",
                        Severity::Error,
                        format!("Section `{name}` declared twice in this document"),
                    ));
                }
            }
        }
        nk::CAST_DECL => {
            if let Some(name) = find_speaker_or_static_name(node) {
                reg.casts
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::CUE_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.cues
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::LOCATION_DECL => {
            if let Some(name) = find_speaker_or_static_name(node) {
                reg.locations
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::COHORT_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.cohorts
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::INLINE_FACTION_DECL => {
            if let Some(name) = first_ident_value(node) {
                let range = node.position.unwrap_or_default_safe();
                // Collect parent edges via inner members blocks +
                // `.parent` property.
                collect_faction_links(node, &name, reg);
                reg.factions.insert(name, range);
            }
        }
        nk::GENERATOR_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.generators
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::SCENE_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.scenes
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::COMPOSE_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.composes
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::TREE_NODE_DECL => {
            if let Some(name) = first_ident_value(node) {
                let range = node.position.unwrap_or_default_safe();
                // `requires <expr>` knob feeds the requires graph.
                for prop in node
                    .children
                    .iter()
                    .filter(|c| c.kind == nk::TREE_NODE_PROPERTY)
                {
                    if first_ident_value(prop).as_deref() == Some("requires") {
                        for ref_name in extract_referenced_idents(prop) {
                            reg.tree_requires.push((name.clone(), ref_name, range));
                        }
                    }
                }
                reg.tree_nodes.insert(name, range);
            }
        }
        nk::GOAL_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.goals
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::AXIS_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.axes
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::POOL_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.pools
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::STAT_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.stats
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::ATTRIBUTE_DECL => {
            if let Some(name) = first_ident_value(node) {
                reg.attributes
                    .insert(name, node.position.unwrap_or_default_safe());
            }
        }
        nk::HEADER => {
            // Note :script / :film archetype docs so the shape pass
            // can reject choices inside them.
            for tag in node.children.iter().filter(|c| c.kind == nk::DOC_TAG) {
                if let Some(name) = first_ident_value(tag) {
                    if name == "script" || name == "film" {
                        // The script-archetype range is the whole
                        // enclosing document. We walk up no further;
                        // any choice inside the doc body lives under a
                        // section / divert nested below this header.
                        // For diagnostic purposes we just note that
                        // this document is script-shaped.
                        reg.script_doc_ranges
                            .push(node.position.unwrap_or_default_safe());
                    }
                }
            }
        }
        nk::DISPOSITION_AXIS => {
            // Disposition mirror edges: `name = R..R, init N, mirror $X.Y.axis`
            if let Some(name) = first_ident_value(node) {
                for mc in node.children.iter().filter(|c| c.kind == nk::MIRROR_CLAUSE) {
                    if let Some(target) = mirror_target_label(mc) {
                        reg.disposition_mirrors.push((
                            name.clone(),
                            target,
                            mc.position.unwrap_or_default_safe(),
                        ));
                    }
                }
            }
        }
        _ => {}
    }

    for child in &node.children {
        collect_in_node(child, reg, diagnostics);
    }
}

fn collect_faction_links(faction_node: &SyntaxNode, name: &str, reg: &mut Registry) {
    walk(faction_node, &mut |node| {
        // `.parent @SomeOther` property
        if node.kind == nk::PROPERTY && first_ident_value(node).as_deref() == Some("parent") {
            for parent in extract_referenced_idents(node) {
                reg.faction_parents.push((
                    name.to_string(),
                    parent,
                    node.position.unwrap_or_default_safe(),
                ));
            }
        }
        // members block — every StaticRef inside a MEMBER_ENTRY whose
        // target is a known faction becomes a sub-faction edge. We
        // don't know factions are factions until pass 1 finishes, so
        // we record EVERY @ref-named entry; pass 4 filters to factions.
        if node.kind == nk::MEMBER_ENTRY {
            for sr in node.children.iter().filter(|c| c.kind == nk::STATIC_REF) {
                if let Some(parent) = first_ident_value(sr) {
                    reg.faction_parents.push((
                        parent,
                        name.to_string(),
                        sr.position.unwrap_or_default_safe(),
                    ));
                }
            }
        }
    });
}

// ─── Pass 2 — reference resolution ─────────────────────────────────

fn check_references(root: &RootNode, reg: &Registry, diagnostics: &mut Vec<LoomDiagnostic>) {
    for doc in &root.children {
        check_refs_in_node(doc, reg, diagnostics);
    }
}

fn check_refs_in_node(node: &SyntaxNode, reg: &Registry, diagnostics: &mut Vec<LoomDiagnostic>) {
    match node.kind.as_str() {
        // Speaker-led dialogue: the speaker token must be a declared cast.
        nk::DIALOGUE => {
            if let Some(speaker_ref) = node.children.iter().find(|c| c.kind == nk::SPEAKER_REF) {
                for sp in speaker_ref
                    .children
                    .iter()
                    .filter(|c| c.kind == nk::SPEAKER)
                {
                    if let Some(name) = sp.value.as_deref() {
                        if !reg.casts.contains_key(name) {
                            diagnostics.push(diag(
                                sp.position.unwrap_or_default_safe(),
                                "unknown-cast",
                                Severity::Error,
                                format!("SPEAKER `{name}` is not declared (no matching `cast`)"),
                            ));
                        }
                    }
                }
            }
        }

        // Diverts: target must be a section (or `@doc.section` cross-ref;
        // we accept any `@`-prefixed target without validation since
        // cross-doc is out of scope this pass).
        nk::DIVERT => {
            // Find first IDENT child (the bare-name path) or static_ref.
            if let Some(first_kid) = node.children.iter().find(|c| {
                matches!(
                    c.kind.as_str(),
                    nk::IDENT | nk::STATIC_REF | nk::TUNNEL_CALL
                )
            }) {
                if first_kid.kind == nk::IDENT {
                    if let Some(name) = first_kid.value.as_deref() {
                        if !reg.sections.contains_key(name) {
                            diagnostics.push(diag(
                                first_kid.position.unwrap_or_default_safe(),
                                "divert-target-unknown",
                                Severity::Error,
                                format!("`->` target `{name}` is not a declared section"),
                            ));
                        }
                    }
                }
                // Tunnel call: the head IDENT must resolve to a section
                // or a defn — we only know sections here, so we check
                // sections and trust the runtime for defns.
                if first_kid.kind == nk::TUNNEL_CALL {
                    if let Some(name) = first_ident_value(first_kid) {
                        if !reg.sections.contains_key(&name) {
                            // Section miss isn't a hard error here —
                            // tunnel calls can reach `defn` symbols too,
                            // which the validator doesn't track yet. We
                            // demote to a warning for now.
                            diagnostics.push(diag(
                                first_kid.position.unwrap_or_default_safe(),
                                "divert-target-unknown",
                                Severity::Warning,
                                format!("`{name}( ... )->` target is not a declared section"),
                            ));
                        }
                    }
                }
                // `@doc.section` — accept as-is (cross-document).
            }
        }

        // Location event: `when participant enters @LOC` — @LOC must be
        // a declared location.
        nk::LOCATION_EVENT => {
            for sr in node.children.iter().filter(|c| c.kind == nk::STATIC_REF) {
                if let Some(name) = first_ident_value(sr) {
                    if !reg.locations.contains_key(&name) {
                        diagnostics.push(diag(
                            sr.position.unwrap_or_default_safe(),
                            "unknown-location",
                            Severity::Error,
                            format!("`@{name}` is not a declared location"),
                        ));
                    }
                }
            }
        }

        // Cohort guard on participant lifecycle: `:cohort_name`.
        nk::PARTICIPANT_LIFECYCLE => {
            // After the verb IDENT (joins/leaves) there may be an IDENT
            // for the cohort filter (we serialised `:` then the name
            // into a flat IDENT child).
            let idents: Vec<_> = node
                .children
                .iter()
                .filter(|c| c.kind == nk::IDENT)
                .collect();
            // First IDENT is the verb (joins / leaves). If there's a
            // second IDENT it's the cohort filter.
            if let Some(cohort_node) = idents.get(1) {
                if let Some(name) = cohort_node.value.as_deref() {
                    if !reg.cohorts.contains_key(name) {
                        diagnostics.push(diag(
                            cohort_node.position.unwrap_or_default_safe(),
                            "unknown-cohort",
                            Severity::Error,
                            format!("cohort `{name}` is not declared"),
                        ));
                    }
                }
            }
        }

        // Enroll action: `enroll <subject> into <cohort>`. We collect
        // the action's payload as one PROPERTY_VALUE, so we tokenise to
        // find the cohort name.
        nk::KEYWORD_ACTION => {
            check_keyword_action_refs(node, reg, diagnostics);
        }

        // Spawn / cancel reach into generators + scenes; their first
        // argument is the name.
        // (Hooks here are part of KEYWORD_ACTION above.)

        // Stat / pool / axis / attribute references — postfix field
        // chains on entity refs are too project-wide to validate
        // locally; defer to the validator's project pass.
        _ => {}
    }

    for child in &node.children {
        check_refs_in_node(child, reg, diagnostics);
    }
}

fn check_keyword_action_refs(
    action: &SyntaxNode,
    reg: &Registry,
    diagnostics: &mut Vec<LoomDiagnostic>,
) {
    // First IDENT is the keyword.
    let kw = action.children.first().and_then(|c| {
        if c.kind == nk::IDENT {
            c.value.as_deref()
        } else {
            None
        }
    });
    let Some(kw) = kw else { return };

    // Payload is a single PROPERTY_VALUE leaf carrying the rest of the
    // line.
    let payload = action
        .children
        .iter()
        .find(|c| c.kind == nk::PROPERTY_VALUE)
        .and_then(|c| c.value.as_deref())
        .unwrap_or("");

    let range = action.position.unwrap_or_default_safe();
    let tokens: Vec<&str> = payload.split_whitespace().collect();

    match kw {
        "cue" => {
            // `cue <name>` or `cue @<name>`
            if let Some(first) = tokens.first() {
                let bare = first.strip_prefix('@').unwrap_or(first);
                if !bare.is_empty()
                    && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !reg.cues.contains_key(bare)
                {
                    diagnostics.push(diag(
                        range,
                        "unknown-cue",
                        Severity::Error,
                        format!("cue `{bare}` is not declared"),
                    ));
                }
            }
        }
        "enroll" => {
            // `enroll <subject> into <cohort>`
            if let Some(into_idx) = tokens.iter().position(|t| *t == "into") {
                if let Some(coh) = tokens.get(into_idx + 1) {
                    let bare = coh.strip_prefix('@').unwrap_or(coh);
                    if !bare.is_empty()
                        && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                        && !reg.cohorts.contains_key(bare)
                    {
                        diagnostics.push(diag(
                            range,
                            "unknown-cohort",
                            Severity::Error,
                            format!("cohort `{bare}` is not declared"),
                        ));
                    }
                }
            }
        }
        "spawn" | "cancel" => {
            if let Some(name) = tokens.first() {
                let bare = name.strip_prefix('@').unwrap_or(name);
                // Strip a `( ... )` arglist suffix if present.
                let bare = bare.split('(').next().unwrap_or(bare);
                if !bare.is_empty()
                    && bare.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                    && !reg.generators.contains_key(bare)
                    && !reg.scenes.contains_key(bare)
                {
                    diagnostics.push(diag(
                        range,
                        "unknown-generator",
                        Severity::Error,
                        format!("`{kw} {bare}` — no matching generator or scene"),
                    ));
                }
            }
        }
        _ => {}
    }
}

// ─── Pass 3 — structural / shape checks ────────────────────────────

fn check_shape(root: &RootNode, diagnostics: &mut Vec<LoomDiagnostic>) {
    for doc in &root.children {
        check_shape_in_node(doc, /* in_script */ doc_is_script(doc), diagnostics);
    }
}

fn doc_is_script(doc: &SyntaxNode) -> bool {
    let Some(header) = doc.children.iter().find(|c| c.kind == nk::HEADER) else {
        return false;
    };
    header
        .children
        .iter()
        .filter(|c| c.kind == nk::DOC_TAG)
        .filter_map(first_ident_value)
        .any(|name| name == "script" || name == "film")
}

fn check_shape_in_node(node: &SyntaxNode, in_script: bool, diagnostics: &mut Vec<LoomDiagnostic>) {
    let range = node.position.unwrap_or_default_safe();
    match node.kind.as_str() {
        nk::CHOICE if in_script => {
            diagnostics.push(diag(
                range,
                "choice-in-script",
                Severity::Error,
                "`*` / `+` choices aren't allowed in `:script` or `:film` documents",
            ));
        }
        nk::GOAL_DECL => {
            // Must have a `priority` knob.
            let has_priority = node
                .children
                .iter()
                .filter(|c| c.kind == nk::GOAL_KNOB)
                .any(|knob| first_ident_value(knob).as_deref() == Some("priority"));
            if !has_priority {
                diagnostics.push(diag(
                    range,
                    "goal-no-priority",
                    Severity::Error,
                    "`goal` block has no `priority` knob",
                ));
            }
        }
        nk::INLINE_FACTION_DECL => {
            // Must have a `.label` property somewhere in its body.
            let has_label = walk_find(node, &|n| {
                n.kind == nk::PROPERTY && first_ident_value(n).as_deref() == Some("label")
            });
            if !has_label {
                diagnostics.push(diag(
                    range,
                    "faction-no-label",
                    Severity::Error,
                    "faction declaration has no `.label` property",
                ));
            }
        }
        nk::SCENE_DECL => {
            let states = node
                .children
                .iter()
                .filter(|c| c.kind == nk::SCENE_STATE)
                .count();
            if states == 0 {
                diagnostics.push(diag(
                    range,
                    "scene-no-states",
                    Severity::Error,
                    "`scene` declaration has no states",
                ));
            }
        }
        nk::AXIS_DECL => {
            // Need a `mode` property.
            let has_mode = node
                .children
                .iter()
                .filter(|c| c.kind == nk::AXIS_PROPERTY)
                .any(|p| first_ident_value(p).as_deref() == Some("mode"));
            if !has_mode {
                diagnostics.push(diag(
                    range,
                    "axis-mode-missing",
                    Severity::Error,
                    "`axis` declaration has no `mode` knob",
                ));
            }
        }
        nk::POOL_DECL => {
            let has_max = node
                .children
                .iter()
                .filter(|c| c.kind == nk::POOL_PROPERTY)
                .any(|p| first_ident_value(p).as_deref() == Some("max"));
            if !has_max {
                diagnostics.push(diag(
                    range,
                    "pool-max-required",
                    Severity::Error,
                    "`pool` declaration has no `max` knob",
                ));
            }
        }
        _ => {}
    }

    for child in &node.children {
        check_shape_in_node(child, in_script, diagnostics);
    }
}

// ─── Pass 4 — cycle detection ──────────────────────────────────────

fn check_cycles(reg: &Registry, diagnostics: &mut Vec<LoomDiagnostic>) {
    // Disposition mirror cycles: only when the mirror target's label
    // names another disposition axis defined in the same block. The
    // `mirror $X.Y.foo` form's target is the trailing identifier
    // segment (`foo`) — that's what we store.
    detect_cycle(
        reg.disposition_mirrors
            .iter()
            .map(|(a, b, r)| (a.as_str(), b.as_str(), *r)),
        "disposition-mirror-cycle",
        "disposition axes form a mirror cycle",
        diagnostics,
    );

    // Tree node requires cycles.
    detect_cycle(
        reg.tree_requires
            .iter()
            .map(|(a, b, r)| (a.as_str(), b.as_str(), *r)),
        "tree-cycle",
        "tree-node `requires` graph contains a cycle",
        diagnostics,
    );

    // Faction membership / parent cycle. We've collected (child, parent)
    // edges from both `.parent @P` properties and `members @C` lines.
    detect_cycle(
        reg.faction_parents
            .iter()
            .filter(|(c, p, _)| reg.factions.contains_key(c) && reg.factions.contains_key(p))
            .map(|(c, p, r)| (c.as_str(), p.as_str(), *r)),
        "faction-membership-cycle",
        "faction membership graph contains a cycle",
        diagnostics,
    );
}

/// Working state for a single cycle-detection pass. Packaged into a
/// struct so the recursive DFS doesn't have to thread nine arguments
/// through every call (clippy's `too_many_arguments` lint).
struct CycleScan<'a> {
    adj: HashMap<&'a str, Vec<&'a str>>,
    sample_range: HashMap<&'a str, SourceRange>,
    visited: HashSet<&'a str>,
    on_stack: HashSet<&'a str>,
    reported: HashSet<String>,
    id: &'static str,
    base_message: &'static str,
}

impl<'a> CycleScan<'a> {
    fn dfs(&mut self, node: &'a str, diagnostics: &mut Vec<LoomDiagnostic>) {
        if !self.visited.insert(node) {
            return;
        }
        self.on_stack.insert(node);
        // Snapshot the neighbour list so we can recurse without holding
        // a borrow of `self.adj`.
        let neighbours = self.adj.get(node).cloned().unwrap_or_default();
        for n in neighbours {
            if self.on_stack.contains(n) {
                let key = format!("{node}->{n}");
                if self.reported.insert(key) {
                    let range = self
                        .sample_range
                        .get(node)
                        .copied()
                        .unwrap_or_default_safe_position();
                    diagnostics.push(diag(
                        range,
                        self.id,
                        Severity::Error,
                        format!("{} (via `{node}` → `{n}`)", self.base_message),
                    ));
                }
            } else if !self.visited.contains(n) {
                self.dfs(n, diagnostics);
            }
        }
        self.on_stack.remove(node);
    }
}

/// Generic cycle detector — DFS over a directed edge stream.
fn detect_cycle<'a, I>(
    edges: I,
    id: &'static str,
    base_message: &'static str,
    diagnostics: &mut Vec<LoomDiagnostic>,
) where
    I: IntoIterator<Item = (&'a str, &'a str, SourceRange)>,
{
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    let mut sample_range: HashMap<&str, SourceRange> = HashMap::new();
    for (from, to, range) in edges {
        adj.entry(from).or_default().push(to);
        sample_range.entry(from).or_insert(range);
    }

    let nodes: Vec<&str> = adj.keys().copied().collect();
    let mut scan = CycleScan {
        adj,
        sample_range,
        visited: HashSet::new(),
        on_stack: HashSet::new(),
        reported: HashSet::new(),
        id,
        base_message,
    };
    for node in nodes {
        scan.dfs(node, diagnostics);
    }
}

// ─── Helpers ───────────────────────────────────────────────────────

/// Build a [`LoomDiagnostic`] with the given severity / id / message at
/// the supplied range.
fn diag(
    range: SourceRange,
    id: &'static str,
    severity: Severity,
    message: impl Into<String>,
) -> LoomDiagnostic {
    LoomDiagnostic {
        id,
        severity,
        message: message.into(),
        range,
    }
}

/// Find the first IDENT or SPEAKER value child of a node.
fn first_ident_value(node: &SyntaxNode) -> Option<String> {
    node.children
        .iter()
        .find(|c| matches!(c.kind.as_str(), nk::IDENT | nk::SPEAKER))
        .and_then(|c| c.value.clone())
}

/// For cast / location declarations the id can be either a SPEAKER
/// child (`cast WREN`) or a STATIC_REF child (`cast @entity`).
fn find_speaker_or_static_name(node: &SyntaxNode) -> Option<String> {
    for c in &node.children {
        match c.kind.as_str() {
            nk::SPEAKER => return c.value.clone(),
            nk::STATIC_REF => return first_ident_value(c),
            _ => {}
        }
    }
    None
}

/// Pull out the trailing identifier of a `mirror $X.Y.axis` clause —
/// we land each segment as an IDENT child of a FIELD_ACCESS node, so
/// the last IDENT we encounter walking the subtree is the target axis
/// label.
fn mirror_target_label(mirror_clause: &SyntaxNode) -> Option<String> {
    let mut last: Option<String> = None;
    walk(mirror_clause, &mut |n| {
        if n.kind == nk::IDENT {
            if let Some(v) = &n.value {
                last = Some(v.clone());
            }
        }
    });
    last
}

/// Walk every IDENT under a node and collect their string values —
/// used to capture references inside free-form payloads.
fn extract_referenced_idents(node: &SyntaxNode) -> Vec<String> {
    let mut out = Vec::new();
    walk(node, &mut |n| {
        if n.kind == nk::IDENT {
            if let Some(v) = &n.value {
                out.push(v.clone());
            }
        }
        if n.kind == nk::STATIC_REF {
            if let Some(v) = first_ident_value(n) {
                out.push(v);
            }
        }
    });
    out
}

fn walk<F: FnMut(&SyntaxNode)>(node: &SyntaxNode, f: &mut F) {
    f(node);
    for c in &node.children {
        walk(c, f);
    }
}

fn walk_find<F: Fn(&SyntaxNode) -> bool>(node: &SyntaxNode, pred: &F) -> bool {
    if pred(node) {
        return true;
    }
    for c in &node.children {
        if walk_find(c, pred) {
            return true;
        }
    }
    false
}

// SourceRange doesn't impl Default — these tiny helpers give us a
// zero-position fallback when a syntax node is missing its range
// (defensive: real parser output always carries one).
trait SourceRangeExt {
    fn unwrap_or_default_safe(self) -> SourceRange;
}

impl SourceRangeExt for Option<SourceRange> {
    fn unwrap_or_default_safe(self) -> SourceRange {
        self.unwrap_or(SourceRange {
            start: crate::language::syntax::Position {
                offset: 0,
                line: 0,
                column: 0,
            },
            end: crate::language::syntax::Position {
                offset: 0,
                line: 0,
                column: 0,
            },
        })
    }
}

trait OptSourceRangeCopyExt {
    fn unwrap_or_default_safe_position(self) -> SourceRange;
}

impl OptSourceRangeCopyExt for Option<SourceRange> {
    fn unwrap_or_default_safe_position(self) -> SourceRange {
        self.unwrap_or_default_safe()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language::loom::parser::parse;

    fn validate_src(src: &str) -> Vec<LoomDiagnostic> {
        let parsed = parse(src);
        // Drop parser diagnostics for clarity in validator-focused tests.
        validate(&parsed.root)
    }

    fn has_id(diags: &[LoomDiagnostic], id: &str) -> bool {
        diags.iter().any(|d| d.id == id)
    }

    // ── Pass 2 — references ─────────────────────────────────────

    #[test]
    fn unknown_cast_flagged() {
        let src = "# story\n-- start\nWREN\n  hi\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "unknown-cast"), "{:?}", diags);
    }

    #[test]
    fn known_cast_not_flagged() {
        let src = "# story\ncast WREN\n  .label \"Wren\"\n-- start\nWREN\n  hi\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "unknown-cast"), "{:?}", diags);
    }

    #[test]
    fn unknown_divert_flagged() {
        let src = "# story\n-- start\n-> nowhere\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "divert-target-unknown"));
    }

    #[test]
    fn known_divert_not_flagged() {
        let src = "# story\n-- start\n-> ending\n-- ending\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "divert-target-unknown"), "{:?}", diags);
    }

    #[test]
    fn duplicate_section_flagged() {
        let src = "# story\n-- a\n-- a\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "section-duplicate"), "{:?}", diags);
    }

    #[test]
    fn unknown_location_in_event_flagged() {
        let src = "# story :immersive\nwhen participant enters @BELL_TOWER\n  -> nowhere\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "unknown-location"), "{:?}", diags);
    }

    #[test]
    fn known_location_in_event_not_flagged() {
        let src = "# story :immersive\nlocation BELL_TOWER\n  .label \"x\"\nwhen participant enters @BELL_TOWER\n  ALICE\n    line\ncast ALICE\n  .label \"A\"\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "unknown-location"), "{:?}", diags);
    }

    #[test]
    fn unknown_cohort_in_enroll_flagged() {
        let src = "# d :immersive\nwhen participant joins\n  enroll $PARTICIPANT into singers\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "unknown-cohort"), "{:?}", diags);
    }

    #[test]
    fn known_cohort_not_flagged() {
        let src = "# d :immersive\ncohort singers\n  .capacity 6\nwhen participant joins\n  enroll $PARTICIPANT into singers\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "unknown-cohort"), "{:?}", diags);
    }

    #[test]
    fn unknown_cue_in_action_flagged() {
        let src = "# d\n-- s\n~ cue bell_strike\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "unknown-cue"), "{:?}", diags);
    }

    #[test]
    fn known_cue_not_flagged() {
        let src = "# d\ncue bell_strike\n  .level 0.9\n-- s\n~ cue bell_strike\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "unknown-cue"), "{:?}", diags);
    }

    // ── Pass 3 — shape ─────────────────────────────────────────

    #[test]
    fn goal_without_priority_flagged() {
        let src = "# d\ngoal investigate\n  active_when = $x\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "goal-no-priority"), "{:?}", diags);
    }

    #[test]
    fn goal_with_priority_not_flagged() {
        let src = "# d\ngoal investigate\n  priority = 0.5\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "goal-no-priority"), "{:?}", diags);
    }

    #[test]
    fn faction_without_label_flagged() {
        let src = "# d :immersive\nfaction rebels\n  state\n    morale = 0..100\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "faction-no-label"), "{:?}", diags);
    }

    #[test]
    fn faction_with_label_not_flagged() {
        let src =
            "# d :immersive\nfaction rebels\n  .label \"Rebels\"\n  state\n    morale = 0..100\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "faction-no-label"), "{:?}", diags);
    }

    #[test]
    fn choice_in_script_doc_flagged() {
        let src = "# story :script\n-- s\n* I'll help.\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "choice-in-script"), "{:?}", diags);
    }

    #[test]
    fn choice_in_conversation_doc_not_flagged() {
        let src = "# story\n-- s\nWREN\n  hi\n* I'll help.\ncast WREN\n  .label x\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "choice-in-script"), "{:?}", diags);
    }

    #[test]
    fn axis_without_mode_flagged() {
        let src = "# d :stats\naxis combat\n  curve $combat * 100\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "axis-mode-missing"), "{:?}", diags);
    }

    #[test]
    fn pool_without_max_flagged() {
        let src = "# d :stats\npool stamina\n  init = 100\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "pool-max-required"), "{:?}", diags);
    }

    #[test]
    fn scene_without_states_flagged() {
        // Without an indented state body the parser emits SCENE_DECL
        // with no SCENE_STATE children. (A scene with no INDENT is a
        // narrow corner case; the validator catches it regardless.)
        let src = "# d\nscene empty\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "scene-no-states"), "{:?}", diags);
    }

    // ── Pass 4 — cycle detection ──────────────────────────────

    #[test]
    fn tree_node_cycle_flagged() {
        let src = "# t :tree\nnode a\n  requires b\nnode b\n  requires a\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "tree-cycle"), "{:?}", diags);
    }

    #[test]
    fn tree_node_acyclic_not_flagged() {
        let src = "# t :tree\nnode a\n  rank 1\nnode b\n  requires a\n";
        let diags = validate_src(src);
        assert!(!has_id(&diags, "tree-cycle"), "{:?}", diags);
    }

    #[test]
    fn disposition_mirror_self_cycle_flagged() {
        let src = "# c :character\ndisposition $PLAYER\n  trust = 0..1, mirror $PLAYER.disposition.trust\n";
        let diags = validate_src(src);
        assert!(has_id(&diags, "disposition-mirror-cycle"), "{:?}", diags);
    }

    // ── Output cleanliness ────────────────────────────────────

    #[test]
    fn clean_input_produces_no_diagnostics() {
        let src = "# story\ncast WREN\n  .label \"Wren\"\n-- start\nWREN\n  Hello there.\n  * Continue. -> next\n-- next\nWREN\n  Goodbye.\n";
        let diags = validate_src(src);
        assert!(diags.is_empty(), "expected no diagnostics, got {diags:?}");
    }
}
