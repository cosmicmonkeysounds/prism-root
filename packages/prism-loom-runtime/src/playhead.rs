//! `Playhead` — the cursor that walks a [`Document`]'s sections and
//! yields renderable [`Frame`]s.
//!
//! Phase 2 model:
//!   - Each `step()` returns one [`Frame`] (Dialogue, Flavor, Stage,
//!     Choices, Action, Annotation, End).
//!   - Diverts resolve immediately and the cursor jumps. Returns
//!     (`<-`) pop the stack one level.
//!   - At a `Choice` group, the playhead pauses, returns the visible
//!     options (filtering once-only spent + guarded-false), and waits
//!     for [`Playhead::choose`] before continuing.
//!   - Sections fall through to the next entry in `Document::section_order`
//!     when their items finish without a divert (default behaviour for
//!     conversation flow). Guarded-false sections are skipped during
//!     fall-through.
//!   - Mutation items (`~ $x := v` / `~ fire e`) and inline assigns
//!     surface as `Frame::Mutate` / `Frame::Fire` so the host can apply
//!     them through the same `Show::apply_mutation` lane the booth
//!     uses. The `Show` wrapper is the one that actually drives those
//!     applies — the [`Playhead`] is intentionally evaluator-free.
//!   - `each visit` / `after` / `match` blocks resolve at frame time
//!     into a chosen branch (or skip silently when no branch matches).

use crate::bundle::{Document, Item, Mutation, TextSeg};
use crate::ledger::{Ledger, LedgerEntry};
use crate::resolver::{evaluate, Expr, ResolverContext};
use crate::value::Value;

/// What the renderer sees on each step. `lines` / `text` are the
/// rendered strings after interpolation against the [`ResolverContext`]
/// the host supplies via [`Playhead::step`].
#[derive(Debug, Clone, PartialEq)]
pub enum Frame {
    /// Speaker line(s). `lines` is the concatenated body — one
    /// renderable bubble's worth of text.
    Dialogue {
        speaker: String,
        attrs: Vec<(String, String)>,
        lines: Vec<String>,
    },
    /// `> ...` flavour / narrator prose.
    Flavor { text: String },
    /// Stage direction / unstructured prose at content position.
    Stage { text: String },
    /// `~ keyword payload` — registry-driven side effect. The
    /// playhead surfaces it so the host can dispatch; it does not
    /// execute the action itself.
    Action { keyword: String, payload: String },
    /// `@name body` — registry-driven annotation marker.
    Annotation { name: String, body: String },
    /// A captured mutation the host should apply through
    /// [`super::show::Show::apply_mutation`]. Returned both for
    /// `~ $x := v` action lines AND for inline `<$x := v>` assigns
    /// fired during text rendering.
    Mutate(Mutation),
    /// `~ fire <event>` action line.
    Fire { event: String },
    /// A set of choices the player must pick from. Once returned, the
    /// playhead waits for [`Playhead::choose`].
    Choices(Vec<ChoiceFrame>),
    /// The show reached an explicit terminal state (no more sections).
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChoiceFrame {
    /// Index in the original choices list — pass this back to
    /// [`Playhead::choose`].
    pub index: usize,
    pub label: String,
    pub once: bool,
}

/// State machine for the playhead. The cursor stack lets us descend
/// into a choice's body and pop back out when it finishes.
#[derive(Debug, Clone)]
pub struct Playhead {
    /// Stack of (section_id, items_ref, cursor) frames. The top of
    /// the stack is the currently executing frame; popping resumes
    /// the parent at the position after the descent point.
    stack: Vec<StackFrame>,
    /// `true` when we're currently waiting for a [`Playhead::choose`]
    /// to advance past the last-emitted Choices frame.
    pending_choices: Option<Vec<usize>>,
    /// Section ids in document source order — drives fall-through.
    section_order: Vec<String>,
    /// Once-only choice ids that have been spent in this run.
    taken_choices: std::collections::HashSet<String>,
    /// `true` after `step()` has emitted a `Frame::End`. The next
    /// `step()` call returns `None` so loops drive cleanly to a halt.
    end_emitted: bool,
    /// Inline `<$x := v>` assigns triggered during the most-recent
    /// text-rendering pass. The [`Show`] drains and applies them after
    /// the current frame returns so the host can replay deterministically.
    pending_assigns: Vec<Mutation>,
}

#[derive(Debug, Clone)]
struct StackFrame {
    /// Section id of the *root* section that owns these items —
    /// nested choice bodies inherit their enclosing section's id.
    section_id: String,
    /// Path key for the once-only choice tracker.
    item_path: String,
    /// Pointer into a flat clone of the items for this frame.
    items: Vec<Item>,
    cursor: usize,
}

impl Playhead {
    /// Start a playhead at the document's first section (per the
    /// `section_order` index — anonymous sections included).
    pub fn new(doc: &Document) -> Self {
        let mut play = Self {
            stack: Vec::new(),
            pending_choices: None,
            section_order: doc.section_order.clone(),
            taken_choices: Default::default(),
            end_emitted: false,
            pending_assigns: Vec::new(),
        };
        if let Some(first) = doc.section_order.first().cloned() {
            play.push_section(doc, &first);
        }
        play
    }

