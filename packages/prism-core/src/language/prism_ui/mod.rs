//! `language/prism_ui` — the `prism-ui` DSL `LanguageContribution`.
//!
//! `prism-ui` is the HTMX-flavoured declarative surface for Prism's
//! Clay-based UI runtime (see `docs/dev/clay-migration-plan.md` and
//! ADR-008). Files use the `.prism-ui` extension and carry tag-element
//! markup with attribute-namespace behaviour (`on:click`, `bind:value`,
//! `style:background`, `fct:items`, `sig:clicked`, `if`/`for`/`else`).
//!
//! This module owns:
//!
//! - [`ast`] — typed AST (`Document`, `Element`, `Attribute`,
//!   `AttributeName`, `AttributeValue`, `Node`, `Expression`).
//! - [`grammar`] — `Scanner`-driven parser. **No regex, no hand-rolled
//!   string indexing** per the project's standing rule that all parsers
//!   must go through Prism Syntax.
//! - [`contribution`] — `create_prism_ui_contribution()` returning a
//!   `LanguageContribution<R, E>` for `LanguageRegistry::register`.
//! - [`provider`] — `PrismUiSyntaxProvider` with basic diagnostics,
//!   tag-name / attribute-namespace completions, and hover.
//!
//! Phase-2 of the Clay migration plan. The compile-time codegen
//! (`prism-ui-build`) and runtime interpret path (`prism-ui-runtime`)
//! both consume the AST emitted by [`grammar::parse`].

pub mod ast;
pub mod contribution;
pub mod grammar;
pub mod provider;

pub use ast::{
    split_state_suffix, Attribute, AttributeName, AttributeNamespace, AttributeValue, Document,
    Element, Expression, Node, ParseError, STATE_SUFFIXES,
};
pub use contribution::{create_prism_ui_contribution, PRISM_UI_EXTENSIONS, PRISM_UI_ID};
pub use grammar::parse;
pub use provider::PrismUiSyntaxProvider;
