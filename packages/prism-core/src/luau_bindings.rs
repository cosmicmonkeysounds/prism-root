//! Hand-rolled `mlua::UserData` impls for the stateful Prism
//! subsystems whose Lua surface is a method API rather than a flat
//! field projection. Phase 2 of `docs/dev/luau-integration-plan.md`:
//! the `#[luau_expose]` macro covers leaf data types; everything
//! that has to mediate borrows, dispatch, or interior mutability
//! lives here.
//!
//! What's exposed:
//!
//! * [`GraphObject`] / [`ObjectEdge`] read-side userdata — every
//!   shell field surfaces as a getter, with timestamps coerced to
//!   ISO-8601 strings and the opaque `data` payload promoted to a
//!   plain Lua table via mlua's serde bridge.
//! * [`ObjectsHandle`] — a thin clone of [`Rc<RefCell<ObjectRegistry>>`]
//!   wired up with the registry's read API (`list_types`, `get_type`,
//!   `list_edge_types`, `get_edge_type`, `can_connect`,
//!   `get_effective_tabs`, `get_entity_fields`).
//! * [`ConfigHandle`] — a clone of [`ConfigModel`] with `get` /
//!   `set` / `reset` / `is_overridden` bound to Lua. `set` accepts a
//!   scope as the third arg (defaults to `User`).
//!
//! All four are re-exported through [`crate::luau_types`] so the
//! `prism codegen luau-types` pipeline picks them up alongside the
//! macro-emitted constants.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{Lua, LuaSerdeExt, UserData, UserDataFields, UserDataMethods, Value};

use crate::foundation::object_model::registry::ObjectRegistry;
use crate::foundation::object_model::types::{GraphObject, ObjectEdge};
use crate::kernel::config::model::ConfigModel;
use crate::kernel::config::types::SettingScope;

// Re-export the type-stub constants from the always-compiled
// `luau_bindings_consts` module so callers find both the runtime
// `UserData` impl and its declared type in the same place.
pub use crate::luau_bindings_consts::{
    CONFIG_HANDLE_TYPE_DEF, CONFIG_HANDLE_TYPE_NAME, GRAPH_OBJECT_TYPE_DEF, GRAPH_OBJECT_TYPE_NAME,
    OBJECTS_HANDLE_TYPE_DEF, OBJECTS_HANDLE_TYPE_NAME, OBJECT_EDGE_TYPE_DEF, OBJECT_EDGE_TYPE_NAME,
};

// ───── GraphObject ──────────────────────────────────────────────────

impl UserData for GraphObject {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.0.clone()));
        fields.add_field_method_get("type", |_, this| Ok(this.type_name.clone()));
        fields.add_field_method_get("name", |_, this| Ok(this.name.clone()));
        fields.add_field_method_get("parent_id", |_, this| {
            Ok(this.parent_id.as_ref().map(|i| i.0.clone()))
        });
        fields.add_field_method_get("position", |_, this| Ok(this.position));
        fields.add_field_method_get("status", |_, this| Ok(this.status.clone()));
        fields.add_field_method_get("tags", |_, this| Ok(this.tags.clone()));
        fields.add_field_method_get("date", |_, this| Ok(this.date.clone()));
        fields.add_field_method_get("end_date", |_, this| Ok(this.end_date.clone()));
        fields.add_field_method_get("description", |_, this| Ok(this.description.clone()));
        fields.add_field_method_get("color", |_, this| Ok(this.color.clone()));
        fields.add_field_method_get("image", |_, this| Ok(this.image.clone()));
        fields.add_field_method_get("pinned", |_, this| Ok(this.pinned));
        fields.add_field_method_get("data", |lua, this| lua.to_value(&this.data));
        fields.add_field_method_get("created_at", |_, this| Ok(this.created_at.to_rfc3339()));
        fields.add_field_method_get("updated_at", |_, this| Ok(this.updated_at.to_rfc3339()));
        fields.add_field_method_get("deleted_at", |_, this| {
            Ok(this.deleted_at.map(|d| d.to_rfc3339()))
        });
    }
}

