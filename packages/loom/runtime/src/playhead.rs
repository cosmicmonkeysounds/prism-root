//! Playhead — a cursor over the woven program (spec §4).
//!
//! The playhead reads top-to-bottom, advancing one *yield point* per
//! call to [`Playhead::step`]. A yield point is a single narrator-
//! visible event: an action paragraph, a dialogue line, a scene
//! heading, a metadata fence, or a choice prompt. Diverts, tunnel
//! returns, and `-> END` are consumed internally and do not yield.
//!
//! Phase-3 scope: enough machinery to play the §16 worked example
//! end-to-end with manual choice input. Reactive `let`, hook draining
//! between yields, generator interleaving, and the tiered scheduler
//! (spec §12.5) come online in subsequent phases.

use std::collections::VecDeque;
use std::sync::Arc;

use loom_parser::ast::{
    BodyItem, Choice, Conditional, DialogueLine, Directive, DirectiveBlock, Divert, DivertTarget,
    Item,
};

use crate::bundle::{BeatRef, Bundle};
use crate::directives::{self, DirectiveError, HandlerOutcome, Registry};
use crate::expr::{self, Expr, Value, World};
use crate::ledger::{self, ChoiceOption, Event, Ledger};
use crate::resolver::ResolveError;

/// The visible outcome of one [`Playhead::step`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Step {
    Event(Event),
    /// The playhead is offering a choice menu and is now waiting on
    /// [`Playhead::choose`].
    Choice(Vec<ChoiceOption>),
    /// The playhead halted (`-> END` or ran out of body to advance).
    Ended,
}

#[derive(Clone, Debug, thiserror::Error, PartialEq)]
pub enum PlayError {
    #[error("project has no resolvable entry beat")]
    NoEntry,
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("choose() called but no choice prompt is active")]
    NoChoiceActive,
    #[error("choice index {0} is out of range")]
    ChoiceOutOfRange(usize),
    #[error("<- tunnel return with no tunnel frame on the stack")]
    BareReturn,
    #[error(transparent)]
    Directive(#[from] DirectiveError),
}

/// Internal queue entry — the unit the playhead lowers body items
/// into. One `Yield` corresponds to either one visible event or one
/// internal control-flow operation.
#[derive(Clone, Debug)]
enum Yield {
    Event(Event),
    Choice(Vec<PendingChoice>),
    Divert {
        target: DivertTarget,
        params: indexmap::IndexMap<String, String>,
    },
    Tunnel {
        target: DivertTarget,
        params: indexmap::IndexMap<String, String>,
    },
    Return,
    End,
    Directive(Directive),
    /// Conditional — at step time, find the first arm whose
    /// condition expression is truthy and lower its body into the
    /// queue head.
    Conditional(Conditional),
    /// A `<broadcast: …>`-style directive that runs first, then
    /// lowers its body into the queue head.
    DirectiveBlock(DirectiveBlock),
}

#[derive(Clone, Debug)]
struct PendingChoice {
    option: ChoiceOption,
    /// Body of the choice — lowered into the queue when the option
    /// is selected.
    body: Vec<BodyItem>,
}

/// One in-flight beat. Tunnel calls push; `<-` returns pop.
#[derive(Debug)]
struct Frame {
    beat: BeatRef,
    file_qualifier: String,
}

pub struct Playhead {
    bundle: Arc<Bundle>,
    queue: VecDeque<Yield>,
    stack: Vec<Frame>,
    ledger: Ledger,
    world: World,
    registry: Arc<Registry>,
    halted: bool,
    awaiting_choice: bool,
    /// Parsed `let name = expr` bindings collected from every file
    /// in the bundle (spec §12.1). Re-evaluated before each
    /// expression read so dependent values stay current.
    let_bindings: Vec<LetSlot>,
    /// Project-global set of `*` (once-only) choice texts that have
    /// already been picked. Sticky `+` choices are never added.
    /// Filtering happens at choice-yield time in `step` (spec §5).
    taken_once_only: std::collections::HashSet<String>,
    /// Stack of saved queues, one per active tunnel call. A `<-`
    /// pops the most recent entry and resumes the caller's queue.
    pending_returns: Vec<VecDeque<Yield>>,
    /// Compiled characters, cloned from the bundle so the playhead
    /// can mutate disposition/knowledge without borrowing the bundle.
    characters: std::collections::HashMap<String, crate::simulacra::CharacterState>,
}

#[derive(Debug)]
struct LetSlot {
    name: String,
    expr: Expr,
    /// Last evaluated value — kept so we only push a
    /// `LetEvaluated` envelope when the value actually changes.
    last: Option<Value>,
}

