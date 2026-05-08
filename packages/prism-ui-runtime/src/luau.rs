//! Luau bindings for the runtime — `mlua` `UserData` + a `ui.*`
//! constructor table that lets Luau scripts author, edit, generate,
//! and hook into Clay-backed components from inside Prism's existing
//! script surface.
//!
//! Status: **Phase 1 starter** for the requirement locked in
//! `docs/dev/clay-migration-plan.md` §11 (decision #5, 2026-05-04).
//! Surface-area is intentionally narrow — enough to construct a tree,
//! mutate a node by id, drive a `Surface`, and pull commands as JSON.
//! The richer pieces (signal handlers, lifecycle hooks, the full
//! action grammar) land alongside the DSL in Phase 2.
//!
//! Direction of design (see plan §11.5):
//!
//! 1. `Node`, `ContainerProps`, `TextProps`, `Sizing`, `Padding`,
//!    `Color`, `CornerRadius`, `Viewport` are already plain
//!    `serde`-friendly value types in the parent module — no
//!    lifetimes, no trait objects. That's what lets us wrap them as
//!    `mlua::UserData` *mechanically* via `serde::Deserialize` from
//!    Lua tables (`mlua::LuaSerdeExt::from_value`) instead of
//!    hand-coding every field.
//! 2. Stable string `id`s on every node — Luau handles index by id,
//!    not by tree-walk path, so a CRDT op or a re-render doesn't
//!    dangle a handle.
//! 3. The DSL and Luau authoring produce the **same** typed `Node`
//!    tree. Luau is an alternative front-end, never a parallel
//!    pipeline.
//!
//! ## Example (Luau)
//!
//! ```lua
//! local tree = ui.container({
//!   direction = "column",
//!   gap = 8,
//!   padding = { left = 16, right = 16, top = 16, bottom = 16 },
//!   background = { r = 240, g = 240, b = 240, a = 255 },
//!   children = {
//!     ui.text("Hello", { font_size = 24 }),
//!     ui.spacer({ width = 0, height = 8 }),
//!     ui.text("World", {}),
//!   },
//! })
//!
//! local surface = ui.surface(tree, { width = 800, height = 600 })
//! surface:edit("title", function(node)
//!   node.props.font_size = 32
//! end)
//! local commands = surface:commands()  -- JSON-shaped table
//! ```

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{
    AnyUserData, Function, Lua, LuaSerdeExt, Result as LuaResult, UserData, UserDataMethods, Value,
};
use serde::Deserialize;

use crate::command::{CornerRadius, RenderCommand};
use crate::layout::{ContainerProps, Node, Sizing, Surface, TextProps, Viewport};

/// The Luau-side wrapper around a `Node`. Wrapping in
/// `Rc<RefCell<Node>>` lets multiple Luau handles point at the same
/// underlying node and lets `Surface::edit` reach into a tree by id
/// without duplicating it.
#[derive(Clone)]
pub struct LuaNode(pub Rc<RefCell<Node>>);

impl LuaNode {
    fn new(node: Node) -> Self {
        Self(Rc::new(RefCell::new(node)))
    }

    fn into_inner(self) -> Node {
        match Rc::try_unwrap(self.0) {
            Ok(cell) => cell.into_inner(),
            Err(rc) => rc.borrow().clone(),
        }
    }

    fn snapshot(&self) -> Node {
        self.0.borrow().clone()
    }
}

impl UserData for LuaNode {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("id", |_, this, ()| Ok(this.0.borrow().id().to_owned()));

        methods.add_method("kind", |_, this, ()| Ok(this.0.borrow().kind()));

        methods.add_method("to_json", |lua, this, ()| {
            let value =
                serde_json::to_value(&*this.0.borrow()).map_err(|e| mlua::Error::external(e))?;
            lua.to_value(&value)
        });