// ───── ObjectEdge ───────────────────────────────────────────────────

impl UserData for ObjectEdge {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("id", |_, this| Ok(this.id.0.clone()));
        fields.add_field_method_get("source_id", |_, this| Ok(this.source_id.0.clone()));
        fields.add_field_method_get("target_id", |_, this| Ok(this.target_id.0.clone()));
        fields.add_field_method_get("relation", |_, this| Ok(this.relation.clone()));
        fields.add_field_method_get("position", |_, this| Ok(this.position));
        fields.add_field_method_get("created_at", |_, this| Ok(this.created_at.to_rfc3339()));
        fields.add_field_method_get("data", |lua, this| lua.to_value(&this.data));
    }
}

// ───── ObjectsHandle ────────────────────────────────────────────────

/// Cheaply cloneable Lua-side handle around the
/// [`ObjectRegistry`]. Mutation goes through interior mutability so
/// scripts can register types without taking `&mut PrismContext`.
#[derive(Clone, Default)]
pub struct ObjectsHandle {
    inner: Rc<RefCell<ObjectRegistry>>,
}

impl ObjectsHandle {
    pub fn new(registry: ObjectRegistry) -> Self {
        Self {
            inner: Rc::new(RefCell::new(registry)),
        }
    }

    pub fn from_shared(inner: Rc<RefCell<ObjectRegistry>>) -> Self {
        Self { inner }
    }

    pub fn registry(&self) -> Rc<RefCell<ObjectRegistry>> {
        self.inner.clone()
    }
}

impl UserData for ObjectsHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("list_types", |_, this, ()| {
            Ok(this
                .inner
                .borrow()
                .all_types()
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>())
        });
        methods.add_method("get_type", |lua, this, type_name: String| {
            let reg = this.inner.borrow();
            match reg.get(&type_name) {
                Some(def) => lua.to_value(def),
                None => Ok(Value::Nil),
            }
        });
        methods.add_method("list_edge_types", |_, this, ()| {
            Ok(this
                .inner
                .borrow()
                .all_edge_types()
                .into_iter()
                .map(String::from)
                .collect::<Vec<String>>())
        });
        methods.add_method("get_edge_type", |lua, this, relation: String| {
            let reg = this.inner.borrow();
            match reg.get_edge_type(&relation) {
                Some(def) => lua.to_value(def),
                None => Ok(Value::Nil),
            }
        });
        methods.add_method("get_category", |_, this, type_name: String| {
            Ok(this.inner.borrow().get_category(&type_name).to_string())
        });
        methods.add_method(
            "can_connect",
            |_, this, (relation, source_type, target_type): (String, String, String)| {
                Ok(this
                    .inner
                    .borrow()
                    .can_connect(&relation, &source_type, &target_type))
            },
        );
        methods.add_method("get_effective_tabs", |lua, this, type_name: String| {
            lua.to_value(&this.inner.borrow().get_effective_tabs(&type_name))
        });
        methods.add_method("get_entity_fields", |lua, this, type_name: String| {
            lua.to_value(&this.inner.borrow().get_entity_fields(&type_name))
        });
    }
}

// ───── ConfigHandle ─────────────────────────────────────────────────

/// Lua-side handle around [`ConfigModel`]. `ConfigModel` is already
/// `Clone`-as-handle (interior `Rc<RefCell<_>>`), so this is a thin
/// wrapper.
#[derive(Clone)]
pub struct ConfigHandle {
    model: ConfigModel,
}

impl ConfigHandle {
    pub fn new(model: ConfigModel) -> Self {
        Self { model }
    }

    pub fn model(&self) -> &ConfigModel {
        &self.model
    }
}