impl std::fmt::Debug for Playhead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Playhead")
            .field("stack_depth", &self.stack.len())
            .field("queue_len", &self.queue.len())
            .field("halted", &self.halted)
            .field("awaiting_choice", &self.awaiting_choice)
            .finish()
    }
}

impl Playhead {
    /// Start a new playhead at the bundle's entry beat with the
    /// builtin directive registry.
    pub fn new(bundle: Arc<Bundle>) -> Result<Self, PlayError> {
        Self::with_registry(bundle, Arc::new(Registry::with_builtins()))
    }

    /// Start a playhead with a caller-supplied directive registry.
    pub fn with_registry(bundle: Arc<Bundle>, registry: Arc<Registry>) -> Result<Self, PlayError> {
        let entry = bundle.entry.ok_or(PlayError::NoEntry)?;
        let let_bindings = collect_let_bindings(&bundle);
        let mut world = World::new();
        // Publish every character's namespace before the playhead
        // starts so dotted-path reads (`Wren.trusts.Player`,
        // `Wren.health.max`) resolve from beat one.
        for character in bundle.characters.values() {
            character.publish(&mut world);
        }
        let characters: std::collections::HashMap<String, crate::simulacra::CharacterState> =
            bundle.characters.clone();
        let mut p = Self {
            bundle,
            queue: VecDeque::new(),
            stack: Vec::new(),
            ledger: Ledger::default(),
            world,
            registry,
            halted: false,
            awaiting_choice: false,
            let_bindings,
            taken_once_only: std::collections::HashSet::new(),
            pending_returns: Vec::new(),
            characters,
        };
        p.enter_beat(entry);
        Ok(p)
    }

    /// Borrow a compiled CHARACTER / TRAIT by name (spec §10).
    pub fn character(&self, name: &str) -> Option<&crate::simulacra::CharacterState> {
        self.characters.get(name)
    }

    /// Mutate a compiled CHARACTER by name. Caller is responsible for
    /// republishing into the world if the change is visible there.
    pub fn character_mut(&mut self, name: &str) -> Option<&mut crate::simulacra::CharacterState> {
        self.characters.get_mut(name)
    }

    /// Apply a `<set: path OP rhs>` mutation through the character
    /// store first. When the path resolves to a character's
    /// disposition or knowledge slot the character is updated and
    /// republished into the world; otherwise this returns `false` so
    /// the generic `World` set falls through.
    pub fn route_set_through_character(
        &mut self,
        path: &[String],
        value: &crate::expr::Value,
        op: crate::simulacra::SetOp,
    ) -> bool {
        if path.len() < 3 {
            return false;
        }
        let owner = path[0].clone();
        let Some(character) = self.characters.get_mut(&owner) else {
            return false;
        };
        if character.apply_set(path, value, op) {
            character.refresh_own_reacts(&self.world);
            character.publish(&mut self.world);
            return true;
        }
        false
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
    }

    pub fn world(&self) -> &World {
        &self.world
    }

    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    pub fn halted(&self) -> bool {
        self.halted
    }

