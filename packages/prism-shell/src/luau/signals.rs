//! `prism.signals` userdata. Scripts call `prism.signals:fire(node,
//! signal, payload?)`; the entries are queued for the shell to drain
//! after the script returns. From the `Custom`-handler entry point
//! the shell calls [`ShellInner::fire_signal`] for each entry once
//! the script's borrow has dropped (avoiding mid-script re-entrancy);
//! from the facet-resolver entry the entries are buffered for the
//! next sync pass so a resolver can't loop on its own dispatches.

use std::cell::RefCell;
use std::rc::Rc;

use mlua::{LuaSerdeExt, UserData, UserDataMethods, Value};
use serde_json::{Map as JsonMap, Value as JsonValue};

#[derive(Clone, Debug)]
pub struct SignalEntry {
    pub node_id: String,
    pub signal: String,
    pub payload: JsonMap<String, JsonValue>,
}

#[derive(Clone)]
pub struct SignalsHandle {
    queue: Rc<RefCell<Vec<SignalEntry>>>,
}

impl SignalsHandle {
    pub fn new(queue: Rc<RefCell<Vec<SignalEntry>>>) -> Self {
        Self { queue }
    }
}

impl UserData for SignalsHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        methods.add_method(
            "fire",
            |lua, this, (node_id, signal, payload): (String, String, Option<Value>)| {
                let payload = match payload {
                    None => JsonMap::new(),
                    Some(v) => {
                        let json: JsonValue = lua.from_value(v)?;
                        match json {
                            JsonValue::Object(map) => map,
                            JsonValue::Null => JsonMap::new(),
                            _ => {
                                return Err(mlua::Error::external(
                                    "prism.signals:fire payload must be a table",
                                ))
                            }
                        }
                    }
                };
                this.queue.borrow_mut().push(SignalEntry {
                    node_id,
                    signal,
                    payload,
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

    #[test]
    fn fire_queues_entry() {
        let queue: Rc<RefCell<Vec<SignalEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let handle = SignalsHandle::new(queue.clone());
        let lua = Lua::new();
        lua.globals().set("sigs", handle).unwrap();
        lua.load("sigs:fire('n1', 'clicked', { x = 5 })")
            .exec()
            .unwrap();
        let q = queue.borrow();
        assert_eq!(q.len(), 1);
        assert_eq!(q[0].node_id, "n1");
        assert_eq!(q[0].signal, "clicked");
        assert_eq!(q[0].payload.get("x"), Some(&serde_json::json!(5)));
    }

    #[test]
    fn fire_without_payload() {
        let queue: Rc<RefCell<Vec<SignalEntry>>> = Rc::new(RefCell::new(Vec::new()));
        let handle = SignalsHandle::new(queue.clone());
        let lua = Lua::new();
        lua.globals().set("sigs", handle).unwrap();
        lua.load("sigs:fire('n1', 'mounted')").exec().unwrap();
        assert_eq!(queue.borrow().len(), 1);
    }
}
