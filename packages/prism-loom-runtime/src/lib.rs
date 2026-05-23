//! `prism-loom-runtime` — the playback engine for the Loom
//! storytelling language.
//!
//! The runtime layers cleanly:
//!
//! - [`value::Value`] — the universe of things `$` lookups + expression
//!   evaluation land on.
//! - [`ledger::Ledger`] — append-only event history backing
//!   `played(X)` / `visits(X)` / `chose(X)` / `since(X)`.
//! - [`resolver`] — the §11.1 `$` scope chain (let → var → role → entity
//!   → cohort) + a small evaluator over [`resolver::Expr`].
//! - [`bundle::LoomDatabase`] — the postcard-serializable compiled
//!   bundle produced by [`bundle::compile`] from a parsed
//!   `prism_core::language::loom::parser::RootNode`.
//! - [`playhead::Playhead`] — the cursor walking a [`bundle::Document`]
//!   and yielding [`playhead::Frame`]s.
//! - [`show::Show`] — front door wrapping all of the above with
//!   `load(source)` / `step` / `choose`.
//!
//! ## What's in / what's deferred
//!
//! **Phase 1 ships:** compile + ledger + resolver + playhead +
//! choice resolution. Conversation archetypes (`:conversation`,
//! `:immersive`, `:script` shape) play through end-to-end. Diverts
//! resolve, returns pop the stack, choice runs collapse with
//! once-only filtering, sections fall through to the next entry in
//! source order.
//!
//! **Phase 2 ships:** parser expressions lowered to [`resolver::Expr`]
//! ([`expr::compile_expr`]); `if`-guards on choices / sections /
//! after-blocks filter visibility through a Show-time evaluator;
//! [`bundle::Mutation`] captures `~ var $x := v` / `$x := v` / `$x += v`
//! / `~ fire <event>` action lines and applies them through the
//! playhead's mutation lane; top-level `let name = <expr>` bindings
//! evaluate at boot (and re-evaluate after every var write); `each
//! visit` / `after`/`otherwise` / `match` blocks compile to dispatchable
//! [`Item`] variants the playhead resolves at frame time; inline-text
//! `$name` / `${expr}` interpolation resolves through the live context.
//!
//! **Phase 3+ (later):** generators / scenes / scheduler tiers
//! (§9), faction simulator + believed-stance layer (§10), live
//! immersive participant + broadcast machinery (§8), LoroDoc-backed
//! state store + hot reload (§12).

pub mod bundle;
pub mod expr;
pub mod ledger;
pub mod playhead;
pub mod resolver;
pub mod show;
pub mod value;

pub use bundle::{
    compile, AssignOp, CastSlot, CohortDef, CueDef, Document, Item, LetBinding, LocationDef,
    LoomDatabase, Mutation, Section, VisitBranch,
};
pub use expr::compile_expr;
pub use ledger::{Ledger, LedgerEntry, LedgerField};
pub use playhead::{ChoiceFrame, Frame, Playhead};
pub use resolver::{
    evaluate, field_chain, field_chain_safe, resolve_name, Expr, LedgerPredKind, ResolveSource,
    Resolved, ResolverContext,
};
pub use show::{LoadError, LoadResult, Show};
pub use value::Value;
