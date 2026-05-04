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
    CONFIG_HANDLE_TYPE_DEF, CONFIG_HANDLE_TYPE_NAME, EDGES_HANDLE_TYPE_DEF, EDGES_HANDLE_TYPE_NAME,
    GRAPH_OBJECT_TYPE_DEF, GRAPH_OBJECT_TYPE_NAME, OBJECTS_HANDLE_TYPE_DEF,
    OBJECTS_HANDLE_TYPE_NAME, OBJECT_EDGE_TYPE_DEF, OBJECT_EDGE_TYPE_NAME,
};

#[cfg(feature = "crdt")]
use crate::foundation::object_model::types::{EdgeId, ObjectEdge as ObjectEdgeType};
#[cfg(feature = "crdt")]
use crate::foundation::object_model::ObjectId;
#[cfg(feature = "crdt")]
use crate::foundation::persistence::{CollectionStore, EdgeFilter, ObjectFilter, ParentIdFilter};
#[cfg(feature = "crdt")]
use chrono::Utc;
#[cfg(feature = "crdt")]
use std::collections::BTreeMap;

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
///
/// When constructed via [`ObjectsHandle::with_collection`], the
/// handle additionally carries a shared reference to a live
/// [`CollectionStore`], which unlocks the instance-level
/// `get` / `list` / `query` / `create` / `update` / `delete`
/// surface (Phase 4 of `docs/dev/luau-integration-plan.md`).
/// Without a collection these methods raise a Luau error so scripts
/// fail loudly rather than silently no-op.
#[derive(Clone, Default)]
pub struct ObjectsHandle {
    inner: Rc<RefCell<ObjectRegistry>>,
    #[cfg(feature = "crdt")]
    collection: Option<Rc<RefCell<CollectionStore>>>,
}

impl ObjectsHandle {
    pub fn new(registry: ObjectRegistry) -> Self {
        Self {
            inner: Rc::new(RefCell::new(registry)),
            #[cfg(feature = "crdt")]
            collection: None,
        }
    }

    pub fn from_shared(inner: Rc<RefCell<ObjectRegistry>>) -> Self {
        Self {
            inner,
            #[cfg(feature = "crdt")]
            collection: None,
        }
    }

    pub fn registry(&self) -> Rc<RefCell<ObjectRegistry>> {
        self.inner.clone()
    }

    /// Attach a live collection store so instance methods (`get`,
    /// `create`, `update`, …) become available to scripts.
    #[cfg(feature = "crdt")]
    pub fn with_collection(mut self, collection: Rc<RefCell<CollectionStore>>) -> Self {
        self.collection = Some(collection);
        self
    }