    /// Drain any inline assigns triggered by the last frame's text
    /// render. The host (typically [`super::show::Show`]) calls this
    /// after consuming the frame and applies each mutation through the
    /// same lane as explicit `~ $x := v` actions.
    pub fn take_pending_assigns(&mut self) -> Vec<Mutation> {
        std::mem::take(&mut self.pending_assigns)
    }

    /// Jump directly to a named section. Used by diverts and by the
    /// host to start a show at an explicit entry point.
    pub fn jump_to(&mut self, doc: &Document, section_id: &str) {
        self.stack.clear();
        self.end_emitted = false;
        self.push_section(doc, section_id);
    }

    fn push_section(&mut self, doc: &Document, section_id: &str) {
        if let Some(section) = doc.sections.get(section_id) {
            self.stack.push(StackFrame {
                section_id: section.id.clone(),
                item_path: section.id.clone(),
                items: section.items.clone(),
                cursor: 0,
            });
        }
    }

    /// Advance one step. Returns the frame the renderer should display
    /// next, or `None` if the show is at `End`.
    ///
    /// `ctx` is the resolver context (`var` / `let` / `roles` / ledger)
    /// used to evaluate guards and interpolate inline text. The host
    /// (typically [`super::show::Show`]) builds it once per step and
    /// passes it in.
    pub fn step(
        &mut self,
        doc: &Document,
        ledger: &mut Ledger,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) -> Option<Frame> {
        if self.pending_choices.is_some() {
            return None;
        }
        if self.end_emitted {
            return None;
        }
        loop {
            let Some(frame_idx) = self.stack.len().checked_sub(1) else {
                self.end_emitted = true;
                return Some(Frame::End);
            };
            let cursor = self.stack[frame_idx].cursor;
            if cursor == 0 && self.stack[frame_idx].item_path == self.stack[frame_idx].section_id {
                let section_id = self.stack[frame_idx].section_id.clone();
                ledger.push(LedgerEntry::Visited {
                    section: section_id,
                    at_ms: now_ms,
                });
            }
            let item = self.stack[frame_idx].items.get(cursor).cloned();
            match item {
                None => {
                    let was_root = frame_idx == 0;
                    let popped_section_id = self.stack[frame_idx].section_id.clone();
                    self.stack.pop();
                    if was_root {
                        if let Some(next_id) =
                            self.next_enterable_section(doc, &popped_section_id, ctx, now_ms)
                        {
                            self.push_section(doc, &next_id);
                            continue;
                        }
                        self.end_emitted = true;
                        return Some(Frame::End);
                    }
                    continue;
                }
                Some(item) => {
                    self.stack[frame_idx].cursor += 1;
                    if let Some(frame) = self.handle_item(doc, frame_idx, item, ledger, ctx, now_ms)
                    {
                        return Some(frame);
                    }
                }
            }
        }
    }