impl UserData for ConfigHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("get", |lua, this, key: String| {
            lua.to_value(&this.model.get(&key))
        });
        methods.add_method(
            "set",
            |lua, this, (key, value, scope): (String, Value, Option<SettingScope>)| {
                let json: serde_json::Value = lua.from_value(value)?;
                let scope = scope.unwrap_or(SettingScope::User);
                this.model
                    .set(&key, json, scope)
                    .map_err(mlua::Error::external)?;
                Ok(())
            },
        );
        methods.add_method("reset", |_, this, (key, scope): (String, Option<SettingScope>)| {
            this.model.reset(&key, scope.unwrap_or(SettingScope::User));
            Ok(())
        });
        methods.add_method("is_overridden", |_, this, key: String| {
            Ok(this.model.is_overridden(&key))
        });
    }
}

// ───── helper: install each type's stub into a target Lua state ─────

/// Convenience wrapper used by tests to install a fresh
/// [`ObjectsHandle`] / [`ConfigHandle`] pair onto a Lua state under
/// `prism.objects` / `prism.config`. The production injection lives
/// in `prism-daemon::modules::prism_context`.
pub fn install_for_test(
    lua: &Lua,
    objects: ObjectsHandle,
    config: ConfigHandle,
) -> mlua::Result<()> {
    lua.globals().set("objects", objects)?;
    lua.globals().set("config", config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::object_model::types::{EntityDef, GraphObject};
    use crate::kernel::config::registry::ConfigRegistry;
    use serde_json::json;

    fn empty_config() -> ConfigHandle {
        ConfigHandle::new(ConfigModel::new(Rc::new(ConfigRegistry::new())))
    }

    #[test]
    fn graph_object_field_getters_round_trip() {
        let lua = Lua::new();
        let mut obj = GraphObject::new("task-1", "task", "Buy milk");
        obj.description = "two pints".into();
        obj.pinned = true;
        obj.tags = vec!["errand".into()];
        lua.globals().set("o", obj).unwrap();
        let id: String = lua.load("return o.id").eval().unwrap();
        assert_eq!(id, "task-1");
        let ty: String = lua.load("return o.type").eval().unwrap();
        assert_eq!(ty, "task");
        let name: String = lua.load("return o.name").eval().unwrap();
        assert_eq!(name, "Buy milk");
        let pinned: bool = lua.load("return o.pinned").eval().unwrap();
        assert!(pinned);
        let tag0: String = lua.load("return o.tags[1]").eval().unwrap();
        assert_eq!(tag0, "errand");
        // `created_at` should round-trip as an ISO-8601 string.
        let created: String = lua.load("return o.created_at").eval().unwrap();
        assert!(created.contains('T'));
    }

    #[test]
    fn objects_handle_lists_registered_types() {
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
        install_for_test(&lua, ObjectsHandle::new(reg), empty_config()).unwrap();
        let count: i64 = lua.load("return #objects:list_types()").eval().unwrap();
        assert_eq!(count, 1);
        let label: String = lua
            .load("return objects:get_type('task').label")
            .eval()
            .unwrap();
        assert_eq!(label, "Task");
    }

    #[test]
    fn config_handle_get_set_round_trips_through_lua() {
        let mut registry = ConfigRegistry::new();
        registry.register(crate::kernel::config::types::SettingDefinition::new(
            "editor.fontSize",
            crate::kernel::config::types::SettingType::Number,
            json!(14),
            "Font Size",
        ));
        let model = ConfigModel::new(Rc::new(registry));
        let lua = Lua::new();
        install_for_test(&lua, ObjectsHandle::default(), ConfigHandle::new(model)).unwrap();
        let initial: f64 = lua
            .load("return config:get('editor.fontSize')")
            .eval()
            .unwrap();
        assert_eq!(initial, 14.0);
        lua.load("config:set('editor.fontSize', 18)").exec().unwrap();
        let updated: f64 = lua
            .load("return config:get('editor.fontSize')")
            .eval()
            .unwrap();
        assert_eq!(updated, 18.0);
        let overridden: bool = lua
            .load("return config:is_overridden('editor.fontSize')")
            .eval()
            .unwrap();
        assert!(overridden);
    }
}
