//! Luau debugger module — breakpoints, stepping, variable inspection.
//!
//! Each debug session is a thread-per-session Lua state with debug
//! hooks installed. When a breakpoint fires, the thread parks via a
//! condvar while the host inspects state through `luau.debug.*`
//! commands.
//!
//! | Command                  | Payload                                       | Result                                  |
//! |--------------------------|-----------------------------------------------|-----------------------------------------|
//! | `luau.debug.launch`      | `{ script, args?, stop_on_entry? }`           | `{ session_id }`                        |
//! | `luau.debug.set_breakpoints` | `{ session_id, breakpoints: [{line, condition?}] }` | `{ confirmed: [...] }` |
//! | `luau.debug.continue`    | `{ session_id }`                              | `{ stopped_at?, reason? }`              |
//! | `luau.debug.step_in`     | `{ session_id }`                              | `{ stopped_at?, reason? }`              |
//! | `luau.debug.step_over`   | `{ session_id }`                              | `{ stopped_at?, reason? }`              |
//! | `luau.debug.step_out`    | `{ session_id }`                              | `{ stopped_at?, reason? }`              |
//! | `luau.debug.inspect`     | `{ session_id }`                              | `{ locals, call_stack }`                |
//! | `luau.debug.evaluate`    | `{ session_id, expression }`                  | `{ result?, error? }`                   |
//! | `luau.debug.terminate`   | `{ session_id }`                              | `{ terminated: bool }`                  |

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use mlua::{Lua, MultiValue, Value as LuaValue, VmState};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map as JsonMap, Value as JsonValue};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

