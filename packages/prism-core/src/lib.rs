//! `prism-core` — shared Rust foundations for the Clay-era Prism stack.
//!
//! This crate is the Phase-2 target of the Clay migration plan (see
//! `docs/dev/clay-migration-plan.md`). The TypeScript `@prism/core`
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
//!
//! Everything else from the legacy tree (foundation, identity,
//! language, kernel, interaction, network, domain) is TODO and will
//! land in Phase 2.

pub mod boot_config;
pub mod design_tokens;
pub mod shell_mode;

pub use boot_config::{BootConfig, DEFAULT_BOOT_CONFIG};
pub use design_tokens::DesignTokens;
pub use shell_mode::{Permission, ShellMode, ShellModeContext};
