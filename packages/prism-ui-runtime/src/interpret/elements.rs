//! Element-walking + attribute application lowered out of
//! `interpret/mod.rs` during Phase 0. See
//! `docs/dev/prui-expressiveness-roadmap.md` §7.0. Pure code move, no
//! behaviour change. The recursive `lower_ast_children` → `lower_node`
//! → `lower_element` → `lower_element_body` cascade lives here along
//! with the per-tag factories (`spacer_from`, `image_from`,
//! `input_from`), the giant `apply_container_attributes` /
//! `apply_text_attributes` switches, the attribute-value resolvers
//! (`bare_attr_value`, `evaluate_bare_attr_typed`, `attribute_string`,
//! `resolved_attribute_string`), and the `animator:easing` encoder.

#[cfg(feature = "luau")]
use std::sync::Arc;

#[cfg(feature = "luau")]
use prism_core::language::prism_ui::parse;
use prism_core::language::prism_ui::{
    split_state_suffix, AttributeNamespace, AttributeValue, Element, Node as AstNode,
};

use crate::command::{Color, CornerRadius};
use crate::layout::{ContainerProps, Node, Semantic, Sizing, TextProps};

#[cfg(feature = "luau")]
use super::control_flow::expand_language;
use super::control_flow::{expand_control_flow, expand_match, expand_suspense};
use super::document::element_with_retagged;
#[cfg(feature = "luau")]
use super::expression::looks_like_closure;
use super::expression::{
    evaluate_expression, interpolate, lookup_expression, lookup_path_owned, stringify_value,
};
#[cfg(feature = "luau")]
use super::lower_document_with_scope;
use super::style::{
    active_class_names, apply_descendant_selectors, apply_prss_class, apply_style_override,
    expand_length_units, parse_color, parse_direction, parse_f32, parse_padding_shorthand,
    parse_sizing, resolve_short_token, set_padding_side, Side,
};
use super::{on_event_attr_key, LowerScope};

/// Lower an arbitrary AST sibling list with the given scope. Public
/// so [`TagResolver`] impls can pre-lower an element's children
/// before invoking a host-supplied component (e.g. composition-style
/// blocks like `shell.app-window` that host real subtrees from
/// `.prui` source). Re-uses the same control-flow + slot +
/// resolver-propagation logic the document walk uses — single
/// chokepoint for every "lower these AST children" call.
pub fn lower_ast_children(nodes: &[AstNode], scope: &LowerScope) -> Vec<Node> {
    lower_children(nodes, scope)
}

/// Walk a sibling list, expanding control flow then dispatching each
/// element through [`lower_node`]. The control-flow expansion runs
/// once per parent so `else-if`/`else` chains see their preceding
/// `if` siblings — handling them inside [`lower_node`] would lose
/// that context.
pub(super) fn lower_children(nodes: &[AstNode], scope: &LowerScope) -> Vec<Node> {
    let expanded = expand_control_flow(nodes, scope);
    let mut out = Vec::with_capacity(expanded.len());
    for (node, child_scope) in expanded {
        out.extend(lower_node(&node, child_scope.as_ref().unwrap_or(scope)));
    }
    out
}

fn lower_node(node: &AstNode, scope: &LowerScope) -> Vec<Node> {
    match node {
        AstNode::Element(el) => lower_element(el, scope),
        AstNode::Text { value, .. } => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                Vec::new()
            } else {
                vec![Node::Text {
                    id: String::new(),
                    content: interpolate(trimmed, scope),
                    props: TextProps::default(),
                }]
            }
        }
        AstNode::Interpolation(expr) => {
            // **Wave C** — inside a macro expansion, `{children}`
            // (and `<fragment>{children}</fragment>`) splices the
            // caller's already-lowered child nodes verbatim. The
            // macro path binds them via `with_host_children_ui`;
            // the unnamed `<slot/>` is the equivalent explicit form
            // (Phase 5 collapsed `<host-children/>` into it).
            if expr.body.trim() == "children" {
                if let Some(injected) = scope.host_children_ui() {
                    return injected.to_vec();
                }
            }
            let resolved = lookup_expression(&expr.body, scope)
                .map(stringify_value)
                .unwrap_or_default();
            if resolved.is_empty() {
                Vec::new()
            } else {
                vec![Node::Text {
                    id: String::new(),
                    content: resolved,
                    props: TextProps::default(),
                }]
            }
        }
        AstNode::Comment { .. } => Vec::new(),
    }
}

fn lower_element(el: &Element, scope: &LowerScope) -> Vec<Node> {
    // **Wave 14.3** — `memo="[dep1, dep2]"` cache. When the host
    // installed a [`MemoCache`] and this element carries both a
    // `memo` attribute and a resolvable `id`, check the cache
    // before lowering. A dep-tuple match returns the cached subtree
    // verbatim; a mismatch (or cache miss) falls through to the
    // normal lowering body and stores the result on the way out.
    // Authors without an id, or running on a host that didn't
    // install a cache, see `memo` round-trip as a no-op.
    if let Some(cache_handle) = scope.memo_cache() {
        // **Phase 3** — reactive dirty-set splice. When the shell
        // installed the per-frame dirty NodeId set, the signal graph
        // is authoritative: an id'd element is reused verbatim from
        // cache iff its own id is not dirty *and* its cached subtree
        // contains no dirty descendant. Anything dirty (or on its
        // ancestor path) re-lowers; the recursion prunes at the
        // highest clean boundary. Supersedes the authored-`memo=`
        // path while a dirty set is active.
        if let Some(dirty) = scope.dirty_nodes() {
            if let Some(id) = resolve_element_id(el, scope) {
                let reuse = !dirty.contains(&id)
                    && cache_handle
                        .borrow()
                        .entries
                        .get(&id)
                        .is_some_and(|(_, nodes)| !subtree_has_dirty(nodes, dirty));
                if reuse {
                    return cache_handle.borrow().entries[&id].1.clone();
                }
                let lowered = lower_element_body(el, scope);
                let mut cache = cache_handle.borrow_mut();
                cache
                    .entries
                    .insert(id.clone(), (Vec::new(), lowered.clone()));
                cache.touched.insert(id);
                return lowered;
            }
            return lower_element_body(el, scope);
        }

        // **Wave 14.3** — no dirty set: the authored `memo="[…]"`
        // dep-tuple cache. A match returns the cached subtree; a
        // mismatch re-lowers and stores.
        if let Some((deps, id)) = extract_memo_and_id(el, scope) {
            {
                let cache = cache_handle.borrow();
                if let Some((cached_deps, cached_nodes)) = cache.entries.get(&id) {
                    if cached_deps == &deps {
                        return cached_nodes.clone();
                    }
                }
            }
            let lowered = lower_element_body(el, scope);
            cache_handle
                .borrow_mut()
                .entries
                .insert(id, (deps, lowered.clone()));
            return lowered;
        }

        // **Phase 3** — even with no authored `memo=`, populate the
        // per-id cache on a full (dirty-set-less) pass so the *next*
        // reactive frame can splice this subtree. Elements without a
        // resolvable id can't be keyed and always re-lower.
        if let Some(id) = resolve_element_id(el, scope) {
            let lowered = lower_element_body(el, scope);
            cache_handle
                .borrow_mut()
                .entries
                .insert(id, (Vec::new(), lowered.clone()));
            return lowered;
        }
    }
    lower_element_body(el, scope)
}