// ── Types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepMode {
    Continue,
    StepIn,
    StepOver,
    StepOut,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Breakpoint {
    pub line: usize,
    #[serde(default)]
    pub condition: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopInfo {
    pub line: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalVar {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackFrame {
    pub name: Option<String>,
    pub source: Option<String>,
    pub line: Option<usize>,
}

// ── Shared state between debug thread and host ────────────────────────

struct DebugShared {
    breakpoints: Mutex<HashSet<usize>>,
    step_mode: Mutex<StepMode>,
    paused: AtomicBool,
    stopped_at: Mutex<Option<StopInfo>>,
    locals: Mutex<Vec<LocalVar>>,
    call_stack: Mutex<Vec<StackFrame>>,
    resume_signal: Condvar,
    resume_mutex: Mutex<bool>,
    eval_request: Mutex<Option<String>>,
    eval_result: Mutex<Option<JsonValue>>,
    eval_signal: Condvar,
    eval_done: Condvar,
    step_depth: Mutex<i32>,
}

impl DebugShared {
    fn new() -> Self {
        Self {
            breakpoints: Mutex::new(HashSet::new()),
            step_mode: Mutex::new(StepMode::Continue),
            paused: AtomicBool::new(false),
            stopped_at: Mutex::new(None),
            locals: Mutex::new(Vec::new()),
            call_stack: Mutex::new(Vec::new()),
            resume_signal: Condvar::new(),
            resume_mutex: Mutex::new(false),
            eval_request: Mutex::new(None),
            eval_result: Mutex::new(None),
            eval_signal: Condvar::new(),
            eval_done: Condvar::new(),
            step_depth: Mutex::new(0),
        }
    }
}

// ── Session ───────────────────────────────────────────────────────────

struct DebugSession {
    shared: Arc<DebugShared>,
    alive: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

// ── Manager ───────────────────────────────────────────────────────────

pub struct DebugManager {
    next_id: AtomicU64,
    sessions: Mutex<HashMap<u64, DebugSession>>,
}

impl Default for DebugManager {
    fn default() -> Self {
        Self::new()
    }
}

impl DebugManager {
    pub fn new() -> Self {
        Self {
            next_id: AtomicU64::new(1),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn launch(
        &self,
        script: String,
        args: Option<JsonMap<String, JsonValue>>,
        stop_on_entry: bool,
    ) -> Result<u64, String> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let shared = Arc::new(DebugShared::new());
        let alive = Arc::new(AtomicBool::new(true));

        if stop_on_entry {
            *shared.step_mode.lock().unwrap() = StepMode::StepIn;
        }

        let shared_t = shared.clone();
        let alive_t = alive.clone();
        let join = thread::Builder::new()
            .name(format!("prism-debug-{id}"))
            .spawn(move || run_debug_session(script, args, shared_t, alive_t))
            .map_err(|e| format!("spawn failed: {e}"))?;

        let session = DebugSession {
            shared,
            alive,
            thread: Some(join),
        };
        self.sessions.lock().unwrap().insert(id, session);
        Ok(id)
    }

    pub fn set_breakpoints(
        &self,
        session_id: u64,
        breakpoints: Vec<Breakpoint>,
    ) -> Result<Vec<Breakpoint>, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("unknown session: {session_id}"))?;
        let mut bp_set = session.shared.breakpoints.lock().unwrap();
        bp_set.clear();
        let mut confirmed = Vec::new();
        for bp in breakpoints {
            bp_set.insert(bp.line);
            confirmed.push(bp);
        }
        Ok(confirmed)
    }

    pub fn resume(&self, session_id: u64, mode: StepMode) -> Result<Option<StopInfo>, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("unknown session: {session_id}"))?;

        {
            let mut sm = session.shared.step_mode.lock().unwrap();
            *sm = mode;
            if mode == StepMode::StepOut {
                let mut depth = session.shared.step_depth.lock().unwrap();
                *depth = 1;
            }
        }
        session.shared.paused.store(false, Ordering::SeqCst);

        {
            let mut guard = session.shared.resume_mutex.lock().unwrap();
            *guard = true;
            session.shared.resume_signal.notify_all();
        }

        for _ in 0..100 {
            thread::sleep(Duration::from_millis(20));
            if session.shared.paused.load(Ordering::SeqCst) || !session.alive.load(Ordering::SeqCst)
            {
                break;
            }
        }

        let stopped = session.shared.stopped_at.lock().unwrap().clone();
        Ok(stopped)
    }

    pub fn inspect(&self, session_id: u64) -> Result<(Vec<LocalVar>, Vec<StackFrame>), String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("unknown session: {session_id}"))?;
        let locals = session.shared.locals.lock().unwrap().clone();
        let stack = session.shared.call_stack.lock().unwrap().clone();
        Ok((locals, stack))
    }

    pub fn evaluate(&self, session_id: u64, expression: String) -> Result<JsonValue, String> {
        let sessions = self.sessions.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("unknown session: {session_id}"))?;

        if !session.shared.paused.load(Ordering::SeqCst) {
            return Err("session not paused".into());
        }

        {
            let mut req = session.shared.eval_request.lock().unwrap();
            *req = Some(expression);
        }
        session.shared.eval_signal.notify_one();

        let mut result = session.shared.eval_result.lock().unwrap();
        let (guard, _) = session
            .shared
            .eval_done
            .wait_timeout(result, Duration::from_secs(5))
            .unwrap();
        result = guard;
        Ok(result.take().unwrap_or(JsonValue::Null))
    }

    pub fn terminate(&self, session_id: u64) -> Result<bool, String> {
        let mut sessions = self.sessions.lock().unwrap();
        let mut session = match sessions.remove(&session_id) {
            Some(s) => s,
            None => return Ok(false),
        };
        session.alive.store(false, Ordering::SeqCst);
        session.shared.paused.store(false, Ordering::SeqCst);
        {
            let mut guard = session.shared.resume_mutex.lock().unwrap();
            *guard = true;
            session.shared.resume_signal.notify_all();
        }
        if let Some(join) = session.thread.take() {
            let _ = join.join();
        }
        Ok(true)
    }

    pub fn stop_all(&self) {
        let ids: Vec<u64> = self
            .sessions
            .lock()
            .map(|s| s.keys().copied().collect())
            .unwrap_or_default();
        for id in ids {
            let _ = self.terminate(id);
        }
    }
}

impl Drop for DebugManager {
    fn drop(&mut self) {
        self.stop_all();
    }
}

// ── Debug thread ──────────────────────────────────────────────────────