    #[cfg(feature = "crdt")]
    pub fn collection(&self) -> Option<Rc<RefCell<CollectionStore>>> {
        self.collection.clone()
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

        // ── Instance-level surface (Phase 4) ────────────────────────
        // Available only when the handle was built via
        // `with_collection`. Without one, every call surfaces a typed
        // Luau error so scripts fail loudly instead of silently
        // no-oping — matches the contract documented in the
        // `Sandbox installation paths` table of the plan.
        #[cfg(feature = "crdt")]
        {
            methods.add_method("get", |lua, this, id: String| {
                let store = require_collection(this)?;
                let store = store.borrow();
                match store.get_object(&ObjectId(id)) {
                    Some(obj) => Ok(lua.to_value(&obj)?),
                    None => Ok(mlua::Value::Nil),
                }
            });
            methods.add_method("list", |lua, this, args: Option<mlua::Table>| {
                let store = require_collection(this)?;
                let filter = args.map(filter_from_table).transpose()?;
                let objects = store.borrow().list_objects(filter.as_ref());
                lua.to_value(&objects)
            });
            // `query` is a thin alias for `list` today — Phase 4 keeps
            // them parallel so script authors can pick the verb that
            // reads best at the call site. Phase 6 widens `query` with
            // sort / group / column projection on top of `list`.
            methods.add_method("query", |lua, this, args: Option<mlua::Table>| {
                let store = require_collection(this)?;
                let filter = args.map(filter_from_table).transpose()?;
                let objects = store.borrow().list_objects(filter.as_ref());
                lua.to_value(&objects)
            });
            methods.add_method(
                "create",
                |lua, this, (type_name, payload): (String, Option<mlua::Table>)| {
                    let store = require_collection(this)?;
                    let payload_json: serde_json::Value = match payload {
                        Some(t) => lua.from_value(mlua::Value::Table(t))?,
                        None => serde_json::Value::Object(Default::default()),
                    };
                    let obj = build_object_from_payload(&type_name, payload_json)
                        .map_err(mlua::Error::external)?;
                    store
                        .borrow_mut()
                        .put_object(&obj)
                        .map_err(mlua::Error::external)?;
                    Ok(obj.id.0.clone())
                },
            );
            methods.add_method("update", |lua, this, (id, patch): (String, mlua::Table)| {
                let store = require_collection(this)?;
                let patch_json: serde_json::Value = lua.from_value(mlua::Value::Table(patch))?;
                let mut current = store
                    .borrow()
                    .get_object(&ObjectId(id.clone()))
                    .ok_or_else(|| mlua::Error::external(format!("object '{id}' not found")))?;
                apply_object_patch(&mut current, patch_json).map_err(mlua::Error::external)?;
                current.updated_at = Utc::now();
                store
                    .borrow_mut()
                    .put_object(&current)
                    .map_err(mlua::Error::external)?;
                Ok(())
            });
            methods.add_method("delete", |_, this, id: String| {
                let store = require_collection(this)?;
                let removed = store
                    .borrow_mut()
                    .remove_object(&ObjectId(id))
                    .map_err(mlua::Error::external)?;
                Ok(removed)
            });
        }
    }
}

#[cfg(feature = "crdt")]
fn require_collection(this: &ObjectsHandle) -> mlua::Result<Rc<RefCell<CollectionStore>>> {
    this.collection.clone().ok_or_else(|| {
        mlua::Error::external(
            "prism.objects: instance API unavailable in this context (no live collection)",
        )
    })
}

#[cfg(feature = "crdt")]
fn require_edge_collection(this: &EdgesHandle) -> mlua::Result<Rc<RefCell<CollectionStore>>> {
    this.collection.clone().ok_or_else(|| {
        mlua::Error::external(
            "prism.edges: instance API unavailable in this context (no live collection)",
        )
    })
}

#[cfg(feature = "crdt")]
fn filter_from_table(t: mlua::Table) -> mlua::Result<ObjectFilter> {
    let mut filter = ObjectFilter::default();
    if let Ok(Some(v)) = t.get::<Option<Vec<String>>>("types") {
        filter.types = Some(v);
    }
    if let Ok(Some(v)) = t.get::<Option<Vec<String>>>("tags") {
        filter.tags = Some(v);
    }
    if let Ok(Some(v)) = t.get::<Option<Vec<String>>>("statuses") {
        filter.statuses = Some(v);
    }
    if let Ok(Some(v)) = t.get::<Option<bool>>("exclude_deleted") {
        filter.exclude_deleted = v;
    }
    if let Ok(Some(parent)) = t.get::<Option<mlua::Value>>("parent_id") {
        filter.parent_id = match parent {
            mlua::Value::Nil => ParentIdFilter::Root,
            mlua::Value::String(s) => ParentIdFilter::Some(ObjectId(s.to_str()?.to_string())),
            _ => ParentIdFilter::Any,
        };
    }
    Ok(filter)
}

