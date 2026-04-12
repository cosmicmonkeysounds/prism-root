//! Actors module — sandboxed long-running Luau executors with mailbox
//! semantics. Exposed behind six `actors.*` commands.
//!
//! | Command          | Payload                                 | Result                               |
//! |------------------|-----------------------------------------|--------------------------------------|
//! | `actors.spawn`   | `{ kind, script?, name?, config? }`     | `{ id }`                             |
//! | `actors.send`    | `{ id, message }`                       | `{ delivered: bool, inbox_depth }`   |
//! | `actors.recv`    | `{ id, max?: usize }`                   | `{ messages: [...] }`                |
//! | `actors.status`  | `{ id }`                                | `{ alive, inbox, outbox, kind, name }` |
//! | `actors.list`    | `{}`                                    | `{ actors: [{id,kind,name,alive}] }` |
//! | `actors.stop`    | `{ id }`                                | `{ stopped: bool }`                  |
//!
//! ## Sandboxing model
//!
//! Each actor runs on its own OS thread with its own [`mlua::Lua`]
//! instance. That instance is the sandbox: the script gets loaded exactly
//! once at spawn-time, and the preloaded environment (any globals the
//! script sets, any functions it defines) is reused across every incoming
//! message. The script may define a top-level function:
//!
//! ```lua
//! function on_message(msg)
//!   -- msg is whatever JSON value `actors.send` was given
//!   return { kind = "ack", received = msg }
//! end
//! ```
//!
//! If `on_message` returns a non-nil value the actor enqueues it on its
//! outbox, from which `actors.recv` drains.
//!
//! Actors are the daemon's substrate for bridging to expensive or
//! long-lived sidecars — Whisper, Python interpreters, local LLM servers.
//! Each of those eventually becomes an `ActorKind` variant that wraps the
//! same mailbox API around a different execution engine. Today we ship
//! Luau; the other kinds return a clear "unsupported" error so the call
//! shape is stable from day one and hosts can feature-detect by payload
//! kind rather than by command existence.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use mlua::{Function, Lua, MultiValue, Value as LuaValue};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// Kind of sandbox an actor runs inside.
///
/// The wire format is the lowercase snake_case identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    /// Luau script sandbox. Fully supported today.
    Luau,
    /// Placeholder for a Python sidecar. Reserved for future work.
    Python,
    /// Placeholder for a local LLM/whisper sidecar. Reserved for future work.
    LlmSidecar,
}

impl ActorKind {
    fn as_str(self) -> &'static str {
        match self {
            ActorKind::Luau => "luau",
            ActorKind::Python => "python",
            ActorKind::LlmSidecar => "llm_sidecar",
        }
    }
}

/// The message envelope handed to an actor's on_message handler.
///
/// Kept deliberately small — arbitrary JSON plus a correlation id the
/// sender can assign for request/response patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorMessage {
    #[serde(default)]
    pub id: Option<String>,
    pub body: JsonValue,
}

/// The actors manager — owns every running actor by numeric id and serves
/// as the shared state for the `actors.*` command handlers.
pub struct ActorsManager {
    next_id: AtomicU64,
    state: Mutex<HashMap<u64, ActorHandle>>,
}

/// One actor's public-facing state and control channel.
struct ActorHandle {
    kind: ActorKind,
    name: String,
    alive: Arc<AtomicBool>,
    tx: mpsc::Sender<InboxMsg>,
    outbox: Arc<Mailbox>,
    thread: Option<JoinHandle<()>>,
}

/// Internal inbox message (a wrapped `ActorMessage` plus a terminate
/// signal). Keeping the stop signal inside the same channel means the
/// actor loop has exactly one place to block.
enum InboxMsg {
    Deliver(ActorMessage),
    Stop,
}

/// Blocking mailbox used for both inbox depth reporting and the outbox.
#[derive(Default)]
struct Mailbox {
    queue: Mutex<VecDeque<JsonValue>>,
    cv: Condvar,
}

impl Mailbox {
    fn push(&self, value: JsonValue) {
        let mut q = self.queue.lock().expect("mailbox mutex poisoned");
        q.push_back(value);
        self.cv.notify_one();
    }