/// **Phase 3** — does any node in `nodes` (recursively) carry an id
/// in `dirty`? Used to decide whether a cached subtree is safe to
/// splice: a clean cached subtree is reused; one containing a dirty
/// descendant re-lowers so the change propagates.
fn subtree_has_dirty(nodes: &[Node], dirty: &std::collections::HashSet<String>) -> bool {
    nodes.iter().any(|n| {
        let id = n.id();
        (!id.is_empty() && dirty.contains(id))
            || matches!(n, Node::Container { children, .. } if subtree_has_dirty(children, dirty))
    })
}

/// **Wave 14.3 / Phase 3** — resolve an element's `id` attribute to
/// a non-empty string, or `None`. Shared by the memo-dep path and
/// the reactive dirty-set splice so id resolution lives once.
fn resolve_element_id(el: &Element, scope: &LowerScope) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| {
            matches!(a.name.namespace, AttributeNamespace::Identifier) && a.name.local == "id"
        })
        .and_then(|a| resolved_attribute_string(&a.value, scope))
        .filter(|s| !s.is_empty())
}

/// **Wave 14.3** — extract the memo dep tuple and the resolved id
/// of an element in one pass. Returns `None` when *either* the
/// element has no `memo=` attribute or its id resolves to an empty
/// string — both are required for the cache to key cleanly.
fn extract_memo_and_id(
    el: &Element,
    scope: &LowerScope,
) -> Option<(Vec<serde_json::Value>, String)> {
    let memo = el
        .attributes
        .iter()
        .find_map(|attr| match attr.name.namespace {
            AttributeNamespace::Bare if attr.name.local == "memo" => {
                let body = attribute_string(&attr.value).unwrap_or_default();
                Some(eval_memo_deps(&body, scope))
            }
            _ => None,
        })?;
    let id = resolve_element_id(el, scope)?;
    Some((memo, id))
}

/// **Wave 14.3** — evaluate a `memo="[a, b]"` body to a list of
/// JSON values. The leading `[` and trailing `]` are optional —
/// `memo="a, b"` and `memo="[a, b]"` are equivalent. Each
/// comma-separated expression goes through the same Pratt parser
/// the `{a + b}` interpolation path uses, so `memo="count, mode"`
/// reads bare bindings and `memo="state.kind, items.length"`
/// resolves dotted paths.
fn eval_memo_deps(body: &str, scope: &LowerScope) -> Vec<serde_json::Value> {
    let body = body
        .trim()
        .trim_start_matches('{')
        .trim_end_matches('}')
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']');
    body.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|expr| {
            lookup_expression(expr, scope)
                .cloned()
                .or_else(|| evaluate_expression(expr, scope))
                .unwrap_or(serde_json::Value::Null)
        })
        .collect()
}

