//! `PrismContext` — the `prism` global injected into every Luau
//! execution. Phase 1 of `docs/dev/luau-integration-plan.md`.
//!
//! The plan describes a god-object userdata that namespaces every
//! Prism subsystem (`prism.objects`, `prism.document`, `prism.signals`,
//! …). Phase 1 ships only the leaves that are wired today: the design
//! tokens and the shell-mode tag. Each later phase plugs another
//! subsystem onto the same `PrismContext` struct without changing the
//! injection plumbing.

use mlua::{Lua, UserData, UserDataFields};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
use prism_core::shell_mode::{Permission, ShellMode};

/// What every Luau script sees as the `prism` global. Cheap to clone —
/// every field is either `Copy` or future-`Arc`-shared.
#[derive(Debug, Clone, Copy)]
pub struct PrismContext {
    pub tokens: DesignTokens,
    pub shell_mode: ShellMode,
    pub permission: Permission,
}

impl Default for PrismContext {
    fn default() -> Self {
        Self {
            tokens: DEFAULT_TOKENS,
            shell_mode: ShellMode::Build,
            permission: Permission::Dev,
        }
    }
}

impl UserData for PrismContext {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        // Design tokens — surfaced as a userdata so scripts can drill
        // into `prism.tokens.colors.accent.r` without round-tripping
        // through serde.
        fields.add_field_method_get("tokens", |_, this| Ok(this.tokens));
        // Tagged enums round-trip as Luau strings via the
        // `#[luau_expose]` IntoLua impl.
        fields.add_field_method_get("shell_mode", |_, this| Ok(this.shell_mode));
        fields.add_field_method_get("permission", |_, this| Ok(this.permission));
    }
}

/// Inject `prism` into the Lua globals table. Lifted out of
/// `luau_module::exec` so other entry points (REPL, signal-handler
/// dispatch, facet resolver) can share the wiring.
pub fn install(lua: &Lua, ctx: PrismContext) -> mlua::Result<()> {
    lua.globals().set("prism", ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    #[test]
    fn prism_global_exposes_default_design_tokens() {
        let lua = Lua::new();
        install(&lua, PrismContext::default()).unwrap();
        let r: u8 = lua
            .load("return prism.tokens.colors.accent.r")
            .eval()
            .unwrap();
        assert_eq!(r, 110);
    }

    #[test]
    fn prism_global_exposes_spacing_constants() {
        let lua = Lua::new();
        install(&lua, PrismContext::default()).unwrap();
        let md: u16 = lua.load("return prism.tokens.spacing.md").eval().unwrap();
        assert_eq!(md, 12);
    }

    #[test]
    fn shell_mode_enum_round_trips_as_string() {
        let lua = Lua::new();
        install(&lua, PrismContext::default()).unwrap();
        let mode: String = lua.load("return prism.shell_mode").eval().unwrap();
        assert_eq!(mode, "Build");
    }

    #[test]
    fn permission_enum_round_trips_as_string() {
        let lua = Lua::new();
        install(&lua, PrismContext::default()).unwrap();
        let perm: String = lua.load("return prism.permission").eval().unwrap();
        assert_eq!(perm, "Dev");
    }

    #[test]
    fn nested_userdata_supports_chained_field_access() {
        let lua = Lua::new();
        let mut ctx = PrismContext::default();
        ctx.tokens.colors.danger.g = 42;
        install(&lua, ctx).unwrap();
        let g: u8 = lua
            .load("return prism.tokens.colors.danger.g")
            .eval()
            .unwrap();
        assert_eq!(g, 42);
    }
}
