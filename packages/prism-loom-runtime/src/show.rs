//! `Show` — the runtime's front door. Wraps the compiled
//! [`LoomDatabase`] + the [`Ledger`] + the [`Playhead`] in one
//! struct, exposes `step` / `choose` / `vars` / `lets` for the host.
//!
//! Construction goes through [`Show::load`], which parses the source,
//! drops a project-style validator's diagnostics into the result, and
//! compiles the bundle. Hosts that want the diagnostics separately
//! call [`Show::load_with_diagnostics`].
//!
//! The Show owns the canonical reactive surface for Phase 2:
//!   - `vars` is the writable runtime store. Mutated by `~ $x := v`
//!     actions, inline `<$x := v>` assigns surfaced from frames, and
//!     direct host writes.
//!   - `lets` is the bindings table. Re-evaluated after every var
//!     mutation (eager, not yet incremental — `Memo<T>` wiring lands
//!     in Phase 3).
//!   - `roles` is host-controlled per-frame.

use std::collections::HashMap;

use prism_core::language::loom::parser::parse;
use prism_core::language::loom::validator::validate;

use crate::bundle::{compile, AssignOp, Document, LetBinding, LoomDatabase, Mutation};
use crate::ledger::{Ledger, LedgerEntry};
use crate::playhead::{Frame, Playhead};
use crate::resolver::{evaluate, ResolverContext};
use crate::value::Value;
use indexmap::IndexMap;

/// A parsed + compiled + booted Loom show ready for the renderer to
/// pull frames from.
#[derive(Debug)]
pub struct Show {
    bundle: LoomDatabase,
    /// Index of the currently active document — Phase 1 supports a
    /// single document per show; the host is responsible for picking
    /// which one to play if the source file holds several.
    active_doc: usize,
    ledger: Ledger,
    playhead: Playhead,
    /// `var` registry — the writable runtime store. Mutated by
    /// `~ var $x := value` actions (Phase 2 wires those up; today the
    /// host can poke directly).
    pub vars: HashMap<String, Value>,
    /// `let` bindings — reactive, write-once per scope. The runtime
    /// owns the storage; Phase 2 wires the parser-level let to land
    /// values here.
    pub lets: HashMap<String, Value>,
    /// Role bindings (`$SPEAKER`, `$LISTENER`, `$PLAYER`, …) the host
    /// sets per-frame.
    pub roles: HashMap<String, Value>,
    /// Monotonic virtual clock the runtime stamps onto every ledger
    /// entry. Hosts driving from real time advance it via
    /// [`Show::set_clock`].
    clock_ms: u64,
}

#[derive(Debug)]
pub struct LoadResult {
    pub show: Show,
    pub diagnostics: Vec<prism_core::language::loom::parser::LoomDiagnostic>,
}

impl Show {
    /// Parse, validate, compile, and boot a show. Diagnostics from
    /// the parser + validator are silently dropped; use
    /// [`Show::load_with_diagnostics`] when the host wants them.
    pub fn load(source: &str) -> Result<Show, LoadError> {
        let LoadResult { show, .. } = Self::load_with_diagnostics(source)?;
        Ok(show)
    }

    /// Like [`Show::load`] but also returns every parser + validator
    /// diagnostic the source produced. Hosts that want to refuse to
    /// boot a broken show should check `diagnostics.is_empty()`.
    pub fn load_with_diagnostics(source: &str) -> Result<LoadResult, LoadError> {
        let parsed = parse(source);
        let validator_diags = validate(&parsed.root);

        let bundle = compile(&parsed.root);
        if bundle.documents.is_empty() {
            return Err(LoadError::NoDocuments);
        }

        let mut diagnostics = parsed.diagnostics;
        diagnostics.extend(validator_diags);

        let mut show = Show {
            playhead: Playhead::new(&bundle.documents[0]),
            bundle,
            active_doc: 0,
            ledger: Ledger::new(),
            vars: HashMap::new(),
            lets: HashMap::new(),
            roles: HashMap::new(),
            clock_ms: 0,
        };
        show.recompute_lets();

        Ok(LoadResult { show, diagnostics })
    }

    /// Evaluate every top-level `let` binding against the current
    /// `vars` / `roles` / `ledger` snapshot and refresh `self.lets` in
    /// place. Phase 2 is eager — every var write triggers a full
    /// recomputation. Phase 3 will swap this for incremental
    /// `Memo<T>` dependency tracking.
    pub fn recompute_lets(&mut self) {
        // Snapshot the previous lets so a binding that references an
        // earlier let can still resolve through the *new* one as it
        // lands — we build a fresh map in source order.
        let mut fresh = HashMap::with_capacity(self.bundle.documents[self.active_doc].lets.len());
        let bindings: Vec<LetBinding> = self.bundle.documents[self.active_doc].lets.clone();
        for binding in &bindings {
            let ctx = ResolverContext {
                lets: &fresh,
                vars: &self.vars,
                roles: &self.roles,
                ledger: &self.ledger,
            };
            let v = evaluate(&binding.body, &ctx, self.clock_ms);
            fresh.insert(binding.name.clone(), v);
        }
        self.lets = fresh;
    }