/// The original `lower_element` body, factored out so the memo
/// short-circuit at the top of `lower_element` can wrap it cleanly.
/// Every author-visible lowering still flows through this function;
/// the memo gate is purely a cache layer.
fn lower_element_body(el: &Element, scope: &LowerScope) -> Vec<Node> {
    // **`<dispatch tag="{expr}"/>` — runtime-tag dispatch.** When the
    // tag attribute resolves to a closed-set runtime primitive
    // (`container`, `text`, `heading`, `image`, `spacer`, `input`,
    // `fragment`, `slot`), rebuild a synthetic element with the
    // resolved tag and lower it through the same
    // primitive arms below. When it resolves to anything else, the
    // synthetic element falls through to the resolver — which now
    // sees the resolved tag instead of `dispatch`, so plugin / shell
    // tags route uniformly. Closes the §15 PRUI-reference gap that
    // listed `<{panel.tag} .../>` as inexpressible: data-driven tag
    // routing now works for the runtime's primitive vocabulary as
    // well as registered tags. The pre-existing `component=` form
    // (resolver-side dispatch by registered-component id) keeps
    // working — that path is still handled by `RegistryTagResolver`
    // when the synthesised tag stays "dispatch".
    if el.tag == "dispatch" {
        if let Some(target_tag) = bare_attr_value(el, "tag", scope) {
            let trimmed = target_tag.trim();
            if !trimmed.is_empty() && trimmed != "dispatch" {
                let synthetic = element_with_retagged(el, trimmed);
                return lower_element_body(&synthetic, scope);
            }
        }
    }

    // **Phase 7** — PascalCase tag dispatch. When a tag starts with
    // an uppercase letter *and* a `<component name="X">` declaration
    // with that name is present on the scope's local-component
    // table, instantiate it: bind the call's props, apply declared
    // defaults, enforce `required`, and lower the body in a child
    // scope. Mixed-case sub-tags (`<shell.icon-button/>`) still
    // route to the host's `TagResolver` — only ASCII-uppercase-led
    // tags qualify. Documents that declare no local components
    // skip the map probe via the `has_local_components` early-out.
    if scope.has_local_components()
        && super::components::is_pascal_case_tag(&el.tag)
    {
        if let Some(def) = scope.local_component(&el.tag) {
            return super::components::instantiate_component(def.as_ref(), el, scope);
        }
    }

    match el.tag.as_str() {
        // **Phase 7** — a `<component name="X">` element with a
        // non-empty `name=` attribute is a *declaration*, not a
        // container. It contributes to the local component table
        // during the document pre-pass and renders nothing at its
        // source position (same shape as `<script>` / `<style>` /
        // `<import>`). The legacy `<component>`-as-`<container>`
        // alias (no `name=` attribute) keeps working below; the
        // §7.1 cutover that retires it is Phase 17.
        "component" if bare_attr_value(el, "name", scope).is_some() => Vec::new(),
        // **Phase 7** — sibling top-level declaration tags. The
        // canonical parser emits `<trait>` / `<mixin>` / `<macro>` /
        // `<type>` / `<fn>` / `<let>` / `<namespace>` from the
        // canonical declaration syntax; their semantics live in
        // Phases 8–11. They round-trip through the AST and render
        // nothing at the document scope today.
        "trait" | "mixin" | "macro" | "type" | "fn" | "namespace"
            if bare_attr_value(el, "name", scope).is_some() =>
        {
            Vec::new()
        }
        "container" | "component" => {
            let mut props = ContainerProps::default();
            let mut id = String::new();
            apply_container_attributes(el, scope, &mut props, &mut id);
            // **PRSS descendant selectors** — extend the ancestor
            // class chain for child lowering so `[class."btn icon"]`
            // can match an `<icon>` nested under this container's
            // `class="btn"`. The active-class collection runs again
            // here (and once already inside `apply_container_attributes`);
            // it's a cheap walk over the element's attribute list and
            // keeps the function-level seam intact. Empty class set
            // skips the scope clone via `with_class_chain_appending`'s
            // early return.
            let active = active_class_names(el, scope);
            let child_scope_owned;
            let child_scope: &LowerScope = if active.is_empty() {
                scope
            } else {
                child_scope_owned = scope.clone().with_class_chain_appending(active);
                &child_scope_owned
            };
            let mut children = lower_children(&el.children, child_scope);
            // **Wave 14.3** — `<teleport to="X">payload</teleport>`
            // routing. Any teleport in the document whose `to` matches
            // this element's id appends its payload here, lowered in
            // the target's scope. Payload binding resolution flows
            // from the destination, not the source — overlay tags at
            // the root naturally see the root scope. Authors who need
            // source-scope bindings should compute the values into
            // literal strings at the source.
            if !id.is_empty() {
                let payload = scope.teleport_payload_for(&id);
                if !payload.is_empty() {
                    children.extend(lower_children(payload, scope));
                }
            }
            vec![Node::Container {
                id,
                props,
                children,
            }]
        }
        // **Wave 14.3** — at its source position, a `<teleport>`
        // emits nothing. Its children were collected by the
        // document-level pre-scan and will land at the target site
        // when the matching `id="…"` is lowered.
        "teleport" => Vec::new(),
        // **Wave A** — `<script>` is behaviour, `<style>` is theme;
        // neither is tree. Their bodies are harvested in the
        // document pre-pass (`collect_script_bodies`) / the Wave H
        // stylesheet pass — at their source position they render
        // nothing. Listed here so they never fall through to the
        // unknown-tag resolver and accidentally surface as an empty
        // container.
        // **Wave H** — `<import>` is resolved in the document
        // pre-pass; at its source position it renders nothing.
        "script" | "style" | "import" => Vec::new(),
        // **Wave D (`prui-luau-fusion.md` §7.5)** — `<match on="{x}">`
        // with `<case>` children. Pure parser sugar: rewritten to a
        // synthetic `<let>` (the matched value, bound once) plus a
        // chained `if`/`else-if`/`else` over `<fragment>` wrappers,
        // then lowered through the existing control-flow expander.
        "match" => expand_match(el, scope),
        // A stray `<case>` (outside `<match>`) is malformed — it
        // renders nothing rather than leaking its body.
        "case" => Vec::new(),
        // **Wave D (§7.6)** — `<suspense>` / `<fallback>`. The
        // fallback body shows while any binding the primary subtree
        // reads is a pending coroutine marker; otherwise the primary
        // subtree renders (fallback stripped).
        "suspense" => expand_suspense(el, scope),
        // `<fallback>` only has meaning as a `<suspense>` child;
        // consumed there. Standalone → nothing.
        "fallback" => Vec::new(),
        // **Wave E (`prui-luau-fusion.md` §7.8)** — sub-dialect
        // block. `<language name="md">…</language>` (and the
        // `~md{…}` sigil it desugars from) routes the raw body
        // through the script-registered dialect's `parse(source)`,
        // then re-parses + lowers the returned `prui[[…]]` source.
        // Unknown dialect / no Luau scope → nothing (graceful, like
        // an unresolved tag).
        #[cfg(feature = "luau")]
        "language" => expand_language(el, scope),
        #[cfg(not(feature = "luau"))]
        "language" => Vec::new(),
        "text" | "heading" => {
            let mut props = TextProps::default();
            let mut id = String::new();
            apply_text_attributes(el, scope, &mut props, &mut id);
            if el.tag == "heading" && font_size_is_default(&props) {
                props.font_size = heading_font_size(el);
            }
            let content = collect_text_content(&el.children, scope);
            vec![Node::Text { id, content, props }]
        }
        "spacer" => vec![spacer_from(el, scope)],
        "input" => vec![input_from(el, scope)],
        "image" => vec![image_from(el, scope)],
        // `<slot/>` and `<slot name="x"/>` resolve to whatever the
        // caller injected. After Phase 5 collapsed the
        // `<host-children/>` element into this single surface, the
        // unnamed `<slot/>` is the canonical default-slot spelling —
        // the DSL loader / macro caller seeds pre-lowered children
        // via [`LowerScope::with_host_children_ui`] and the unnamed
        // slot emits them verbatim.
        //
        // Lookup order:
        //   1. AST-level slot bindings (`LowerScope::slots`) — set
        //      when a parent component's body interpolated AST-level
        //      slot content (used by template expansion).
        //   2. Pre-lowered named-slot map (`host_children_by_slot`) —
        //      set when the resolver partitioned a dispatched
        //      element's children by `slot="X"` attribute. The
        //      unnamed slot reads from the empty-string bucket.
        //   3. **Unnamed slot only** — pre-lowered children from the
        //      DSL loader / macro caller (`host_children_ui`). This
        //      is the Phase-5 default-slot seam.
        //   4. The element's own children (fallback content).
        "slot" => {
            let name = bare_attr_value(el, "name", scope);
            if let Some(injected) = scope.slots.resolve(name.as_deref()) {
                return lower_children(injected, scope);
            }
            let key = name.as_deref().unwrap_or("");
            if let Some(injected) = scope.host_children_for_slot(key) {
                return injected.to_vec();
            }
            if name.is_none() {
                if let Some(injected) = scope.host_children_ui() {
                    return injected.to_vec();
                }
            }
            lower_children(&el.children, scope)
        }
        // **Fragment** — `<fragment>…</fragment>` (also `<></>`-shape
        // counterpart). React `<>…</>` / Vue `<template>` /
        // Svelte `<svelte:fragment>` equivalent. Emits children
        // verbatim with no wrapping container, useful for grouping
        // a multi-element `if`/`else` branch or `for` body without
        // imposing a flex parent. Drops `if=` / `for=` correctly
        // because those are handled at the sibling expansion layer.
        "fragment" => lower_children(&el.children, scope),
        // Unknown tag — first ask the host's tag resolver (if any).
        // Hosts plug a `TagResolver` (e.g. `prism-builder`'s
        // `RegistryTagResolver`) through `LowerScope::with_resolver`
        // so registered component vocabularies (`shell.icon-button`,
        // user prefabs) materialise into runtime nodes here. If no
        // resolver claims the tag, fall back to the default behaviour:
        // drop the wrapping element and keep its children, so a host
        // can nest a scene inside `<scene>` without forcing the runtime
        // to know about it.
        _ => {
            // **Wave C (`prui-luau-fusion.md` §7.7)** — a Luau macro
            // tag. Checked *before* the host resolver so a script can
            // shadow / define element vocabulary document-locally.
            // The macro receives the caller's resolved attributes +
            // already-lowered children and returns `prui[[…]]` source
            // the host re-parses and lowers in a hygienic scope.
            #[cfg(feature = "luau")]
            if let Some(frame) = scope.luau_scope() {
                if frame.has_macro(&el.tag) {
                    return expand_macro_element(el, scope, frame);
                }
            }
            if let Some(resolver) = scope.resolver() {
                if let Some(nodes) = resolver.resolve(el, scope) {
                    return nodes;
                }
            }
            lower_children(&el.children, scope)
        }
    }
}

