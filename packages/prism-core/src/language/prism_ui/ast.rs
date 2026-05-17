//! Typed AST for the `prism-ui` DSL.
//!
//! Mirrors the HTMX-flavoured surface from `clay-migration-plan.md` §4:
//! a [`Document`] is a tree of [`Node`]s; an [`Element`] carries
//! tag-name, namespaced [`Attribute`]s, and child nodes. Inline `{...}`
//! interpolations are represented as [`Expression`] bodies — the body
//! string is later handed to the Prism Syntax expression parser.

use serde::{Deserialize, Serialize};

use crate::language::syntax::SourceRange;

/// Top-level parsed `.prism-ui` source file.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub nodes: Vec<Node>,
}

/// A node in the document tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Node {
    Element(Element),
    /// Static text run (no `{...}` interpolation).
    Text {
        value: String,
        range: SourceRange,
    },
    /// `{expr}` interpolation that lives directly in element children.
    Interpolation(Expression),
    /// `<!-- ... -->` HTML-style comment. Preserved so the formatter
    /// can round-trip.
    Comment {
        value: String,
        range: SourceRange,
    },
}

/// `<tag attr="value" ns:name="value">...children...</tag>`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Element {
    pub tag: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<Node>,
    pub self_closing: bool,
    pub range: SourceRange,
    pub tag_range: SourceRange,
}

/// Single attribute on an element. The [`AttributeName`] carries the
/// namespace classification so downstream lowering can dispatch
/// without restringifying.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Attribute {
    pub name: AttributeName,
    pub value: AttributeValue,
    pub range: SourceRange,
}

/// Parsed attribute name. Stores the raw `local` part (without the
/// namespace prefix) plus the classified [`AttributeNamespace`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeName {
    /// Full attribute name as written, e.g. `on:click` or `style:background`.
    pub raw: String,
    /// The local part after the namespace (`click` for `on:click`).
    /// Equals `raw` when the namespace is `Bare`.
    pub local: String,
    pub namespace: AttributeNamespace,
    pub range: SourceRange,
}

/// Attribute namespace — see plan §4.3.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributeNamespace {
    /// Bare component property (`title="..."`, `level="3"`).
    Bare,
    /// `on:<event>` — signal connection lowered to `Connection`.
    On,
    /// `bind:<prop>` — two-way binding sugar for a signal pair.
    Bind,
    /// Control-flow attribute keyword: `if`, `else-if`, `else`, `for`.
    ControlFlow,
    /// `style:<token>` — token-resolved style attribute.
    Style,
    /// `fct:<name>` — facet binding; carried through as a
    /// `data-fct-*` semantic attr for a host/SSR consumer.
    Facet,
    /// `sig:<name>` — declared signal shorthand.
    Signal,
    /// `aria:*` — pass-through to HTML lowering.
    Aria,
    /// `data:*` — pass-through to HTML lowering.
    Data,
    /// Wave 9.1 of `docs/dev/composable-builder-plan.md` —
    /// `route:<key>="..."` lowers to `data-<key>` semantic attrs.
    /// The shell's chrome components emit `data-role` /
    /// `data-target-id` / `data-direction` / `data-on-click` by
    /// hand; the `route:` namespace lifts the convention into the
    /// DSL so authoring a routable affordance reads
    /// `<container route:role="foo" route:target-id="bar"/>`
    /// instead of a `data-` ladder. Consumers (lowering pipeline,
    /// HTML emitter, SSR) treat `route:k` and `data-k` as
    /// equivalent — the namespace is sugar, not a new runtime
    /// concept.
    Route,
    /// Wave 9.4 — `transition:<prop>="200ms"` declares a CSS-style
    /// transition on a prop. The grammar layer parses the
    /// namespace verbatim; runtime interpretation (animator
    /// install, per-prop interpolation) lands when the
    /// `Effect`-driven animator does (`docs/dev/composable-builder-plan.md` §9.4).
    Transition,
    /// **Wave 13.3** — `use:<modifier-id>[="<value>"]` directive
    /// (Vue `v-X`, Svelte `use:X`). Author shorthand for attaching a
    /// registered `ModifierBehaviour` from DSL. Today the runtime
    /// lowers `use:hover` / `use:tooltip="Click"` to a `data-use-<id>`
    /// semantic attr so author intent round-trips; full modifier-fold
    /// integration is a follow-up alongside the resolver-side
    /// modifier seam.
    Use,
    /// `class:<name>="{cond}"` — Svelte-style reactive class toggle.
    /// The local part is the class name; the value is a boolean
    /// expression. When truthy, the named PRSS class is applied to
    /// the container as if it were part of `class="..."`. No-op when
    /// no stylesheet is loaded — same shape `class="..."` itself
    /// degrades to. Distinct from [`Self::Identifier`], which
    /// classifies bare `class="..."` (the static class list).
    Class,
    /// **Wave 14.6** — `animate:<prop>="<from> <duration>"` declares
    /// an entry transition that fires on the first frame the node
    /// appears: the animator interpolates the prop from `<from>` to
    /// its declared (or default) value over `<duration>`. Lowers to
    /// `data-animate-in-<prop>` semantic attr the runtime animator
    /// reads at observe time. `animate:opacity="0 200ms"` on a
    /// toast / overlay produces a fade-in; `animate:height="0 250ms"`
    /// produces a slide-down expand. Companion to the `transition:`
    /// namespace which owns the mid-life value-change case.
    Animate,
    /// **Wave G (`prui-luau-fusion.md` §7.11)** — `probe:<name>=
    /// "event-key"` taps a render-time value / interaction into a
    /// document-scoped event stream. Lowers to `data-probe-<name>`;
    /// `prism.probes:on(name, fn)` subscribes Luau-side.
    Probe,
    /// **Wave G (§7.12)** — `at:<time>="{ …keyframe… }"` declares a
    /// keyframe animation state at `<time>` (`0`, `50%`, `200ms`).
    /// Lowers to `data-at-<time>`; companion to `transition:` /
    /// `animate:` for multi-stop timelines.
    At,
    /// `class` / `id` — CSS-style addressing for inspector + HTML.
    Identifier,
}

