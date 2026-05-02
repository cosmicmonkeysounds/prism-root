//! `prism-core` — shared Rust foundations for the Slint-era Prism stack.
//!
//! This crate is the Phase-2 target of the Slint migration plan (see
//! `docs/dev/slint-migration-plan.md`). The TypeScript `@prism/core`
//! package has been deleted; everything reloaded onto Rust lands
//! here module by module, leaf-first.
//!
//! Modules ported so far:
//!
//! - [`design_tokens`] — color / spacing / typography constants. Was
//!   `@prism/core/design-tokens` in the legacy tree. Leaf, no deps.
//! - [`shell_mode`]   — the `(shellMode, permission)` runtime context
//!   Studio used to decide which lenses and panels were reachable.
//!   Pure data + pure functions, straightforward port.
//! - [`boot_config`]  — the four-source resolver that used to live in
//!   `src/boot/load-boot-config.ts`. Uses `shell_mode`.
//! - [`foundation`]   — pure data primitives: batch, clipboard, date,
//!   object_model, template, undo, vfs. The optional `crdt` feature
//!   also enables `foundation::persistence` (Loro-backed
//!   `CollectionStore` + `VaultManager` orchestrating a manifest's
//!   collections against a pluggable `PersistenceAdapter`).
//! - [`identity`]     — DID identities and vault encryption: `did`
//!   (Ed25519 sign/verify, multi-sig, import/export) and `encryption`
//!   (AES-GCM-256 vault key manager with HKDF-derived keys).
//! - [`language`]     — syntax scanner / expression parser + evaluator,
//!   the unified `LanguageContribution` registry, the `PrismFile`
//!   document abstraction, the `forms` subtree (field / document /
//!   form schema, form state, wiki links, and Prism's in-house
//!   markdown dialect), the Luau + Markdown contributions, and the
//!   ADR-002 §A3 `codegen` pipeline (symbol DSL + TS/C#/EmmyDoc/GDScript
//!   emitters + AST `TextEmitter` trait).
//! - [`kernel`]       — runtime wiring: the reducer-style `Store<S>`
//!   that replaces `zustand` (§6.1 / §7 of the Slint migration plan),
//!   `state_machine::machine` (the flat, context-free FSM from
//!   `kernel/state-machine/machine.ts`), and `config` — the layered
//!   `ConfigRegistry` + `ConfigModel` + `FeatureFlags` port with
//!   scope cascade, watchers, pluggable stores, and a JSON Schema
//!   subset validator. The xstate-backed tool machine is deferred to
//!   its own `statig` rewrite.
//! - [`interaction`]  — pure-logic counterparts to the legacy
//!   `@prism/core/interaction/*` subtree: `notification` (registry +
//!   debounced queue), `activity` (per-object log + formatter +
//!   date-bucketing), and `query` (filter / sort / group pipeline
//!   over `GraphObject`). The legacy `ViewMode` enum is deliberately
//!   not ported — every view is a `prism_builder::Component`.
//! - [`domain`]       — Layer-1 application domains ported from
//!   `@prism/core/domain/*`: `flux` (Flux life-OS entity / edge /
//!   automation registry + CSV / JSON import-export), `timeline`
//!   (pure-data NLE / show-control engine with `TempoMap`,
//!   `ManualClock`, transport / track / clip / automation / marker
//!   CRUD, and an event bus), and `graph_analysis` (topological sort,
//!   cycle detection, blocking-chain / impact-analysis BFS, and CPM
//!   `compute_plan`).
//!
//! Phase 2b (see `docs/dev/slint-migration-plan.md` §6.2) is the
//! residual port scope: the ADR-002 `kernel` orchestration kit
//! (`actor`, `automation`, `intelligence`, `plugin`, `plugin_bundles`,
//! `builder`, `initializer`) that `PrismKernel` will compose, plus
//! `network` and the `statig` rewrite of the xstate tool machine.
//! None are on Phase 3's critical path.

pub mod boot_config;
pub mod design_tokens;
pub mod domain;
pub mod editor;
pub mod foundation;
pub mod help;
pub mod identity;
pub mod interaction;
pub mod kernel;
pub mod language;
pub mod network;
pub mod shell_mode;
pub mod widget;

pub use boot_config::{BootConfig, DEFAULT_BOOT_CONFIG};
pub use design_tokens::DesignTokens;
pub use kernel::atom::{select, select_ref, Atom, AtomSubscription, SharedAtom};
#[cfg(feature = "crdt")]
pub use kernel::crdt_sync::{CrdtSync, SyncEvent, SyncSubscription};
pub use kernel::{Action, Store, Subscription};
pub use shell_mode::{Permission, ShellMode, ShellModeContext};