/// **Wave C** — expand a Luau macro element. Resolves the call
/// site's attributes to a JSON object and lowers its children in the
/// *caller's* scope (so `{caller_binding}` inside macro children
/// resolves at the call site, Vue-slot style), then hands both to
/// the macro. The returned `prui[[…]]` source is parsed and lowered
/// in a **hygienic** scope: document context (resolver, tokens,
/// Luau frame) is carried so nested tags / `{tokens.*}` / nested
/// macros still work, but the caller's PRUI bindings are dropped —
/// the macro body sees only `attrs` + `children` (§7.7 hygiene).
#[cfg(feature = "luau")]
fn expand_macro_element(
    el: &Element,
    scope: &LowerScope,
    frame: &crate::luau_scope::LuauScopeFrame,
) -> Vec<Node> {
    // Resolved attribute object: bare/identifier attrs keyed by
    // local name. Empty (valueless) attrs are booleans.
    let mut attrs = serde_json::Map::new();
    for a in &el.attributes {
        if !matches!(
            a.name.namespace,
            AttributeNamespace::Bare | AttributeNamespace::Identifier
        ) {
            continue;
        }
        let v = match &a.value {
            AttributeValue::Empty => serde_json::Value::Bool(true),
            AttributeValue::Expression(e) => lookup_expression(&e.body, scope)
                .cloned()
                .or_else(|| evaluate_expression(&e.body, scope))
                .unwrap_or(serde_json::Value::Null),
            _ => resolved_attribute_string(&a.value, scope)
                .map(serde_json::Value::String)
                .unwrap_or(serde_json::Value::Null),
        };
        attrs.insert(a.name.local.clone(), v);
    }
    let attrs_json = serde_json::Value::Object(attrs);

    // Caller's children, lowered at the call site.
    let children_nodes = lower_children(&el.children, scope);
    let children_json = serde_json::to_value(&children_nodes).unwrap_or(serde_json::Value::Null);

    let src = match frame.expand_macro(&el.tag, &attrs_json, &children_json) {
        Some(Ok(s)) => s,
        // Macro error / not-a-macro (shouldn't happen — `has_macro`
        // gated) → render nothing rather than a broken subtree.
        _ => return Vec::new(),
    };

    let (doc, errors) = parse(&src);
    if !errors.is_empty() {
        return Vec::new();
    }

    // Hygienic scope: keep document context, drop caller bindings.
    let mut macro_scope = LowerScope::default().with_luau_scope(frame.clone());
    if let Some(resolver) = scope.resolver() {
        macro_scope = macro_scope.with_resolver(Arc::clone(resolver));
    }
    if let Some(tokens) = scope.binding("tokens") {
        macro_scope = macro_scope.with_binding("tokens", tokens.clone());
    }
    // **Wave C** — re-thread the PRSS sheet so a macro body's
    // `class="…"` resolves named classes exactly as it would at the
    // call site. Installed after the `tokens` binding so the sheet's
    // `[tokens.*]` overrides cascade over it (the §4.6 order), then
    // the macro-specific bindings layer on last.
    if let Some(sheet) = scope.stylesheet_arc() {
        macro_scope = macro_scope.with_stylesheet(sheet);
    }
    macro_scope = macro_scope
        .with_binding("attrs", attrs_json)
        .with_host_children_ui(children_nodes);
    lower_document_with_scope(&doc, &macro_scope)
}

// ---------------------------------------------------------------------------
// Per-element attribute application
// ---------------------------------------------------------------------------

fn spacer_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut width = 0.0;
    let mut height = 0.0;
    for attr in &el.attributes {
        if !matches!(
            attr.name.namespace,
            AttributeNamespace::Bare | AttributeNamespace::Identifier
        ) {
            continue;
        }
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.local.as_str() {
            "id" => id = raw.unwrap_or_default(),
            "width" => width = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
            "height" => height = raw.as_deref().and_then(parse_f32).unwrap_or(0.0),
            _ => {}
        }
    }
    Node::Spacer { id, width, height }
}

/// Lower `<image src="…" width="…" height="…"/>` to [`Node::Image`].
/// Wave 11.2: closes the last shape gap blocking icon-bearing shell
/// components (icon-button, nav-button, section-header, app-card, …)
/// from migrating to `.prui` source. Same attr vocabulary the
/// container shape uses — `width`/`height` parse through
/// [`parse_sizing`], `style:radius` builds a uniform [`CornerRadius`],
/// `style:tint` parses through [`parse_color`], `aria-label` /
/// `aria:*` / `data:*` round-trip onto [`Semantic`].
fn image_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut source = String::new();
    let mut width = Sizing::default();
    let mut height = Sizing::default();
    let mut radius = CornerRadius::default();
    let mut tint: Option<Color> = None;
    let mut semantic = Semantic::default();
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "src" | "source" => source = raw.unwrap_or_default(),
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        height = s;
                    }
                }
                "tag" => semantic.tag = raw,
                "role" => semantic.role = raw,
                "aria-label" => semantic.aria_label = raw,
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    id = v;
                }
            }
            AttributeNamespace::Style => match local {
                "radius" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        radius = CornerRadius {
                            tl: v,
                            tr: v,
                            br: v,
                            bl: v,
                        };
                    }
                }
                "tint" | "color" => {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        tint = Some(c);
                    }
                }
                _ => {}
            },
            AttributeNamespace::Aria => {
                if let Some(value) = raw {
                    semantic.attrs.push((format!("aria-{}", local), value));
                }
            }
            AttributeNamespace::Data => {
                if let Some(value) = raw {
                    semantic.attrs.push((format!("data-{}", local), value));
                }
            }
            _ => {}
        }
    }
    Node::Image {
        id,
        source,
        width,
        height,
        radius,
        tint,
        semantic,
    }
}