/// Attribute right-hand side. Either a literal string, a single
/// `{expr}` interpolation, or a mix — held as an ordered template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AttributeValue {
    /// No `=` after the attribute name (boolean attribute, like `disabled`).
    Empty,
    /// Pure string literal.
    String { value: String, range: SourceRange },
    /// Pure `{...}` interpolation, no surrounding text.
    Expression(Expression),
    /// String + interpolation segments interleaved.
    Template {
        parts: Vec<TemplatePart>,
        range: SourceRange,
    },
}

/// A single segment of a templated attribute value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TemplatePart {
    Literal { value: String, range: SourceRange },
    Expression(Expression),
}

/// `{expr}` body. The body string is parsed by Prism Syntax's
/// expression scanner downstream — at this level we only carry the
/// textual range and contents.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Expression {
    pub body: String,
    pub range: SourceRange,
}

/// Parser error. The `grammar::parse` entry point returns a
/// `Result<Document, Vec<ParseError>>` so editor diagnostics can
/// surface multiple errors at once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParseError {
    pub message: String,
    pub range: SourceRange,
    pub code: &'static str,
}

impl AttributeNamespace {
    /// Classify a raw attribute name into a namespace + local part.
    /// Returns `(namespace, local_string)` where `local_string` is the
    /// part after the `:` delimiter (or the whole name for bare /
    /// control-flow / identifier attributes).
    ///
    /// **Sugar prefixes** (Vue / Svelte / Solid conventions):
    /// - `@event` ≡ `on:event` (Vue `@click`)
    /// - `:prop` ≡ `bind:prop` (Vue `:value` — note: ambiguous with
    ///   `style:` etc., so only matches a bare `:<local>` without
    ///   another `:` segment).
    ///
    /// Both resolve to the canonical namespace at parse time so the
    /// AST shape is identical and downstream consumers don't have
    /// to learn the alias.
    pub fn classify(raw: &str) -> (AttributeNamespace, String) {
        if let Some(rest) = raw.strip_prefix('@') {
            // `@click` → On / "click". Match Vue exactly.
            return (AttributeNamespace::On, rest.to_string());
        }
        if let Some(rest) = raw.strip_prefix(':') {
            // `:value` → Bind / "value". Vue `:foo` shorthand. The
            // body of `rest` must NOT contain another `:` to avoid
            // accidentally matching pseudo-namespaces that future
            // prefixes might use.
            if !rest.contains(':') && !rest.is_empty() {
                return (AttributeNamespace::Bind, rest.to_string());
            }
        }
        if let Some((prefix, rest)) = raw.split_once(':') {
            let ns = match prefix {
                "on" => AttributeNamespace::On,
                "bind" => AttributeNamespace::Bind,
                "style" => AttributeNamespace::Style,
                "fct" => AttributeNamespace::Facet,
                "sig" => AttributeNamespace::Signal,
                "aria" => AttributeNamespace::Aria,
                "data" => AttributeNamespace::Data,
                "route" => AttributeNamespace::Route,
                "transition" => AttributeNamespace::Transition,
                "animate" => AttributeNamespace::Animate,
                "probe" => AttributeNamespace::Probe,
                "at" => AttributeNamespace::At,
                "use" => AttributeNamespace::Use,
                "class" => AttributeNamespace::Class,
                _ => return (AttributeNamespace::Bare, raw.to_string()),
            };
            return (ns, rest.to_string());
        }
        match raw {
            "if" | "else-if" | "else" | "for" => (AttributeNamespace::ControlFlow, raw.to_string()),
            "class" | "id" => (AttributeNamespace::Identifier, raw.to_string()),
            _ => (AttributeNamespace::Bare, raw.to_string()),
        }
    }
}

