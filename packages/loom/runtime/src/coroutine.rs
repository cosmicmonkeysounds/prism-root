//! SCENE / GENERATOR coroutines (spec §12.3, §12.4).
//!
//! A coroutine is a small state machine driving one labelled SCENE
//! or top-level GENERATOR. The runtime lowers each declaration's
//! raw body once into a flat [`Vec<Step>`] of opcodes (`Yield`,
//! `WaitUntil`, `WaitDuration`, `Goto`, `Return`, …); the coroutine
//! holds a program counter and a locals map.
//!
//! Coroutines do not preempt the playhead. They tick from the
//! [`crate::scheduler::Scheduler`] in between visible playhead steps.
//! Each [`Coroutine::step`] runs at most one opcode and returns a
//! [`CoroutineStatus`] describing what happened — `Running` while the
//! coroutine still has work, `Yielded` when it emitted ambient
//! content, `Waiting` when it is parked on a predicate or sleep, and
//! `Returned` when it finished.
//!
//! The opcode set is deliberately small and string-based; expressions
//! are kept as raw text so the same scheduler tick that evaluates a
//! `wait until` predicate sees the most recent [`World`].

use std::collections::HashMap;
use std::time::Duration;

use loom_parser::ast::{GeneratorBody, RawLine, SceneBody, SceneState};

use crate::expr::{self, ExprError, Value, World};
use crate::ledger::{Event, Ledger};

/// Lowered SCENE / GENERATOR program. Built once at bundle load
/// time; cloned into every [`Coroutine`] instance the scheduler
/// owns.
#[derive(Clone, Debug, Default)]
pub struct Program {
    pub name: String,
    pub params: Vec<String>,
    pub steps: Vec<Step>,
    /// Labelled state entry points keyed by name (`approach`,
    /// `examine`, …). `""` is the implicit entry.
    pub labels: HashMap<String, usize>,
}

/// One lowered coroutine opcode. Source-line spans are not kept —
/// diagnostics on this layer point back at the declaration body.
#[derive(Clone, Debug)]
pub enum Step {
    /// `yield bark from <list>` — pull one phrase from a `|`-separated
    /// list and surface it as a `GeneratorYielded` event.
    YieldBark { choices: Vec<String> },
    /// `yield with_chance(p) <body-line>` — emit the body if a uniform
    /// random draw lands under `p`. The body is the raw text after the
    /// directive.
    YieldChance { chance: f64, text: String },
    /// `wait until <expr>` — park until the expression evaluates truthy.
    WaitUntil { predicate: String },
    /// `wait <duration>` — park for at least `duration`.
    WaitDuration { duration: Duration },
    /// `-> <state>` — branch the program counter to the labelled
    /// state entry.
    Goto { state: String },
    /// `return [expr]` — exit the coroutine, optionally with a value.
    Return { value: Option<String> },
    /// `loop` — branch the program counter to the previous label
    /// (typically the implicit entry).
    LoopHead { back_to: usize },
    /// `for <var> in <expr>` — open an iteration body. The matching
    /// [`Step::ForEnd`] closes it. Each iteration writes `var` into
    /// the coroutine locals.
    ForBegin {
        var: String,
        source: String,
        end: usize,
    },
    /// Marks the end of a [`Step::ForBegin`] block; jumps back to it
    /// if more items remain.
    ForEnd { begin: usize },
    /// Catch-all narrative opcode — surfaces as a `GeneratorYielded`
    /// event with the raw line text. Used for free-form lines inside
    /// a SCENE / GENERATOR body that don't match a higher-priority
    /// shape (dialogue, etc.).
    EmitLine { text: String },
}

/// What happened when a [`Coroutine::step`] was called.
#[derive(Clone, Debug, PartialEq)]
pub enum CoroutineStatus {
    /// More work remains; call `step` again.
    Running,
    /// The coroutine emitted ambient content this tick.
    Yielded,
    /// The coroutine is parked. `until` describes why — `"until \
    /// <expr>"` or `"<n>ms"`.
    Waiting { until: String },
    /// The coroutine finished. The return value (if any) is the
    /// owner's responsibility to surface.
    Returned { value: Option<Value> },
}

