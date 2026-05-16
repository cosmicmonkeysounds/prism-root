//! `prism-core` — shared Rust foundations for the post-Slint Prism
//! stack. Post-cutover the workspace is all-Rust on `prism-ui-runtime`;
//! the Slint runtime + source emitter were deleted (see
//! `docs/dev/clay-migration-plan.md` Phase 5). The legacy TypeScript
//! `@prism/core` has been retired in favour of the modules below.
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
//! Residual port scope (tracked in `docs/dev/clay-migration-plan.md`):
//! the ADR-002 `kernel` orchestration kit (`actor`, `automation`,
//! `intelligence`, `plugin`, `plugin_bundles`, `builder`,
//! `initializer`) that `PrismKernel` composes, plus `network` and the
//! `statig` rewrite of the xstate tool machine. None are on the
//! critical path; the table in `CLAUDE.md` tracks per-module status.

pub mod app;
pub mod app_registry;
pub mod boot_config;
pub mod design_tokens;
pub mod domain;
pub mod foundation;
pub mod help;
pub mod identity;
pub mod interaction;
pub mod kernel;
pub mod language;
#[cfg(feature = "luau")]
pub mod luau_bindings;
pub mod luau_bindings_consts;
#[cfg(feature = "luau")]
pub mod luau_reactive;
#[cfg(feature = "luau")]
pub mod luau_runtime;
pub mod luau_types;
pub mod network;
pub mod reactive;
pub mod registry;
pub mod shell_mode;
pub mod widget;

pub use app::{
    AppEntry, AppManifest, AppManifestError, AppPanelDef, AppPanelsSpec, AppServicesSpec,
};
pub use app_registry::{
    AppRegistrar, ComponentRegistration, NoopAppRegistrar, PanelRegistration, RegistrationError,
    ServiceRegistration,
};
pub use boot_config::{BootConfig, DEFAULT_BOOT_CONFIG};
pub use design_tokens::DesignTokens;
pub use help::{HelpEntry, HelpProvider, HelpRegistry};
pub use kernel::atom::{select, select_memo, select_ref, Atom, AtomSubscription};
#[cfg(feature = "crdt")]
pub use kernel::crdt_sync::{CrdtSync, SyncEvent, SyncSubscription};
pub use kernel::{Action, Store, Subscription};
pub use reactive::{
    DirtyQueue, Effect, Memo, Owner, ReactiveContext, Resource, ResourceState, Signal,
};
pub use registry::{Catalog, HasId};
pub use shell_mode::{Permission, ShellMode, ShellModeContext};
