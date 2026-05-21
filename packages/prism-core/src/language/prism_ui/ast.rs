//! Typed AST for the `prism-ui` DSL.
//!
//! Mirrors the HTMX-flavoured surface from `clay-migration-plan.md` §4:
//! a [`Document`] is a tree of [`Node`]s; an [`Element`] carries
//! tag-name, namespaced [`Attribute`]s, and child nodes. Inline `{...}`
//! interpolations are represented as [`Expression`] bodies — the body
//! string is later handed to the Prism Syntax expression parser.

use serde::{Deserialize, Serialize};

use crate::language::syntax::SourceRange;

/// Top-level parsed `.prui` source file.
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
    /// `sig:<name>` — declared signal shorthand.
    Signal,
    /// `aria:*` — pass-through to HTML lowering.
    Aria,
    /// `data:*` — pass-through to HTML lowering.
    Data,
    /// `class:<name>="{cond}"` — Svelte-style reactive class toggle.
    /// The local part is the class name; the value is a boolean
    /// expression. When truthy, the named PRSS class is applied to
    /// the container as if it were part of `class="..."`. No-op when
    /// no stylesheet is loaded — same shape `class="..."` itself
    /// degrades to. Distinct from [`Self::Identifier`], which
    /// classifies bare `class="..."` (the static class list).
    Class,
    /// **Wave G (`prui-luau-fusion.md` §7.11)** — `probe:<name>=
    /// "event-key"` taps a render-time value / interaction into a
    /// document-scoped event stream. Lowers to `data-probe-<name>`;
    /// `prism.probes:on(name, fn)` subscribes Luau-side.
    Probe,
    /// **§7.15 — unified `Animator` trait.** `animator:<method>=<value>`
    /// is the single home for every animation surface — mid-life
    /// value change, entry / exit transitions, and keyframe stops.
    /// Methods recognised today (lowered to the matching semantic
    /// attribute):
    /// - `animator:in-<prop>="<from> <duration>"` → entry transition
    ///   (`data-animate-in-<prop>`).
    /// - `animator:out-<prop>="<to> <duration>"` → exit transition
    ///   (`data-animate-out-<prop>`).
    /// - `animator:keyframes="<spec>"` → multi-stop timeline
    ///   (`data-animator-keyframes`).
    /// - any other method round-trips as `data-animator-<method>`,
    ///   ready for the §7.15 pipeline / nested-record shapes (Phase 15+).
    ///
    /// Subsumes the retired Phase 4 namespaces (`transition:` /
    /// `animate:` / `at:`) — every spelling collapsed into this one
    /// surface so the AttributeNamespace enum carries one animator
    /// home, not four (`docs/dev/prui-expressiveness-roadmap.md` §7.15).
    Animator,
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
                "sig" => AttributeNamespace::Signal,
                "aria" => AttributeNamespace::Aria,
                "data" => AttributeNamespace::Data,
                "animator" => AttributeNamespace::Animator,
                "probe" => AttributeNamespace::Probe,
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

/// Recognized pseudo-state suffix on an inline-style key.
/// `style:<key>:<state>="<value>"` splits at the trailing colon and
/// the state segment is matched against this set. Anything outside
/// the set is treated as part of the key (no state).
///
/// **Phase 1 (§7.7)** — expanded from the original 3-suffix set
/// (`hovered` / `selected` / `focused`) to cover the pseudo-states
/// authors actually reach for: `pressed` (active-pointer feedback),
/// `disabled` (control-blocked styling + dispatcher click suppression
/// per Q10), `focus-within` (subtree-focused parents), `empty`
/// (placeholder styling), `checked` (toggle states), plus the
/// transition-lifecycle markers `entry` / `exit` owned by the §7.15
/// `Animator` trait. Runtime application is gated separately — see
/// `interpret::style::STATE_PROP_WHITELIST` and the per-state
/// override buckets on `StateOverrides` / siblings.
pub const STATE_SUFFIXES: &[&str] = &[
    "hovered",
    "pressed",
    "focused",
    "focus-within",
    "selected",
    "disabled",
    "empty",
    "checked",
    "entry",
    "exit",
];

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
    fn classify_animator_namespace() {
        // §7.15 — `animator:<method>=<value>` is the unified trait
        // surface for the three Tier-3 namespaces (transition /
        // animate / at). `keyframes` is the canonical Phase-1 method.
        let (ns, local) = AttributeNamespace::classify("animator:keyframes");
        assert_eq!(ns, AttributeNamespace::Animator);
        assert_eq!(local, "keyframes");
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

    #[test]
    fn split_state_suffix_recognizes_phase_1_expanded_states() {
        // §7.7 Phase 1 — the seven states added on top of the
        // original `hovered` / `selected` / `focused` trio. Each must
        // round-trip through `split_state_suffix` as a recognised
        // state segment, not get folded back into the key.
        for state in [
            "pressed",
            "focus-within",
            "disabled",
            "empty",
            "checked",
            "entry",
            "exit",
        ] {
            let local = format!("background:{state}");
            assert_eq!(
                split_state_suffix(&local),
                ("background", Some(state)),
                "expected `{state}` to be recognised as a state suffix",
            );
        }
    }

    #[test]
    fn split_state_suffix_still_rejects_unsupported_states() {
        // Sanity: the expansion is bounded — well-meaning misspellings
        // (`hover` without the `ed`, CSS `:active`, etc.) still fall
        // back to the key path so authors get unstyled output they can
        // diagnose, not silent state binding.
        for unsupported in ["hover", "active", "valid", "checked-radio"] {
            let local = format!("background:{unsupported}");
            let (key, state) = split_state_suffix(&local);
            assert_eq!(state, None, "unexpectedly accepted `{unsupported}`");
            assert_eq!(key, local);
        }
    }
}