#[cfg(feature = "crdt")]
fn build_object_from_payload(
    type_name: &str,
    payload: serde_json::Value,
) -> Result<crate::foundation::object_model::types::GraphObject, String> {
    use crate::foundation::object_model::types::GraphObject;
    use serde_json::Value as J;
    let obj_map = match payload {
        J::Object(map) => map,
        J::Null => serde_json::Map::new(),
        _ => return Err("payload must be a table".into()),
    };
    let id = obj_map
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("obj-{}", uuid_like()));
    let name = obj_map
        .get("name")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_default();
    let mut obj = GraphObject::new(id, type_name.to_string(), name);
    apply_object_patch(&mut obj, J::Object(obj_map))?;
    Ok(obj)
}

#[cfg(feature = "crdt")]
fn apply_object_patch(
    obj: &mut crate::foundation::object_model::types::GraphObject,
    patch: serde_json::Value,
) -> Result<(), String> {
    use serde_json::Value as J;
    let map = match patch {
        J::Object(m) => m,
        _ => return Err("patch must be a table".into()),
    };
    for (key, value) in map {
        match key.as_str() {
            "id" | "type" | "createdAt" | "updatedAt" => {} // immutable on update
            "name" => {
                if let J::String(s) = value {
                    obj.name = s;
                }
            }
            "parent_id" | "parentId" => match value {
                J::Null => obj.parent_id = None,
                J::String(s) => obj.parent_id = Some(ObjectId(s)),
                _ => {}
            },
            "position" => {
                if let Some(n) = value.as_f64() {
                    obj.position = n;
                }
            }
            "status" => {
                obj.status = match value {
                    J::Null => None,
                    J::String(s) => Some(s),
                    _ => obj.status.take(),
                }
            }
            "tags" => {
                if let J::Array(arr) = value {
                    obj.tags = arr
                        .into_iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect();
                }
            }
            "date" => {
                obj.date = match value {
                    J::Null => None,
                    J::String(s) => Some(s),
                    _ => obj.date.take(),
                }
            }
            "end_date" | "endDate" => {
                obj.end_date = match value {
                    J::Null => None,
                    J::String(s) => Some(s),
                    _ => obj.end_date.take(),
                }
            }
            "description" => {
                if let J::String(s) = value {
                    obj.description = s;
                }
            }
            "color" => {
                obj.color = match value {
                    J::Null => None,
                    J::String(s) => Some(s),
                    _ => obj.color.take(),
                }
            }
            "image" => {
                obj.image = match value {
                    J::Null => None,
                    J::String(s) => Some(s),
                    _ => obj.image.take(),
                }
            }
            "pinned" => {
                if let Some(b) = value.as_bool() {
                    obj.pinned = b;
                }
            }
            "data" => {
                if let J::Object(m) = value {
                    obj.data = m.into_iter().collect();
                }
            }
            other => {
                // Unrecognised top-level keys are nested into `data`,
                // matching the convention every UI mutation already
                // follows (see `MutateNodeProp` in the shell).
                obj.data.insert(other.to_string(), value);
            }
        }
    }
    Ok(())
}

#[cfg(feature = "crdt")]
fn uuid_like() -> String {
    // Cheap monotonic-ish id without pulling in `uuid` from a method
    // hot path. Collisions inside one script tick are vanishingly
    // unlikely and the CRDT layer dedups anyway.
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("{nanos:x}-{n:x}")
}

// ───── EdgesHandle ──────────────────────────────────────────────────

/// Cheaply cloneable Lua-side handle for the edge half of the object
/// graph. Mirrors [`ObjectsHandle`] but for [`ObjectEdge`]s.
/// Constructed via [`EdgesHandle::with_collection`]; without one, the
/// instance API raises a typed Luau error.
#[derive(Clone, Default)]
pub struct EdgesHandle {
    #[cfg(feature = "crdt")]
    collection: Option<Rc<RefCell<CollectionStore>>>,
}

impl EdgesHandle {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(feature = "crdt")]
    pub fn with_collection(mut self, collection: Rc<RefCell<CollectionStore>>) -> Self {
        self.collection = Some(collection);
        self
    }

    #[cfg(feature = "crdt")]
    pub fn collection(&self) -> Option<Rc<RefCell<CollectionStore>>> {
        self.collection.clone()
    }
}