/// Wave 9.2 — recognized pseudo-state suffix on an inline-style key.
/// `style:<key>:<state>="<value>"` splits at the trailing colon and
/// the state segment is matched against this set. Anything outside
/// the set is treated as part of the key (no state).
pub const STATE_SUFFIXES: &[&str] = &["hovered", "selected", "focused"];

impl AttributeName {
    /// Wave 9.2 — split a namespaced local part on its trailing
    /// `:state` suffix. `style:background:hovered` is parsed by
    /// [`AttributeNamespace::classify`] as
    /// `(Style, "background:hovered")`; this helper splits the local
    /// further into `("background", Some("hovered"))`. Returns
    /// `(local, None)` when no recognized state segment is found, so
    /// callers can route uniformly. Recognized states: `hovered`,
    /// `selected`, `focused` (see [`STATE_SUFFIXES`]).
    pub fn state_suffix(&self) -> (&str, Option<&str>) {
        split_state_suffix(&self.local)
    }
}

/// Wave 9.2 — pure helper for splitting a local attribute string on
/// its trailing `:state` suffix. Exposed so call sites that already
/// hold a `&str` (lowering passes, codegen) don't need to allocate
/// an [`AttributeName`] to do the split.
pub fn split_state_suffix(local: &str) -> (&str, Option<&str>) {
    if let Some((head, tail)) = local.rsplit_once(':') {
        if STATE_SUFFIXES.contains(&tail) {
            return (head, Some(tail));
        }
    }
    (local, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_bare() {
        let (ns, local) = AttributeNamespace::classify("title");
        assert_eq!(ns, AttributeNamespace::Bare);
        assert_eq!(local, "title");
    }

    #[test]
    fn classify_on_event() {
        let (ns, local) = AttributeNamespace::classify("on:click");
        assert_eq!(ns, AttributeNamespace::On);
        assert_eq!(local, "click");
    }

    #[test]
    fn classify_style_token() {
        let (ns, local) = AttributeNamespace::classify("style:background");
        assert_eq!(ns, AttributeNamespace::Style);
        assert_eq!(local, "background");
    }

    #[test]
    fn classify_control_flow_keyword() {
        let (ns, local) = AttributeNamespace::classify("if");
        assert_eq!(ns, AttributeNamespace::ControlFlow);
        assert_eq!(local, "if");
    }

    #[test]
    fn classify_identifier() {
        let (ns, local) = AttributeNamespace::classify("class");
        assert_eq!(ns, AttributeNamespace::Identifier);
        assert_eq!(local, "class");
    }

    #[test]
    fn classify_unknown_namespace_falls_back_to_bare() {
        let (ns, local) = AttributeNamespace::classify("weird:thing");
        assert_eq!(ns, AttributeNamespace::Bare);
        assert_eq!(local, "weird:thing");
    }

    #[test]
    fn classify_class_toggle() {
        let (ns, local) = AttributeNamespace::classify("class:active");
        assert_eq!(ns, AttributeNamespace::Class);
        assert_eq!(local, "active");
    }

    #[test]
    fn classify_bare_class_stays_identifier() {
        let (ns, local) = AttributeNamespace::classify("class");
        assert_eq!(ns, AttributeNamespace::Identifier);
        assert_eq!(local, "class");
    }

    #[test]
    fn split_state_suffix_recognizes_hovered() {
        assert_eq!(
            split_state_suffix("background:hovered"),
            ("background", Some("hovered"))
        );
    }

    #[test]
    fn split_state_suffix_recognizes_selected_and_focused() {
        assert_eq!(
            split_state_suffix("radius:selected"),
            ("radius", Some("selected"))
        );
        assert_eq!(
            split_state_suffix("background:focused"),
            ("background", Some("focused"))
        );
    }

    #[test]
    fn split_state_suffix_returns_none_for_unknown_suffix() {
        assert_eq!(
            split_state_suffix("background:active"),
            ("background:active", None)
        );
    }

    #[test]
    fn split_state_suffix_returns_none_for_plain_local() {
        assert_eq!(split_state_suffix("background"), ("background", None));
    }
}
