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

use crate::directives::{
    AssignOp, CallContext, DirectiveError, Handler, HandlerOutcome, Registry,
};
use crate::expr::Value;
use crate::ledger::Event;

pub fn register(registry: &mut Registry) {
    registry.register("sfx", Generic);
    registry.register("cue", Generic);
    registry.register("pause", Generic);
    registry.register("anchor", Generic);
    registry.register("fire", FireHandler);
    registry.register("set", SetHandler);
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
        let rhs = crate::expr::eval(&assign.rhs, ctx.world, &mut |name, _| {
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