    fn drain(&self, max: usize) -> Vec<JsonValue> {
        let mut q = self.queue.lock().expect("mailbox mutex poisoned");
        let n = q.len().min(max);
        q.drain(..n).collect()
    }

    fn len(&self) -> usize {
        self.queue.lock().map(|q| q.len()).unwrap_or(0)
    }
}

impl Default for ActorsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ActorsManager {
    /// Fresh, empty manager. No actors until `spawn` is called.
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            state: Mutex::new(HashMap::new()),
        }
    }

    /// Spawn a Luau actor backed by `script`. Returns the actor id.
    ///
    /// The script is loaded once on the actor thread. Any `on_message`
    /// function it defines is invoked for every message delivered via
    /// [`ActorsManager::send`]; any non-nil return value is enqueued on
    /// the actor's outbox.
    pub fn spawn_luau(&self, script: String, name: Option<String>) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = mpsc::channel::<InboxMsg>();
        let alive = Arc::new(AtomicBool::new(true));
        let outbox = Arc::new(Mailbox::default());

        let alive_t = alive.clone();
        let outbox_t = outbox.clone();
        let thread_name = format!("prism-actor-{id}");
        let join = thread::Builder::new()
            .name(thread_name.clone())
            .spawn(move || run_luau_actor(script, rx, alive_t, outbox_t))
            .map_err(|e| format!("failed to spawn actor thread: {e}"))?;

        let handle = ActorHandle {
            kind: ActorKind::Luau,
            name: name.unwrap_or_else(|| thread_name.clone()),
            alive,
            tx,
            outbox,
            thread: Some(join),
        };
        self.state
            .lock()
            .map_err(|_| "actors state poisoned".to_string())?
            .insert(id, handle);
        Ok(id)
    }

    /// Deliver a message to an actor. Returns the resulting inbox depth
    /// snapshot (best-effort — the actor thread may have popped it
    /// before the number is read, which is fine).
    ///
    /// Errors if the actor is unknown or has been stopped.
    pub fn send(&self, id: u64, message: ActorMessage) -> Result<usize, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "actors state poisoned".to_string())?;
        let handle = state
            .get(&id)
            .ok_or_else(|| format!("unknown actor: {id}"))?;
        if !handle.alive.load(Ordering::SeqCst) {
            return Err(format!("actor {id} is not alive"));
        }
        handle
            .tx
            .send(InboxMsg::Deliver(message))
            .map_err(|e| format!("actor {id} inbox closed: {e}"))?;
        // The channel does not expose a length. Report the outbox depth
        // as a proxy for "is the actor keeping up" — callers that want
        // precise accounting can call `actors.status`.
        Ok(handle.outbox.len())
    }

    /// Drain up to `max` pending outbox messages for `id`.
    pub fn recv(&self, id: u64, max: usize) -> Result<Vec<JsonValue>, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "actors state poisoned".to_string())?;
        let handle = state
            .get(&id)
            .ok_or_else(|| format!("unknown actor: {id}"))?;
        Ok(handle.outbox.drain(max))
    }

    /// Block waiting for at least one outbox message, up to `timeout`.
    /// Convenience helper for tests and for hosts that want to poll.
    pub fn wait_recv(
        &self,
        id: u64,
        max: usize,
        timeout: Duration,
    ) -> Result<Vec<JsonValue>, String> {
        let outbox = {
            let state = self
                .state
                .lock()
                .map_err(|_| "actors state poisoned".to_string())?;
            state
                .get(&id)
                .ok_or_else(|| format!("unknown actor: {id}"))?
                .outbox
                .clone()
        };
        let mut q = outbox
            .queue
            .lock()
            .map_err(|_| "mailbox mutex poisoned".to_string())?;
        if q.is_empty() {
            let (guard, _) = outbox
                .cv
                .wait_timeout(q, timeout)
                .map_err(|_| "mailbox mutex poisoned".to_string())?;
            q = guard;
        }
        let n = q.len().min(max);
        Ok(q.drain(..n).collect())
    }

    /// Snapshot of one actor's state.
    pub fn status(&self, id: u64) -> Result<ActorStatus, String> {
        let state = self
            .state
            .lock()
            .map_err(|_| "actors state poisoned".to_string())?;
        let handle = state
            .get(&id)
            .ok_or_else(|| format!("unknown actor: {id}"))?;
        Ok(ActorStatus {
            id,
            kind: handle.kind.as_str().to_string(),
            name: handle.name.clone(),
            alive: handle.alive.load(Ordering::SeqCst),
            outbox: handle.outbox.len() as u64,
        })
    }

    /// List every actor the manager knows about. Stopped actors remain
    /// visible until [`ActorsManager::stop`] is called explicitly, at
    /// which point they are removed.
    pub fn list(&self) -> Vec<ActorStatus> {
        let state = match self.state.lock() {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let mut out: Vec<ActorStatus> = state
            .iter()
            .map(|(id, h)| ActorStatus {
                id: *id,
                kind: h.kind.as_str().to_string(),
                name: h.name.clone(),
                alive: h.alive.load(Ordering::SeqCst),
                outbox: h.outbox.len() as u64,
            })
            .collect();
        out.sort_by_key(|s| s.id);
        out
    }

    /// Signal an actor to stop, then join its thread. Idempotent —
    /// re-stopping a gone actor is a no-op that returns `false`.
    pub fn stop(&self, id: u64) -> Result<bool, String> {
        let mut handle = {
            let mut state = self
                .state
                .lock()
                .map_err(|_| "actors state poisoned".to_string())?;
            match state.remove(&id) {
                Some(h) => h,
                None => return Ok(false),
            }
        };
        // Best-effort: if the channel is already closed the actor is gone.
        let _ = handle.tx.send(InboxMsg::Stop);
        if let Some(join) = handle.thread.take() {
            // Don't hold any locks across the join. A misbehaving
            // actor that refuses to exit should not deadlock stop().
            let _ = join.join();
        }
        handle.alive.store(false, Ordering::SeqCst);
        Ok(true)
    }

    /// Stop every known actor. Used by `dispose()` and by tests.
    pub fn stop_all(&self) {
        let ids: Vec<u64> = self
            .state
            .lock()
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.stop(id);
        }
    }
}