    /// Pick the next section in source order that either has no guard
    /// or whose guard evaluates truthy under `ctx`. Returns `None` when
    /// the source-order tail is empty.
    fn next_enterable_section(
        &self,
        doc: &Document,
        from: &str,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) -> Option<String> {
        let mut iter = self.section_order.iter();
        for id in iter.by_ref() {
            if id == from {
                break;
            }
        }
        for id in iter {
            let pass = match doc.sections.get(id).and_then(|s| s.guard.as_ref()) {
                Some(g) => evaluate(g, ctx, now_ms).is_truthy(),
                None => true,
            };
            if pass {
                return Some(id.clone());
            }
        }
        None
    }

    fn handle_item(
        &mut self,
        doc: &Document,
        frame_idx: usize,
        item: Item,
        ledger: &mut Ledger,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) -> Option<Frame> {
        match item {
            Item::Dialogue {
                speaker,
                attrs,
                lines,
            } => {
                let rendered_lines: Vec<String> = lines
                    .iter()
                    .map(|line| render_segments(line, ctx, now_ms, &mut self.pending_assigns))
                    .collect();
                Some(Frame::Dialogue {
                    speaker,
                    attrs,
                    lines: rendered_lines,
                })
            }
            Item::Flavor { text } => Some(Frame::Flavor {
                text: render_segments(&text, ctx, now_ms, &mut self.pending_assigns),
            }),
            Item::Stage { text } => Some(Frame::Stage {
                text: render_segments(&text, ctx, now_ms, &mut self.pending_assigns),
            }),
            Item::Action { keyword, payload } => Some(Frame::Action { keyword, payload }),
            Item::Annotation { name, body } => Some(Frame::Annotation { name, body }),
            Item::Mutate(m) => Some(Frame::Mutate(m)),
            Item::Fire { event } => Some(Frame::Fire { event }),
            Item::Divert { target } => {
                let bare = target.trim_start_matches('@');
                let key = bare.split('.').next().unwrap_or(bare).to_string();
                self.jump_to(doc, &key);
                None
            }
            Item::Return { .. } => {
                if frame_idx > 0 {
                    self.stack.pop();
                }
                None
            }
            Item::Choice { .. } => {
                let (options, indices) =
                    self.collect_choice_run(frame_idx, true, &item, ctx, now_ms);
                if options.is_empty() {
                    return None;
                }
                self.pending_choices = Some(indices);
                Some(Frame::Choices(options))
            }
            Item::Conditional { cond, body } => {
                if evaluate(&cond, ctx, now_ms).is_truthy() {
                    self.push_inline_body(frame_idx, body);
                }
                None
            }
            Item::After {
                cond,
                body_if,
                body_else,
            } => {
                let pick = if evaluate(&cond, ctx, now_ms).is_truthy() {
                    body_if
                } else {
                    body_else
                };
                if !pick.is_empty() {
                    self.push_inline_body(frame_idx, pick);
                }
                None
            }
            Item::Match { scrutinee, arms } => {
                let scrut = evaluate(&scrutinee, ctx, now_ms);
                let key = stringify_value(&scrut);
                let chosen = arms
                    .into_iter()
                    .find(|(k, _)| k == &key || k == "_")
                    .map(|(_, body)| body);
                if let Some(body) = chosen {
                    if !body.is_empty() {
                        self.push_inline_body(frame_idx, body);
                    }
                }
                None
            }
            Item::EachVisit { branches } => {
                let visits = ledger.visits(&self.stack[frame_idx].section_id);
                let pick = pick_visit_branch(&branches, visits);
                if let Some(body) = pick {
                    if !body.is_empty() {
                        self.push_inline_body(frame_idx, body);
                    }
                }
                None
            }
            Item::Other { node_kind, text } => {
                if text.is_empty() {
                    None
                } else {
                    Some(Frame::Stage {
                        text: format!("[{node_kind}] {text}"),
                    })
                }
            }
        }
    }

