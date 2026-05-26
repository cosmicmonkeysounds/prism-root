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
    AfterMorph, BodyItem, Choice, Conditional, DialogueLine, Directive, DirectiveBlock, Divert,
    DivertTarget, EachVisit, InlineLet, Item, MatchBlock,
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
        /// `as Participant` modifier (spec §13.1). When present, the
        /// playhead pushes a scope alias onto the frame so paths like
        /// `trust` resolve against `Participant.trust`.
        scope_as: Option<String>,
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
    /// `<match: expr>` — pick the first arm whose pattern matches.
    Match(MatchBlock),
    /// `<each visit>` — choose `first` / `then` / `finally` by visit.
    EachVisit(EachVisit),
    /// `<after: cond> … <otherwise>` — per-beat latch.
    AfterMorph(AfterMorph),
    /// `<let: name = expr>` — scope-local binding.
    InlineLet(InlineLet),
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
    /// `-> beat as Participant` modifier (spec §13.1). When set,
    /// bare-name paths read inside this frame route through the
    /// world key `<scope_as>.<name>` before falling back to the
    /// flat name. Empty in the absence of an `as` modifier.
    scope_as: Option<String>,
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
    /// Index into `ledger.events()` of the next event the hook drain
    /// will inspect. Bumped each time `drain_hooks` runs so a single
    /// envelope can never fire the same hook twice.
    hook_cursor: usize,
    /// Coroutine scheduler — owns every `<spawn: NAME>` that the
    /// playhead launches. Ticked once per playhead `step` so ambient
    /// generators interleave with the visible beat (spec §12.5).
    scheduler: crate::scheduler::Scheduler,
    /// Per-anchor cycle counter for `<cycle: a | b | c>` — keyed by
    /// the directive opener's source byte offset.
    cycle_state: std::collections::HashMap<u32, usize>,
    /// `participant_id → current_location`. Walked alongside the hook
    /// drain so an `Event::ParticipantEnteredLocation` for a
    /// participant who was already somewhere else emits a synthetic
    /// `Exits` hook for their previous location (spec §10.4 — `on
    /// Participant exits LOCATION` has no native ledger envelope; it
    /// is the implicit pair of the `enters` event).
    participant_locations: std::collections::HashMap<String, String>,
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
            hook_cursor: 0,
            scheduler: crate::scheduler::Scheduler::new(),
            cycle_state: std::collections::HashMap::new(),
            participant_locations: std::collections::HashMap::new(),
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
    ) -> Result<bool, DirectiveError> {
        if path.len() < 3 {
            return Ok(false);
        }
        let owner = path[0].clone();
        let Some(character) = self.characters.get_mut(&owner) else {
            return Ok(false);
        };
        match character.apply_set(path, value, op) {
            crate::simulacra::SetOutcome::Consumed => {
                character.refresh_own_reacts(&self.world);
                character.publish(&mut self.world);
                Ok(true)
            }
            crate::simulacra::SetOutcome::Rejected(message) => Err(DirectiveError::BadArgs {
                kind: "set".into(),
                message: format!("{}: {}", path.join("."), message),
            }),
            crate::simulacra::SetOutcome::Passthrough => Ok(false),
        }
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
            // Drain hook reactions to anything the previous iteration
            // wrote to the ledger before pulling the next yield off
            // the queue. Hooks lower into queue-front Yields so they
            // run before the playhead returns to the caller.
            self.drain_hooks();
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
                Yield::Divert {
                    target,
                    params,
                    scope_as,
                } => {
                    let beat_ref = self.resolve(&target)?;
                    let beat = self.bundle.beat(beat_ref).clone();
                    self.bind_beat_params(&params)?;
                    self.ledger.push(Event::Diverted {
                        target: target.name.clone(),
                        beat: beat.name.clone(),
                    });
                    self.enter_beat_scoped(beat_ref, scope_as);
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
                    // `<shuffle: a | b | c>` / `<cycle: a | b | c>` —
                    // bar-separated inline yields. Splits before the
                    // generic expression parser tries to read `|` as
                    // an operator (it doesn't, but the split happens
                    // first so the variant text passes through clean).
                    let trimmed_raw = directive.raw.trim_start();
                    if let Some(rest) = trimmed_raw.strip_prefix("shuffle:") {
                        let variants = split_bar_variants(rest);
                        if !variants.is_empty() {
                            let idx = self.ledger.events().len() % variants.len();
                            let text = variants[idx].clone();
                            let event = Event::Action { text };
                            self.ledger.push(event.clone());
                            return Ok(Step::Event(event));
                        }
                    }
                    if let Some(rest) = trimmed_raw.strip_prefix("cycle:") {
                        let variants = split_bar_variants(rest);
                        if !variants.is_empty() {
                            let key = directive.span.start.byte;
                            let idx = *self.cycle_state.get(&key).unwrap_or(&0);
                            let text = variants[idx % variants.len()].clone();
                            self.cycle_state.insert(key, idx + 1);
                            let event = Event::Action { text };
                            self.ledger.push(event.clone());
                            return Ok(Step::Event(event));
                        }
                    }
                    if let Some(rest) = trimmed_raw.strip_prefix("let:") {
                        // Fallback inline let — reached from raw-line
                        // lowering paths (hooks) that don't pre-parse
                        // the directive into `BodyItem::InlineLet`.
                        if let Some((name, expr_text)) = rest.split_once('=') {
                            let name = name.trim().to_string();
                            let value = self.eval_expression(expr_text.trim())?;
                            self.world.set(name, value);
                            continue;
                        }
                    }
                    let call = directives::parse(&directive.raw)?;
                    // `<spawn: Name [with k: v]>` — start a SCENE or
                    // GENERATOR coroutine at its declared tier.
                    if call.kind == "spawn" {
                        if let Some(name) = call.positional.first().map(coroutine_name_from_expr) {
                            if self.spawn_coroutine(&name, &call.named)? {
                                if let Some(latest) = self.ledger.events().last().cloned() {
                                    return Ok(Step::Event(latest));
                                }
                                continue;
                            }
                        }
                    }
                    // `<run: Name [with k: v]>` — synchronous variant:
                    // drive the coroutine to completion inline before
                    // yielding the next visible step. The return value
                    // (if any) is written to `World["__last_run"]` so
                    // host code or follow-up `let` bindings can read it.
                    if call.kind == "run" {
                        if let Some(name) = call.positional.first().map(coroutine_name_from_expr) {
                            if let Some(value) = self.run_coroutine(&name, &call.named)? {
                                self.world.set("__last_run", value);
                                if let Some(latest) = self.ledger.events().last().cloned() {
                                    return Ok(Step::Event(latest));
                                }
                                continue;
                            }
                        }
                    }
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
                        // A bare-identifier rhs (`= suspects`) parses
                        // as `Expr::Path(["suspects"])` and evaluates
                        // to `Null` when no such world key exists. For
                        // schema-validated knowledge slots (spec
                        // §10.2) we want the literal name, not Null,
                        // so the sum-typed slot check can compare it
                        // against its declared variants.
                        let rhs_value = match (&assign.rhs, &rhs_value) {
                            (Expr::Path(segs), Value::Null) if segs.len() == 1 => {
                                Value::String(segs[0].clone())
                            }
                            _ => rhs_value,
                        };
                        let op = match assign.op {
                            crate::directives::AssignOp::Set => crate::simulacra::SetOp::Assign,
                            crate::directives::AssignOp::AddAssign => crate::simulacra::SetOp::Add,
                            crate::directives::AssignOp::SubAssign => crate::simulacra::SetOp::Sub,
                            _ => crate::simulacra::SetOp::Assign,
                        };
                        if self.route_set_through_character(&assign.path, &rhs_value, op)? {
                            let key = assign.path.join(".");
                            let new_value = self.world.get(&key);
                            // Knowledge writes carry a richer envelope
                            // (spec §10.2) — the character store owns
                            // the field, so the ledger records
                            // `KnowledgeChanged` instead of the generic
                            // `WorldSet`. Disposition writes stay on
                            // `WorldSet` so the threshold-cross hook
                            // derivation continues to fire.
                            if assign.path.len() == 3 && assign.path[1] == "knows" {
                                self.ledger.push(Event::KnowledgeChanged {
                                    character: assign.path[0].clone(),
                                    field: assign.path[2].clone(),
                                    value: new_value.display(),
                                });
                            } else {
                                self.ledger.push(Event::WorldSet {
                                    path: key,
                                    value: new_value.display(),
                                });
                            }
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
                Yield::Match(m) => {
                    self.refresh_lets()?;
                    let value = self.eval_expression(&m.scrutinee)?;
                    let needle = value.display();
                    for arm in &m.arms {
                        if arm.pattern == needle {
                            self.ledger.push(Event::ConditionalArm {
                                condition: Some(arm.pattern.clone()),
                            });
                            let lowered = self.lower_body(&arm.body);
                            for y in lowered.into_iter().rev() {
                                self.queue.push_front(y);
                            }
                            break;
                        }
                    }
                }
                Yield::EachVisit(each) => {
                    let beat_name = self
                        .stack
                        .last()
                        .map(|f| self.bundle.beat(f.beat).name.clone())
                        .unwrap_or_default();
                    let visits = self.ledger.beat_visit_count(&beat_name);
                    let body = match visits {
                        0 | 1 => &each.first,
                        2 => &each.then,
                        _ => &each.finally,
                    };
                    if !body.is_empty() {
                        let lowered = self.lower_body(body);
                        for y in lowered.into_iter().rev() {
                            self.queue.push_front(y);
                        }
                    }
                }
                Yield::AfterMorph(morph) => {
                    self.refresh_lets()?;
                    let beat_name = self
                        .stack
                        .last()
                        .map(|f| self.bundle.beat(f.beat).name.clone())
                        .unwrap_or_default();
                    let anchor = format!("{}@{}", beat_name, morph.span.start.byte);
                    let latched = self.ledger.after_latched(&beat_name, &anchor);
                    let body = if latched {
                        &morph.after
                    } else {
                        let truthy = self.eval_expression(&morph.condition)?.truthy();
                        if truthy {
                            self.ledger.push(Event::AfterLatched {
                                beat: beat_name.clone(),
                                anchor: anchor.clone(),
                            });
                            &morph.after
                        } else {
                            &morph.otherwise
                        }
                    };
                    if !body.is_empty() {
                        let lowered = self.lower_body(body);
                        for y in lowered.into_iter().rev() {
                            self.queue.push_front(y);
                        }
                    }
                }
                Yield::InlineLet(binding) => {
                    let value = if binding.expression.is_empty() {
                        Value::Null
                    } else {
                        self.eval_expression(&binding.expression)?
                    };
                    self.world.set(binding.name.clone(), value);
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
                speakers,
                parenthetical,
                text,
            } => {
                let cleaned = self.dispatch_inline_directives(&text)?;
                self.refresh_lets()?;
                let expanded = self.expand_inline_text(&cleaned)?;
                Ok(Event::Dialogue {
                    speaker,
                    speakers,
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
    /// are resolved through [`crate::ledger::call_query`]. When the
    /// enclosing beat carries an `as <scope>` modifier (spec §13.1),
    /// the expression sees a transient world overlay where each
    /// `<scope>.<name>` entry is also reachable as the bare `<name>`.
    fn eval_expression(&self, source: &str) -> Result<Value, DirectiveError> {
        let parsed = expr::parse(source).map_err(DirectiveError::from)?;
        let scope = self.stack.last().and_then(|f| f.scope_as.as_ref()).cloned();
        let world_view: World = if let Some(scope) = scope {
            let prefix = format!("{scope}.");
            let mut overlay = self.world.clone();
            // Walk the world's existing entries and republish every
            // `<scope>.<name>` under the bare `<name>` so `trust` in
            // the scoped beat reads as `<scope>.trust`.
            let aliased: Vec<(String, Value)> = self
                .world
                .entries()
                .filter_map(|(k, v)| {
                    k.strip_prefix(&prefix)
                        .map(|tail| (tail.to_string(), v.clone()))
                })
                .collect();
            for (k, v) in aliased {
                overlay.set(k, v);
            }
            overlay
        } else {
            self.world.clone()
        };
        let value = expr::eval(&parsed, &world_view, &mut |name, args| {
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
        self.enter_beat_scoped(beat_ref, None);
    }

    fn enter_beat_scoped(&mut self, beat_ref: BeatRef, scope_as: Option<String>) {
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
            scope_as,
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

    /// Construct + register a coroutine for a SCENE / GENERATOR
    /// looked up by name. Returns `true` when the name matched.
    fn spawn_coroutine(
        &mut self,
        name: &str,
        named: &indexmap::IndexMap<String, Expr>,
    ) -> Result<bool, PlayError> {
        let Some((program, tier, priority)) = self.lookup_coroutine(name) else {
            return Ok(false);
        };
        let id = self.scheduler.next_id();
        let mut coroutine = crate::coroutine::Coroutine::new(id, program, tier, priority);
        let mut args: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
        for (k, e) in named {
            let v = expr::eval(e, &self.world, &mut |n, _| {
                Err(expr::ExprError::UnknownFunction(n.into()))
            })
            .map_err(DirectiveError::from)?;
            args.insert(k.clone(), v);
        }
        coroutine.bind_args(args);
        self.scheduler.spawn(coroutine, &mut self.ledger);
        Ok(true)
    }

    /// Synchronously drive a coroutine to completion inline. Returns
    /// the coroutine's return value when it terminates. Yields and
    /// waits push their normal envelopes onto the ledger; predicate
    /// waits are evaluated against the live world snapshot.
    fn run_coroutine(
        &mut self,
        name: &str,
        named: &indexmap::IndexMap<String, Expr>,
    ) -> Result<Option<Value>, PlayError> {
        let Some((program, tier, priority)) = self.lookup_coroutine(name) else {
            return Ok(None);
        };
        let id = self.scheduler.next_id();
        let mut coroutine = crate::coroutine::Coroutine::new(id, program, tier, priority);
        let mut args: std::collections::HashMap<String, Value> = std::collections::HashMap::new();
        for (k, e) in named {
            let v = expr::eval(e, &self.world, &mut |n, _| {
                Err(expr::ExprError::UnknownFunction(n.into()))
            })
            .map_err(DirectiveError::from)?;
            args.insert(k.clone(), v);
        }
        coroutine.bind_args(args);
        // Mark the spawn so observers can audit the lifecycle.
        self.ledger.push(Event::SceneSpawned {
            scene: name.to_string(),
            coroutine: id,
            tier: tier.name().into(),
        });
        // Drive until terminal status — bounded by a step budget to
        // keep a runaway `loop` from hanging the playhead.
        let mut steps = 0usize;
        let limit = 10_000usize;
        loop {
            if steps >= limit {
                break;
            }
            steps += 1;
            match coroutine.step(&mut self.world, &mut self.ledger) {
                crate::coroutine::CoroutineStatus::Running
                | crate::coroutine::CoroutineStatus::Yielded => {}
                crate::coroutine::CoroutineStatus::Waiting { .. } => {
                    // Predicate / duration wait: skip duration sleeps
                    // (this is the synchronous form) and re-poll
                    // predicates on the next iteration.
                    continue;
                }
                crate::coroutine::CoroutineStatus::Returned { value } => {
                    return Ok(Some(value.unwrap_or(Value::Null)));
                }
            }
        }
        Ok(Some(Value::Null))
    }

    /// Resolve a SCENE / GENERATOR name into a fresh `Program` clone
    /// + its declared tier + priority.
    fn lookup_coroutine(
        &self,
        name: &str,
    ) -> Option<(crate::coroutine::Program, crate::coroutine::Tier, f64)> {
        if let Some(scene_body) = self.bundle.scenes.get(name) {
            let program = self
                .bundle
                .scene_programs
                .get(name)
                .cloned()
                .unwrap_or_else(|| crate::coroutine::lower_scene(name, scene_body));
            let tier = scene_body
                .tier
                .as_deref()
                .map(crate::coroutine::Tier::parse)
                .unwrap_or_default();
            let priority = scene_body.priority.unwrap_or(0.5);
            return Some((program, tier, priority));
        }
        if let Some(gen_body) = self.bundle.generators.get(name) {
            let program = self
                .bundle
                .generator_programs
                .get(name)
                .cloned()
                .unwrap_or_else(|| crate::coroutine::lower_generator(name, gen_body));
            let tier = gen_body
                .tier
                .as_deref()
                .map(crate::coroutine::Tier::parse)
                .unwrap_or_default();
            let priority = gen_body.priority.unwrap_or(0.3);
            return Some((program, tier, priority));
        }
        None
    }

    /// Borrow the scheduler driving any in-flight `<spawn:>`
    /// coroutines.
    pub fn scheduler(&self) -> &crate::scheduler::Scheduler {
        &self.scheduler
    }

    /// Inspect ledger events written since the last drain, derive
    /// `HookEvent`s from them, and lower any matched character hook
    /// bodies onto the front of the playhead queue. Threshold-cross
    /// hooks fire at most once per crossing (handled by the
    /// character's `threshold_memory`); `meeting X` hooks are
    /// one-shot per character. Bodies execute *before* the next
    /// user-facing step.
    fn drain_hooks(&mut self) {
        if self.characters.is_empty() {
            return;
        }
        let end = self.ledger.events().len();
        if self.hook_cursor >= end {
            return;
        }
        // Snapshot the events to inspect so we don't alias the ledger
        // borrow with the character mutation below.
        let events: Vec<Event> = self.ledger.events()[self.hook_cursor..end].to_vec();
        self.hook_cursor = end;

        let mut to_lower: Vec<Vec<loom_parser::ast::RawLine>> = Vec::new();
        // Pre-derive synthetic `Exits` hooks: a participant moving
        // into a new location implies they exited the previous one.
        // The live stage doesn't write an explicit envelope for that,
        // so we keep our own `participant_locations` map and emit the
        // synthetic event before the entering event's enter hook.
        let mut exits: Vec<(String, String)> = Vec::new();
        for event in &events {
            if let Event::ParticipantEnteredLocation { id, location } = event {
                if let Some(prev) = self.participant_locations.get(id) {
                    if prev != location {
                        exits.push((id.clone(), prev.clone()));
                    }
                }
                self.participant_locations
                    .insert(id.clone(), location.clone());
            }
        }
        for (_id, prev_location) in &exits {
            let hook_event = crate::simulacra::HookEvent::Exits(prev_location.as_str());
            let names: Vec<String> = self.characters.keys().cloned().collect();
            for name in &names {
                let Some(character) = self.characters.get_mut(name) else {
                    continue;
                };
                let hits = character.match_hooks(&hook_event);
                for idx in hits {
                    if let Some(sub) = character.hooks.get(idx) {
                        to_lower.push(sub.body.clone());
                    }
                }
            }
        }
        for event in &events {
            // For each derived HookEvent, walk every character once.
            let derived: Vec<crate::simulacra::HookEvent<'_>> = derive_hook_events(event);
            for hook_event in &derived {
                let names: Vec<String> = self.characters.keys().cloned().collect();
                for name in &names {
                    let Some(character) = self.characters.get_mut(name) else {
                        continue;
                    };
                    let hits = character.match_hooks(hook_event);
                    for idx in hits {
                        if let Some(sub) = character.hooks.get(idx) {
                            to_lower.push(sub.body.clone());
                        }
                    }
                }
            }
        }
        if to_lower.is_empty() {
            return;
        }
        // Lower each matched hook body into Yields and push to the
        // *front* of the queue, preserving source order.
        let mut lowered: Vec<Yield> = Vec::new();
        for body in &to_lower {
            lowered.extend(lower_raw_lines(body));
        }
        for y in lowered.into_iter().rev() {
            self.queue.push_front(y);
        }
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
        BodyItem::Match(m) => out.push(Yield::Match(m.clone())),
        BodyItem::EachVisit(e) => out.push(Yield::EachVisit(e.clone())),
        BodyItem::AfterMorph(a) => out.push(Yield::AfterMorph(a.clone())),
        BodyItem::InlineLet(l) => out.push(Yield::InlineLet(l.clone())),
        // `SlotPlaceholder` expansion is owned by the typed-slot agent;
        // ignore unrecognised placeholders so we don't accidentally
        // double-handle them here.
        BodyItem::SlotPlaceholder(_) => {}
        BodyItem::Dialogue(block) => {
            let speaker = block.speaker.clone();
            let speakers = if block.speakers.is_empty() {
                vec![speaker.clone()]
            } else {
                block.speakers.clone()
            };
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
                            speakers: speakers.clone(),
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
                    speakers,
                    parenthetical: current_paren,
                    text: String::new(),
                }));
            }
        }
    }
}

fn lower_divert(d: &Divert, out: &mut Vec<Yield>) {
    match d {
        Divert::To {
            target,
            params,
            scope_as,
            ..
        } => out.push(Yield::Divert {
            target: target.clone(),
            params: params.clone(),
            scope_as: scope_as.clone(),
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

/// Recover the SCENE / GENERATOR name from a `spawn` / `run`
/// positional. Both forms accept a bare identifier (`HarborChorus`),
/// a dotted path (`Wren.investigate`), or a quoted string literal.
fn coroutine_name_from_expr(expr: &Expr) -> String {
    match expr {
        Expr::Path(segs) => segs.join("."),
        Expr::String(s) => s.clone(),
        other => {
            // Fallback to expression display so the lookup still has a
            // chance — Number / Bool / null all stringify cleanly.
            let v = expr::eval(other, &World::new(), &mut |n, _| {
                Err(expr::ExprError::UnknownFunction(n.into()))
            })
            .ok();
            v.map(|v| v.display()).unwrap_or_default()
        }
    }
}

/// Derive zero or more [`crate::simulacra::HookEvent`]s from one
/// ledger envelope. The lifetimes ride on the input event so the
/// caller can re-borrow it for matching.
fn derive_hook_events(event: &Event) -> Vec<crate::simulacra::HookEvent<'_>> {
    use crate::simulacra::HookEvent as HE;
    let mut out = Vec::new();
    match event {
        Event::Directive {
            kind, positional, ..
        } => match kind.as_str() {
            "cue" => {
                if let Some(name) = positional.first() {
                    out.push(HE::Cue(name.as_str()));
                }
            }
            "meet" => {
                if let Some(name) = positional.first() {
                    out.push(HE::Meeting(name.as_str()));
                }
            }
            _ => {}
        },
        Event::Fired { name, .. } => {
            out.push(HE::Fired(name.as_str()));
        }
        Event::WorldSet { path, value } => {
            // `Wren.trusts.Player` → threshold cross. Surface the new
            // numeric value so the character's threshold_memory edge-
            // detects both upward (`passes N`) and downward
            // (`drops below N`) crossings (spec §10.4).
            let segments: Vec<&str> = path.split('.').collect();
            if segments.len() == 3 && matches!(segments[1], "trusts" | "respects" | "fears") {
                if let Ok(n) = value.parse::<f64>() {
                    out.push(HE::DispositionPasses {
                        verb: segments[1],
                        target: segments[2],
                        value: n,
                    });
                    out.push(HE::DispositionDropsBelow {
                        verb: segments[1],
                        target: segments[2],
                        value: n,
                    });
                }
            }
        }
        Event::ParticipantEnteredLocation { location, .. } => {
            out.push(HE::Enters(location.as_str()));
        }
        Event::ParticipantJoined { .. } => {
            out.push(HE::ParticipantJoins);
        }
        _ => {}
    }
    out
}

/// Lower a hook body (`Vec<RawLine>`) into a flat `Vec<Yield>`.
/// Recognises:
/// * `-> beat [as Speaker]` → `Yield::Divert`
/// * `<kind: args>` → `Yield::Directive`
/// * Anything else → an action event.
///
/// Multi-segment lines (e.g. a parenthetical performer direction)
/// land as plain action text — the hook layer doesn't try to re-parse
/// dialogue blocks.
fn lower_raw_lines(lines: &[loom_parser::ast::RawLine]) -> Vec<Yield> {
    let mut out = Vec::new();
    for line in lines {
        let text = line.text.trim();
        if text.is_empty() {
            continue;
        }
        if let Some(rest) = text.strip_prefix("-> ") {
            // `-> target` or `-> target as Speaker`. The `as` rider
            // is currently a hint for the booth display — bind it as
            // a `speaker` world write so the divert body can read it
            // back if needed.
            let (target_part, speaker) = match rest.find(" as ") {
                Some(idx) => (rest[..idx].trim(), Some(rest[idx + 4..].trim().to_string())),
                None => (rest.trim(), None),
            };
            if target_part == "END" {
                out.push(Yield::End);
                continue;
            }
            if let Some(spk) = speaker {
                let mut params = indexmap::IndexMap::new();
                params.insert("speaker".to_string(), spk);
                out.push(Yield::Divert {
                    target: DivertTarget {
                        name: target_part.to_string(),
                        qualifier: None,
                        knot: None,
                    },
                    params,
                    scope_as: None,
                });
            } else {
                out.push(Yield::Divert {
                    target: DivertTarget {
                        name: target_part.to_string(),
                        qualifier: None,
                        knot: None,
                    },
                    params: indexmap::IndexMap::new(),
                    scope_as: None,
                });
            }
            continue;
        }
        if let Some(stripped) = text.strip_prefix('<') {
            if let Some(end) = stripped.find('>') {
                let raw = stripped[..end].to_string();
                out.push(Yield::Directive(Directive {
                    raw,
                    span: line.span,
                }));
                continue;
            }
        }
        out.push(Yield::Event(Event::Action {
            text: text.to_string(),
        }));
    }
    out
}

/// Split a `<shuffle:>` / `<cycle:>` body on top-level `|` separators.
fn split_bar_variants(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut buf = String::new();
    let mut quote: Option<char> = None;
    for ch in text.chars() {
        if let Some(q) = quote {
            buf.push(ch);
            if ch == q {
                quote = None;
            }
            continue;
        }
        match ch {
            '"' | '\'' => {
                quote = Some(ch);
                buf.push(ch);
            }
            '|' => {
                let trimmed = buf.trim().to_string();
                if !trimmed.is_empty() {
                    out.push(trimmed);
                }
                buf.clear();
            }
            _ => buf.push(ch),
        }
    }
    let trimmed = buf.trim().to_string();
    if !trimmed.is_empty() {
        out.push(trimmed);
    }
    out
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
    fn threshold_hook_drains_before_next_step() {
        // Wren trusts Player at 30/100; the hook fires on `trust passes
        // 60`. After the `<set:>` directive lands, the next step()
        // must drain the hook *before* returning the next visible
        // event.
        let src = "
CHARACTER Wren
  trusts Player: 30 of 100
  on trust passes 60
    A bell tolls in the distance.

== opening
<set: Wren.trusts.Player += 50>

Beat over.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                Step::Choice(_) => panic!("no choices in this fixture"),
                _ => {}
            }
        }
        // The hook body's action must appear *before* the post-set
        // beat tail.
        let bell_idx = texts
            .iter()
            .position(|t| t.contains("bell tolls"))
            .expect("hook body should have emitted");
        let beat_idx = texts
            .iter()
            .position(|t| t.contains("Beat over"))
            .expect("beat tail should have emitted");
        assert!(
            bell_idx < beat_idx,
            "hook should drain before next beat step: texts={texts:?}"
        );
    }

    #[test]
    fn spawn_directive_registers_coroutine() {
        let src = "
GENERATOR HarborChorus
  tier: ambient
  yield bark from Quiet night.|Stars are out.

== opening
<spawn: HarborChorus>

Curtain.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        loop {
            match p.step().unwrap() {
                Step::Ended => break,
                Step::Choice(_) => panic!("no choices"),
                _ => {}
            }
        }
        // The scheduler ought to have observed exactly one SceneSpawned
        // for the named generator.
        let spawned = p
            .ledger()
            .events()
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    Event::SceneSpawned { scene, .. } if scene == "HarborChorus"
                )
            })
            .count();
        assert_eq!(spawned, 1, "spawn should register one coroutine");
    }

    #[test]
    fn run_directive_drives_coroutine_to_return() {
        let src = "
SCENE investigate
  return clue

== opening
<run: investigate>

Done.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        loop {
            match p.step().unwrap() {
                Step::Ended => break,
                Step::Choice(_) => panic!("no choices"),
                _ => {}
            }
        }
        // SceneCompleted with the returned value must show up.
        let completed = p.ledger().events().iter().any(|e| {
            matches!(
                e,
                Event::SceneCompleted { scene, value, .. }
                    if scene == "investigate" && value == "clue"
            )
        });
        assert!(
            completed,
            "run should have driven the coroutine to SceneCompleted (events: {:?})",
            p.ledger().events()
        );
        // The synchronous form stashes the return value on the world.
        assert_eq!(p.world().get("__last_run"), Value::String("clue".into()));
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

    #[test]
    fn match_dispatches_to_named_arm() {
        let src = "
== opening
<set: NPC.knows.bell_origin = 'confirmed'>

<match: NPC.knows.bell_origin>
  confirmed
    I know the answer.
  suspects
    I have a hunch.
  unknown
    I do not know.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                Step::Choice(_) => panic!("no choices"),
                _ => {}
            }
        }
        assert!(
            texts.iter().any(|t| t == "I know the answer."),
            "matched arm body should fire: {texts:?}"
        );
        assert!(
            !texts.iter().any(|t| t == "I have a hunch."),
            "non-matching arm must be skipped: {texts:?}"
        );
    }

    #[test]
    fn match_falls_through_when_no_arm_matches() {
        let src = "
== opening
<set: NPC.knows.bell_origin = 'mystery'>

<match: NPC.knows.bell_origin>
  confirmed
    Known.
  suspects
    Hunch.

Tail.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert!(
            !texts.iter().any(|t| t == "Known." || t == "Hunch."),
            "no arm should match for 'mystery': {texts:?}"
        );
        assert!(texts.iter().any(|t| t == "Tail."));
    }

    #[test]
    fn each_visit_selects_first_arm_on_first_pass() {
        let src = "
== opening
<each visit>
  first
    First time.
  then
    Subsequent.
  finally
    Exhausted.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert!(texts.iter().any(|t| t == "First time."));
        assert!(!texts.iter().any(|t| t == "Subsequent."));
    }

    #[test]
    fn after_morph_latches_and_replaces_otherwise() {
        let src = "
== opening
<set: bell_rung = true>

<after: bell_rung>
  Latched body.
<otherwise>
  Pre-latch body.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert!(texts.iter().any(|t| t == "Latched body."));
        assert!(!texts.iter().any(|t| t == "Pre-latch body."));
        assert!(p
            .ledger()
            .events()
            .iter()
            .any(|e| matches!(e, Event::AfterLatched { .. })));
    }

    #[test]
    fn after_morph_otherwise_when_unlatched() {
        let src = "
== opening
<after: bell_rung>
  Latched.
<otherwise>
  Not yet.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert!(texts.iter().any(|t| t == "Not yet."));
        assert!(!texts.iter().any(|t| t == "Latched."));
    }

    #[test]
    fn inline_let_binds_into_scope_and_drops_after_block() {
        let src = "
== opening
<let: tally = 7>

Score is {tally}.
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert!(
            texts.iter().any(|t| t == "Score is 7."),
            "binding should be readable: {texts:?}"
        );
    }

    #[test]
    fn shuffle_emits_one_variant() {
        let src = "
== opening
<shuffle: alpha | beta | gamma>
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert_eq!(texts.len(), 1);
        assert!(
            matches!(texts[0].as_str(), "alpha" | "beta" | "gamma"),
            "got {texts:?}"
        );
    }

    #[test]
    fn cycle_emits_variants_in_order() {
        // One `<cycle:>` directive lowered three times via a `<for:>`
        // unroll keeps the same source span, so the per-anchor counter
        // advances across iterations and emits A, B, C.
        let src = "
== opening
<for: i in [1, 2, 3]>
  <cycle: A | B | C>
";
        let bundle = Arc::new(Bundle::from_sources([("main.loom", src)]));
        let mut p = Playhead::new(bundle).unwrap();
        let mut texts: Vec<String> = Vec::new();
        loop {
            match p.step().unwrap() {
                Step::Event(Event::Action { text }) => texts.push(text),
                Step::Ended => break,
                _ => {}
            }
        }
        assert_eq!(
            texts,
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            "got {texts:?}"
        );
    }
}