fn run_debug_session(
    script: String,
    args: Option<JsonMap<String, JsonValue>>,
    shared: Arc<DebugShared>,
    alive: Arc<AtomicBool>,
) {
    let lua = Lua::new();

    if let Some(args) = &args {
        let globals = lua.globals();
        for (key, value) in args {
            if let Ok(lua_val) = json_to_lua(&lua, value) {
                let _ = globals.set(key.as_str(), lua_val);
            }
        }
    }

    let shared_hook = shared.clone();
    let alive_hook = alive.clone();
    lua.set_interrupt(move |lua| {
        if !alive_hook.load(Ordering::SeqCst) {
            return Err(mlua::Error::runtime("debug session terminated"));
        }

        let current_line = lua
            .inspect_stack(0)
            .map(|d| d.curr_line() as usize)
            .unwrap_or(0);

        // Track stack depth by counting active stack levels
        let mut stack_depth: i32 = 0;
        while lua.inspect_stack(stack_depth as usize).is_some() {
            stack_depth += 1;
        }
        *shared_hook.step_depth.lock().unwrap() = stack_depth;

        let should_stop = {
            let mode = *shared_hook.step_mode.lock().unwrap();
            let at_bp = shared_hook
                .breakpoints
                .lock()
                .unwrap()
                .contains(&current_line);

            match mode {
                StepMode::Continue => at_bp,
                StepMode::StepIn => true,
                StepMode::StepOver => {
                    let depth = *shared_hook.step_depth.lock().unwrap();
                    depth <= 1 || at_bp
                }
                StepMode::StepOut => {
                    let depth = *shared_hook.step_depth.lock().unwrap();
                    depth <= 1 || at_bp
                }
            }
        };

        if should_stop && current_line > 0 {
            capture_stack(lua, &shared_hook);

            {
                let reason = if shared_hook
                    .breakpoints
                    .lock()
                    .unwrap()
                    .contains(&current_line)
                {
                    "breakpoint"
                } else {
                    "step"
                };
                *shared_hook.stopped_at.lock().unwrap() = Some(StopInfo {
                    line: current_line,
                    reason: reason.into(),
                });
            }

            shared_hook.paused.store(true, Ordering::SeqCst);

            let mut guard = shared_hook.resume_mutex.lock().unwrap();
            while !*guard {
                let (g, _) = shared_hook
                    .resume_signal
                    .wait_timeout(guard, Duration::from_millis(100))
                    .unwrap();
                guard = g;
                if !alive_hook.load(Ordering::SeqCst) {
                    return Err(mlua::Error::runtime("debug session terminated"));
                }

                if let Some(expr) = shared_hook.eval_request.lock().unwrap().take() {
                    let result = lua
                        .load(&expr)
                        .eval::<MultiValue>()
                        .map(|vals| {
                            let first = vals.into_iter().next().unwrap_or(LuaValue::Nil);
                            lua_to_json(&first).unwrap_or(JsonValue::Null)
                        })
                        .unwrap_or_else(|e| json!({"error": e.to_string()}));
                    *shared_hook.eval_result.lock().unwrap() = Some(result);
                    shared_hook.eval_done.notify_one();
                }
            }
            *guard = false;
            shared_hook.paused.store(false, Ordering::SeqCst);
            *shared_hook.stopped_at.lock().unwrap() = None;
        }

        Ok(VmState::Continue)
    });

    let result: Result<MultiValue, mlua::Error> =
        lua.load(&script).into_function().and_then(|f| f.call(()));

    if let Err(e) = result {
        if !e.to_string().contains("debug session terminated") {
            *shared.stopped_at.lock().unwrap() = Some(StopInfo {
                line: 0,
                reason: format!("error: {e}"),
            });
        }
    }

    alive.store(false, Ordering::SeqCst);
}

fn capture_stack(lua: &Lua, shared: &DebugShared) {
    // Luau's mlua binding doesn't expose local variable introspection
    *shared.locals.lock().unwrap() = Vec::new();

    let mut frames = Vec::new();
    for level in 0..20 {
        let Some(debug) = lua.inspect_stack(level) else {
            break;
        };
        let names = debug.names();
        let source = debug.source();
        frames.push(StackFrame {
            name: names.name.map(|n| n.into_owned()),
            source: source.short_src.map(|s| s.into_owned()),
            line: {
                let l = debug.curr_line();
                if l > 0 {
                    Some(l as usize)
                } else {
                    None
                }
            },
        });
    }
    *shared.call_stack.lock().unwrap() = frames;
}

// ── JSON ↔ Lua conversion ─────────────────────────────────────────────

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

// ── Module wiring ─────────────────────────────────────────────────────

pub struct DebugModule;

