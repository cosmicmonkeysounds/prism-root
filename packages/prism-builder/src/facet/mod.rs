//! Facet — the data-repeat block.
//!
//! A `facet` node lowers its child subtree (the template) once per item
//! in its data source, interpolating `{{field}}` expressions against
//! each item. The template is ordinary `Node.children` and the data
//! source is read from `Node.props`, so the whole thing is reachable
//! from the standard `Component::lower_ui` contract — no
//! `BuilderDocument` side-table. This replaced the `FacetDef`
//! data-model subsystem; see `docs/dev/wysiwyg-builder-roadmap.md` §4.

mod render;
mod resolve;

pub use render::FacetComponent;
pub use resolve::resolve_template_expressions;