/// One live coroutine instance.
#[derive(Clone, Debug)]
pub struct Coroutine {
    pub id: u64,
    pub program: Program,
    pub locals: HashMap<String, Value>,
    pub pc: usize,
    pub tier: Tier,
    pub priority: f64,
    /// Stack of in-flight `for` loop frames. Each frame stores the
    /// iteration cursor + remaining items.
    for_stack: Vec<ForFrame>,
    /// `true` once `Returned` has been observed once.
    done: bool,
}

#[derive(Clone, Debug)]
struct ForFrame {
    var: String,
    items: Vec<Value>,
    cursor: usize,
    /// Program counter at the matching [`Step::ForBegin`] — kept so
    /// the runtime can surface the loop in diagnostics.
    #[allow(dead_code)]
    begin_pc: usize,
}

/// Scheduling tier (spec §12.5).
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum Tier {
    /// Every frame (~16ms).
    Focal,
    /// Every ~100ms.
    Active,
    /// Every ~2s. Default for top-level generators.
    #[default]
    Ambient,
}

impl Tier {
    pub fn parse(s: &str) -> Tier {
        match s.trim() {
            "focal" => Tier::Focal,
            "active" => Tier::Active,
            _ => Tier::Ambient,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Tier::Focal => "focal",
            Tier::Active => "active",
            Tier::Ambient => "ambient",
        }
    }
}

impl Coroutine {
    /// Construct a new coroutine instance bound to a lowered
    /// [`Program`].
    pub fn new(id: u64, program: Program, tier: Tier, priority: f64) -> Self {
        Self {
            id,
            program,
            locals: HashMap::new(),
            pc: 0,
            tier,
            priority,
            for_stack: Vec::new(),
            done: false,
        }
    }

    /// Bind positional / named arguments into the coroutine's locals.
    pub fn bind_args(&mut self, args: HashMap<String, Value>) {
        for (k, v) in args {
            self.locals.insert(k, v);
        }
    }

