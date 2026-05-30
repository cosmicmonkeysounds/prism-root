//! Core directive builtins (spec §14).
//!
//! These are the directives every Loom project gets out of the box.
//! They live as Rust handlers rather than Luau closures for now —
//! the Luau bridge will subsume / shadow them when it lands without
//! changing the surface syntax.
//!
//! | Builtin   | Purpose                                                                |
//! |-----------|------------------------------------------------------------------------|
//! | `sfx`     | Plays a sound cue. Emits `Event::Directive` with the cue + named args. |
//! | `cue`     | Generic "fire a named stage cue" — same envelope, different verb.      |
//! | `pause`   | Holds the playhead for `duration` seconds (emitted as an event).       |
//! | `anchor`  | Names a position in the weave — used by tests + analytics.             |
//! | `fire`    | Pushes a custom named event onto the ledger.                           |
//! | `set`     | Applies the directive's `<set: lhs OP rhs>` assignment to `World`.     |

use crate::directives::{AssignOp, CallContext, DirectiveError, Handler, HandlerOutcome, Registry};
use crate::expr::{Expr, Value, World};
use crate::ledger::Event;

pub fn register(registry: &mut Registry) {
    registry.register("sfx", Generic);
    registry.register("cue", Generic);
    registry.register("pause", Generic);
    registry.register("anchor", Generic);
    registry.register("fire", FireHandler);
    registry.register("set", SetHandler);
    // Live-performance core directives (spec §13). Implementations
    // are intentionally surface-level — they push the generic envelope
    // through the ledger; the live-layer mutations are routed by the
    // playhead through `LiveStage` directly so this stays a Luau-
    // friendly seam.
    registry.register("broadcast", Generic);
    registry.register("enroll", Generic);
    // Cast / roster builtins (spec v3 §13.4). Implementations are
    // ledger-only: each directive emits a structured envelope and
    // also mutates the world so dotted lookups (`Wren.player`,
    // `jamie_lee.role`) round-trip without coupling to LiveStage.
    registry.register("cast", CastHandler);
    registry.register("uncast", UncastHandler);
    registry.register("recast", RecastHandler);
    registry.register("promote", PromoteHandler);
    registry.register("demote", DemoteHandler);
    registry.register("load_roster", LoadRosterHandler);
}

/// Default handler — surfaces the call as an `Event::Directive`
/// envelope. The runtime emits the envelope after `dispatch` returns
/// (`HandlerOutcome::Handled`), so this body is a no-op.
struct Generic;
impl Handler for Generic {
    fn call(&self, _ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        Ok(HandlerOutcome::Handled)
    }
}

/// `<fire: name, key: value, …>` — push an arbitrary named event onto
/// the ledger and suppress the default envelope (the `Fired` event is
/// the envelope).
struct FireHandler;
impl Handler for FireHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let name = match ctx.positional.first() {
            Some(Value::String(s)) => s.clone(),
            Some(other) => other.display(),
            None => {
                return Err(DirectiveError::BadArgs {
                    kind: ctx.kind.into(),
                    message: "missing event name".into(),
                });
            }
        };
        let payload: Vec<(String, String)> = ctx
            .named
            .iter()
            .map(|(k, v)| (k.clone(), v.display()))
            .collect();
        ctx.ledger.push(Event::Fired { name, payload });
        Ok(HandlerOutcome::Suppressed)
    }
}

/// `<set: path OP expr>` — mutate `World`.
struct SetHandler;
impl Handler for SetHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let assign = ctx.assign.ok_or(DirectiveError::BadAssignment)?;
        let key = assign.path.join(".");
        let rhs = crate::expr::eval(&assign.rhs, ctx.world, &mut |name, _args| {
            Err(crate::expr::ExprError::UnknownFunction(name.into()))
        })?;
        let new_value = match assign.op {
            AssignOp::Set => rhs,
            AssignOp::AddAssign => arithmetic(ctx.world.get(&key), rhs, |a, b| a + b),
            AssignOp::SubAssign => arithmetic(ctx.world.get(&key), rhs, |a, b| a - b),
            AssignOp::MulAssign => arithmetic(ctx.world.get(&key), rhs, |a, b| a * b),
            AssignOp::DivAssign => arithmetic(ctx.world.get(&key), rhs, |a, b| a / b),
        };
        ctx.world.set(key.clone(), new_value.clone());
        ctx.ledger.push(Event::WorldSet {
            path: key,
            value: new_value.display(),
        });
        Ok(HandlerOutcome::Suppressed)
    }
}

fn arithmetic(lhs: Value, rhs: Value, op: fn(f64, f64) -> f64) -> Value {
    let l = lhs.as_number().unwrap_or(0.0);
    let r = rhs.as_number().unwrap_or(0.0);
    Value::Number(op(l, r))
}

// ---------------------------------------------------------------------
// Cast / roster builtins (spec v3 §13.4).
// ---------------------------------------------------------------------

/// Resolve a single arg by name — preferring the raw `Expr::Path`
/// (so bare identifiers like `jamie_lee` round-trip even though world
/// lookup returns Null) and falling back to the evaluated value's
/// display form.
fn pull_named(ctx: &CallContext<'_>, key: &str) -> Option<String> {
    if let Some(expr) = ctx.call.named.get(key) {
        if let Some(s) = expr_as_name(expr) {
            return Some(s);
        }
    }
    ctx.named.get(key).map(|v| v.display())
}

