//! `PrismContext` — the `prism` global injected into every Luau
//! execution. Phases 1–2 of `docs/dev/luau-integration-plan.md`.
//!
//! Phase 1 shipped the leaf surfaces: design tokens and the
//! shell-mode tag. Phase 2 plugs the first stateful subsystems onto
//! the same struct — [`prism_core::luau_bindings::ObjectsHandle`]
//! (wraps `ObjectRegistry`) and [`prism_core::luau_bindings::ConfigHandle`]
//! (wraps `ConfigModel`) — so scripts can query the entity-type
//! registry and read/write user settings without an additional
//! daemon command. Each later phase plugs another subsystem onto
//! this same struct without changing the injection plumbing.

#[cfg(feature = "crdt")]
use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, UserData, UserDataFields};
use prism_core::design_tokens::{DesignTokens, DEFAULT_TOKENS};
#[cfg(feature = "crdt")]
use prism_core::foundation::persistence::CollectionStore;
use prism_core::kernel::config::model::ConfigModel;
use prism_core::kernel::config::registry::ConfigRegistry;
use prism_core::luau_bindings::{ConfigHandle, EdgesHandle, ObjectsHandle};
use prism_core::shell_mode::{Permission, ShellMode};

/// What every Luau script sees as the `prism` global. Cheap to clone:
/// `objects` / `edges` / `config` are `Rc`-backed handles, the rest
/// is `Copy`.
///
/// Phase 4 of `docs/dev/luau-integration-plan.md` adds [`edges`] and
/// the instance-API on [`objects`]; both light up only when the host
/// constructs the context with [`PrismContext::with_collection`].
/// Daemon-only callers stick with [`PrismContext::default`] and see
/// the read-only registry surface they had before.
#[derive(Clone)]
pub struct PrismContext {
    pub tokens: DesignTokens,
    pub shell_mode: ShellMode,
    pub permission: Permission,
    pub objects: ObjectsHandle,
    pub edges: EdgesHandle,
    pub config: ConfigHandle,
}

impl Default for PrismContext {
    fn default() -> Self {
        Self {
            tokens: DEFAULT_TOKENS,
            shell_mode: ShellMode::Build,
            permission: Permission::Dev,
            objects: ObjectsHandle::default(),
            edges: EdgesHandle::default(),
            config: ConfigHandle::new(ConfigModel::new(Rc::new(ConfigRegistry::new()))),
        }
    }
}

impl PrismContext {
    /// Attach a live [`CollectionStore`] so the `prism.objects` and
    /// `prism.edges` instance APIs (`get` / `create` / `update` /
    /// `delete` / `list` / `query`) become available to scripts.
    /// Hosts running outside a shell context (the daemon's bare
    /// `luau.exec`) skip this call so the same scripts surface a
    /// typed error instead of silently mutating no state.
    #[cfg(feature = "crdt")]
    pub fn with_collection(mut self, collection: Rc<RefCell<CollectionStore>>) -> Self {
        self.objects = self.objects.with_collection(collection.clone());
        self.edges = self.edges.with_collection(collection);
        self
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
        // Stateful handles — each clone hands the script another
        // owner of the underlying `Rc<...>`, so script-side mutations
        // are visible in subsequent calls and to the host.
        fields.add_field_method_get("objects", |_, this| Ok(this.objects.clone()));
        fields.add_field_method_get("edges", |_, this| Ok(this.edges.clone()));
        fields.add_field_method_get("config", |_, this| Ok(this.config.clone()));
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
    use prism_core::foundation::object_model::registry::ObjectRegistry;
    use prism_core::foundation::object_model::types::EntityDef;
    use prism_core::kernel::config::types::{SettingDefinition, SettingType};
    use serde_json::json;

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

    #[test]
    fn objects_handle_round_trips_through_prism_global() {
        let mut reg = ObjectRegistry::new();
        reg.register(EntityDef {
            type_name: "task".into(),
            nsid: None,
            category: "record".into(),
            label: "Task".into(),
            plural_label: None,
            description: None,
            color: None,
            default_child_view: None,
            tabs: None,
            child_only: None,
            extra_child_types: None,
            extra_parent_types: None,
            fields: None,
            api: None,
        });
        let lua = Lua::new();
        let ctx = PrismContext {
            objects: ObjectsHandle::new(reg),
            ..Default::default()
        };
        install(&lua, ctx).unwrap();
        let count: i64 = lua
            .load("return #prism.objects:list_types()")
            .eval()
            .unwrap();
        assert_eq!(count, 1);
        let label: String = lua
            .load("return prism.objects:get_type('task').label")
            .eval()
            .unwrap();
        assert_eq!(label, "Task");
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn objects_instance_api_lights_up_when_collection_is_attached() {
        use prism_core::foundation::persistence::CollectionStore;
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let ctx = PrismContext::default().with_collection(store.clone());
        let lua = Lua::new();
        install(&lua, ctx).unwrap();
        let id: String = lua
            .load("return prism.objects:create('task', { name = 'Wire it up' })")
            .eval()
            .unwrap();
        assert!(!id.is_empty());
        let name: String = lua
            .load(format!("return prism.objects:get('{id}').name"))
            .eval()
            .unwrap();
        assert_eq!(name, "Wire it up");
        // Mutation must be visible to the host through the same store.
        let host_view = store
            .borrow()
            .get_object(&prism_core::foundation::object_model::types::ObjectId(
                id.clone(),
            ))
            .unwrap();
        assert_eq!(host_view.name, "Wire it up");
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn edges_instance_api_round_trips_through_prism_global() {
        use prism_core::foundation::persistence::CollectionStore;
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let ctx = PrismContext::default().with_collection(store);
        let lua = Lua::new();
        install(&lua, ctx).unwrap();
        let id: String = lua
            .load(
                "return prism.edges:create('depends-on', \
                 { source_id = 'a', target_id = 'b' })",
            )
            .eval()
            .unwrap();
        assert!(!id.is_empty());
        let listed: i64 = lua
            .load("return #prism.edges:list({ source_id = 'a' })")
            .eval()
            .unwrap();
        assert_eq!(listed, 1);
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn objects_instance_api_errors_in_default_daemon_context() {
        // Default `PrismContext` has no collection — the same script
        // that works in the shell must surface a typed error in the
        // daemon's bare `luau.exec` path.
        let lua = Lua::new();
        install(&lua, PrismContext::default()).unwrap();
        let err = lua
            .load("return prism.objects:get('anything')")
            .eval::<mlua::Value>()
            .unwrap_err();
        assert!(err.to_string().contains("instance API unavailable"));
    }

    #[test]
    fn config_handle_get_set_through_prism_global() {
        let mut registry = ConfigRegistry::new();
        registry.register(SettingDefinition::new(
            "editor.fontSize",
            SettingType::Number,
            json!(14),
            "Font Size",
        ));
        let model = ConfigModel::new(Rc::new(registry));
        let lua = Lua::new();
        let ctx = PrismContext {
            config: ConfigHandle::new(model),
            ..Default::default()
        };
        install(&lua, ctx).unwrap();
        let initial: f64 = lua
            .load("return prism.config:get('editor.fontSize')")
            .eval()
            .unwrap();
        assert_eq!(initial, 14.0);
        lua.load("prism.config:set('editor.fontSize', 18)")
            .exec()
            .unwrap();
        let updated: f64 = lua
            .load("return prism.config:get('editor.fontSize')")
            .eval()
            .unwrap();
        assert_eq!(updated, 18.0);
    }
}