impl Drop for ActorsManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

/// Public snapshot returned by `actors.status` and `actors.list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorStatus {
    pub id: u64,
    pub kind: String,
    pub name: String,
    pub alive: bool,
    pub outbox: u64,
}

/// Body of one actor's main loop. Exits when the inbox closes or a
/// `Stop` message arrives. Errors raised by the script surface as
/// error-kind outbox messages so the host can observe them without
/// killing the whole kernel.
///
/// The actor owns a long-lived [`mlua::Lua`] state pinned to its own
/// OS thread — the script is executed exactly once at spawn time, so
/// top-level code runs (and can define `on_message`) without leaking
/// into subsequent messages. On each incoming message we pull the
/// global `on_message` function out of the state and call it; if the
/// script never defined one, deliveries silently no-op.
fn run_luau_actor(
    script: String,
    rx: mpsc::Receiver<InboxMsg>,
    alive: Arc<AtomicBool>,
    outbox: Arc<Mailbox>,
) {
    let lua = Lua::new();

    // Run the script once. Any error here prevents us from accepting
    // messages — but the actor stays registered so `actors.status` can
    // still observe that it died, matching the "loud failure" model
    // the Tauri shell expects.
    if let Err(err) = lua.load(script).exec() {
        outbox.push(json!({
            "kind": "error",
            "error": format!("actor script failed at load: {err}"),
        }));
        alive.store(false, Ordering::SeqCst);
        // Drain the inbox so senders don't block on a dead channel
        // indefinitely — they'll see InboxMsg::Stop and move on.
        for _ in rx.iter() {}
        return;
    }

    while let Ok(msg) = rx.recv() {
        match msg {
            InboxMsg::Stop => break,
            InboxMsg::Deliver(message) => {
                let on_message: Option<Function> = match lua.globals().get("on_message") {
                    Ok(LuaValue::Function(f)) => Some(f),
                    _ => None,
                };
                let Some(on_message) = on_message else {
                    // No handler defined — silently drop the message.
                    continue;
                };
                let lua_msg = match json_to_lua(&lua, &serde_json::to_value(&message).unwrap()) {
                    Ok(v) => v,
                    Err(e) => {
                        outbox.push(json!({ "kind": "error", "error": e.to_string() }));
                        continue;
                    }
                };
                match on_message.call::<MultiValue>(lua_msg) {
                    Ok(results) => {
                        let first = results.into_iter().next().unwrap_or(LuaValue::Nil);
                        if let LuaValue::Nil = first {
                            // Handler returned nothing — don't enqueue.
                            continue;
                        }
                        match lua_to_json(&first) {
                            Ok(v) => outbox.push(v),
                            Err(e) => outbox.push(json!({
                                "kind": "error",
                                "error": e.to_string(),
                            })),
                        }
                    }
                    Err(err) => outbox.push(json!({
                        "kind": "error",
                        "error": err.to_string(),
                    })),
                }
            }
        }
    }
    alive.store(false, Ordering::SeqCst);
}