    /// Advance to the next visible step. Returns [`Step::Choice`]
    /// when the playhead is waiting on `choose`; callers must call
    /// [`Playhead::choose`] before stepping again.
    pub fn step(&mut self) -> Result<Step, PlayError> {
        if self.halted {
            return Ok(Step::Ended);
        }
        if self.awaiting_choice {
            // Re-yield the still-active prompt for caller convenience.
            if let Some(Yield::Choice(options)) = self.queue.front() {
                let opts: Vec<ChoiceOption> = options.iter().map(|c| c.option.clone()).collect();
                return Ok(Step::Choice(opts));
            }
        }

        loop {
            let next = match self.queue.pop_front() {
                Some(y) => y,
                None => {
                    // Beat exhausted without an explicit divert.
                    return Ok(self.halt());
                }
            };
            match next {
                Yield::Event(event) => {
                    let expanded = self.expand_event_inlines(event)?;
                    self.ledger.push(expanded.clone());
                    return Ok(Step::Event(expanded));
                }
                Yield::Choice(options) => {
                    // Drop `*` (once-only) options whose text the
                    // playhead has already taken. `+` (sticky) options
                    // always stay.
                    let filtered: Vec<PendingChoice> = options
                        .into_iter()
                        .filter(|c| {
                            c.option.sticky || !self.taken_once_only.contains(&c.option.text)
                        })
                        .collect();
                    if filtered.is_empty() {
                        // Every option has been exhausted — fall
                        // through to the next yield without prompting.
                        continue;
                    }
                    // Re-index the visible options so callers get a
                    // contiguous 0..n list.
                    let mut renumbered = filtered;
                    for (i, c) in renumbered.iter_mut().enumerate() {
                        c.option.index = i;
                    }
                    self.awaiting_choice = true;
                    let opts: Vec<ChoiceOption> =
                        renumbered.iter().map(|c| c.option.clone()).collect();
                    // Re-park the choice at the head of the queue so
                    // it survives the pop above.
                    self.queue.push_front(Yield::Choice(renumbered));
                    let prompted = Event::ChoicePrompted {
                        options: opts.clone(),
                    };
                    self.ledger.push(prompted);
                    return Ok(Step::Choice(opts));
                }
                Yield::Divert { target, params } => {
                    let beat_ref = self.resolve(&target)?;
                    let beat = self.bundle.beat(beat_ref).clone();
                    self.bind_beat_params(&params)?;
                    self.ledger.push(Event::Diverted {
                        target: target.name.clone(),
                        beat: beat.name.clone(),
                    });
                    self.enter_beat(beat_ref);
                }
                Yield::Tunnel { target, params } => {
                    let beat_ref = self.resolve(&target)?;
                    let beat = self.bundle.beat(beat_ref).clone();
                    self.bind_beat_params(&params)?;
                    // Snapshot the *current* queue so `<-` can restore
                    // it. The tunnel target's body is lowered on top.
                    let saved = std::mem::take(&mut self.queue);
                    self.ledger.push(Event::Tunneled {
                        target: target.name.clone(),
                        beat: beat.name.clone(),
                    });
                    self.enter_beat(beat_ref);
                    // Re-park the saved continuation behind a Return
                    // sentinel so the next `<-` pops the frame and
                    // resumes the saved queue.
                    self.pending_returns.push(saved);
                }
                Yield::Return => {
                    if self.stack.len() < 2 {
                        return Err(PlayError::BareReturn);
                    }
                    self.stack.pop();
                    self.ledger.push(Event::Returned);
                    if let Some(saved) = self.pending_returns.pop() {
                        // Append whatever's left of the called beat
                        // (usually nothing) ahead of the saved queue,
                        // so any straggler yields finish before resume.
                        let mut resumed = saved;
                        while let Some(y) = self.queue.pop_back() {
                            resumed.push_front(y);
                        }
                        self.queue = resumed;
                    }
                }
                Yield::End => {
                    return Ok(self.halt());
                }
                Yield::Directive(directive) => {
                    let call = directives::parse(&directive.raw)?;
                    // Route `<set: Character.knows.X …>` /
                    // `<set: Character.trusts.Target …>` through the
                    // character store; if the path matches we surface
                    // the world mutation as a `WorldSet` envelope and
                    // *skip* the generic SetHandler so it doesn't
                    // double-apply the compound op (spec §10.2).
                    if let Some(assign) = &call.assign {
                        let rhs_value = expr::eval(&assign.rhs, &self.world, &mut |n, _| {
                            Err(expr::ExprError::UnknownFunction(n.into()))
                        })
                        .map_err(crate::directives::DirectiveError::from)?;
                        let op = match assign.op {
                            crate::directives::AssignOp::Set => crate::simulacra::SetOp::Assign,
                            crate::directives::AssignOp::AddAssign => crate::simulacra::SetOp::Add,
                            crate::directives::AssignOp::SubAssign => crate::simulacra::SetOp::Sub,
                            _ => crate::simulacra::SetOp::Assign,
                        };
                        if self.route_set_through_character(&assign.path, &rhs_value, op) {
                            let key = assign.path.join(".");
                            let new_value = self.world.get(&key);
                            self.ledger.push(Event::WorldSet {
                                path: key,
                                value: new_value.display(),
                            });
                            if let Some(latest) = self.ledger.events().last().cloned() {
                                return Ok(Step::Event(latest));
                            }
                            continue;
                        }
                    }
                    let (outcome, positional, named) = directives::dispatch(
                        &call,
                        &self.registry,
                        &mut self.world,
                        &mut self.ledger,
                    )?;
                    if matches!(outcome, HandlerOutcome::Handled) {
                        let event = Event::Directive {
                            kind: call.kind.clone(),
                            positional: call
                                .positional
                                .iter()
                                .zip(positional.iter())
                                .map(|(expr, value)| match (expr, value) {
                                    // `<anchor: bell_seen>` — a path
                                    // arg that didn't resolve in the
                                    // world keeps its written name
                                    // (so `played(bell_seen)` matches).
                                    (Expr::Path(segs), Value::Null) => segs.join("."),
                                    _ => value.display(),
                                })
                                .collect(),
                            named: named
                                .iter()
                                .map(|(k, v)| (k.clone(), v.display()))
                                .collect(),
                        };
                        self.ledger.push(event.clone());
                        return Ok(Step::Event(event));
                    }
                    // Suppressed handlers (set, fire) already pushed
                    // their own event; surface the most recent one to
                    // the caller so `step()` still yields visible work.
                    if let Some(latest) = self.ledger.events().last().cloned() {
                        return Ok(Step::Event(latest));
                    }
                }
                Yield::Conditional(cond) => {
                    let chosen = self.resolve_conditional(&cond)?;
                    if let Some(body) = chosen {
                        let lowered = self.lower_body(&body);
                        for y in lowered.into_iter().rev() {
                            self.queue.push_front(y);
                        }
                    }
                }
                Yield::DirectiveBlock(block) => {
                    if let Some((var, src)) = parse_for_directive(&block.directive.raw) {
                        // `<for: x in expr>` — evaluate `expr`, then
                        // for each element bind `x` and lower the
                        // body. Iterations are unrolled in order; the
                        // var slot keeps its final value when the loop
                        // exits.
                        self.refresh_lets()?;
                        let value = self.eval_expression(&src)?;
                        let items: Vec<Value> = match value {
                            Value::List(items) => items,
                            other => {
                                // Single-value coercion: iterate once
                                // with the value bound. Matches the
                                // "list of LOCATION" feel of the spec
                                // example without forcing brackets.
                                vec![other]
                            }
                        };
                        for item in items.iter().rev() {
                            // Lowered for each iteration so the world
                            // sees the binding at lower-time, but the
                            // actual binding write happens at yield
                            // time via a `BindThenLower` wrapper. To
                            // keep this minimal we push a synthetic
                            // set directive ahead of the body.
                            let lowered_body = self.lower_body(&block.body);
                            for y in lowered_body.into_iter().rev() {
                                self.queue.push_front(y);
                            }
                            // Prepend a write of `var = item` so the
                            // body sees it.
                            let set_raw = format!("set: {} = {}", var, literal_for_value(item));
                            self.queue.push_front(Yield::Directive(Directive {
                                raw: set_raw,
                                span: block.directive.span,
                            }));
                        }
                        continue;
                    }
                    // Run the leading directive's side effects first,
                    // then lower the body. We push the body items
                    // ahead of the queue so they execute after this
                    // step's directive event surfaces.
                    let lowered_body = self.lower_body(&block.body);
                    for y in lowered_body.into_iter().rev() {
                        self.queue.push_front(y);
                    }
                    // Re-enqueue the directive itself at the head so
                    // the next loop iteration dispatches it.
                    self.queue.push_front(Yield::Directive(block.directive));
                }
            }
        }
    }