    /// Push a body of items onto the stack as a child frame — the
    /// playhead resumes the parent after the child completes.
    fn push_inline_body(&mut self, frame_idx: usize, body: Vec<Item>) {
        let section_id = self.stack[frame_idx].section_id.clone();
        let item_path = format!(
            "{}#child{}",
            self.stack[frame_idx].item_path,
            self.stack[frame_idx].cursor - 1
        );
        self.stack.push(StackFrame {
            section_id,
            item_path,
            items: body,
            cursor: 0,
        });
    }

    fn collect_choice_run(
        &mut self,
        frame_idx: usize,
        include_current: bool,
        first: &Item,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) -> (Vec<ChoiceFrame>, Vec<usize>) {
        let mut options = Vec::new();
        let mut indices = Vec::new();

        if include_current {
            if let Item::Choice {
                once, label, guard, ..
            } = first
            {
                if self.choice_visible(
                    frame_idx,
                    self.stack[frame_idx].cursor - 1,
                    *once,
                    guard,
                    ctx,
                    now_ms,
                ) {
                    indices.push(self.stack[frame_idx].cursor - 1);
                    let mut assigns_sink = Vec::new();
                    options.push(ChoiceFrame {
                        index: options.len(),
                        label: render_segments(label, ctx, now_ms, &mut assigns_sink),
                        once: *once,
                    });
                    // Choice labels can't fire assigns — drop the sink.
                }
            }
        }

        loop {
            let cursor = self.stack[frame_idx].cursor;
            let next_clone = self.stack[frame_idx].items.get(cursor).cloned();
            let Some(next) = next_clone else { break };
            let Item::Choice {
                once, label, guard, ..
            } = &next
            else {
                break;
            };
            if !self.choice_visible(frame_idx, cursor, *once, guard, ctx, now_ms) {
                self.stack[frame_idx].cursor += 1;
                continue;
            }
            indices.push(cursor);
            let mut assigns_sink = Vec::new();
            options.push(ChoiceFrame {
                index: options.len(),
                label: render_segments(label, ctx, now_ms, &mut assigns_sink),
                once: *once,
            });
            self.stack[frame_idx].cursor += 1;
        }
        (options, indices)
    }

    fn choice_visible(
        &self,
        frame_idx: usize,
        cursor: usize,
        once: bool,
        guard: &Option<Expr>,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) -> bool {
        if once {
            let path = self.choice_path(frame_idx, cursor);
            if self.taken_choices.contains(&path) {
                return false;
            }
        }
        if let Some(g) = guard {
            if !evaluate(g, ctx, now_ms).is_truthy() {
                return false;
            }
        }
        true
    }

    fn choice_path(&self, frame_idx: usize, item_idx: usize) -> String {
        format!("{}#{}", self.stack[frame_idx].item_path, item_idx)
    }

    /// Select one of the choices from the last `Choices` frame. The
    /// `option_idx` is the 0-based index into the *visible* options
    /// vector — `once` choices already spent are filtered out, so the
    /// indices the host sees stay packed.
    pub fn choose(
        &mut self,
        doc: &Document,
        option_idx: usize,
        ledger: &mut Ledger,
        ctx: &ResolverContext<'_>,
        now_ms: u64,
    ) {
        let Some(indices) = self.pending_choices.take() else {
            return;
        };
        let Some(&item_idx) = indices.get(option_idx) else {
            return;
        };
        let Some(top) = self.stack.last_mut() else {
            return;
        };
        let Some(item) = top.items.get(item_idx).cloned() else {
            return;
        };
        let Item::Choice {
            once, label, body, ..
        } = item
        else {
            return;
        };

        let path = format!("{}#{}", top.item_path, item_idx);
        if once {
            self.taken_choices.insert(path.clone());
        }

        let section_id = top.section_id.clone();
        let mut assigns_sink = Vec::new();
        let rendered_label = render_segments(&label, ctx, now_ms, &mut assigns_sink);
        ledger.push(LedgerEntry::Chose {
            section: section_id.clone(),
            index: item_idx,
            label: rendered_label,
            at_ms: now_ms,
        });

        self.stack.push(StackFrame {
            section_id,
            item_path: path,
            items: body,
            cursor: 0,
        });
        let _ = doc;
    }

