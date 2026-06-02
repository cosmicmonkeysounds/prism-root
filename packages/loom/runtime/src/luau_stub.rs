//! No-op Luau registry — used when the `luau` Cargo feature is off.
//!
//! Web builds (`loom-wasm`) disable the `luau` feature because mluau's
//! Luau C++ requires `wasm32-unknown-emscripten`, which is
//! incompatible with the wasm-bindgen pipeline the editor consumes
//! (see `docs/dev/loom-web-shipping.md` §7). This stub keeps the
//! `Registry` shape and call sites unchanged: any project that loads a
//! `.luau` extension or invokes a non-builtin directive surfaces as
//! `DirectiveError::UnknownKind`, which the playhead already reports
//! cleanly. Built-in syntactic forms (`if`, `let`, `match`, …) and
//! core Rust builtins (`broadcast`, `enroll`, `set`, …) keep working
//! because they go through the trait-object path in
//! [`crate::directives::Registry`], not through Luau.
//!
//! When a real Lua-on-web backend lands (wasmoon / piccolo / TS-side
//! transpile — TBD), this file goes away and the `luau` feature
//! becomes the default for wasm too.

use std::path::Path;

use crate::directives::{DirectiveCall, DirectiveError, DispatchResult};
use crate::expr::World;
use crate::ledger::Ledger;

/// Stub registry. Holds no state; every `contains` returns `false` and
/// every `dispatch` reports `UnknownKind`.
pub struct LuauRegistry;

impl LuauRegistry {
    pub fn new() -> Result<Self, DirectiveError> {
        Ok(Self)
    }

    pub fn with_core_builtins() -> Result<Self, DirectiveError> {
        Ok(Self)
    }

    pub fn contains(&self, _name: &str) -> bool {
        false
    }

    pub fn load_extension(&self, _path: &Path) -> Result<(), DirectiveError> {
        // No Luau VM on this build, so there is nothing to load. We
        // *succeed* (rather than error) so a project that ships `.luau`
        // extensions still loads and plays — the extension's directives
        // degrade to logged envelopes via the registry's lenient mode
        // (see `directives::Registry::lenient`). lua-on-web (a pure-Rust
        // Lua VM) is tracked as a follow-up milestone.
        Ok(())
    }

    pub fn load_extension_str(&self, _name: &str, _source: &str) -> Result<(), DirectiveError> {
        Ok(())
    }

    pub fn dispatch(
        &self,
        call: &DirectiveCall,
        _world: &mut World,
        _ledger: &mut Ledger,
    ) -> Result<DispatchResult, DirectiveError> {
        Err(DirectiveError::UnknownKind(call.kind.clone()))
    }
}

/// Stub `register_core_builtins` — succeeds without registering
/// anything. Keeps the `with_core_builtins` chain functional for
/// callers that build a `Registry` outside the runtime.
pub fn register_core_builtins(_reg: &mut LuauRegistry) -> Result<(), DirectiveError> {
    Ok(())
}