    fn resolve_conditional(
        &mut self,
        cond: &Conditional,
    ) -> Result<Option<Vec<BodyItem>>, PlayError> {
        self.refresh_lets()?;
        for arm in &cond.arms {
            let truthy = match &arm.condition {
                None => true,
                Some(expr_text) => self.eval_expression(expr_text)?.truthy(),
            };
            if truthy {
                self.ledger.push(Event::ConditionalArm {
                    condition: arm.condition.clone(),
                });
                return Ok(Some(arm.body.clone()));
            }
        }
        Ok(None)
    }

    /// Substitute `{expr}` chunks (spec §5 — the reader-facing
    /// bracket) inside an event's text fields against the current
    /// world + ledger. Inline `<kind: args>` directives are
    /// extracted and dispatched first (they may set world values
    /// that `{expr}` substitution then reads).
    fn expand_event_inlines(&mut self, event: Event) -> Result<Event, PlayError> {
        match event {
            Event::Action { text } => {
                let cleaned = self.dispatch_inline_directives(&text)?;
                self.refresh_lets()?;
                let expanded = self.expand_inline_text(&cleaned)?;
                Ok(Event::Action { text: expanded })
            }
            Event::Dialogue {
                speaker,
                parenthetical,
                text,
            } => {
                let cleaned = self.dispatch_inline_directives(&text)?;
                self.refresh_lets()?;
                let expanded = self.expand_inline_text(&cleaned)?;
                Ok(Event::Dialogue {
                    speaker,
                    parenthetical,
                    text: expanded,
                })
            }
            other => Ok(other),
        }
    }

