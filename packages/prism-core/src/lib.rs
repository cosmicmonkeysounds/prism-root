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
//!   plus `state_machine::machine` — the flat, context-free FSM from
//!   `kernel/state-machine/machine.ts`. The xstate-backed tool machine
//!   is deferred to its own `statig` rewrite.
//!
//! `kernel::actor`, `kernel::automation`, `interaction`, `network`,
//! and `domain` remain TODO.

pub mod boot_config;
pub mod design_tokens;
pub mod foundation;
pub mod identity;
pub mod interaction;
pub mod kernel;
pub mod language;
pub mod shell_mode;

pub use boot_config::{BootConfig, DEFAULT_BOOT_CONFIG};
pub use design_tokens::DesignTokens;
pub use kernel::{Action, Store, Subscription};
pub use shell_mode::{Permission, ShellMode, ShellModeContext};