        // Append a child to this node. No-op for non-containers; we
        // surface that as an error so Luau scripts catch the mistake
        // instead of silently dropping the call.
        methods.add_method_mut("push", |_, this, child: AnyUserData| {
            let child_node = child.borrow::<LuaNode>()?.snapshot();
            match &mut *this.0.borrow_mut() {
                Node::Container { children, .. } => {
                    children.push(child_node);
                    Ok(())
                }
                _ => Err(mlua::Error::external(
                    "ui.Node:push only valid on container nodes",
                )),
            }
        });
    }
}

/// The Luau-side wrapper around a `Surface`. Backed by `Rc<RefCell>`
/// so multiple Luau handles see the same retained-mode cache.
#[derive(Clone)]
pub struct LuaSurface(pub Rc<RefCell<Surface>>);

impl UserData for LuaSurface {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("set_tree", |_, this, node: AnyUserData| {
            let tree = node.borrow::<LuaNode>()?.snapshot();
            this.0.borrow_mut().set_tree(tree);
            Ok(())
        });

        methods.add_method("set_viewport", |lua, this, vp: Value| {
            let viewport: Viewport = lua.from_value(vp)?;
            this.0.borrow_mut().set_viewport(viewport);
            Ok(())
        });

        methods.add_method("invalidate", |_, this, ()| {
            this.0.borrow_mut().invalidate();
            Ok(())
        });

        methods.add_method("is_dirty", |_, this, ()| Ok(this.0.borrow().is_dirty()));

        // `surface:edit("node-id", function(node) ... end)` — finds
        // the node by id, hands it to the Luau callback as a JSON
        // table, then writes any mutations back. Marks the surface
        // dirty unconditionally. This is the §11 "edit" capability.
        methods.add_method("edit", |lua, this, (id, f): (String, Function)| {
            let mut surface = this.0.borrow_mut();
            let mut applied = false;
            surface.with_tree_mut(|root| {
                if let Some(target) = find_node_mut(root, &id) {
                    if let Ok(value) = serde_json::to_value(&*target) {
                        if let Ok(lua_value) = lua.to_value(&value) {
                            if let Ok(returned) = f.call::<Value>(lua_value) {
                                if let Ok(json) = lua.from_value::<serde_json::Value>(returned) {
                                    if let Ok(updated) = serde_json::from_value::<Node>(json) {
                                        *target = updated;
                                        applied = true;
                                    }
                                }
                            }
                        }
                    }
                }
            });
            Ok(applied)
        });

        methods.add_method("commands", |lua, this, ()| {
            let mut surface = this.0.borrow_mut();
            let commands: &[RenderCommand] = surface.commands();
            let value = serde_json::to_value(commands).map_err(mlua::Error::external)?;
            lua.to_value(&value)
        });
    }
}

fn find_node_mut<'a>(root: &'a mut Node, target_id: &str) -> Option<&'a mut Node> {
    if root.id() == target_id {
        return Some(root);
    }
    if let Node::Container { children, .. } = root {
        for child in children {
            if let Some(hit) = find_node_mut(child, target_id) {
                return Some(hit);
            }
        }
    }
    None
}

/// Schema for the `props` argument to `ui.container` — every field
/// optional and serde-deserialised from a Lua table. Keeping this
/// inline (rather than reusing `ContainerProps` directly) lets us
/// also accept a `children` field, which the parent type doesn't
/// carry.
#[derive(Debug, Default, Deserialize)]
struct ContainerSpec {
    #[serde(default)]
    id: String,
    #[serde(default, flatten)]
    props: ContainerProps,
    #[serde(default)]
    children: Vec<serde_json::Value>,
}