    /// Apply a single captured [`Mutation`] to the runtime stores.
    /// Surfaced both from `Item::Mutate` frames and from inline
    /// `<$x := v>` assigns drained off the playhead after each step.
    pub fn apply_mutation(&mut self, m: &Mutation) {
        let rhs_value = match m.op {
            AssignOp::Inc => Value::Int(1),
            _ => match &m.rhs {
                Some(expr) => {
                    let ctx = ResolverContext {
                        lets: &self.lets,
                        vars: &self.vars,
                        roles: &self.roles,
                        ledger: &self.ledger,
                    };
                    evaluate(expr, &ctx, self.clock_ms)
                }
                None => Value::Nil,
            },
        };
        let current = self.var_at(&m.name, &m.chain);
        let next = combine(current, &rhs_value, m.op);
        self.set_var_at(&m.name, &m.chain, next);
        self.recompute_lets();
    }

    fn var_at(&self, name: &str, chain: &[String]) -> Value {
        let mut current = self.vars.get(name).cloned().unwrap_or(Value::Nil);
        for step in chain {
            current = match current {
                Value::Map(mut m) => m.shift_remove(step).unwrap_or(Value::Nil),
                _ => Value::Nil,
            };
        }
        current
    }

    fn set_var_at(&mut self, name: &str, chain: &[String], value: Value) {
        if chain.is_empty() {
            self.vars.insert(name.to_string(), value);
            return;
        }
        let entry = self
            .vars
            .entry(name.to_string())
            .or_insert_with(|| Value::Map(IndexMap::new()));
        write_into_map(entry, chain, value);
    }

    pub fn bundle(&self) -> &LoomDatabase {
        &self.bundle
    }