impl UserData for EdgesHandle {
    fn add_methods<M: UserDataMethods<Self>>(_methods: &mut M) {
        #[cfg(feature = "crdt")]
        {
            _methods.add_method("get", |lua, this, id: String| {
                let store = require_edge_collection(this)?;
                let edge = store.borrow().get_edge(&EdgeId(id));
                match edge {
                    Some(edge) => Ok(lua.to_value(&edge)?),
                    None => Ok(mlua::Value::Nil),
                }
            });
            _methods.add_method("list", |lua, this, args: Option<mlua::Table>| {
                let store = require_edge_collection(this)?;
                let filter = args.map(edge_filter_from_table).transpose()?;
                let edges = store.borrow().list_edges(filter.as_ref());
                lua.to_value(&edges)
            });
            _methods.add_method("query", |lua, this, args: Option<mlua::Table>| {
                let store = require_edge_collection(this)?;
                let filter = args.map(edge_filter_from_table).transpose()?;
                let edges = store.borrow().list_edges(filter.as_ref());
                lua.to_value(&edges)
            });
            _methods.add_method(
                "create",
                |lua, this, (relation, payload): (String, mlua::Table)| {
                    let store = require_edge_collection(this)?;
                    let payload_json: serde_json::Value =
                        lua.from_value(mlua::Value::Table(payload))?;
                    let edge = build_edge_from_payload(&relation, payload_json)
                        .map_err(mlua::Error::external)?;
                    store
                        .borrow_mut()
                        .put_edge(&edge)
                        .map_err(mlua::Error::external)?;
                    Ok(edge.id.0.clone())
                },
            );
            _methods.add_method("delete", |_, this, id: String| {
                let store = require_edge_collection(this)?;
                let removed = store
                    .borrow_mut()
                    .remove_edge(&EdgeId(id))
                    .map_err(mlua::Error::external)?;
                Ok(removed)
            });
        }
    }
}

#[cfg(feature = "crdt")]
fn edge_filter_from_table(t: mlua::Table) -> mlua::Result<EdgeFilter> {
    let mut filter = EdgeFilter::default();
    if let Ok(Some(s)) = t.get::<Option<String>>("source_id") {
        filter.source_id = Some(ObjectId(s));
    }
    if let Ok(Some(s)) = t.get::<Option<String>>("target_id") {
        filter.target_id = Some(ObjectId(s));
    }
    if let Ok(Some(s)) = t.get::<Option<String>>("relation") {
        filter.relation = Some(s);
    }
    Ok(filter)
}