// JSON ↔ Lua conversion for the long-lived per-actor state. These
// mirror the helpers inside `luau_module` but can't be reused directly
// because they're private to that module.

fn json_to_lua(lua: &Lua, value: &JsonValue) -> mlua::Result<LuaValue> {
    match value {
        JsonValue::Null => Ok(LuaValue::Nil),
        JsonValue::Bool(b) => Ok(LuaValue::Boolean(*b)),
        JsonValue::Number(n) => Ok(LuaValue::Number(n.as_f64().unwrap_or(0.0))),
        JsonValue::String(s) => Ok(LuaValue::String(lua.create_string(s)?)),
        JsonValue::Array(arr) => {
            let t = lua.create_table()?;
            for (i, v) in arr.iter().enumerate() {
                t.set(i + 1, json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
        JsonValue::Object(obj) => {
            let t = lua.create_table()?;
            for (k, v) in obj {
                t.set(k.as_str(), json_to_lua(lua, v)?)?;
            }
            Ok(LuaValue::Table(t))
        }
    }
}

fn lua_to_json(value: &LuaValue) -> mlua::Result<JsonValue> {
    match value {
        LuaValue::Nil => Ok(JsonValue::Null),
        LuaValue::Boolean(b) => Ok(JsonValue::Bool(*b)),
        LuaValue::Integer(i) => Ok(JsonValue::Number((*i).into())),
        LuaValue::Number(f) => Ok(serde_json::Number::from_f64(*f)
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null)),
        LuaValue::String(s) => Ok(JsonValue::String(s.to_str()?.to_string())),
        LuaValue::Table(t) => {
            let len = t.raw_len();
            if len > 0 {
                let mut arr = Vec::with_capacity(len);
                for i in 1..=len {
                    let v: LuaValue = t.raw_get(i)?;
                    arr.push(lua_to_json(&v)?);
                }
                Ok(JsonValue::Array(arr))
            } else {
                let mut map = JsonMap::new();
                for pair in t.clone().pairs::<String, LuaValue>() {
                    let (k, v) = pair?;
                    map.insert(k, lua_to_json(&v)?);
                }
                Ok(JsonValue::Object(map))
            }
        }
        _ => Ok(JsonValue::Null),
    }
}

// ── Module wiring ──────────────────────────────────────────────────────

/// The built-in actors module. Stateless — the state lives on the shared
/// [`ActorsManager`] stashed on the builder.
pub struct ActorsModule;

impl DaemonModule for ActorsModule {
    fn id(&self) -> &str {
        "prism.actors"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = builder
            .actors_manager_slot()
            .get_or_insert_with(|| Arc::new(ActorsManager::new()))
            .clone();
        let registry = builder.registry().clone();

        let m = manager.clone();
        registry.register("actors.spawn", move |payload| {
            let args: SpawnArgs = parse(payload, "actors.spawn")?;
            match args.kind {
                ActorKind::Luau => {
                    let script = args.script.ok_or_else(|| {
                        CommandError::handler(
                            "actors.spawn",
                            "luau actors require a `script` field",
                        )
                    })?;
                    let id = m
                        .spawn_luau(script, args.name)
                        .map_err(|e| CommandError::handler("actors.spawn", e))?;
                    Ok(json!({ "id": id }))
                }
                ActorKind::Python | ActorKind::LlmSidecar => Err(CommandError::handler(
                    "actors.spawn",
                    format!(
                        "actor kind {:?} is not yet supported by this build",
                        args.kind.as_str()
                    ),
                )),
            }
        })?;

        let m = manager.clone();
        registry.register("actors.send", move |payload| {
            let args: SendArgs = parse(payload, "actors.send")?;
            let depth = m
                .send(
                    args.id,
                    ActorMessage {
                        id: args.correlation_id,
                        body: args.message,
                    },
                )
                .map_err(|e| CommandError::handler("actors.send", e))?;
            Ok(json!({ "delivered": true, "outbox_depth": depth }))
        })?;

        let m = manager.clone();
        registry.register("actors.recv", move |payload| {
            let args: RecvArgs = parse(payload, "actors.recv")?;
            let max = args.max.unwrap_or(64);
            let messages = m
                .recv(args.id, max)
                .map_err(|e| CommandError::handler("actors.recv", e))?;
            Ok(json!({ "messages": messages }))
        })?;

        let m = manager.clone();
        registry.register("actors.status", move |payload| {
            let args: IdArgs = parse(payload, "actors.status")?;
            let status = m
                .status(args.id)
                .map_err(|e| CommandError::handler("actors.status", e))?;
            serde_json::to_value(status)
                .map_err(|e| CommandError::handler("actors.status", e.to_string()))
        })?;

        let m = manager.clone();
        registry.register("actors.list", move |_payload| {
            Ok(json!({ "actors": m.list() }))
        })?;

        let m = manager;
        registry.register("actors.stop", move |payload| {
            let args: IdArgs = parse(payload, "actors.stop")?;
            let stopped = m
                .stop(args.id)
                .map_err(|e| CommandError::handler("actors.stop", e))?;
            Ok(json!({ "stopped": stopped }))
        })?;

        Ok(())
    }
}

fn parse<T: for<'de> Deserialize<'de>>(
    payload: JsonValue,
    command: &str,
) -> Result<T, CommandError> {
    serde_json::from_value::<T>(payload)
        .map_err(|e| CommandError::handler(command.to_string(), e.to_string()))
}

#[derive(Debug, Deserialize)]
struct SpawnArgs {
    kind: ActorKind,
    #[serde(default)]
    script: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct SendArgs {
    id: u64,
    message: JsonValue,
    #[serde(rename = "correlationId", default)]
    correlation_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RecvArgs {
    id: u64,
    #[serde(default)]
    max: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct IdArgs {
    id: u64,
}

// ── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

    fn kernel() -> crate::DaemonKernel {
        DaemonBuilder::new()
            .with_luau()
            .with_actors()
            .build()
            .unwrap()
    }

    #[test]
    fn actors_module_registers_six_commands() {
        let kernel = kernel();
        let caps = kernel.capabilities();
        for name in [
            "actors.spawn",
            "actors.send",
            "actors.recv",
            "actors.status",
            "actors.list",
            "actors.stop",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn spawn_send_recv_roundtrip_via_kernel_invoke() {
        let kernel = kernel();
        let spawn = kernel
            .invoke(
                "actors.spawn",
                json!({
                    "kind": "luau",
                    "script": "function on_message(msg) return { echoed = msg.body } end",
                    "name": "echo",
                }),
            )
            .unwrap();
        let id = spawn["id"].as_u64().unwrap();

        kernel
            .invoke(
                "actors.send",
                json!({ "id": id, "message": { "greeting": "hi" } }),
            )
            .unwrap();

        // The actor runs on a background thread, so poll recv briefly.
        let mut messages: Vec<JsonValue> = Vec::new();
        for _ in 0..50 {
            let out = kernel.invoke("actors.recv", json!({ "id": id })).unwrap();
            messages = serde_json::from_value(out["messages"].clone()).unwrap();
            if !messages.is_empty() {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0]["echoed"]["greeting"], "hi");

        kernel.invoke("actors.stop", json!({ "id": id })).unwrap();
    }

    #[test]
    fn actor_without_on_message_returns_nothing() {
        let mgr = ActorsManager::new();
        let id = mgr.spawn_luau("return 'noop'".to_string(), None).unwrap();
        mgr.send(
            id,
            ActorMessage {
                id: None,
                body: json!(42),
            },
        )
        .unwrap();
        // No on_message defined → outbox stays empty even after a beat.
        thread::sleep(Duration::from_millis(100));
        assert!(mgr.recv(id, 16).unwrap().is_empty());
        mgr.stop(id).unwrap();
    }

    #[test]
    fn script_errors_surface_as_error_kind_outbox_messages() {
        let mgr = ActorsManager::new();
        let id = mgr
            .spawn_luau(
                "function on_message(msg) error('boom!') end".to_string(),
                None,
            )
            .unwrap();
        mgr.send(
            id,
            ActorMessage {
                id: None,
                body: json!({}),
            },
        )
        .unwrap();
        let msgs = mgr.wait_recv(id, 8, Duration::from_secs(1)).unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["kind"], "error");
        assert!(msgs[0]["error"].as_str().unwrap().contains("boom"));
        mgr.stop(id).unwrap();
    }

    #[test]
    fn list_reports_every_spawned_actor() {
        let mgr = ActorsManager::new();
        let a = mgr
            .spawn_luau("return nil".to_string(), Some("a".into()))
            .unwrap();
        let b = mgr
            .spawn_luau("return nil".to_string(), Some("b".into()))
            .unwrap();
        let list = mgr.list();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a);
        assert_eq!(list[0].name, "a");
        assert_eq!(list[1].id, b);
        assert_eq!(list[1].name, "b");
        mgr.stop_all();
        assert!(mgr.list().is_empty());
    }

    #[test]
    fn unsupported_kind_returns_clear_error() {
        let kernel = kernel();
        let err = kernel
            .invoke(
                "actors.spawn",
                json!({ "kind": "python", "script": "print('hi')" }),
            )
            .unwrap_err();
        if let CommandError::Handler { command, message } = err {
            assert_eq!(command, "actors.spawn");
            assert!(message.contains("python"));
            assert!(message.contains("not yet supported"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn send_to_missing_actor_errors_out() {
        let kernel = kernel();
        let err = kernel
            .invoke("actors.send", json!({ "id": 999, "message": {} }))
            .unwrap_err();
        if let CommandError::Handler { message, .. } = err {
            assert!(message.contains("unknown actor"));
        } else {
            panic!("wrong variant");
        }
    }

    #[test]
    fn stop_returns_false_for_unknown_id() {
        let mgr = ActorsManager::new();
        assert!(!mgr.stop(42).unwrap());
    }

    #[test]
    fn status_reports_alive_and_outbox_depth() {
        let mgr = ActorsManager::new();
        let id = mgr
            .spawn_luau(
                "function on_message(msg) return { n = msg.body } end".to_string(),
                Some("status-test".into()),
            )
            .unwrap();

        let status = mgr.status(id).unwrap();
        assert!(status.alive);
        assert_eq!(status.name, "status-test");
        assert_eq!(status.kind, "luau");
        assert_eq!(status.outbox, 0);

        for i in 0..3 {
            mgr.send(
                id,
                ActorMessage {
                    id: None,
                    body: json!(i),
                },
            )
            .unwrap();
        }
        // Wait for the worker to catch up, then check the depth.
        for _ in 0..50 {
            if mgr.status(id).unwrap().outbox == 3 {
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(mgr.status(id).unwrap().outbox, 3);
        mgr.stop(id).unwrap();
    }

    #[test]
    fn preserved_injection_point_for_custom_manager() {
        // Hosts can plug in their own manager before the module
        // installs, so a single shared manager can power multiple
        // transport adapters against the same actor pool.
        let injected = Arc::new(ActorsManager::new());
        let mut builder = DaemonBuilder::new().with_luau();
        *builder.actors_manager_slot() = Some(injected.clone());
        let kernel = builder.with_actors().build().unwrap();
        assert!(Arc::ptr_eq(&kernel.actors_manager().unwrap(), &injected));
    }
}
