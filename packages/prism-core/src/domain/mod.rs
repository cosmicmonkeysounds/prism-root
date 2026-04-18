//! `domain` — Layer-1 application domains.
//!
//! Port of `@prism/core/src/domain/*` at commit 8426588 per Phase 2b
//! of the Slint migration plan (see `docs/dev/slint-migration-plan.md`
//! §6.2). Three sub-domains:
//!
//! - [`flux`] — operational hub registry (productivity, people,
//!   finance, inventory) with entity defs, edge defs, automation
//!   presets, and CSV/JSON import/export.
//! - [`timeline`] — pure-data NLE engine: transport, tracks, clips,
//!   automation lanes, tempo map (PPQN), markers. Layer 1 only — no
//!   audio/video APIs.
//! - [`graph_analysis`] — graph utilities over `GraphObject`:
//!   topological sort, cycle detection, blocking-chain / impact
//!   analysis, and a generic CPM planning engine.

pub mod flux;
pub mod graph_analysis;
pub mod timeline;