#[cfg(feature = "crdt")]
fn build_edge_from_payload(
    relation: &str,
    payload: serde_json::Value,
) -> Result<ObjectEdgeType, String> {
    use serde_json::Value as J;
    let map = match payload {
        J::Object(m) => m,
        _ => return Err("payload must be a table".into()),
    };
    let source = map
        .get("source_id")
        .or_else(|| map.get("sourceId"))
        .or_else(|| map.get("source"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "edge payload missing source_id".to_string())?
        .to_string();
    let target = map
        .get("target_id")
        .or_else(|| map.get("targetId"))
        .or_else(|| map.get("target"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| "edge payload missing target_id".to_string())?
        .to_string();
    let id = map
        .get("id")
        .and_then(|v| v.as_str())
        .map(String::from)
        .unwrap_or_else(|| format!("edge-{}", uuid_like()));
    let position = map.get("position").and_then(|v| v.as_f64());
    let data: BTreeMap<String, serde_json::Value> = match map.get("data") {
        Some(J::Object(m)) => m.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
        _ => BTreeMap::new(),
    };
    Ok(ObjectEdgeType {
        id: EdgeId(id),
        source_id: ObjectId(source),
        target_id: ObjectId(target),
        relation: relation.to_string(),
        position,
        created_at: Utc::now(),
        data,
    })
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
        methods.add_method(
            "reset",
            |_, this, (key, scope): (String, Option<SettingScope>)| {
                this.model.reset(&key, scope.unwrap_or(SettingScope::User));
                Ok(())
            },
        );
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
        lua.load("config:set('editor.fontSize', 18)")
            .exec()
            .unwrap();
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

    // ── Phase 4 instance API ────────────────────────────────────────

    #[cfg(feature = "crdt")]
    #[test]
    fn objects_create_get_update_delete_round_trip() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let handle = ObjectsHandle::default().with_collection(store.clone());
        let lua = Lua::new();
        lua.globals().set("o", handle).unwrap();
        let id: String = lua
            .load("return o:create('task', { name = 'Write tests', status = 'active' })")
            .eval()
            .unwrap();
        assert!(!id.is_empty());
        let name: String = lua
            .load(format!("return o:get('{id}').name"))
            .eval()
            .unwrap();
        assert_eq!(name, "Write tests");
        lua.load(format!("o:update('{id}', {{ name = 'Done' }})"))
            .exec()
            .unwrap();
        let renamed: String = lua
            .load(format!("return o:get('{id}').name"))
            .eval()
            .unwrap();
        assert_eq!(renamed, "Done");
        let count: i64 = lua.load("return #o:list()").eval().unwrap();
        assert_eq!(count, 1);
        let removed: bool = lua.load(format!("return o:delete('{id}')")).eval().unwrap();
        assert!(removed);
        let count_after: i64 = lua.load("return #o:list()").eval().unwrap();
        assert_eq!(count_after, 0);
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn objects_query_filters_by_type() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        store
            .borrow_mut()
            .put_object(&GraphObject::new("a", "task", "A"))
            .unwrap();
        store
            .borrow_mut()
            .put_object(&GraphObject::new("b", "note", "B"))
            .unwrap();
        let handle = ObjectsHandle::default().with_collection(store);
        let lua = Lua::new();
        lua.globals().set("o", handle).unwrap();
        let count: i64 = lua
            .load("return #o:query({ types = { 'task' } })")
            .eval()
            .unwrap();
        assert_eq!(count, 1);
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn objects_instance_api_errors_without_collection() {
        let lua = Lua::new();
        lua.globals().set("o", ObjectsHandle::default()).unwrap();
        let err = lua
            .load("return o:get('anything')")
            .eval::<mlua::Value>()
            .unwrap_err();
        assert!(err.to_string().contains("instance API unavailable"));
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn edges_create_get_delete_round_trip() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let handle = EdgesHandle::default().with_collection(store);
        let lua = Lua::new();
        lua.globals().set("e", handle).unwrap();
        let id: String = lua
            .load("return e:create('depends-on', { source_id = 'a', target_id = 'b' })")
            .eval()
            .unwrap();
        assert!(!id.is_empty());
        let relation: String = lua
            .load(format!("return e:get('{id}').relation"))
            .eval()
            .unwrap();
        assert_eq!(relation, "depends-on");
        let listed: i64 = lua
            .load("return #e:list({ source_id = 'a' })")
            .eval()
            .unwrap();
        assert_eq!(listed, 1);
        let removed: bool = lua.load(format!("return e:delete('{id}')")).eval().unwrap();
        assert!(removed);
    }

    #[cfg(feature = "crdt")]
    #[test]
    fn edges_create_requires_source_and_target() {
        let store = Rc::new(RefCell::new(CollectionStore::new()));
        let handle = EdgesHandle::default().with_collection(store);
        let lua = Lua::new();
        lua.globals().set("e", handle).unwrap();
        let err = lua
            .load("return e:create('depends-on', { source_id = 'a' })")
            .eval::<mlua::Value>()
            .unwrap_err();
        assert!(err.to_string().contains("missing target_id"));
    }
}