fn input_from(el: &Element, scope: &LowerScope) -> Node {
    let mut id = String::new();
    let mut value = String::new();
    let mut placeholder = String::new();
    let mut props = TextProps::default();
    let mut width = Sizing::default();
    let mut height = Sizing::default();
    let mut background: Option<crate::command::Color> = None;
    let mut hover: Option<crate::layout::StateOverrides> = None;
    let mut focused = false;
    let mut multiline = false;
    let mut caret_byte: Option<usize> = None;
    let mut selection: Option<(usize, usize)> = None;
    let mut spans: Vec<crate::command::TextSpan> = Vec::new();
    let mut scroll_x: f32 = 0.0;
    let mut scroll_y: f32 = 0.0;
    let mut underline: Option<(usize, usize)> = None;
    let mut bracket_match: Vec<usize> = Vec::new();
    let mut highlight_current_line = false;
    // `syntax-language` chooses the tokenizer the interpret pass
    // runs against the input's `value`. Set by the code editor's
    // DSL (`<input syntax-language="luau"/>`); ignored when empty.
    let mut syntax_language: Option<String> = None;
    let mut semantic = Semantic::default();
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope);
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "value" => value = raw.unwrap_or_default(),
                "placeholder" => placeholder = raw.unwrap_or_default(),
                "font-size" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.font_size = v;
                    }
                }
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        height = s;
                    }
                }
                // Wave 11.3 — focus state surfaces through the DSL so
                // `<input focused="{is-focused}"/>` reports the
                // live edit target without a Rust helper.
                "focused" => {
                    focused = matches!(raw.as_deref(), Some("true"));
                }
                // Editor support: `multiline="true"` flips the input
                // from string-property mode (single line, newlines
                // stripped) to code-editor mode (`\n` is real, Up/Down
                // arrows navigate rows). `caret-byte` + `selection`
                // surface the host's editor state so the renderer can
                // paint caret + selection at the exact byte offsets.
                "multiline" => {
                    multiline = matches!(raw.as_deref(), Some("true"));
                }
                "caret-byte" => {
                    if let Some(n) = raw.as_deref().and_then(|s| s.parse::<usize>().ok()) {
                        caret_byte = Some(n);
                    }
                }
                // `selection="<start>,<end>"` — two non-negative byte
                // offsets, comma-separated, with `start <= end`. The
                // shell binding emits this only when an active
                // selection is non-empty.
                "selection" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                if a < b {
                                    selection = Some((a, b));
                                }
                            }
                        }
                    }
                }
                // `scroll-x` / `scroll-y` — host-controlled
                // viewport offsets. Subtracted from the text's
                // draw origin and combined with a scissor at
                // emit time so glyphs outside the bounds are
                // clipped. The shell auto-updates these to keep
                // the caret visible on every editor mutation.
                "scroll-x" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        scroll_x = v;
                    }
                }
                "scroll-y" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        scroll_y = v;
                    }
                }
                // `bracket-match="<open>,<close>"` — pair of byte
                // offsets the renderer should paint thin outlines
                // around. Used for the matching-bracket highlight.
                "bracket-match" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                bracket_match = vec![a, b];
                            }
                        }
                    }
                }
                // `highlight-current-line="true"` — paint a subtle
                // strip behind the caret's row. Default off so
                // inline string-property fields don't gain one.
                "highlight-current-line" => {
                    highlight_current_line = matches!(raw.as_deref(), Some("true"));
                }
                // `underline="<start>,<end>"` — byte range the
                // renderer should paint an underline through. Used
                // by IME preedit decoration; same parser shape as
                // `selection`.
                "underline" => {
                    if let Some(s) = raw.as_deref() {
                        if let Some((a, b)) = s.split_once(',') {
                            if let (Ok(a), Ok(b)) =
                                (a.trim().parse::<usize>(), b.trim().parse::<usize>())
                            {
                                if a < b {
                                    underline = Some((a, b));
                                }
                            }
                        }
                    }
                }
                // `syntax-language="luau"` — the input's value is
                // tokenized against this language and the resulting
                // [`TextSpan`]s ride the runtime node into the paint
                // pass. Empty / unknown languages produce no spans
                // (the input paints in `style:color`).
                "syntax-language" => {
                    if let Some(s) = raw {
                        if !s.is_empty() {
                            syntax_language = Some(s);
                        }
                    }
                }
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    id = v;
                }
            }
            AttributeNamespace::Style if local == "color" => {
                if let Some(c) = raw.as_deref().and_then(parse_color) {
                    props.color = c;
                }
            }
            // `style:background="#…"` / `style:background:hovered="#…"`
            // give the input its own resting + hover fill. Without these
            // the input falls back to the legacy white default at emit
            // time. Inline rows (property fields) use this to put the
            // hover affordance on the input itself rather than spilling
            // it onto the parent row.
            AttributeNamespace::Style => {
                let (key, state) = split_state_suffix(local);
                if key == "background" {
                    if let Some(c) = raw.as_deref().and_then(parse_color) {
                        match state {
                            None => background = Some(c),
                            Some("hovered") => {
                                hover
                                    .get_or_insert_with(crate::layout::StateOverrides::default)
                                    .background = Some(c);
                            }
                            _ => {}
                        }
                    }
                }
            }
            // `bind:value="<node-id>.<key>"` lowers to a
            // `data-bind-value` semantic attr on the input. The shell
            // event router consumes it at pointer-down time to start
            // a field-focus session against the bound source. Same
            // round-trip shape the container path uses for `bind:`.
            AttributeNamespace::Bind => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-bind-{}", local), v));
                }
            }
            AttributeNamespace::Signal => {
                if let Some(v) = raw {
                    semantic.attrs.push((format!("data-sig-{}", local), v));
                }
            }
            // `aria:*` / `data:*` pass through to the semantic carrier
            // so inputs participate in the same hit-test / SSR routing
            // the containers do.
            AttributeNamespace::Aria => {
                if let Some(v) = raw.filter(|s| !s.is_empty()) {
                    semantic.attrs.push((format!("aria-{}", local), v));
                }
            }
            AttributeNamespace::Data => {
                if let Some(v) = raw.filter(|s| !s.is_empty()) {
                    semantic.attrs.push((format!("data-{}", local), v));
                }
            }
            _ => {}
        }
    }
    // Resolve `syntax-language` against the input's `value` to
    // produce per-byte spans. Done at DSL-lower time (rather than
    // at paint time) so the same shape JSON-snapshots / SSR
    // emitters reach can carry the highlighted ranges unchanged —
    // and so a code-editor input that's bound to a 5000-line
    // buffer doesn't retokenize on every animator tick.
    if let Some(lang) = syntax_language.as_deref() {
        if !value.is_empty() {
            spans = crate::syntax::highlight(&value, lang);
        }
    }
    // Interaction defaults — single-line inputs are the canonical
    // "property-row text box" surface, so resting + hover backgrounds
    // come for free. `.prui` authors override with `style:background`
    // / `style:background:hovered` when they want a different palette.
    // Multi-line inputs (code editors, text-buffer primitives) keep
    // the legacy white-on-nothing default so a 5000-line editor
    // doesn't gain a stray hover tint.
    if !multiline {
        if background.is_none() {
            background = Some(crate::command::Color {
                r: 0xf5,
                g: 0xf5,
                b: 0xf7,
                a: 0xff,
            });
        }
        let needs_hover = hover.as_ref().is_none_or(|h| h.background.is_none());
        if needs_hover {
            hover
                .get_or_insert_with(crate::layout::StateOverrides::default)
                .background = Some(crate::command::Color {
                r: 0xe7,
                g: 0xe9,
                b: 0xee,
                a: 0xff,
            });
        }
    }
    Node::TextInput {
        id,
        value,
        placeholder,
        props,
        width,
        height,
        radius: CornerRadius::default(),
        semantic,
        background,
        hover,
        focused,
        multiline,
        caret_byte,
        selection,
        spans,
        scroll_x,
        scroll_y,
        underline,
        bracket_match,
        highlight_current_line,
    }
}

