//! `Playhead` — the cursor that walks a [`Document`]'s sections and
//! yields renderable [`Frame`]s.
//!
//! Phase 1 model:
//!   - Each `step()` returns one [`Frame`] (Dialogue, Flavor, Stage,
//!     Choices, End).
//!   - Diverts resolve immediately and the cursor jumps. Returns
//!     (`<-`) pop a tunnel stack (deferred to Phase 2 — for now the
//!     playhead just falls through).
//!   - At a `Choice` group, the playhead pauses, returns the visible
//!     options, and waits for [`Playhead::choose`] before continuing.
//!   - Sections fall through to the next entry in `Document::section_order`
//!     when their items finish without a divert (default behaviour for
//!     conversation flow).

use crate::bundle::{Document, Item};
use crate::ledger::{Ledger, LedgerEntry};

/// What the renderer sees on each step.
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
        };
        if let Some(first) = doc.section_order.first().cloned() {
            play.push_section(doc, &first);
        }
        play
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
    pub fn step(&mut self, doc: &Document, ledger: &mut Ledger, now_ms: u64) -> Option<Frame> {
        if self.pending_choices.is_some() {
            // Caller must call `choose(...)` first.
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
            // Snapshot whether we just entered this frame's section.
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
                    // End of this frame's items — pop. If this was the
                    // top-level section, fall through to the next one
                    // in source order.
                    let was_root = frame_idx == 0;
                    let popped_section_id = self.stack[frame_idx].section_id.clone();
                    self.stack.pop();
                    if was_root {
                        if let Some(next_id) = self.next_section_in_order(&popped_section_id) {
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
                    if let Some(frame) =
                        self.handle_item(doc, frame_idx, item, ledger, now_ms)
                    {
                        return Some(frame);
                    }
                    // Item produced no Frame (e.g. Return, processed
                    // Divert) — loop and consume the next.
                }
            }
        }
    }

    fn handle_item(
        &mut self,
        doc: &Document,
        frame_idx: usize,
        item: Item,
        _ledger: &mut Ledger,
        _now_ms: u64,
    ) -> Option<Frame> {
        match item {
            Item::Dialogue {
                speaker,
                attrs,
                lines,
            } => Some(Frame::Dialogue {
                speaker,
                attrs,
                lines,
            }),
            Item::Flavor { text } => Some(Frame::Flavor { text }),
            Item::Stage { text } => Some(Frame::Stage { text }),
            Item::Action { keyword, payload } => Some(Frame::Action { keyword, payload }),
            Item::Annotation { name, body } => Some(Frame::Annotation { name, body }),
            Item::Divert { target } => {
                // Cross-document `@doc.section` is out of Phase 1 scope —
                // strip the `@` prefix and fall through to in-doc lookup.
                let bare = target.trim_start_matches('@');
                let key = bare.split('.').next().unwrap_or(bare).to_string();
                self.jump_to(doc, &key);
                None
            }
            Item::Return { .. } => {
                // Pop the current stack frame if there's a parent to
                // return to; otherwise fall through.
                if frame_idx > 0 {
                    self.stack.pop();
                }
                None
            }
            Item::Choice { .. } => {
                // A bare choice at script position means a choice
                // *group* of size 1 collapsing into a fan. Collect any
                // adjacent choices into a single Choices frame.
                let (options, indices) =
                    self.collect_choice_run(frame_idx, /* include_current */ true, &item);
                if options.is_empty() {
                    return None;
                }
                self.pending_choices = Some(indices);
                Some(Frame::Choices(options))
            }
            Item::Other { node_kind, text } => {
                // Treat unrecognised items as stage prose so the show
                // can keep going. The node-kind tag is preserved for
                // the host renderer that wants finer control.
                if text.is_empty() {
                    None
                } else {
                    Some(Frame::Stage { text: format!("[{node_kind}] {text}") })
                }
            }
        }
    }

    fn collect_choice_run(
        &mut self,
        frame_idx: usize,
        include_current: bool,
        first: &Item,
    ) -> (Vec<ChoiceFrame>, Vec<usize>) {
        let mut options = Vec::new();
        let mut indices = Vec::new();

        if include_current {
            if let Item::Choice { once, label, .. } = first {
                let path = self.choice_path(frame_idx, self.stack[frame_idx].cursor - 1);
                if !(*once && self.taken_choices.contains(&path)) {
                    indices.push(self.stack[frame_idx].cursor - 1);
                    options.push(ChoiceFrame {
                        index: options.len(),
                        label: label.clone(),
                        once: *once,
                    });
                }
            }
        }

        // Pull adjacent choices off the run.
        loop {
            let cursor = self.stack[frame_idx].cursor;
            let Some(next) = self.stack[frame_idx].items.get(cursor) else {
                break;
            };
            let Item::Choice { once, label, .. } = next else {
                break;
            };
            let path = self.choice_path(frame_idx, cursor);
            if *once && self.taken_choices.contains(&path) {
                self.stack[frame_idx].cursor += 1;
                continue;
            }
            indices.push(cursor);
            options.push(ChoiceFrame {
                index: options.len(),
                label: label.clone(),
                once: *once,
            });
            self.stack[frame_idx].cursor += 1;
        }
        (options, indices)
    }

    fn choice_path(&self, frame_idx: usize, item_idx: usize) -> String {
        format!("{}#{}", self.stack[frame_idx].item_path, item_idx)
    }

    fn next_section_in_order(&self, current: &str) -> Option<String> {
        let mut iter = self.section_order.iter();
        while let Some(id) = iter.next() {
            if id == current {
                return iter.next().cloned();
            }
        }
        None
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
        let Item::Choice { once, label, body } = item else {
            return;
        };

        // Record the path so once-only choices can't reappear.
        let path = format!("{}#{}", top.item_path, item_idx);
        if once {
            self.taken_choices.insert(path.clone());
        }

        let section_id = top.section_id.clone();
        ledger.push(LedgerEntry::Chose {
            section: section_id.clone(),
            index: item_idx,
            label,
            at_ms: now_ms,
        });

        // Push the choice body as a new stack frame.
        self.stack.push(StackFrame {
            section_id,
            item_path: path,
            items: body,
            cursor: 0,
        });
        let _ = doc; // suppress unused-warning — caller passes for symmetry.
    }

    pub fn is_awaiting_choice(&self) -> bool {
        self.pending_choices.is_some()
    }

    pub fn is_at_end(&self) -> bool {
        self.stack.is_empty() && self.pending_choices.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bundle::{compile, LoomDatabase};
    use prism_core::language::loom::parser::parse;

    fn boot(src: &str) -> (LoomDatabase, Ledger) {
        (compile(&parse(src).root), Ledger::new())
    }

    fn frames_until_choice_or_end(
        play: &mut Playhead,
        doc: &Document,
        ledger: &mut Ledger,
    ) -> Vec<Frame> {
        let mut frames = Vec::new();
        for _ in 0..50 {
            let Some(frame) = play.step(doc, ledger, 0) else {
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
        play.choose(doc, 0, &mut ledger, 0);
        let next = play.step(doc, &mut ledger, 0).expect("post-choose frame");
        assert!(matches!(next, Frame::Dialogue { .. }));
    }

    #[test]
    fn ledger_records_visited_and_chose() {
        let src = "# d\ncast WREN\n  .label x\n-- start\nWREN\n  Hi.\n  * help -> next\n-- next\nWREN\n  Thanks.\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let _ = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        play.choose(doc, 0, &mut ledger, 10);
        for _ in 0..10 {
            if play.step(doc, &mut ledger, 20).is_none() {
                break;
            }
        }
        assert!(ledger.played("start"));
        assert!(ledger.played("next"));
        // `chose(...)` matches on either the section path or the
        // rendered label — the section form is robust to label
        // whitespace normalisation across the parser's inline-text
        // pipeline.
        assert!(ledger.chose("start"));
    }

    #[test]
    fn once_choices_dont_reappear_after_taken() {
        let src = "# d\ncast WREN\n  .label x\n-- hub\nWREN\n  Pick.\n  * one\n    -> hub\n  + two\n    -> hub\n";
        let (db, mut ledger) = boot(src);
        let doc = &db.documents[0];
        let mut play = Playhead::new(doc);
        let _ = frames_until_choice_or_end(&mut play, doc, &mut ledger);
        play.choose(doc, 0, &mut ledger, 0);
        // After taking `one`, return to `hub`. The choice run should
        // now have only `two` visible.
        let mut next_choices: Option<Vec<ChoiceFrame>> = None;
        for _ in 0..20 {
            match play.step(doc, &mut ledger, 0) {
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
        // Two dialogues, then End.
        let dialogue_count = frames
            .iter()
            .filter(|f| matches!(f, Frame::Dialogue { .. }))
            .count();
        assert_eq!(dialogue_count, 2);
    }
}
