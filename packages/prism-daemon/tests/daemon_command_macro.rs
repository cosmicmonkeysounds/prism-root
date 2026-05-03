//! End-to-end coverage for `#[daemon_command]`.
//!
//! Lives in the daemon's tests dir (rather than in `prism-luau-derive`)
//! because the macro expands to paths in `prism_daemon::*` and exercising
//! it requires linking the real `CommandRegistry`.

use prism_daemon::registry::CommandRegistry;
use prism_luau_derive::daemon_command;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::sync::Arc;

#[derive(Deserialize)]
struct Greet {
    name: String,
}

#[derive(Serialize)]
struct GreetResp {
    msg: String,
}

#[daemon_command(id = "macro.greet")]
fn greet(req: Greet) -> Result<GreetResp, std::convert::Infallible> {
    Ok(GreetResp {
        msg: format!("hi {}", req.name),
    })
}

struct Counter(std::sync::Mutex<u64>);

#[daemon_command(id = "macro.bump", permission = User)]
fn bump(state: &Counter, _: Greet) -> Result<u64, String> {
    let mut g = state.0.lock().map_err(|_| "poisoned")?;
    *g += 1;
    Ok(*g)
}

#[test]
fn stateless_command_round_trips() {
    let reg = CommandRegistry::default();
    register_greet(&reg).unwrap();
    let out = reg
        .invoke("macro.greet", json!({ "name": "world" }))
        .unwrap();
    assert_eq!(out, json!({ "msg": "hi world" }));
}

#[test]
fn stateful_command_threads_arc_state() {
    let reg = CommandRegistry::default();
    let state = Arc::new(Counter(std::sync::Mutex::new(0)));
    register_bump(&reg, state.clone()).unwrap();
    let a = reg.invoke("macro.bump", json!({ "name": "x" })).unwrap();
    let b = reg.invoke("macro.bump", json!({ "name": "x" })).unwrap();
    assert_eq!(a, json!(1));
    assert_eq!(b, json!(2));
    assert_eq!(*state.0.lock().unwrap(), 2);
}

#[test]
fn permission_attribute_uses_user_tier() {
    use prism_daemon::permission::Permission;
    let reg = CommandRegistry::default();
    let state = Arc::new(Counter(std::sync::Mutex::new(0)));
    register_bump(&reg, state).unwrap();
    // User tier can invoke.
    assert!(reg
        .invoke_with_permission("macro.bump", json!({ "name": "x" }), Permission::User)
        .is_ok());
}