fn apply_container_attributes(
    el: &Element,
    scope: &LowerScope,
    props: &mut ContainerProps,
    id: &mut String,
) {
    // **PRSS pre-pass.** Resolve any `class="…"` attribute *and* any
    // `class:<name>="{cond}"` toggles before the per-attribute loop
    // so user inline `style:` overrides always win on conflicts (the
    // canonical specificity rule from `prss-reference.md` §4.6).
    // The Svelte-style toggle layers truthy class names on top of the
    // static `class="…"` list, in document order: a later
    // `class:foo="{true}"` wins on conflicts the same way a literal
    // `class="… foo"` would. No-op when no stylesheet is loaded
    // (headless / SSR / first-boot path) — both forms round-trip as
    // stale-but-harmless authored attrs in that case. The active
    // class set is *always* mirrored into [`Semantic::class`] so the
    // SSR / semantic-HTML emitters pick up the same `class="…"`
    // author intent — even when no stylesheet is loaded the
    // static + toggle list still round-trips through HTML.
    //
    // **Descendant selectors** apply *after* flat classes so a more
    // specific match (`btn icon`) overrides the flat (`icon`) entry,
    // matching CSS specificity ordering. The chain comes from the
    // outer `lower_element_body`, which threads each container's
    // active classes into [`LowerScope::with_class_chain_appending`]
    // before lowering children.
    let active = active_class_names(el, scope);
    if !active.is_empty() {
        // Dedup while preserving first-seen order so authors writing
        // `class="btn" class:btn="{true}"` don't get a doubled-up
        // class attr through the SSR backend.
        let mut seen: Vec<&str> = Vec::with_capacity(active.len());
        for name in &active {
            if !seen.contains(&name.as_str()) {
                seen.push(name.as_str());
            }
        }
        props.semantic.class = Some(seen.join(" "));
    }
    if let Some(sheet) = scope.stylesheet() {
        for class_name in &active {
            apply_prss_class(sheet, class_name, props, scope);
        }
        apply_descendant_selectors(sheet, &active, scope, props);
        // **Fusion F.3** — record the class→NodeId dependency so a
        // later `.prss` literal swap can mark just this element dirty
        // (Phase 3 then splices the rest). Keyed by the element's
        // resolvable id; anonymous containers can't be targeted
        // selectively and fall back to the broad path.
        if let Some(deps) = scope.class_deps() {
            if let Some(node_id) = resolve_element_id(el, scope) {
                let mut deps = deps.borrow_mut();
                for class_name in &active {
                    deps.record(class_name, &node_id);
                }
            }
        }
    }
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        let raw = resolved_attribute_string(&attr.value, scope).map(|v| {
            // **Short-name token references** — `padding="md"` /
            // `style:background="accent"` resolve to
            // `tokens.spacing.md` / `tokens.colors.accent` *before*
            // the per-attribute parse runs. Limited to Bare + Style
            // attrs so `data:role="md"` / `aria:level="md"` don't
            // accidentally substitute. See `prss-reference.md` §6.
            // **Token-driven rem base** — any `Nrem` / `Nem` segments
            // expand to pixel-resolved numerics using
            // [`LowerScope::rem_px`] so a custom
            // `tokens.typography.font-size-md` rescales every length
            // through the same parser uniformly.
            match attr.name.namespace {
                AttributeNamespace::Bare | AttributeNamespace::Style => {
                    let resolved = resolve_short_token(local, &v, scope).unwrap_or(v);
                    expand_length_units(&resolved, scope)
                }
                _ => v,
            }
        });
        match attr.name.namespace {
            AttributeNamespace::Bare => match local {
                "direction" => {
                    if let Some(v) = raw.as_deref() {
                        props.direction = parse_direction(v);
                    }
                }
                "gap" => {
                    if let Some(v) = raw.as_deref().and_then(parse_f32) {
                        props.gap = v;
                    }
                }
                "padding" => {
                    // CSS-shorthand: `padding="8"` (uniform),
                    // `padding="8 16"`, `padding="8 16 24"`,
                    // `padding="8 16 24 32"` (TRBL). Single seam with
                    // the inline `style:padding` lowering path —
                    // both call into `parse_padding_shorthand`.
                    if let Some(p) = raw.as_deref().and_then(parse_padding_shorthand) {
                        props.padding = p;
                    }
                }
                "padding-left" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Left),
                "padding-right" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Right)
                }
                "padding-top" => set_padding_side(&mut props.padding, raw.as_deref(), Side::Top),
                "padding-bottom" => {
                    set_padding_side(&mut props.padding, raw.as_deref(), Side::Bottom)
                }
                "width" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.width = s;
                    }
                }
                "height" => {
                    if let Some(s) = raw.as_deref().and_then(parse_sizing) {
                        props.height = s;
                    }
                }
                // Wave 11.2 — Semantic surface for `.prui`-authored
                // shell components. Hand-rolled Rust blocks build
                // `Semantic::tag(..).with_role(..).with_aria_label(..)`
                // imperatively; the DSL needs the same vocabulary so
                // an author can write `<container tag="section"
                // role="navigation" aria-label="Pages"/>` against the
                // same struct. The dedicated fields land on
                // `props.semantic.{tag, role, aria_label}` (separate
                // from the generic `attrs` vec the `data:` / `aria:`
                // namespaces append to) so the HTML emitter picks
                // them up at the same seam it always did.
                "tag" => {
                    if let Some(v) = raw {
                        props.semantic.tag = Some(v);
                    }
                }
                "role" => {
                    if let Some(v) = raw {
                        props.semantic.role = Some(v);
                    }
                }
                "aria-label" => {
                    if let Some(v) = raw {
                        props.semantic.aria_label = Some(v);
                    }
                }
                // Reconciliation hint (Vue `:key`, React `key={}`,
                // Svelte `(x.id)` keyed iteration). Round-trip as a
                // `data-key` semantic attr so SSR and the future
                // incremental-diff substrate inherit author intent
                // verbatim. The runtime tree-diff that would actually
                // consume this hint is the unblock; today it carries
                // through identically to a hand-authored
                // `data:key="…"`. Empty string drops cleanly so a
                // ternary that resolves to `""` omits the attr.
                "key" => {
                    if let Some(v) = raw {
                        if !v.is_empty() {
                            props.semantic.attrs.push(("data-key".to_string(), v));
                        }
                    }
                }
                _ => {}
            },
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            // Wave 9.2: `style:<key>:<state>="<value>"` peels the
            // trailing `:hovered` / `:selected` / `:focused` suffix
            // and routes the override into the matching sparse
            // bundle. `:hovered` lands on
            // [`ContainerProps::hover`], which the layout pass
            // already swaps in when the node's id matches
            // [`Surface::hovered_id`]. `:selected` / `:focused` have
            // no container-level runtime infra yet, so the override
            // round-trips as a `data-style-<key>-<state>="<value>"`
            // semantic attribute — same shape as Wave 9.4
            // transitions: data carries the intent, runtime wiring
            // lands as a follow-up without changing the authoring
            // grammar.
            AttributeNamespace::Style => {
                if let Some(value) = raw.as_deref() {
                    apply_style_override(props, local, value);
                }
            }
            // §43 A1: `on:<event>="<action>"` lowers to a
            // `data-on-<event>` semantic attribute. The shell event
            // router reads it back at pointer-down time and dispatches
            // through `prism_builder::signal::parse_action`. Only
            // containers contribute to the runtime's hit-test cache,
            // so attaching handlers to text / spacer leaves is a
            // separate follow-up (every leaf with author-driven
            // events lives inside a container today).
            //
            // Empty-string filter (PRUI ref §4): `on:event=""` is
            // dropped so a ternary that resolves to `""` omits the
            // handler — matches the `data:` / `aria:` rule.
            //
            // **Wave 14.3** — recognise dotted event-modifier suffixes
            // (Vue `@click.once`, Svelte `on:click|once`). The dot
            // becomes a dash so the existing `data-on-<key>` hit-test
            // cache picks it up uniformly.
            AttributeNamespace::On => {
                if let Some(action) = raw.filter(|s| !s.is_empty()) {
                    let local_key = on_event_attr_key(local);
                    props
                        .semantic
                        .attrs
                        .push((format!("data-on-{}", local_key), action));
                }
            }
            // `aria:<role>="<value>"` and `data:<key>="<value>"` are
            // pass-through; preserve them on the semantic emission
            // so the HTML / SSR backends inherit them and the
            // hit-test cache can route off them like any `data-*`.
            //
            // Both filter empty resolved values so authors can write
            // a ternary (`aria:level="{depth > 0 ? depth + 1 : ''}"`,
            // `data:on-click="{enabled ? 'cmd save' : ''}"`) to omit
            // the attribute conditionally — same shape `on:` uses.
            AttributeNamespace::Aria => {
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("aria-{}", local), value));
                }
            }
            AttributeNamespace::Data => {
                // Skip empty resolved values so authors can use a
                // ternary (`data:on-click="{cmd ? 'cmd ' + cmd : ''}"`)
                // to omit the attr conditionally — Wave 11.2 substrate
                // for the menu-item / row-variant migrations. A literal
                // `data-foo=""` was already meaningless to every
                // hit-test consumer (parse_action returned None) so
                // omitting it is the correct behaviour, not a change.
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-{}", local), value));
                }
            }
            // **Wave G (§7.11)** — `probe:<name>="event-key"` taps a
            // value/interaction into the document probe stream.
            // Lowers to `data-probe-<name>`; `prism.probes:on`
            // subscribes. Live event firing off the data attr is a
            // host event-router follow-up (open question 3 family).
            AttributeNamespace::Probe => {
                if let Some(value) = raw.filter(|s| !s.is_empty()) {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-probe-{}", local), value));
                }
            }
            // **§7.15 — unified `Animator` trait.** The single home
            // for every animation surface; subsumed `transition:` /
            // `animate:` / `at:` when Phase 4 retired them.
            //
            // Method routing — the local part picks the lowered attr:
            // - `animator:in-<prop>` → `data-animate-in-<prop>` (entry
            //   transition; runtime animator reads on first observe).
            // - `animator:out-<prop>` → `data-animate-out-<prop>` (exit
            //   transition; runtime animator reads on unmount).
            // - `animator:transition-<prop>` → `data-transition-<prop>`
            //   (mid-life value-change delta; animator interpolates
            //   between declared values).
            // - `animator:easing` → `data-transition-easing` (curve
            //   selector; a `\fn(t)…end` Luau closure is sampled at
            //   lowering time into a comma-joined LUT so the animator
            //   never calls Lua per frame; a named keyword
            //   (`ease-in`, …) round-trips verbatim).
            // - any other `animator:<method>` → `data-animator-<method>`
            //   (canonical home is `keyframes`; the registry stays
            //   open so adding a new method is a one-line parser
            //   follow-up).
            AttributeNamespace::Animator => {
                if local == "easing" {
                    if let Some(encoded) = encode_easing_attr(&attr.value, scope) {
                        props
                            .semantic
                            .attrs
                            .push(("data-transition-easing".to_string(), encoded));
                    }
                } else if let Some(value) = raw {
                    let attr_name = if let Some(prop) = local.strip_prefix("in-") {
                        format!("data-animate-in-{}", prop)
                    } else if let Some(prop) = local.strip_prefix("out-") {
                        format!("data-animate-out-{}", prop)
                    } else if let Some(prop) = local.strip_prefix("transition-") {
                        format!("data-transition-{}", prop)
                    } else {
                        format!("data-animator-{}", local)
                    };
                    props.semantic.attrs.push((attr_name, value));
                }
            }
            // `bind:<key>="<source>"` is sugar for
            // `prism_builder::signal::ActionKind::Bind { target_key,
            // source }` — but the bind installer runs against the
            // canvas document's `connections` list, not the
            // interpreted shell skeleton. Surface the binding
            // verbatim as `data-bind-<key>` so the host can either
            // install the effect at boot (the dioxus-inspiration
            // Phase 4 path) or no-op cleanly during the SSR walk.
            AttributeNamespace::Bind => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-bind-{}", local), value));
                }
            }
            // `sig:<key>="<source>"` follows the same carry-through
            // pattern as `bind:`. The host walks the lowered tree
            // post-interpret and acts on `data-sig-*` semantic attrs
            // — signal declarations register against the shell's signal
            // scope. SSR backends pass them through to the rendered
            // HTML where consumers (e.g. relay JS) can pick them up.
            AttributeNamespace::Signal => {
                if let Some(value) = raw {
                    props
                        .semantic
                        .attrs
                        .push((format!("data-sig-{}", local), value));
                }
            }
            _ => {}
        }
    }
    // Interaction defaults for button-like containers. Authors signal
    // "this thing is clickable" via `tag="button"` (semantic HTML
    // intent) or `data:type="button"` (legacy carrier from the Slint
    // era). Either gets a resting + hover background for free — the
    // .prui file no longer needs `style:background:hovered="…"` on
    // every icon button / pill / tab to feel alive. Inline overrides
    // still win (the helper short-circuits when either field is set).
    let is_button_like = props.semantic.tag.as_deref() == Some("button")
        || props
            .semantic
            .attrs
            .iter()
            .any(|(k, v)| k == "data-type" && v == "button");
    if is_button_like {
        if props.background.is_none() {
            props.background = Some(crate::command::Color {
                r: 0x00,
                g: 0x00,
                b: 0x00,
                a: 0x08,
            });
        }
        let needs_hover = props.hover.as_ref().is_none_or(|h| h.background.is_none());
        if needs_hover {
            props
                .hover
                .get_or_insert_with(crate::layout::StateOverrides::default)
                .background = Some(crate::command::Color {
                r: 0x00,
                g: 0x00,
                b: 0x00,
                a: 0x14,
            });
        }
    }
}