    /// Pull `<kind: args>` chunks out of `source`, dispatch each
    /// through the directive registry (firing their side effects
    /// and ledger envelopes), and return the surrounding text.
    fn dispatch_inline_directives(&mut self, source: &str) -> Result<String, PlayError> {
        if !source.contains('<') {
            return Ok(source.to_string());
        }
        let (cleaned, chunks) = split_inline_directives(source);
        for raw in chunks {
            let call = directives::parse(&raw)?;
            let (outcome, positional, named) =
                directives::dispatch(&call, &self.registry, &mut self.world, &mut self.ledger)?;
            if matches!(outcome, HandlerOutcome::Handled) {
                self.ledger.push(Event::Directive {
                    kind: call.kind.clone(),
                    positional: call
                        .positional
                        .iter()
                        .zip(positional.iter())
                        .map(|(expr, value)| match (expr, value) {
                            (Expr::Path(segs), Value::Null) => segs.join("."),
                            _ => value.display(),
                        })
                        .collect(),
                    named: named
                        .iter()
                        .map(|(k, v)| (k.clone(), v.display()))
                        .collect(),
                });
            }
        }
        // Collapse leftover double-spaces from chunk extraction.
        Ok(collapse_whitespace(&cleaned))
    }

    /// Scan `source` for top-level `{…}` chunks and replace each
    /// with the evaluated value's [`Value::display`] form. Braces
    /// nest naively (one level of `{` inside a chunk is fine; deeper
    /// nesting is rare in prose and is treated as raw text).
    fn expand_inline_text(&self, source: &str) -> Result<String, PlayError> {
        if !source.contains('{') {
            return Ok(source.to_string());
        }
        let mut out = String::with_capacity(source.len());
        let bytes = source.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            let c = bytes[i] as char;
            if c == '{' {
                if let Some(rel) = bytes[i + 1..].iter().position(|b| *b == b'}') {
                    let inner = &source[i + 1..i + 1 + rel];
                    match self.eval_expression(inner.trim()) {
                        Ok(value) => out.push_str(&value.display()),
                        Err(_) => {
                            // Unresolvable — fall back to leaving the
                            // original `{…}` chunk visible so the
                            // writer can see what they typed.
                            out.push('{');
                            out.push_str(inner);
                            out.push('}');
                        }
                    }
                    i += 2 + rel;
                    continue;
                }
            }
            out.push(c);
            i += 1;
        }
        Ok(out)
    }

    /// Evaluate each `with k: expr` value on a divert site against
    /// the current world, then write the result under `k` (a flat
    /// world key — Phase 5 doesn't shadow per-frame). Lets
    /// `-> ask_about with topic: bell` make `topic` readable inside
    /// `ask_about`'s body.
    fn bind_beat_params(
        &mut self,
        params: &indexmap::IndexMap<String, String>,
    ) -> Result<(), PlayError> {
        if params.is_empty() {
            return Ok(());
        }
        self.refresh_lets()?;
        for (name, source) in params {
            let value = match self.eval_expression(source) {
                // A bare path that doesn't resolve in the world
                // (`with topic: bell` — `bell` isn't a variable)
                // falls back to the literal source text so the
                // screenplay reads naturally.
                Ok(Value::Null) => Value::String(source.trim().to_string()),
                Ok(v) => v,
                Err(_) => Value::String(source.trim().to_string()),
            };
            self.world.set(name.clone(), value);
        }
        Ok(())
    }

    /// Parse + evaluate one expression string against the current
    /// world. Ledger queries (`played(…)`, `visits(…)`, `since(…)`)
    /// are resolved through [`crate::ledger::call_query`].
    fn eval_expression(&self, source: &str) -> Result<Value, DirectiveError> {
        let parsed = expr::parse(source).map_err(DirectiveError::from)?;
        let value = expr::eval(&parsed, &self.world, &mut |name, args| {
            ledger::call_query(&self.ledger, name, &args)
        })
        .map_err(DirectiveError::from)?;
        Ok(value)
    }

    /// Re-evaluate every project-level `let` binding against the
    /// current world. Each binding's value is written back to the
    /// world under its name; a `LetEvaluated` envelope hits the
    /// ledger when the value actually changes.
    fn refresh_lets(&mut self) -> Result<(), PlayError> {
        if self.let_bindings.is_empty() {
            return Ok(());
        }
        // Two passes over the let list: enough for one level of
        // dependency chain. Diamond / deep chains will need a
        // proper topological sort, but Phase 4 keeps it simple — a
        // second sweep catches the common `let a = …; let b = a + 1`
        // case without paying for SCC analysis.
        for _pass in 0..2 {
            for slot_idx in 0..self.let_bindings.len() {
                // Snapshot the parsed expression to avoid holding a
                // borrow over `self.world` mutation.
                let parsed = self.let_bindings[slot_idx].expr.clone();
                let value = expr::eval(&parsed, &self.world, &mut |name, args| {
                    ledger::call_query(&self.ledger, name, &args)
                })
                .map_err(DirectiveError::from)?;
                let slot = &mut self.let_bindings[slot_idx];
                let changed = slot.last.as_ref() != Some(&value);
                slot.last = Some(value.clone());
                let name = slot.name.clone();
                self.world.set(name.clone(), value.clone());
                if changed {
                    self.ledger.push(Event::LetEvaluated {
                        name,
                        value: value.display(),
                    });
                }
            }
        }
        Ok(())
    }

    /// Select one of the choices offered by the most recent
    /// [`Step::Choice`].
    pub fn choose(&mut self, index: usize) -> Result<(), PlayError> {
        if !self.awaiting_choice {
            return Err(PlayError::NoChoiceActive);
        }
        let pending = match self.queue.pop_front() {
            Some(Yield::Choice(options)) => options,
            other => {
                // Restore whatever it was; the caller is in a weird
                // state.
                if let Some(o) = other {
                    self.queue.push_front(o);
                }
                return Err(PlayError::NoChoiceActive);
            }
        };
        self.awaiting_choice = false;

        let chosen = pending
            .get(index)
            .ok_or(PlayError::ChoiceOutOfRange(index))?;
        if !chosen.option.sticky {
            self.taken_once_only.insert(chosen.option.text.clone());
        }
        self.ledger.push(Event::ChoiceTaken {
            index,
            text: chosen.option.text.clone(),
        });
        let body = chosen.body.clone();
        // Lower the chosen choice's body to the front of the queue
        // so it runs immediately. Other choices are discarded — this
        // is once-only / sticky semantics at the surface level. (A
        // real run-tracker for spec §5 sticky `+` choices is a Phase
        // 4 add: the choice yield needs to filter against the
        // ledger's `ChoiceTaken` history.)
        let lowered = self.lower_body(&body);
        for y in lowered.into_iter().rev() {
            self.queue.push_front(y);
        }
        Ok(())
    }

    fn resolve(&self, target: &DivertTarget) -> Result<BeatRef, PlayError> {
        let from = self.stack.last().map(|f| f.beat.file).unwrap_or(0);
        Ok(self.bundle.resolve_divert(from, target)?)
    }

    fn enter_beat(&mut self, beat_ref: BeatRef) {
        let beat = self.bundle.beat(beat_ref).clone();
        let file = self.bundle.file(beat_ref.file);
        self.ledger.push(Event::BeatEntered {
            beat: beat.name.clone(),
            file: file.path.clone(),
            reference: Some(beat_ref),
        });
        self.stack.push(Frame {
            beat: beat_ref,
            file_qualifier: file.qualifier.clone(),
        });
        let lowered = self.lower_body(&beat.body);
        for y in lowered.into_iter().rev() {
            self.queue.push_front(y);
        }
    }

    fn halt(&mut self) -> Step {
        if !self.halted {
            self.halted = true;
            self.ledger.push(Event::Ended);
        }
        Step::Ended
    }

    /// Walk a beat body and lower it into a flat `Vec<Yield>`.
    /// Consecutive `Choice` items collapse into one `Yield::Choice`
    /// (one prompt with all options).
    fn lower_body(&self, body: &[BodyItem]) -> Vec<Yield> {
        let mut out = Vec::new();
        let mut i = 0;
        while i < body.len() {
            if matches!(body[i], BodyItem::Choice(_)) {
                let mut group: Vec<PendingChoice> = Vec::new();
                let mut visible_idx = 0usize;
                while let Some(BodyItem::Choice(choice)) = body.get(i) {
                    group.push(lower_choice(choice, visible_idx));
                    visible_idx += 1;
                    i += 1;
                }
                out.push(Yield::Choice(group));
                continue;
            }
            lower_item(&body[i], &mut out);
            i += 1;
        }
        out
    }
}