impl DaemonModule for DebugModule {
    fn id(&self) -> &str {
        "prism.debug"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        let manager = Arc::new(DebugManager::new());
        let registry = builder.registry().clone();

        let m = manager.clone();
        registry.register("luau.debug.launch", move |payload| {
            let args: LaunchArgs = parse(payload, "luau.debug.launch")?;
            let id = m
                .launch(args.script, args.args, args.stop_on_entry.unwrap_or(false))
                .map_err(|e| CommandError::handler("luau.debug.launch", e))?;
            Ok(json!({ "session_id": id }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.set_breakpoints", move |payload| {
            let args: SetBreakpointsArgs = parse(payload, "luau.debug.set_breakpoints")?;
            let confirmed = m
                .set_breakpoints(args.session_id, args.breakpoints)
                .map_err(|e| CommandError::handler("luau.debug.set_breakpoints", e))?;
            Ok(json!({ "confirmed": confirmed }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.continue", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.continue")?;
            let stopped = m
                .resume(args.session_id, StepMode::Continue)
                .map_err(|e| CommandError::handler("luau.debug.continue", e))?;
            Ok(json!({ "stopped_at": stopped }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.step_in", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.step_in")?;
            let stopped = m
                .resume(args.session_id, StepMode::StepIn)
                .map_err(|e| CommandError::handler("luau.debug.step_in", e))?;
            Ok(json!({ "stopped_at": stopped }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.step_over", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.step_over")?;
            let stopped = m
                .resume(args.session_id, StepMode::StepOver)
                .map_err(|e| CommandError::handler("luau.debug.step_over", e))?;
            Ok(json!({ "stopped_at": stopped }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.step_out", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.step_out")?;
            let stopped = m
                .resume(args.session_id, StepMode::StepOut)
                .map_err(|e| CommandError::handler("luau.debug.step_out", e))?;
            Ok(json!({ "stopped_at": stopped }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.inspect", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.inspect")?;
            let (locals, stack) = m
                .inspect(args.session_id)
                .map_err(|e| CommandError::handler("luau.debug.inspect", e))?;
            Ok(json!({ "locals": locals, "call_stack": stack }))
        })?;

        let m = manager.clone();
        registry.register("luau.debug.evaluate", move |payload| {
            let args: EvalArgs = parse(payload, "luau.debug.evaluate")?;
            let result = m
                .evaluate(args.session_id, args.expression)
                .map_err(|e| CommandError::handler("luau.debug.evaluate", e))?;
            Ok(json!({ "result": result }))
        })?;

        let m = manager;
        registry.register("luau.debug.terminate", move |payload| {
            let args: SessionArgs = parse(payload, "luau.debug.terminate")?;
            let terminated = m
                .terminate(args.session_id)
                .map_err(|e| CommandError::handler("luau.debug.terminate", e))?;
            Ok(json!({ "terminated": terminated }))
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

#[derive(Deserialize)]
struct LaunchArgs {
    script: String,
    #[serde(default)]
    args: Option<JsonMap<String, JsonValue>>,
    #[serde(default)]
    stop_on_entry: Option<bool>,
}

#[derive(Deserialize)]
struct SetBreakpointsArgs {
    session_id: u64,
    breakpoints: Vec<Breakpoint>,
}

#[derive(Deserialize)]
struct SessionArgs {
    session_id: u64,
}

#[derive(Deserialize)]
struct EvalArgs {
    session_id: u64,
    expression: String,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

    fn kernel() -> crate::DaemonKernel {
        DaemonBuilder::new()
            .with_luau()
            .with_debug()
            .build()
            .unwrap()
    }

    #[test]
    fn debug_module_registers_commands() {
        let kernel = kernel();
        let caps = kernel.capabilities();
        for name in [
            "luau.debug.launch",
            "luau.debug.set_breakpoints",
            "luau.debug.continue",
            "luau.debug.step_in",
            "luau.debug.step_over",
            "luau.debug.step_out",
            "luau.debug.inspect",
            "luau.debug.evaluate",
            "luau.debug.terminate",
        ] {
            assert!(caps.contains(&name.to_string()), "missing {name}");
        }
    }

    #[test]
    fn launch_and_terminate() {
        let mgr = DebugManager::new();
        let id = mgr.launch("return 42".into(), None, false).unwrap();
        thread::sleep(Duration::from_millis(100));
        let terminated = mgr.terminate(id).unwrap();
        assert!(terminated);
    }

    #[test]
    fn set_breakpoints() {
        let mgr = DebugManager::new();
        let id = mgr
            .launch("local x = 1\nlocal y = 2\nreturn x + y".into(), None, true)
            .unwrap();
        thread::sleep(Duration::from_millis(100));

        let confirmed = mgr
            .set_breakpoints(
                id,
                vec![
                    Breakpoint {
                        line: 2,
                        condition: None,
                    },
                    Breakpoint {
                        line: 3,
                        condition: None,
                    },
                ],
            )
            .unwrap();
        assert_eq!(confirmed.len(), 2);
        mgr.terminate(id).unwrap();
    }

    #[test]
    fn launch_with_stop_on_entry_pauses() {
        let mgr = DebugManager::new();
        let id = mgr
            .launch("local x = 1\nreturn x".into(), None, true)
            .unwrap();
        thread::sleep(Duration::from_millis(200));

        let sessions = mgr.sessions.lock().unwrap();
        let session = sessions.get(&id).unwrap();
        assert!(session.shared.paused.load(Ordering::SeqCst));
        drop(sessions);

        mgr.terminate(id).unwrap();
    }

    #[test]
    fn terminate_unknown_returns_false() {
        let mgr = DebugManager::new();
        assert!(!mgr.terminate(999).unwrap());
    }

    #[test]
    fn launch_via_kernel_invoke() {
        let kernel = kernel();
        let out = kernel
            .invoke(
                "luau.debug.launch",
                json!({ "script": "return 1", "stop_on_entry": false }),
            )
            .unwrap();
        let session_id = out["session_id"].as_u64().unwrap();
        assert!(session_id > 0);

        thread::sleep(Duration::from_millis(100));
        kernel
            .invoke("luau.debug.terminate", json!({ "session_id": session_id }))
            .unwrap();
    }
}