fn apply_text_attributes(el: &Element, scope: &LowerScope, props: &mut TextProps, id: &mut String) {
    for attr in &el.attributes {
        let local = attr.name.local.as_str();
        // Mirror the resolution + rem-expansion seam used by
        // [`apply_container_attributes`] so `font-size="md"` reads
        // through `tokens.typography.font-size-md` and `font-size="1rem"`
        // honours the scope-driven rem base.
        let raw =
            resolved_attribute_string(&attr.value, scope).map(|v| match attr.name.namespace {
                AttributeNamespace::Bare | AttributeNamespace::Style => {
                    let resolved = resolve_short_token(local, &v, scope).unwrap_or(v);
                    expand_length_units(&resolved, scope)
                }
                _ => v,
            });
        match attr.name.namespace {
            AttributeNamespace::Bare if local == "font-size" => {
                if let Some(v) = raw.as_deref().and_then(parse_f32) {
                    props.font_size = v;
                }
            }
            AttributeNamespace::Identifier if local == "id" => {
                if let Some(v) = raw {
                    *id = v;
                }
            }
            AttributeNamespace::Style if local == "color" => {
                if let Some(c) = raw.as_deref().and_then(parse_color) {
                    props.color = c;
                }
            }
            _ => {}
        }
    }
}

fn font_size_is_default(props: &TextProps) -> bool {
    (props.font_size - TextProps::default().font_size).abs() < f32::EPSILON
}

/// `<heading level="N">` mirrors HTML — h1..h6 step down in 4pt
/// increments from a 28pt base, capped at the body default.
fn heading_font_size(el: &Element) -> f32 {
    let level = el
        .attributes
        .iter()
        .find(|a| a.name.raw == "level")
        .and_then(|a| attribute_string(&a.value))
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(1);
    match level.clamp(1, 6) {
        1 => 28.0,
        2 => 24.0,
        3 => 20.0,
        4 => 18.0,
        5 => 16.0,
        _ => 14.0,
    }
}