    pub fn document(&self) -> &Document {
        &self.bundle.documents[self.active_doc]
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    /// Advance the show by one frame. Returns `None` when the playhead
    /// is parked waiting for a choice — call [`Show::choose`] first.
    ///
    /// `Item::Mutate` items are applied automatically — the surfaced
    /// `Frame::Mutate` is informational. `Item::Fire` similarly pushes
    /// a `Fired` ledger entry before returning the frame so downstream
    /// `since()` reads see it. Inline `<$x := v>` assigns drained from
    /// the playhead apply *after* the frame is built (so the rendered
    /// text sees the pre-assign value).
    pub fn step(&mut self) -> Option<Frame> {
        if self.playhead.is_awaiting_choice() {
            return None;
        }
        let ledger_snapshot = self.ledger.clone();
        let ctx = ResolverContext {
            lets: &self.lets,
            vars: &self.vars,
            roles: &self.roles,
            ledger: &ledger_snapshot,
        };
        let frame = self.playhead.step(
            &self.bundle.documents[self.active_doc],
            &mut self.ledger,
            &ctx,
            self.clock_ms,
        );
        // `ctx` is dropped at the end of this scope; the explicit drop
        // is unnecessary because NLL ends the borrows at last-use.

        // Drain inline assigns triggered by the text we just rendered
        // and apply them before returning the frame.
        let pending = self.playhead.take_pending_assigns();
        for m in pending {
            self.apply_mutation(&m);
        }

        // Auto-apply mutation / fire frames.
        if let Some(frame) = &frame {
            match frame {
                Frame::Mutate(m) => self.apply_mutation(m),
                Frame::Fire { event } => {
                    self.ledger.push(LedgerEntry::Fired {
                        event: event.clone(),
                        at_ms: self.clock_ms,
                    });
                    self.recompute_lets();
                }
                _ => {}
            }
        }
        frame
    }

    /// Resolve the currently pending choice by selecting the 0-based
    /// option index from the last [`Frame::Choices`] returned. Does
    /// nothing if no choice is pending.
    pub fn choose(&mut self, option_idx: usize) {
        let ledger_snapshot = self.ledger.clone();
        let ctx = ResolverContext {
            lets: &self.lets,
            vars: &self.vars,
            roles: &self.roles,
            ledger: &ledger_snapshot,
        };
        self.playhead.choose(
            &self.bundle.documents[self.active_doc],
            option_idx,
            &mut self.ledger,
            &ctx,
            self.clock_ms,
        );
    }

    /// Jump the playhead to a named section — used by the host when a
    /// frame-external trigger (a button, a Luau script) wants to
    /// re-enter the show somewhere specific.
    pub fn jump_to(&mut self, section_id: &str) {
        self.playhead
            .jump_to(&self.bundle.documents[self.active_doc], section_id);
    }

    pub fn is_awaiting_choice(&self) -> bool {
        self.playhead.is_awaiting_choice()
    }

    pub fn is_at_end(&self) -> bool {
        self.playhead.is_at_end()
    }

    /// Move the virtual clock forward. Ledger entries pushed after
    /// this point carry the new timestamp; `since(...)` predicates
    /// read it through [`Ledger::since`].
    pub fn set_clock(&mut self, now_ms: u64) {
        self.clock_ms = now_ms;
    }

    pub fn clock_ms(&self) -> u64 {
        self.clock_ms
    }

    /// Run the show until it parks (awaiting a choice) or terminates.
    /// Returns every frame produced along the way. Useful for tests
    /// and for hosts that batch-render between user actions.
    pub fn play_until_park(&mut self) -> Vec<Frame> {
        let mut out = Vec::new();
        while let Some(frame) = self.step() {
            let stop = matches!(frame, Frame::Choices(_) | Frame::End);
            out.push(frame);
            if stop {
                break;
            }
        }
        out
    }
}

/// Recursively descend `current` along `chain`, creating nested
/// `Map`s as needed, and write `value` at the leaf. Replaces non-map
/// intermediates with a fresh map — last write wins.
fn write_into_map(current: &mut Value, chain: &[String], value: Value) {
    if chain.is_empty() {
        *current = value;
        return;
    }
    if !matches!(current, Value::Map(_)) {
        *current = Value::Map(IndexMap::new());
    }
    if let Value::Map(map) = current {
        let entry = map.entry(chain[0].clone()).or_insert_with(|| Value::Nil);
        write_into_map(entry, &chain[1..], value);
    }
}

/// Combine the existing value with the RHS per the assign operator's
/// semantics. `Set` always replaces; `PlusEq` / `MinusEq` are int-or-
/// list aware (the design's `knowledge.list += @id` shorthand); `Inc`
/// adds 1 to an int (or sets to 1 from Nil).
fn combine(current: Value, rhs: &Value, op: AssignOp) -> Value {
    match op {
        AssignOp::Set => rhs.clone(),
        AssignOp::Inc => match current {
            Value::Int(n) => Value::Int(n.saturating_add(1)),
            Value::Nil => Value::Int(1),
            other => other,
        },
        AssignOp::PlusEq => match (current, rhs) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_add(*b)),
            (Value::Float(a), Value::Float(b)) => Value::Float(a + *b),
            (Value::Int(a), Value::Float(b)) => Value::Float(a as f64 + *b),
            (Value::Float(a), Value::Int(b)) => Value::Float(a + *b as f64),
            (Value::List(mut xs), v) => {
                xs.push(v.clone());
                Value::List(xs)
            }
            (Value::Nil, v) => v.clone(),
            (other, _) => other,
        },
        AssignOp::MinusEq => match (current, rhs) {
            (Value::Int(a), Value::Int(b)) => Value::Int(a.saturating_sub(*b)),
            (Value::Float(a), Value::Float(b)) => Value::Float(a - *b),
            (Value::Int(a), Value::Float(b)) => Value::Float(a as f64 - *b),
            (Value::Float(a), Value::Int(b)) => Value::Float(a - *b as f64),
            (Value::List(mut xs), v) => {
                xs.retain(|x| x != v);
                Value::List(xs)
            }
            (Value::Nil, _) => Value::Nil,
            (other, _) => other,
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The source produced no parseable document (no `#` header).
    NoDocuments,
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::NoDocuments => {
                write!(f, "source contained no Loom documents (no `#` header)")
            }
        }
    }
}