fn lower_choice(choice: &Choice, index: usize) -> PendingChoice {
    // If the choice carried `[suppressed]`, the visible prompt text
    // is the head; the full played form is `text + suppressed`. We
    // store only the visible text on the option; the suppressed
    // tail is replayed as narration via the choice's body prepend,
    // but Phase-3 keeps it simple and folds the tail into the body
    // as a leading `Action`.
    let prompt_text = choice.text.clone();
    let mut body = choice.body.clone();
    if let Some(tail) = &choice.suppressed {
        let played = format!("{}{}", choice.text, tail);
        body.insert(
            0,
            BodyItem::Action(loom_parser::ast::Located {
                value: played,
                span: choice.span,
            }),
        );
    }
    PendingChoice {
        option: ChoiceOption {
            index,
            text: prompt_text,
            sticky: choice.sticky,
        },
        body,
    }
}

fn lower_item(item: &BodyItem, out: &mut Vec<Yield>) {
    match item {
        BodyItem::SceneHeading(l) => out.push(Yield::Event(Event::Scene {
            text: l.value.clone(),
        })),
        BodyItem::Action(l) => out.push(Yield::Event(Event::Action {
            text: l.value.clone(),
        })),
        BodyItem::Metadata(l) => out.push(Yield::Event(Event::Metadata {
            text: l.value.clone(),
        })),
        BodyItem::Directive(d) => out.push(Yield::Directive(d.clone())),
        BodyItem::Choice(_) => {
            // Choices are grouped one level up in `lower_body`;
            // reaching this arm means the grouping logic missed an
            // edge case. Skip rather than panic so the rest of the
            // beat still plays.
        }
        BodyItem::Divert(d) => lower_divert(d, out),
        BodyItem::Conditional(c) => out.push(Yield::Conditional(c.clone())),
        BodyItem::DirectiveBlock(b) => out.push(Yield::DirectiveBlock(b.clone())),
        BodyItem::Dialogue(block) => {
            let speaker = block.speaker.clone();
            // Phase-3 lowering: emit one Dialogue event per textual
            // line. The opener's `(parenthetical)` rides on the
            // *first* line; subsequent inline `(parenthetical)`
            // lines update the rider for everything that follows
            // until the next override.
            let mut current_paren = block.parenthetical.clone();
            let mut emitted_any = false;
            for line in &block.lines {
                match line {
                    DialogueLine::Text(t) => {
                        out.push(Yield::Event(Event::Dialogue {
                            speaker: speaker.clone(),
                            parenthetical: current_paren.clone(),
                            text: t.value.clone(),
                        }));
                        emitted_any = true;
                    }
                    DialogueLine::Parenthetical(p) => {
                        current_paren = Some(p.value.clone());
                    }
                    DialogueLine::Directive(d) => out.push(Yield::Directive(d.clone())),
                    DialogueLine::Divert(d) => lower_divert(d, out),
                }
            }
            if !emitted_any {
                // Speaker with parenthetical-only body — still emit
                // a Dialogue event with empty text so the booth /
                // performer prompter sees the cue.
                out.push(Yield::Event(Event::Dialogue {
                    speaker,
                    parenthetical: current_paren,
                    text: String::new(),
                }));
            }
        }
    }
}