fn pull_positional(ctx: &CallContext<'_>, idx: usize) -> Option<String> {
    if let Some(expr) = ctx.call.positional.get(idx) {
        if let Some(s) = expr_as_name(expr) {
            return Some(s);
        }
    }
    ctx.positional.get(idx).map(|v| v.display())
}

fn expr_as_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Path(segments) => Some(segments.join(".")),
        Expr::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// Pull a (person, role)-shaped pair from the call. Honours the
/// canonical named form (`person:` + `role:`) and falls back to two
/// positional args.
fn cast_pair(ctx: &CallContext<'_>, a: &str, b: &str) -> Option<(String, String)> {
    if let (Some(av), Some(bv)) = (pull_named(ctx, a), pull_named(ctx, b)) {
        return Some((av, bv));
    }
    let p0 = pull_positional(ctx, 0)?;
    let p1 = pull_positional(ctx, 1)?;
    Some((p0, p1))
}

fn bind_cast(world: &mut World, person: &str, role: &str) {
    world.set(format!("{role}.player"), Value::String(person.to_string()));
    world.set(format!("{person}.role"), Value::String(role.to_string()));
}

fn release_cast(world: &mut World, person: &str, role: &str) {
    world.set(format!("{role}.player"), Value::Null);
    world.set(format!("{person}.role"), Value::Null);
}

struct CastHandler;
impl Handler for CastHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let (person, role) =
            cast_pair(ctx, "person", "role").ok_or_else(|| DirectiveError::BadArgs {
                kind: "cast".into(),
                message: "expected `<cast: <person> as <role>>` or named `person:`/`role:`".into(),
            })?;
        bind_cast(ctx.world, &person, &role);
        ctx.ledger.push(Event::CastBound {
            person,
            role,
        });
        Ok(HandlerOutcome::Suppressed)
    }
}

struct UncastHandler;
impl Handler for UncastHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let role = pull_named(ctx, "role")
            .or_else(|| pull_positional(ctx, 0))
            .ok_or_else(|| DirectiveError::BadArgs {
                kind: "uncast".into(),
                message: "expected `<uncast: <role>>`".into(),
            })?;
        // Read the current player out of the world so we can null the
        // back-reference too.
        let player = match ctx.world.get(&format!("{role}.player")) {
            Value::String(s) => Some(s),
            _ => None,
        };
        if let Some(p) = player.as_ref() {
            release_cast(ctx.world, p, &role);
        } else {
            ctx.world.set(format!("{role}.player"), Value::Null);
        }
        ctx.ledger.push(Event::CastReleased {
            person: player.unwrap_or_default(),
            role,
        });
        Ok(HandlerOutcome::Suppressed)
    }
}

struct RecastHandler;
impl Handler for RecastHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        // `<recast: Wren := kim_ho>` lands as an `assign`-style call
        // because of the `:=` token; fall back to the `role` / `to`
        // named form if the parser surfaced positional args instead.
        let (role, new_player) =
            cast_pair(ctx, "role", "to").ok_or_else(|| DirectiveError::BadArgs {
                kind: "recast".into(),
                message: "expected `<recast: <role> := <person>>` or named `role:`/`to:`".into(),
            })?;
        let old_player = match ctx.world.get(&format!("{role}.player")) {
            Value::String(s) => s,
            _ => String::new(),
        };
        if !old_player.is_empty() {
            release_cast(ctx.world, &old_player, &role);
        }
        bind_cast(ctx.world, &new_player, &role);
        ctx.ledger.push(Event::CastSwapped {
            role,
            old_player,
            new_player,
        });
        Ok(HandlerOutcome::Suppressed)
    }
}

struct PromoteHandler;
impl Handler for PromoteHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let (person, role) =
            cast_pair(ctx, "person", "role").ok_or_else(|| DirectiveError::BadArgs {
                kind: "promote".into(),
                message: "expected `<promote: <person> as <role>>`".into(),
            })?;
        bind_cast(ctx.world, &person, &role);
        ctx.ledger.push(Event::CastBound {
            person: person.clone(),
            role: role.clone(),
        });
        ctx.ledger.push(Event::RolePromoted { person, role });
        Ok(HandlerOutcome::Suppressed)
    }
}

struct DemoteHandler;
impl Handler for DemoteHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let person = pull_named(ctx, "person")
            .or_else(|| pull_positional(ctx, 0))
            .ok_or_else(|| DirectiveError::BadArgs {
                kind: "demote".into(),
                message: "expected `<demote: <person>>`".into(),
            })?;
        let role = match ctx.world.get(&format!("{person}.role")) {
            Value::String(s) => s,
            _ => String::new(),
        };
        if !role.is_empty() {
            release_cast(ctx.world, &person, &role);
        }
        ctx.ledger.push(Event::CastReleased {
            person,
            role,
        });
        Ok(HandlerOutcome::Suppressed)
    }
}

struct LoadRosterHandler;
impl Handler for LoadRosterHandler {
    fn call(&self, ctx: &mut CallContext<'_>) -> Result<HandlerOutcome, DirectiveError> {
        let roster = pull_positional(ctx, 0)
            .or_else(|| pull_named(ctx, "name"))
            .ok_or_else(|| DirectiveError::BadArgs {
                kind: "load_roster".into(),
                message: "expected `<load_roster: <roster-name>>`".into(),
            })?;
        ctx.world
            .set("Roster.active".to_string(), Value::String(roster.clone()));
        ctx.ledger.push(Event::RosterLoaded { roster });
        Ok(HandlerOutcome::Suppressed)
    }
}
