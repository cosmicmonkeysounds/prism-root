//! Loom v3 runtime — the playback engine.
//!
//! Compiles a parsed [`loom_parser::ast`] tree into a postcard-
//! serializable bundle and drives the playhead, ledger, reactive
//! graph, tiered scheduler, and Luau directive registry. The full
//! design lives in [`docs/dev/loom-v3.html`](../../../../docs/dev/loom-v3.html).
//!
//! ## Architecture summary
//!
//! A Loom show is **imperative on the outside, reactive on the
//! inside** (spec §4). The playhead reads top-to-bottom; *around* it
//! a reactive graph holds derived state (`let`), predicates (goals,
//! hooks), and live collections (factions, cohorts). Hooks queue and
//! drain at the next playhead yield — they never preempt the
//! playhead.
//!
//! ## Module roadmap
//!
//! | Module          | Spec § | Status                                                 |
//! |-----------------|--------|--------------------------------------------------------|
//! | [`bundle`]      | §3, §6 | compiled program: files + cross-file indices (Phase 3) |
//! | [`project`]     | §3     | folder loader + in-memory `from_sources` (Phase 3)     |
//! | [`resolver`]    | §3, §7 | divert lookup, ambiguity errors (Phase 3)              |
//! | [`ledger`]      | §10–12 | append-only `Event` stream (Phase 3)                   |
//! | [`playhead`]    | §4, §7 | `step` / `choose` interpreter over the woven beat tree (Phase 3) |
//! | [`reactive`]    | §4, §12| `let` graph, predicate subscription, hook queue (stub) |
//! | [`scheduler`]   | §12.5  | focal / active / ambient tiers with per-tick budgets (stub) |
//! | [`simulacra`]   | §10    | characters: disposition, knowledge, goals, hooks (stub) |
//! | [`meridian`]    | §11    | attribute / axis / pool / stat / tree primitives (stub) |
//! | [`live`]        | §13    | participants, cohorts, improv, broadcasts (stub)       |
//! | [`directives`]  | §14    | core Luau registry (`sfx`, `cue`, `set`, …) + bridge (stub) |
//! | [`builtins`]    | §14    | core directive implementations (audio / lighting / …) (stub) |

pub mod builtins;
pub mod bundle;
pub mod directives;
pub mod luau;
pub mod expr;
pub mod ledger;
pub mod live;
pub mod meridian;
pub mod playhead;
pub mod project;
pub mod reactive;
pub mod resolver;
pub mod scheduler;
pub mod simulacra;

pub use bundle::{BeatIdx, BeatRef, Bundle, FileIdx, LoomFileEntry, ProjectDiagnostic};
pub use directives::{DirectiveCall, DirectiveError, Registry};
pub use luau::{register_core_builtins, LuauRegistry};
pub use expr::{Value, World};
pub use ledger::{ChoiceOption, Event, Ledger};
pub use meridian::{AxisMode, AxisState, PoolState, StatsInstance, StatsProfile, Tree};
pub use playhead::{PlayError, Playhead, Step};
pub use resolver::ResolveError;
pub use simulacra::{
    AxisValue, CharacterState, GoalEvent, GoalState, GoalStatus, HookEvent, HookSubscription,
    SetOp,
};