fn lower_divert(d: &Divert, out: &mut Vec<Yield>) {
    match d {
        Divert::To { target, params, .. } => out.push(Yield::Divert {
            target: target.clone(),
            params: params.clone(),
        }),
        Divert::Tunnel { target, .. } => out.push(Yield::Tunnel {
            target: target.clone(),
            params: indexmap::IndexMap::new(),
        }),
        Divert::Return { .. } => out.push(Yield::Return),
        Divert::End { .. } => out.push(Yield::End),
    }
}

// `Frame::file_qualifier` is reserved for the Phase-4 reactive-scope
// work (locating let-bindings declared file-local) but not read yet.
#[allow(dead_code)]
fn _frame_qualifier_unused(f: &Frame) -> &str {
    &f.file_qualifier
}

/// Recognise `for: x in expr` directive bodies. Returns `(var, expr)`
/// when the syntax matches; `None` otherwise (the caller falls back
/// to generic block-directive dispatch).
fn parse_for_directive(raw: &str) -> Option<(String, String)> {
    let trimmed = raw.trim_start();
    let rest = trimmed.strip_prefix("for:")?;
    let body = rest.trim();
    // Find `in` as a word boundary.
    let bytes = body.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        if &bytes[i..i + 2] == b"in"
            && (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && (i + 2 < bytes.len() && bytes[i + 2].is_ascii_whitespace())
        {
            let var = body[..i].trim().to_string();
            let expr = body[i + 2..].trim().to_string();
            if !var.is_empty() && !expr.is_empty() {
                return Some((var, expr));
            }
            return None;
        }
        i += 1;
    }
    None
}

/// Re-spell a `Value` as expression source so it can ride inside a
/// generated `<set: var = …>` directive. Strings are quoted; numbers
/// / bools / null use their `display()` form; lists fall back to
/// JSON-ish bracketed form.
fn literal_for_value(value: &Value) -> String {
    match value {
        Value::String(s) => format!("'{}'", s.replace('\'', "\\'")),
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(_) => value.display(),
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(literal_for_value).collect();
            format!("[{}]", inner.join(", "))
        }
    }
}