impl std::error::Error for LoadError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_compiles_and_boots_a_show() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hello.\n";
        let show = Show::load(src).expect("show");
        assert_eq!(show.document().id, "d");
        assert!(!show.is_at_end());
    }

    #[test]
    fn load_with_diagnostics_returns_validator_findings() {
        // Speaker without cast — validator emits unknown-cast.
        let src = "# d\n-- start\nWREN\n  Hello.\n";
        let result = Show::load_with_diagnostics(src).expect("show");
        assert!(result.diagnostics.iter().any(|d| d.id == "unknown-cast"));
    }

    #[test]
    fn play_until_park_yields_dialogue_then_end() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hello.\n";
        let mut show = Show::load(src).unwrap();
        let frames = show.play_until_park();
        assert_eq!(frames.len(), 2);
        assert!(matches!(&frames[0], Frame::Dialogue { speaker, .. } if speaker == "WREN"));
        assert!(matches!(frames[1], Frame::End));
    }

    #[test]
    fn choice_branch_lands_in_target_section() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hi.\n  * help -> good\n  * leave -> bad\n-- good\nWREN\n  Thanks.\n-- bad\nWREN\n  Bye.\n";
        let mut show = Show::load(src).unwrap();
        let frames = show.play_until_park();
        assert!(matches!(frames.last(), Some(Frame::Choices(_))));
        show.choose(0);
        let next = show.step().expect("post-choose frame");
        // Expect to land in `good` — a dialogue with text containing "Thanks".
        match next {
            Frame::Dialogue { lines, .. } => {
                assert!(lines.iter().any(|l| l.contains("Thanks")));
            }
            other => panic!("expected dialogue after choice, got {other:?}"),
        }
        assert!(show.ledger().played("good"));
    }

    #[test]
    fn missing_header_returns_load_error() {
        let err = Show::load("just some random text\n").unwrap_err();
        assert_eq!(err, LoadError::NoDocuments);
    }

    #[test]
    fn clock_drives_ledger_timestamps() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hi.\n";
        let mut show = Show::load(src).unwrap();
        show.set_clock(123);
        let _ = show.play_until_park();
        // Ledger should have a visit for `start` at the configured time.
        assert!(show
            .ledger()
            .entries()
            .iter()
            .any(|e| matches!(
                e,
                crate::ledger::LedgerEntry::Visited { section, at_ms } if section == "start" && *at_ms == 123
            )));
    }

    #[test]
    fn mutation_action_updates_vars() {
        let src = "# d\n-- s\n~ var $trust := 30\n";
        let mut show = Show::load(src).unwrap();
        let _ = show.play_until_park();
        assert_eq!(show.vars.get("trust"), Some(&Value::Int(30)));
    }

    #[test]
    fn fire_action_writes_to_ledger() {
        let src = "# d\n-- s\n~ fire bell_solved\n";
        let mut show = Show::load(src).unwrap();
        let _ = show.play_until_park();
        assert!(show
            .ledger()
            .entries()
            .iter()
            .any(|e| matches!(e, LedgerEntry::Fired { event, .. } if event == "bell_solved")));
    }

    #[test]
    fn let_binding_resolves_on_boot() {
        let src = "# d\nlet greeting = \"hi\"\n-- s\nWREN\n  body\ncast WREN\n  .label x\n";
        let show = Show::load(src).unwrap();
        assert_eq!(show.lets.get("greeting"), Some(&Value::Str("hi".into())));
    }

    #[test]
    fn let_binding_recomputes_after_mutation() {
        let src = "# d\nlet trusted = $trust > 50\n-- s\n~ var $trust := 75\n";
        let mut show = Show::load(src).unwrap();
        // Pre-mutation: trust is nil, trusted is false.
        assert_eq!(show.lets.get("trusted"), Some(&Value::Bool(false)));
        let _ = show.play_until_park();
        assert_eq!(show.vars.get("trust"), Some(&Value::Int(75)));
        assert_eq!(show.lets.get("trusted"), Some(&Value::Bool(true)));
    }

    #[test]
    fn guarded_choice_filtered_by_let_state() {
        let src = "# d\ncast WREN\n  .label x\nlet trusted = $trust > 50\n-- s\nWREN\n  Pick.\n  * Always.\n    -> done\n  * Only if trusted. if $trusted\n    -> done\n-- done\n";
        let mut show = Show::load(src).unwrap();
        // No trust set → only 1 visible.
        let frames = show.play_until_park();
        let choices = match frames.last() {
            Some(Frame::Choices(c)) => c.clone(),
            other => panic!("expected choices, got {other:?}"),
        };
        assert_eq!(choices.len(), 1);
        assert!(choices[0].label.contains("Always"));
    }

    #[test]
    fn interpolation_renders_var_value() {
        let src = "# d\ncast WREN\n  .label x\n-- s\nWREN\n  Trust is $trust.\n";
        let mut show = Show::load(src).unwrap();
        show.vars.insert("trust".to_string(), Value::Int(42));
        show.recompute_lets();
        let frames = show.play_until_park();
        let dialogue = frames
            .iter()
            .find_map(|f| match f {
                Frame::Dialogue { lines, .. } => Some(lines.clone()),
                _ => None,
            })
            .expect("dialogue");
        assert!(
            dialogue.iter().any(|l| l.contains("42")),
            "expected `42` in rendered text: {dialogue:?}"
        );
    }
}