/// Install the `ui.*` table on the given Lua state. Idempotent —
/// safe to call multiple times; later calls overwrite the previous
/// table.
///
/// Exposes:
/// - `ui.container(spec)` → `LuaNode` (container)
/// - `ui.text(content[, props])` → `LuaNode` (text)
/// - `ui.spacer(spec)` → `LuaNode` (spacer)
/// - `ui.surface(node, viewport)` → `LuaSurface`
pub fn install(lua: &Lua) -> LuaResult<()> {
    let ui = lua.create_table()?;

    let container = lua.create_function(|lua, spec: Value| {
        let parsed: ContainerSpec = lua.from_value(spec)?;
        let mut children = Vec::with_capacity(parsed.children.len());
        for child in parsed.children {
            let node: Node = serde_json::from_value(child).map_err(mlua::Error::external)?;
            children.push(node);
        }
        Ok(LuaNode::new(Node::Container {
            id: parsed.id,
            props: parsed.props,
            children,
        }))
    })?;
    ui.set("container", container)?;

    let text = lua.create_function(|lua, (content, props): (String, Option<Value>)| {
        let props: TextProps = match props {
            Some(v) => lua.from_value(v).unwrap_or_default(),
            None => TextProps::default(),
        };
        Ok(LuaNode::new(Node::Text {
            id: String::new(),
            content,
            props,
        }))
    })?;
    ui.set("text", text)?;

    let spacer = lua.create_function(|lua, spec: Value| {
        #[derive(Default, Deserialize)]
        struct SpacerSpec {
            #[serde(default)]
            id: String,
            #[serde(default)]
            width: f32,
            #[serde(default)]
            height: f32,
        }
        let s: SpacerSpec = lua.from_value(spec)?;
        Ok(LuaNode::new(Node::Spacer {
            id: s.id,
            width: s.width,
            height: s.height,
        }))
    })?;
    ui.set("spacer", spacer)?;

    // `ui.image({ id, source, width, height, radius })` — same value
    // shape `Node::Image` carries on the Rust side. Every field
    // optional; the source string is what the host renderer resolves.
    let image = lua.create_function(|lua, spec: Value| {
        #[derive(Default, Deserialize)]
        struct ImageSpec {
            #[serde(default)]
            id: String,
            #[serde(default)]
            source: String,
            #[serde(default)]
            width: Sizing,
            #[serde(default)]
            height: Sizing,
            #[serde(default)]
            radius: CornerRadius,
        }
        let s: ImageSpec = lua.from_value(spec)?;
        Ok(LuaNode::new(Node::Image {
            id: s.id,
            source: s.source,
            width: s.width,
            height: s.height,
            radius: s.radius,
        }))
    })?;
    ui.set("image", image)?;

    let surface_ctor = lua.create_function(|lua, (node, vp): (AnyUserData, Value)| {
        let tree = node.borrow::<LuaNode>()?.snapshot();
        let viewport: Viewport = lua.from_value(vp)?;
        Ok(LuaSurface(Rc::new(RefCell::new(Surface::new(
            tree, viewport,
        )))))
    })?;
    ui.set("surface", surface_ctor)?;

    lua.globals().set("ui", ui)?;
    Ok(())
}

/// Convenience: take any `LuaNode` and pull the underlying `Node`
/// out without going through Lua. Useful for Rust callers that
/// receive a Luau-authored tree and want to plug it into a
/// `Surface` directly.
pub fn into_node(handle: LuaNode) -> Node {
    handle.into_inner()
}

// `LuaNode` and `LuaSurface` need `FromLua` so test (and downstream)
// code can `eval::<LuaNode>(...)` against a Lua chunk that returns
// one. The blanket `FromLua for AnyUserData` in mlua doesn't apply to
// concrete `UserData` types — we delegate by extracting the user-data
// and borrowing its inner value.
impl mlua::FromLua for LuaNode {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => Ok(ud.borrow::<LuaNode>()?.clone()),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "LuaNode".to_string(),
                message: Some("expected ui.Node user-data".into()),
            }),
        }
    }
}