fn collect_text_content(children: &[AstNode], scope: &LowerScope) -> String {
    let mut out = String::new();
    for c in children {
        match c {
            AstNode::Text { value, .. } => {
                let resolved = interpolate(value.trim(), scope);
                if resolved.is_empty() {
                    continue;
                }
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&resolved);
            }
            AstNode::Interpolation(expr) => {
                // Cheap bare-path lookup first (Wave 11.2 substrate),
                // with virtual `.length`/`.first`/`.last` segments
                // included; operator-bearing bodies (ternary, `||`,
                // `&&`, `==`) fall through to the full evaluator so
                // authors can write `<text>{text ? text : status}</text>`
                // against the same vocabulary attribute interpolations use.
                let resolved = if let Some(v) = lookup_path_owned(&expr.body, scope) {
                    stringify_value(&v)
                } else if let Some(v) = evaluate_expression(&expr.body, scope) {
                    stringify_value(&v)
                } else {
                    continue;
                };
                if resolved.is_empty() {
                    // Skip the leading-space insertion when an interpolation
                    // resolves to "" — `{text}{status}` with `status=""`
                    // shouldn't add a trailing separator.
                    continue;
                }
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(&resolved);
            }
            _ => {}
        }
    }
    out
}

pub(super) fn bare_attr_value(el: &Element, name: &str, scope: &LowerScope) -> Option<String> {
    el.attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)
        .and_then(|a| resolved_attribute_string(&a.value, scope))
}

/// **Wave 14.2** — typed-aware bare-attribute reader for `<let>`.
/// For a pure `{expr}` body, returns the underlying
/// [`serde_json::Value`] (number / bool / object preserved). For a
/// templated `"prefix-{expr}"`, returns a resolved [`Value::String`]
/// — mixing literal text with interpolation can only land as a
/// string. Plain string attributes (`value="lit"`) land as
/// [`Value::String`] verbatim.
pub(super) fn evaluate_bare_attr_typed(
    el: &Element,
    name: &str,
    scope: &LowerScope,
) -> Option<serde_json::Value> {
    let attr = el
        .attributes
        .iter()
        .find(|a| matches!(a.name.namespace, AttributeNamespace::Bare) && a.name.local == name)?;
    match &attr.value {
        AttributeValue::String { value, .. } => Some(serde_json::Value::String(value.clone())),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => {
            if let Some(v) = lookup_expression(&expr.body, scope) {
                return Some(v.clone());
            }
            evaluate_expression(&expr.body, scope)
        }
        AttributeValue::Template { .. } => {
            resolved_attribute_string(&attr.value, scope).map(serde_json::Value::String)
        }
    }
}

/// Plain attribute reader — returns the raw text without resolving
/// `{expr}` interpolations against any scope. Used by `heading_font_size`
/// where the value must be a literal integer.
pub(super) fn attribute_string(value: &AttributeValue) -> Option<String> {
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(format!("{{{}}}", expr.body)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        out.push('{');
                        out.push_str(&e.body);
                        out.push('}');
                    }
                }
            }
            Some(out)
        }
    }
}

/// Same as [`attribute_string`] but resolves `{expr}` parts through
/// the scope's bindings. Single seam for "interpolated attribute" —
/// every namespaced/bare reader routes through here so adding a new
/// expression form is one place to update.
pub(super) fn resolved_attribute_string(
    value: &AttributeValue,
    scope: &LowerScope,
) -> Option<String> {
    fn resolve_one(body: &str, scope: &LowerScope) -> String {
        // Cheap path first — bare dotted-path binding lookup, with
        // virtual `.length`/`.first`/`.last` segments handled by
        // [`lookup_path_owned`]. Keeps typed `Value::String("hi")`
        // stringified verbatim (lookup returns `"hi"`, `stringify_value`
        // strips the quotes) and avoids paying the expression-parser
        // cost for plain `{name}` interpolations. Operator-bearing
        // expressions fall through to `evaluate_expression`.
        if let Some(v) = lookup_path_owned(body, scope) {
            return stringify_value(&v);
        }
        match evaluate_expression(body, scope) {
            Some(v) => stringify_value(&v),
            None => String::new(),
        }
    }
    match value {
        AttributeValue::String { value, .. } => Some(value.clone()),
        AttributeValue::Empty => None,
        AttributeValue::Expression(expr) => Some(resolve_one(&expr.body, scope)),
        AttributeValue::Template { parts, .. } => {
            let mut out = String::new();
            for part in parts {
                match part {
                    prism_core::language::prism_ui::ast::TemplatePart::Literal {
                        value, ..
                    } => out.push_str(value),
                    prism_core::language::prism_ui::ast::TemplatePart::Expression(e) => {
                        out.push_str(&resolve_one(&e.body, scope))
                    }
                }
            }
            Some(out)
        }
    }
}

/// Number of points a Luau easing closure is sampled at when lowered
/// to a `data-transition-easing` LUT. 24 stops resolves a cubic /
/// spring curve smoothly under linear inter-sample interpolation while
/// keeping the one-time sampling cost (24 cached Lua calls) trivial.
#[cfg_attr(not(feature = "luau"), allow(dead_code))]
const EASING_LUT_SAMPLES: usize = 24;

/// **§4.1 (`prism-cross-cutting-systems.md`)** — encode an
/// `animator:easing` value into the `data-transition-easing` attr
/// the [`crate::animator::Animator`] reads.
///
/// - A **named keyword** (`"ease-in"`, `linear`, …), whether written
///   bare or via a binding, round-trips as the keyword string —
///   `animator::parse_easing` maps it to the matching builtin curve.
/// - A **Luau closure** (`{\fn(t) return 1-(1-t)^3 end}`) is sampled
///   *here*, while the per-document Lua frame is live, into a
///   comma-joined LUT of [`EASING_LUT_SAMPLES`] stops. The animator
///   then interpolates the table with zero per-frame Lua calls and
///   never holds a Lua handle past the lowering pass.
///
/// Returns `None` (attr omitted → animator defaults to linear) when
/// the closure can't be sampled (no Lua frame on the SSR/no-`luau`
/// path, or a closure error) — a malformed easing degrades motion to
/// linear, it never fails the render.
fn encode_easing_attr(value: &AttributeValue, scope: &LowerScope) -> Option<String> {
    let body = match value {
        AttributeValue::Expression(e) => e.body.trim().to_string(),
        // A keyword string / binding / template resolves through the
        // normal attribute path (`animator:easing="ease-in"` or
        // `animator:easing={someKeyword}`).
        _ => return resolved_attribute_string(value, scope).filter(|s| !s.is_empty()),
    };

    #[cfg(feature = "luau")]
    {
        if looks_like_closure(&body) {
            let frame = scope.luau_scope()?;
            let mut samples = Vec::with_capacity(EASING_LUT_SAMPLES);
            for i in 0..EASING_LUT_SAMPLES {
                let t = i as f64 / (EASING_LUT_SAMPLES - 1) as f64;
                let out = frame.call_closure(&body, &[serde_json::Value::from(t)])?;
                let v = out.ok()?;
                samples.push(v.as_f64()? as f32);
            }
            let joined = samples
                .iter()
                .map(|f| {
                    // Trim trailing zeros so the attr stays compact
                    // and the round-trip is exact for typical curves.
                    let s = format!("{f:.6}");
                    let s = s.trim_end_matches('0').trim_end_matches('.');
                    if s.is_empty() { "0" } else { s }.to_string()
                })
                .collect::<Vec<_>>()
                .join(",");
            return Some(joined);
        }
    }

    // Not a closure (or no `luau` feature): resolve as a keyword
    // expression (`animator:easing={mode}` where `mode == "ease"`).
    let resolved = resolved_attribute_string(value, scope)?;
    if resolved.is_empty() {
        Some(body).filter(|s| !s.is_empty())
    } else {
        Some(resolved)
    }
}
