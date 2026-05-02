//! `domain` — Layer-1 application domains.
//!
//! Port of `@prism/core/src/domain/*` at commit 8426588 per Phase 2b
//! of the Slint migration plan (see `docs/dev/slint-migration-plan.md`
//! §6.2). Sub-domains:
//!
//! - [`flux`] — operational hub registry (productivity, people,
//!   finance, inventory) with entity defs, edge defs, automation
//!   presets, and CSV/JSON import/export.
//! - [`timekeeping`] — stopwatch / timer engine: state-machine
//!   stopwatch with pause/resume, hooks, listeners, and snapshots.
//! - [`timeline`] — pure-data NLE engine: transport, tracks, clips,
//!   automation lanes, tempo map (PPQN), markers. Layer 1 only — no
//!   audio/video APIs.
//! - [`graph_analysis`] — graph utilities over `GraphObject`:
//!   topological sort, cycle detection, blocking-chain / impact
//!   analysis, and a generic CPM planning engine.
//! - [`projects`] — risk register, scope tracker, velocity
//!   calculator, project health metrics, and burndown/burnup
//!   data series. CPM critical path lives in [`graph_analysis`].
//! - [`spreadsheet`] — pure-data spreadsheet engine: selection,
//!   virtual scrolling, clipboard TSV interop, CSV/JSON import-export,
//!   and a focused formula engine.
//! - [`goals`] — goal hierarchy, milestone tracking, progress rollup,
//!   and deadline proximity alerts.
//! - [`habits`] — streak computation, completion rates, and composite
//!   wellness scoring.
//! - [`fitness`] — MET-based calorie estimation and personal bests.
//! - [`reminders`] — next occurrence computation (with RRULE),
//!   notification payloads, overdue detection, and snooze support.
//! - [`crm`] — deal pipeline stages, contract lifecycle, client
//!   profitability, and deal/contract bridge functions.
//! - [`focus_planner`] — daily context engine, planning methods,
//!   check-ins, and brain dump.

pub mod calendar;
pub mod crm;
pub mod fitness;
pub mod flux;
pub mod focus_planner;
pub mod goals;
pub mod graph_analysis;
pub mod habits;
pub mod ledger;
pub mod projects;
pub mod reminders;
pub mod spreadsheet;
pub mod timekeeping;
pub mod timeline;