impl mlua::FromLua for LuaSurface {
    fn from_lua(value: Value, _lua: &Lua) -> LuaResult<Self> {
        match value {
            Value::UserData(ud) => Ok(ud.borrow::<LuaSurface>()?.clone()),
            other => Err(mlua::Error::FromLuaConversionError {
                from: other.type_name(),
                to: "LuaSurface".to_string(),
                message: Some("expected ui.Surface user-data".into()),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;

    fn lua() -> Lua {
        let lua = Lua::new();
        install(&lua).expect("install ui table");
        lua
    }

    #[test]
    fn ui_container_constructs_a_node() {
        let lua = lua();
        let node: LuaNode = lua
            .load(
                r#"
                return ui.container({
                  id = "root",
                  direction = "row",
                  gap = 8,
                  children = {},
                })
                "#,
            )
            .eval()
            .unwrap();
        let snapshot = node.snapshot();
        match &snapshot {
            Node::Container { id, props, .. } => {
                assert_eq!(id, "root");
                assert_eq!(props.gap, 8.0);
            }
            other => panic!("expected container, got {other:?}"),
        }
    }

    #[test]
    fn ui_text_round_trips_content_and_size() {
        let lua = lua();
        let node: LuaNode = lua
            .load(r#"return ui.text("hi", { font_size = 24, color = { r = 0, g = 0, b = 0, a = 255 } })"#)
            .eval()
            .unwrap();
        let snapshot = node.snapshot();
        match &snapshot {
            Node::Text { content, props, .. } => {
                assert_eq!(content, "hi");
                assert_eq!(props.font_size, 24.0);
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn surface_drives_layout_from_luau() {
        let lua = lua();
        let surface: LuaSurface = lua
            .load(
                r#"
                local root = ui.container({
                  id = "root",
                  direction = "column",
                  gap = 0,
                  children = {
                    { kind = "text", id = "t", content = "x", props = { font_size = 14, color = { r = 0, g = 0, b = 0, a = 255 } } },
                  },
                })
                return ui.surface(root, { width = 100, height = 100 })
                "#,
            )
            .eval()
            .unwrap();
        assert!(surface.0.borrow().is_dirty());
        let _ = surface.0.borrow_mut().commands();
        assert!(!surface.0.borrow().is_dirty());
    }

    #[test]
    fn surface_edit_mutates_a_text_node_by_id() {
        let lua = lua();
        let updated: bool = lua
            .load(
                r#"
                local root = ui.container({
                  id = "root",
                  children = {
                    { kind = "text", id = "title", content = "old", props = { font_size = 14, color = { r = 0, g = 0, b = 0, a = 255 } } },
                  },
                })
                local s = ui.surface(root, { width = 100, height = 100 })
                return s:edit("title", function(n)
                  n.content = "new"
                  return n
                end)
                "#,
            )
            .eval()
            .unwrap();
        assert!(updated);
    }

    #[test]
    fn ui_image_constructs_an_image_node() {
        let lua = lua();
        let node: LuaNode = lua
            .load(
                r#"
                return ui.image({
                  id = "hero",
                  source = "/asset/abc",
                  width = { mode = "grow" },
                  height = { mode = "grow" },
                  radius = { tl = 4, tr = 4, br = 4, bl = 4 },
                })
                "#,
            )
            .eval()
            .unwrap();
        let snapshot = node.snapshot();
        assert_eq!(snapshot.kind(), "image");
        match snapshot {
            Node::Image { id, source, .. } => {
                assert_eq!(id, "hero");
                assert_eq!(source, "/asset/abc");
            }
            other => panic!("expected image, got {other:?}"),
        }
    }

    #[test]
    fn json_round_trip_for_node() {
        let lua = lua();
        let json: serde_json::Value = lua
            .load(
                r#"
                local n = ui.container({ id = "x", direction = "row", children = {} })
                return n:to_json()
                "#,
            )
            .eval::<Value>()
            .and_then(|v| lua.from_value(v))
            .unwrap();
        assert_eq!(json["id"], "x");
        assert_eq!(json["kind"], "container");
    }
}