    /// Run one opcode. Returns immediately — the scheduler is
    /// responsible for budgeting how many calls happen per tick.
    pub fn step(&mut self, world: &mut World, ledger: &mut Ledger) -> CoroutineStatus {
        if self.done {
            return CoroutineStatus::Returned { value: None };
        }
        let Some(step) = self.program.steps.get(self.pc).cloned() else {
            self.done = true;
            ledger.push(Event::SceneCompleted {
                scene: self.program.name.clone(),
                coroutine: self.id,
                value: String::new(),
            });
            return CoroutineStatus::Returned { value: None };
        };

        match step {
            Step::EmitLine { text } => {
                ledger.push(Event::GeneratorYielded {
                    generator: self.program.name.clone(),
                    coroutine: self.id,
                    text: text.clone(),
                });
                self.pc += 1;
                CoroutineStatus::Yielded
            }
            Step::YieldBark { choices } => {
                let pick = pseudo_pick(self.id, ledger.len(), &choices);
                ledger.push(Event::GeneratorYielded {
                    generator: self.program.name.clone(),
                    coroutine: self.id,
                    text: pick,
                });
                self.pc += 1;
                CoroutineStatus::Yielded
            }
            Step::YieldChance { chance, text } => {
                let draw = pseudo_unit(self.id, ledger.len());
                self.pc += 1;
                if draw < chance {
                    ledger.push(Event::GeneratorYielded {
                        generator: self.program.name.clone(),
                        coroutine: self.id,
                        text,
                    });
                    CoroutineStatus::Yielded
                } else {
                    CoroutineStatus::Running
                }
            }
            Step::WaitUntil { predicate } => {
                if eval_truthy(&predicate, world) {
                    self.pc += 1;
                    CoroutineStatus::Running
                } else {
                    let reason = format!("until {predicate}");
                    ledger.push(Event::CoroutineWaiting {
                        coroutine: self.id,
                        reason: reason.clone(),
                    });
                    CoroutineStatus::Waiting { until: reason }
                }
            }
            Step::WaitDuration { duration } => {
                self.pc += 1;
                let reason = format!("{}ms", duration.as_millis());
                ledger.push(Event::CoroutineWaiting {
                    coroutine: self.id,
                    reason: reason.clone(),
                });
                CoroutineStatus::Waiting { until: reason }
            }
            Step::Goto { state } => {
                if let Some(&target) = self.program.labels.get(&state) {
                    self.pc = target;
                    ledger.push(Event::SceneAdvanced {
                        scene: self.program.name.clone(),
                        coroutine: self.id,
                        state,
                    });
                } else {
                    self.pc += 1;
                }
                CoroutineStatus::Running
            }
            Step::Return { value } => {
                // Try to evaluate the return source as an expression
                // first; if the result is Null (e.g. `return clue`
                // where `clue` is a bare symbol), fall back to the
                // raw source text as a string literal so the symbol
                // round-trips visibly to the caller.
                let resolved = value.as_ref().and_then(|src| {
                    let parsed = expr::parse(src).ok()?;
                    let evaluated = expr::eval(&parsed, world, &mut |n, _| {
                        Err(ExprError::UnknownFunction(n.into()))
                    })
                    .ok()?;
                    Some(match evaluated {
                        Value::Null => Value::String(src.trim().to_string()),
                        other => other,
                    })
                });
                let display = resolved
                    .as_ref()
                    .map(|v| v.display())
                    .or_else(|| value.clone())
                    .unwrap_or_default();
                ledger.push(Event::SceneCompleted {
                    scene: self.program.name.clone(),
                    coroutine: self.id,
                    value: display,
                });
                self.done = true;
                CoroutineStatus::Returned { value: resolved }
            }
            Step::LoopHead { back_to } => {
                self.pc = back_to;
                CoroutineStatus::Running
            }
            Step::ForBegin { var, source, end } => {
                let items: Vec<Value> = match expr::parse(&source) {
                    Ok(parsed) => match expr::eval(&parsed, world, &mut |n, _| {
                        Err(ExprError::UnknownFunction(n.into()))
                    }) {
                        Ok(Value::List(items)) => items,
                        Ok(other) => vec![other],
                        Err(_) => Vec::new(),
                    },
                    Err(_) => Vec::new(),
                };
                if items.is_empty() {
                    self.pc = end + 1;
                    return CoroutineStatus::Running;
                }
                self.locals.insert(var.clone(), items[0].clone());
                self.for_stack.push(ForFrame {
                    var,
                    items,
                    cursor: 0,
                    begin_pc: self.pc,
                });
                self.pc += 1;
                CoroutineStatus::Running
            }
            Step::ForEnd { begin } => {
                if let Some(frame) = self.for_stack.last_mut() {
                    frame.cursor += 1;
                    if frame.cursor < frame.items.len() {
                        let val = frame.items[frame.cursor].clone();
                        let var = frame.var.clone();
                        self.locals.insert(var, val);
                        self.pc = begin + 1;
                    } else {
                        self.for_stack.pop();
                        self.pc += 1;
                    }
                } else {
                    self.pc += 1;
                }
                CoroutineStatus::Running
            }
        }
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

fn eval_truthy(source: &str, world: &World) -> bool {
    match expr::parse(source) {
        Ok(parsed) => match expr::eval(&parsed, world, &mut |n, _| {
            Err(ExprError::UnknownFunction(n.into()))
        }) {
            Ok(value) => value.truthy(),
            Err(_) => false,
        },
        Err(_) => false,
    }
}

/// Deterministic pseudo-random unit draw — keyed on coroutine id +
/// ledger length so tests can be made reproducible without pulling
/// in `rand`.
fn pseudo_unit(seed: u64, salt: usize) -> f64 {
    let mut x = seed.wrapping_mul(2862933555777941757).wrapping_add(salt as u64 + 1);
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51_afd7_ed55_8ccd);
    x ^= x >> 33;
    (x >> 11) as f64 / (1u64 << 53) as f64
}

fn pseudo_pick(seed: u64, salt: usize, choices: &[String]) -> String {
    if choices.is_empty() {
        return String::new();
    }
    let idx = (pseudo_unit(seed, salt) * choices.len() as f64).floor() as usize;
    choices[idx.min(choices.len() - 1)].clone()
}

// ---------------------------------------------------------------------
// Lowering — RawLine bodies → Program
// ---------------------------------------------------------------------

/// Lower a SCENE body into a [`Program`].
pub fn lower_scene(name: &str, scene: &SceneBody) -> Program {
    let mut program = Program {
        name: name.to_string(),
        params: scene.params.clone(),
        ..Program::default()
    };
    // Entry sequence first.
    program.labels.insert(String::new(), 0);
    lower_lines(&mut program, &scene.entry);
    for state in &scene.states {
        let pc = program.steps.len();
        program.labels.insert(state.name.clone(), pc);
        lower_state(&mut program, state);
    }
    program
}

fn lower_state(program: &mut Program, state: &SceneState) {
    lower_lines(program, &state.body);
}

/// Lower a top-level GENERATOR body into a [`Program`].
pub fn lower_generator(name: &str, gen: &GeneratorBody) -> Program {
    let mut program = Program {
        name: name.to_string(),
        ..Program::default()
    };
    program.labels.insert(String::new(), 0);
    lower_lines(&mut program, &gen.body);
    program
}

fn lower_lines(program: &mut Program, lines: &[RawLine]) {
    // `loop` head — when we encounter `loop` followed by an
    // indented body, emit the body then a back-edge.
    let mut i = 0;
    while i < lines.len() {
        let line = &lines[i];
        let text = line.text.trim();
        if text == "loop" {
            let head = program.steps.len();
            // Collect indented body and lower it recursively.
            let body_indent = line.indent;
            let mut inner = Vec::new();
            i += 1;
            while i < lines.len() && lines[i].indent > body_indent {
                inner.push(lines[i].clone());
                i += 1;
            }
            lower_lines(program, &inner);
            program.steps.push(Step::LoopHead { back_to: head });
            continue;
        }
        if let Some(rest) = text.strip_prefix("wait until ") {
            program.steps.push(Step::WaitUntil {
                predicate: rest.trim().to_string(),
            });
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("wait ") {
            match parse_duration(rest.trim()) {
                Some(d) => program.steps.push(Step::WaitDuration { duration: d }),
                None => program.steps.push(Step::WaitUntil {
                    predicate: rest.trim().to_string(),
                }),
            }
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("yield bark from ") {
            let choices = rest
                .trim()
                .split('|')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            program.steps.push(Step::YieldBark { choices });
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("yield with_chance(") {
            if let Some(end) = rest.find(')') {
                let chance: f64 = rest[..end].trim().parse().unwrap_or(0.0);
                let body = rest[end + 1..].trim().to_string();
                program.steps.push(Step::YieldChance { chance, text: body });
                i += 1;
                continue;
            }
        }
        if text == "return" || text.starts_with("return ") {
            let value = text
                .strip_prefix("return")
                .map(|s| s.trim())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string());
            program.steps.push(Step::Return { value });
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("-> ") {
            let label = rest
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(',')
                .to_string();
            program.steps.push(Step::Goto { state: label });
            i += 1;
            continue;
        }
        if let Some(rest) = text.strip_prefix("for ") {
            if let Some(idx) = rest.find(" in ") {
                let var = rest[..idx].trim().to_string();
                let src = rest[idx + 4..].trim().to_string();
                let begin = program.steps.len();
                // Placeholder; backpatch end on close.
                program.steps.push(Step::ForBegin {
                    var,
                    source: src,
                    end: 0,
                });
                let body_indent = line.indent;
                i += 1;
                let mut inner = Vec::new();
                while i < lines.len() && lines[i].indent > body_indent {
                    inner.push(lines[i].clone());
                    i += 1;
                }
                lower_lines(program, &inner);
                let end = program.steps.len();
                program.steps.push(Step::ForEnd { begin });
                if let Some(Step::ForBegin { end: e, .. }) = program.steps.get_mut(begin) {
                    *e = end;
                }
                continue;
            }
        }
        // Fallback — treat as a free-form line that emits as ambient
        // content. Skip blank / comment-style lines.
        if !text.is_empty() && !text.starts_with("//") {
            program.steps.push(Step::EmitLine {
                text: text.to_string(),
            });
        }
        i += 1;
    }
}

fn parse_duration(text: &str) -> Option<Duration> {
    let t = text.trim();
    if let Some(num) = t.strip_suffix("ms") {
        return num.trim().parse::<u64>().ok().map(Duration::from_millis);
    }
    if let Some(num) = t.strip_suffix('s') {
        return num
            .trim()
            .parse::<f64>()
            .ok()
            .map(Duration::from_secs_f64);
    }
    if let Some(num) = t.strip_suffix('m') {
        return num
            .trim()
            .parse::<f64>()
            .ok()
            .map(|m| Duration::from_secs_f64(m * 60.0));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parses() {
        assert_eq!(parse_duration("250ms"), Some(Duration::from_millis(250)));
        assert_eq!(parse_duration("2s"), Some(Duration::from_secs(2)));
        assert_eq!(parse_duration("1m"), Some(Duration::from_secs(60)));
    }

    #[test]
    fn yield_bark_picks_one() {
        let prog = Program {
            name: "G".into(),
            steps: vec![Step::YieldBark {
                choices: vec!["a".into(), "b".into(), "c".into()],
            }],
            ..Program::default()
        };
        let mut co = Coroutine::new(7, prog, Tier::Ambient, 0.5);
        let mut world = World::new();
        let mut ledger = Ledger::default();
        let status = co.step(&mut world, &mut ledger);
        assert_eq!(status, CoroutineStatus::Yielded);
        assert_eq!(ledger.events().len(), 1);
    }

    #[test]
    fn wait_until_parks_then_resumes() {
        let prog = Program {
            name: "S".into(),
            steps: vec![
                Step::WaitUntil {
                    predicate: "ready".into(),
                },
                Step::Return { value: None },
            ],
            ..Program::default()
        };
        let mut co = Coroutine::new(1, prog, Tier::Focal, 1.0);
        let mut world = World::new();
        let mut ledger = Ledger::default();
        // First tick: parks.
        let s = co.step(&mut world, &mut ledger);
        assert!(matches!(s, CoroutineStatus::Waiting { .. }));
        world.set("ready", Value::Bool(true));
        // Next tick: predicate clears, advance.
        let s = co.step(&mut world, &mut ledger);
        assert_eq!(s, CoroutineStatus::Running);
        // Return.
        let s = co.step(&mut world, &mut ledger);
        assert!(matches!(s, CoroutineStatus::Returned { .. }));
    }

    #[test]
    fn goto_jumps_to_labelled_state() {
        let mut labels = HashMap::new();
        labels.insert(String::new(), 0);
        labels.insert("examine".into(), 2);
        let prog = Program {
            name: "investigate".into(),
            steps: vec![
                Step::Goto {
                    state: "examine".into(),
                },
                Step::Return { value: None },
                Step::EmitLine {
                    text: "Hmm. Something's not right.".into(),
                },
                Step::Return {
                    value: Some("clue".into()),
                },
            ],
            labels,
            ..Program::default()
        };
        let mut co = Coroutine::new(2, prog, Tier::Focal, 1.0);
        let mut world = World::new();
        let mut ledger = Ledger::default();
        loop {
            match co.step(&mut world, &mut ledger) {
                CoroutineStatus::Returned { value } => {
                    assert_eq!(value.map(|v| v.display()), Some("clue".into()));
                    break;
                }
                CoroutineStatus::Running | CoroutineStatus::Yielded => {}
                CoroutineStatus::Waiting { .. } => panic!("should not wait"),
            }
        }
    }
}
