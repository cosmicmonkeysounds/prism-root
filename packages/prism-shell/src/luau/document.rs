//! `prism.document` userdata. Read access is served from a snapshot
//! of the active page's [`BuilderDocument`]; mutations are queued for
//! the shell to apply through the same `live`-source-edit / undo path
//! the legacy `_actions` protocol uses.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{LuaSerdeExt, UserData, UserDataMethods, Value};
use prism_builder::BuilderDocument;
use serde_json::Value as JsonValue;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DocumentMode {
    /// Mutations are queued and applied by the shell after exec.
    ReadWrite,
    /// Any mutating method raises a Luau error. Used by facet
    /// resolvers — a sync pass that mutates mid-resolution would loop.
    ReadOnly,
}

#[derive(Clone, Debug)]
pub enum DocumentMutation {
    SetProp {
        node_id: String,
        key: String,
        value: JsonValue,
    },
    Insert {
        parent_id: Option<String>,
        descriptor: JsonValue,
    },
    Remove {
        node_id: String,
    },
    Move {
        node_id: String,
        new_parent: String,
        index: usize,
    },
}

#[derive(Clone)]
pub struct DocumentHandle {
    snapshot: Rc<BuilderDocument>,
    mode: DocumentMode,
    queue: Rc<RefCell<Vec<DocumentMutation>>>,
}

impl DocumentHandle {
    pub fn new(
        snapshot: Rc<BuilderDocument>,
        mode: DocumentMode,
        queue: Rc<RefCell<Vec<DocumentMutation>>>,
    ) -> Self {
        Self {
            snapshot,
            mode,
            queue,
        }
    }

    fn require_writable(&self) -> mlua::Result<()> {
        match self.mode {
            DocumentMode::ReadWrite => Ok(()),
            DocumentMode::ReadOnly => Err(mlua::Error::external(
                "prism.document: mutations are not permitted in this context (read-only)",
            )),
        }
    }
}

impl UserData for DocumentHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method("find", |lua, this, id: String| {
            let node = this.snapshot.root.as_ref().and_then(|r| r.find(&id));
            match node {
                Some(n) => lua.to_value(n),
                None => Ok(Value::Nil),
            }
        });

        methods.add_method("prop", |lua, this, (id, key): (String, String)| {
            let value = this
                .snapshot
                .root
                .as_ref()
                .and_then(|r| r.find(&id))
                .and_then(|n| n.props.get(&key))
                .cloned()
                .unwrap_or(JsonValue::Null);
            lua.to_value(&value)
        });

        methods.add_method(
            "set_prop",
            |lua, this, (id, key, value): (String, String, Value)| {
                this.require_writable()?;
                let json_val: JsonValue = lua.from_value(value)?;
                this.queue.borrow_mut().push(DocumentMutation::SetProp {
                    node_id: id,
                    key,
                    value: json_val,
                });
                Ok(())
            },
        );

        methods.add_method(
            "insert",
            |lua, this, (parent, descriptor): (Option<String>, mlua::Table)| {
                this.require_writable()?;
                let json_val: JsonValue = lua.from_value(Value::Table(descriptor))?;
                this.queue.borrow_mut().push(DocumentMutation::Insert {
                    parent_id: parent,
                    descriptor: json_val,
                });
                Ok(())
            },
        );

        methods.add_method("remove", |_, this, id: String| {
            this.require_writable()?;
            this.queue
                .borrow_mut()
                .push(DocumentMutation::Remove { node_id: id });
            Ok(())
        });

        // `move` is a Lua keyword in many editors — we expose it as
        // `move_node` to keep call sites unambiguous and matching the
        // verb in the legacy `_actions` protocol.
        methods.add_method(
            "move_node",
            |_, this, (id, new_parent, index): (String, String, usize)| {
                this.require_writable()?;
                this.queue.borrow_mut().push(DocumentMutation::Move {
                    node_id: id,
                    new_parent,
                    index,
                });
                Ok(())
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mlua::Lua;
    use prism_builder::Node;

    fn doc_with_node(id: &str) -> BuilderDocument {
        BuilderDocument {
            root: Some(Node {
                id: id.to_string(),
                component: "container".into(),
                props: serde_json::json!({ "label": "Hello" }),
                children: vec![],
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    #[test]
    fn prop_returns_snapshot_value() {
        let queue = Rc::new(RefCell::new(Vec::new()));
        let handle =
            DocumentHandle::new(Rc::new(doc_with_node("n1")), DocumentMode::ReadWrite, queue);
        let lua = Lua::new();
        lua.globals().set("doc", handle).unwrap();
        let label: String = lua.load("return doc:prop('n1', 'label')").eval().unwrap();
        assert_eq!(label, "Hello");
    }

    #[test]
    fn set_prop_queues_mutation() {
        let queue: Rc<RefCell<Vec<DocumentMutation>>> = Rc::new(RefCell::new(Vec::new()));
        let handle = DocumentHandle::new(
            Rc::new(doc_with_node("n1")),
            DocumentMode::ReadWrite,
            queue.clone(),
        );
        let lua = Lua::new();
        lua.globals().set("doc", handle).unwrap();
        lua.load("doc:set_prop('n1', 'label', 'World')")
            .exec()
            .unwrap();
        let q = queue.borrow();
        assert_eq!(q.len(), 1);
        match &q[0] {
            DocumentMutation::SetProp {
                node_id,
                key,
                value,
            } => {
                assert_eq!(node_id, "n1");
                assert_eq!(key, "label");
                assert_eq!(value, &serde_json::json!("World"));
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn read_only_mode_blocks_mutations() {
        let queue = Rc::new(RefCell::new(Vec::new()));
        let handle =
            DocumentHandle::new(Rc::new(doc_with_node("n1")), DocumentMode::ReadOnly, queue);
        let lua = Lua::new();
        lua.globals().set("doc", handle).unwrap();
        let err = lua
            .load("doc:set_prop('n1', 'label', 'World')")
            .exec()
            .unwrap_err();
        assert!(err.to_string().contains("read-only"));
    }
}