    pub fn is_awaiting_choice(&self) -> bool {
        self.pending_choices.is_some()
    }

    pub fn is_at_end(&self) -> bool {
        self.stack.is_empty() && self.pending_choices.is_none()
    }
}

/// Concatenate a slice of [`TextSeg`]s into one rendered string,
/// evaluating `Resolve` segments through `ctx` and queueing inline
/// assigns onto `assigns` so the host can apply them after the frame
/// is consumed.
pub(crate) fn render_segments(
    segs: &[TextSeg],
    ctx: &ResolverContext<'_>,
    now_ms: u64,
    assigns: &mut Vec<Mutation>,
) -> String {
    let mut out = String::new();
    for seg in segs {
        match seg {
            TextSeg::Literal(s) => append_with_space(&mut out, s),
            TextSeg::Resolve(expr) => {
                let v = evaluate(expr, ctx, now_ms);
                append_with_space(&mut out, &stringify_value(&v));
            }
            TextSeg::StaticRef(name) => append_with_space(&mut out, &format!("@{name}")),
            TextSeg::Backlink { display, .. } => append_with_space(&mut out, display),
            TextSeg::Assign(m) => assigns.push(m.clone()),
        }
    }
    out
}

fn append_with_space(out: &mut String, s: &str) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return;
    }
    if !out.is_empty() && !out.ends_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(trimmed);
}

pub(crate) fn stringify_value(v: &Value) -> String {
    match v {
        Value::Nil => "nil".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => format_float(*f),
        Value::Str(s) => s.clone(),
        Value::List(_) | Value::Map(_) => format!("{v:?}"),
    }
}

fn format_float(f: f64) -> String {
    if f.fract() == 0.0 && f.is_finite() {
        format!("{f:.0}")
    } else {
        f.to_string()
    }
}

