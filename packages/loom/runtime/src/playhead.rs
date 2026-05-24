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

use loom_parser::ast::{BodyItem, Choice, DialogueLine, Divert, DivertTarget};

use crate::bundle::{BeatRef, Bundle};
use crate::ledger::{ChoiceOption, Event, Ledger};
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

#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
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
}

/// Internal queue entry — the unit the playhead lowers body items
/// into. One `Yield` corresponds to either one visible event or one
/// internal control-flow operation.
#[derive(Clone, Debug)]
enum Yield {
    Event(Event),
    Choice(Vec<PendingChoice>),
    Divert(DivertTarget),
    Tunnel(DivertTarget),
    Return,
    End,
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

#[derive(Debug)]
pub struct Playhead {
    bundle: Arc<Bundle>,
    queue: VecDeque<Yield>,
    stack: Vec<Frame>,
    ledger: Ledger,
    halted: bool,
    awaiting_choice: bool,
}

impl Playhead {
    /// Start a new playhead at the bundle's entry beat.
    pub fn new(bundle: Arc<Bundle>) -> Result<Self, PlayError> {
        let entry = bundle.entry.ok_or(PlayError::NoEntry)?;
        let mut p = Self {
            bundle,
            queue: VecDeque::new(),
            stack: Vec::new(),
            ledger: Ledger::default(),
            halted: false,
            awaiting_choice: false,
        };
        p.enter_beat(entry);
        Ok(p)
    }

    pub fn ledger(&self) -> &Ledger {
        &self.ledger
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
                    self.ledger.push(event.clone());
                    return Ok(Step::Event(event));
                }
                Yield::Choice(options) => {
                    self.awaiting_choice = true;
                    let opts: Vec<ChoiceOption> =
                        options.iter().map(|c| c.option.clone()).collect();
                    // Re-park the choice at the head of the queue so
                    // it survives the pop above.
                    self.queue.push_front(Yield::Choice(options));
                    let prompted = Event::ChoicePrompted {
                        options: opts.clone(),
                    };
                    self.ledger.push(prompted);
                    return Ok(Step::Choice(opts));
                }
                Yield::Divert(target) => {
                    let beat_ref = self.resolve(&target)?;
                    let beat = self.bundle.beat(beat_ref).clone();
                    self.ledger.push(Event::Diverted {
                        target: target.name.clone(),
                        beat: beat.name.clone(),
                    });
                    self.enter_beat(beat_ref);
                }
                Yield::Tunnel(target) => {
                    let beat_ref = self.resolve(&target)?;
                    let beat = self.bundle.beat(beat_ref).clone();
                    self.ledger.push(Event::Tunneled {
                        target: target.name.clone(),
                        beat: beat.name.clone(),
                    });
                    self.enter_beat(beat_ref);
                }
                Yield::Return => {
                    if self.stack.len() < 2 {
                        return Err(PlayError::BareReturn);
                    }
                    self.stack.pop();
                    self.ledger.push(Event::Returned);
                }
                Yield::End => {
                    return Ok(self.halt());
                }
            }
        }
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
        BodyItem::Directive(_) => {
            // Phase-3: directives are recognised but not dispatched.
            // Once the Luau bridge lands, this branch will lower to
            // `Yield::Directive` + a registry lookup.
        }
        BodyItem::Choice(_) => {
            // Choices are grouped one level up in `lower_body`;
            // reaching this arm means the grouping logic missed an
            // edge case. Skip rather than panic so the rest of the
            // beat still plays.
        }
        BodyItem::Divert(d) => lower_divert(d, out),
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
                    DialogueLine::Directive(_) => {}
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
        Divert::To { target, .. } => out.push(Yield::Divert(target.clone())),
        Divert::Tunnel { target, .. } => out.push(Yield::Tunnel(target.clone())),
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