/// Splits `<kind: args>` chunks out of `source`. Quoted strings
/// keep `<` / `>` literal. Returns `(text_without_chunks, raw_chunks)`.
fn split_inline_directives(source: &str) -> (String, Vec<String>) {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len());
    let mut chunks: Vec<String> = Vec::new();
    let mut quote: Option<char> = None;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            out.push(c);
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => {
                quote = Some(c);
                out.push(c);
                i += 1;
            }
            '<' => {
                if let Some(end) = find_matching_angle_close(&source[i..]) {
                    let inner = &source[i + 1..i + end];
                    chunks.push(inner.to_string());
                    i += end + 1;
                    continue;
                }
                out.push(c);
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    (out, chunks)
}

/// Given a slice starting with `<`, find the index of the matching
/// `>` (depth-aware, quote-aware). `None` if unbalanced.
fn find_matching_angle_close(slice: &str) -> Option<usize> {
    let bytes = slice.as_bytes();
    if bytes.first() != Some(&b'<') {
        return None;
    }
    let mut depth = 1i32;
    let mut quote: Option<char> = None;
    let mut i = 1;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

/// Collapse runs of multiple spaces into one and trim trailing
/// whitespace per line — used after stripping inline directive
/// chunks so the dialogue reads naturally.
fn collapse_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last_space = false;
    for ch in text.chars() {
        if ch == ' ' {
            if !last_space {
                out.push(' ');
            }
            last_space = true;
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    // Trim trailing/leading whitespace per overall string.
    out.trim().to_string()
}

fn collect_let_bindings(bundle: &Bundle) -> Vec<LetSlot> {
    let mut slots = Vec::new();
    for entry in &bundle.files {
        for item in &entry.file.items {
            if let Item::LetBinding(binding) = item {
                match expr::parse(&binding.expression) {
                    Ok(parsed) => slots.push(LetSlot {
                        name: binding.name.clone(),
                        expr: parsed,
                        last: None,
                    }),
                    Err(_) => {
                        // A malformed `let` is a parser-level
                        // problem; surface it via diagnostics rather
                        // than failing playback. Phase 4 keeps the
                        // playhead alive; Phase 5's static analysis
                        // pass will reject the bundle.
                    }
                }
            }
        }
    }
    slots
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drain(p: &mut Playhead) -> Vec<Step> {
        let mut steps = Vec::new();
        loop {
            let step = p.step().unwrap();
            let done = matches!(step, Step::Ended) || matches!(step, Step::Choice(_));
            steps.push(step);
            if done {
                break;
            }
        }
        steps
    }

    #[test]
    fn single_beat_plays_to_end() {
        let bundle = Arc::new(Bundle::from_sources([(
            "main.loom",
            "== opening\n\nA bell rope swings.\n\nWREN\n  Hello.\n",
        )]));
        let mut p = Playhead::new(bundle).unwrap();
        let steps = drain(&mut p);
        // Action + Dialogue + Ended.
        assert_eq!(steps.len(), 3);
        assert!(matches!(steps[0], Step::Event(Event::Action { .. })));
        assert!(matches!(steps[1], Step::Event(Event::Dialogue { .. })));
        assert_eq!(steps[2], Step::Ended);
    }

    #[test]
    fn choice_then_divert() {
        let bundle = Arc::new(Bundle::from_sources([
            (
                "main.loom",
                "entry: opening\n\n== opening\n\nIntro.\n\n* Go on.\n  -> next\n",
            ),
            ("beats/next.loom", "== next\n\nDone.\n"),
        ]));
        let mut p = Playhead::new(bundle).unwrap();
        let first = p.step().unwrap();
        assert!(matches!(first, Step::Event(Event::Action { .. })));
        let prompt = p.step().unwrap();
        match &prompt {
            Step::Choice(opts) => assert_eq!(opts.len(), 1),
            _ => panic!("expected choice prompt"),
        }
        p.choose(0).unwrap();
        let mid = p.step().unwrap();
        assert!(matches!(mid, Step::Event(Event::Action { text }) if text == "Done."));
        let end = p.step().unwrap();
        assert_eq!(end, Step::Ended);
    }

    #[test]
    fn end_divert_halts() {
        let bundle = Arc::new(Bundle::from_sources([(
            "main.loom",
            "== opening\n\n* Leave.\n  -> END\n",
        )]));
        let mut p = Playhead::new(bundle).unwrap();
        p.step().unwrap(); // choice prompt
        p.choose(0).unwrap();
        let end = p.step().unwrap();
        assert_eq!(end, Step::Ended);
        assert!(p.halted());
    }
}