/// Pick the body of the [`VisitBranch`] that matches the current visit
/// count. `visits` is 1 on the very first entry to the section
/// (the playhead writes the `Visited` ledger entry at cursor 0, so by
/// the time this runs the count is already incremented).
fn pick_visit_branch(branches: &[crate::bundle::VisitBranch], visits: u32) -> Option<Vec<Item>> {
    if branches.is_empty() {
        return None;
    }
    // Convention from the design: `first` on visit 1, `finally` on the
    // last branch in the list (only on visits ≥ branches.len() - 1 if
    // `finally` is present), `then` for everything in between.
    let has_finally = branches.iter().any(|b| b.kind == "finally");
    let then_branches: Vec<&crate::bundle::VisitBranch> =
        branches.iter().filter(|b| b.kind == "then").collect();
    let first_branch = branches.iter().find(|b| b.kind == "first");
    let finally_branch = branches.iter().find(|b| b.kind == "finally");

    if visits <= 1 {
        if let Some(b) = first_branch {
            return Some(b.body.clone());
        }
    }
    // Visits 2..=2 + then_branches.len() cycle through `then`.
    if !then_branches.is_empty() {
        let then_visit = if first_branch.is_some() {
            visits.saturating_sub(2) as usize
        } else {
            visits.saturating_sub(1) as usize
        };
        if then_visit < then_branches.len() || !has_finally {
            let idx = if then_branches.is_empty() {
                0
            } else {
                then_visit.min(then_branches.len() - 1)
            };
            return Some(then_branches[idx].body.clone());
        }
    }
    if let Some(b) = finally_branch {
        return Some(b.body.clone());
    }
    // Fallback — return the first branch's body.
    Some(branches[0].body.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{compile, LoomDatabase};
    use prism_core::language::loom::parser::parse;
    use std::collections::HashMap;

    fn boot(src: &str) -> (LoomDatabase, Ledger) {
        (compile(&parse(src).root), Ledger::new())
    }

    fn empty_ctx<'a>(
        lets: &'a HashMap<String, Value>,
        vars: &'a HashMap<String, Value>,
        roles: &'a HashMap<String, Value>,
        ledger: &'a Ledger,
    ) -> ResolverContext<'a> {
        ResolverContext {
            lets,
            vars,
            roles,
            ledger,
        }
    }

    fn frames_until_choice_or_end(
        play: &mut Playhead,
        doc: &Document,
        ledger: &mut Ledger,
    ) -> Vec<Frame> {
        let mut frames = Vec::new();
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        for _ in 0..50 {
            // Build ctx per iteration since the ledger may have grown.
            let snapshot_ledger = ledger.clone();
            let ctx = empty_ctx(&lets, &vars, &roles, &snapshot_ledger);
            let Some(frame) = play.step(doc, ledger, &ctx, 0) else {
                break;
            };
            let done = matches!(frame, Frame::End | Frame::Choices(_));
            frames.push(frame);
            if done {
                break;
            }
        }
        frames
    }

    #[test]
    fn plays_through_simple_dialogue() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hello there.\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let frames = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        assert!(matches!(frames[0], Frame::Dialogue { ref speaker, .. } if speaker == "WREN"));
        assert!(matches!(frames.last(), Some(Frame::End)));
    }

    #[test]
    fn pauses_at_choice_and_advances_on_pick() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hi.\n  * Help. -> next\n  * Leave. -> done\n-- next\nWREN\n  Thanks.\n-- done\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let frames = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        let last = frames.last().expect("at least one frame");
        let choices = match last {
            Frame::Choices(opts) => opts.clone(),
            other => panic!("expected choices, got {other:?}"),
        };
        assert_eq!(choices.len(), 2);
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let snap = ledger.clone();
        let ctx = empty_ctx(&lets, &vars, &roles, &snap);
        play.choose(doc, 0, &mut ledger, &ctx, 0);
        let snap = ledger.clone();
        let ctx = empty_ctx(&lets, &vars, &roles, &snap);
        let next = play.step(doc, &mut ledger, &ctx, 0).expect("post-choose");
        assert!(matches!(next, Frame::Dialogue { .. }));
    }

    #[test]
    fn ledger_records_visited_and_chose() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hi.\n  * help -> next\n-- next\nWREN\n  Thanks.\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let _ = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let snap = ledger.clone();
        let ctx = empty_ctx(&lets, &vars, &roles, &snap);
        play.choose(doc, 0, &mut ledger, &ctx, 10);
        for _ in 0..10 {
            let snap = ledger.clone();
            let ctx = empty_ctx(&lets, &vars, &roles, &snap);
            if play.step(doc, &mut ledger, &ctx, 20).is_none() {
                break;
            }
        }
        assert!(ledger.played("start"));
        assert!(ledger.played("next"));
        assert!(ledger.chose("start"));
    }

    #[test]
    fn once_choices_dont_reappear_after_taken() {
        let src = "# d\ncast WREN\n  .label x\n-- hub\nWREN\n  Pick.\n  * one\n    -> hub\n  + two\n    -> hub\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let _ = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let snap = ledger.clone();
        let ctx = empty_ctx(&lets, &vars, &roles, &snap);
        play.choose(doc, 0, &mut ledger, &ctx, 0);
        let mut next_choices: Option<Vec<ChoiceFrame>> = None;
        for _ in 0..20 {
            let snap = ledger.clone();
            let ctx = empty_ctx(&lets, &vars, &roles, &snap);
            match play.step(doc, &mut ledger, &ctx, 0) {
                Some(Frame::Choices(opts)) => {
                    next_choices = Some(opts);
                    break;
                }
                Some(_) => continue,
                None => break,
            }
        }
        let opts = next_choices.expect("choices to reappear");
        assert_eq!(opts.len(), 1);
        assert_eq!(opts[0].label, "two");
    }

    #[test]
    fn fall_through_to_next_section() {
        let src = "# d\ncast WREN\n  .label x\n-- one\nWREN\n  A.\n-- two\nWREN\n  B.\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let frames = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        let dialogue_count = frames
            .iter()
            .filter(|f| matches!(f, Frame::Dialogue { .. }))
            .count();
        assert_eq!(dialogue_count, 2);
    }

    #[test]
    fn guarded_choice_hidden_when_false() {
        let src = "# d\ncast WREN\n  .label x\n-- s\nWREN\n  Pick.\n  * Always.\n    -> done\n  * Only if trusted. if $trusted\n    -> done\n-- done\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let frames = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        let choices = match frames.last() {
            Some(Frame::Choices(c)) => c.clone(),
            other => panic!("expected choices, got {other:?}"),
        };
        assert_eq!(choices.len(), 1);
        assert!(choices[0].label.contains("Always"));
    }

    #[test]
    fn guarded_choice_visible_when_true() {
        let src = "# d\ncast WREN\n  .label x\n-- s\nWREN\n  Pick.\n  * Only if trusted. if $trusted\n    -> done\n-- done\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let mut lets = HashMap::new();
        let mut vars = HashMap::new();
        let roles = HashMap::new();
        vars.insert("trusted".to_string(), Value::Bool(true));
        // Walk explicitly so we can install the var.
        for _ in 0..20 {
            let snap = ledger.clone();
            let ctx = ResolverContext {
                lets: &lets,
                vars: &vars,
                roles: &roles,
                ledger: &snap,
            };
            let Some(frame) = play.step(doc, &mut ledger, &ctx, 0) else {
                break;
            };
            if let Frame::Choices(c) = frame {
                assert_eq!(c.len(), 1);
                return;
            }
            let _ = &mut lets;
        }
        panic!("expected a Choices frame");
    }

    #[test]
    fn after_block_picks_if_branch() {
        let src = "# d\n-- s\nafter $trusted\n  > yes\notherwise\n  > no\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let lets = HashMap::new();
        let mut vars = HashMap::new();
        vars.insert("trusted".to_string(), Value::Bool(true));
        let roles = HashMap::new();
        let mut text = String::new();
        for _ in 0..20 {
            let snap = ledger.clone();
            let ctx = ResolverContext {
                lets: &lets,
                vars: &vars,
                roles: &roles,
                ledger: &snap,
            };
            let Some(frame) = play.step(doc, &mut ledger, &ctx, 0) else {
                break;
            };
            if let Frame::Flavor { text: t } = frame {
                text = t;
                break;
            }
        }
        assert_eq!(text, "yes");
    }

    #[test]
    fn after_block_picks_else_branch() {
        let src = "# d\n-- s\nafter $trusted\n  > yes\notherwise\n  > no\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let lets = HashMap::new();
        let vars = HashMap::new();
        let roles = HashMap::new();
        let mut text = String::new();
        for _ in 0..20 {
            let snap = ledger.clone();
            let ctx = ResolverContext {
                lets: &lets,
                vars: &vars,
                roles: &roles,
                ledger: &snap,
            };
            let Some(frame) = play.step(doc, &mut ledger, &ctx, 0) else {
                break;
            };
            if let Frame::Flavor { text: t } = frame {
                text = t;
                break;
            }
        }
        assert_eq!(text, "no");
    }
}
