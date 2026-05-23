//! `Show` — the runtime's front door. Wraps the compiled
//! [`LoomDatabase`] + the [`Ledger`] + the [`Playhead`] in one
//! struct, exposes `step` / `choose` / `vars` / `lets` for the host.
//!
//! Construction goes through [`Show::load`], which parses the source,
//! drops a project-style validator's diagnostics into the result, and
//! compiles the bundle. Hosts that want the diagnostics separately
//! call [`Show::load_with_diagnostics`].

use std::collections::HashMap;

use prism_core::language::loom::parser::parse;
use prism_core::language::loom::validator::validate;

use crate::bundle::{compile, Document, LoomDatabase};
use crate::ledger::Ledger;
use crate::playhead::{Frame, Playhead};
use crate::value::Value;

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

        let show = Show {
            playhead: Playhead::new(&bundle.documents[0]),
            bundle,
            active_doc: 0,
            ledger: Ledger::new(),
            vars: HashMap::new(),
            lets: HashMap::new(),
            roles: HashMap::new(),
            clock_ms: 0,
        };

        Ok(LoadResult { show, diagnostics })
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
    pub fn step(&mut self) -> Option<Frame> {
        if self.playhead.is_awaiting_choice() {
            return None;
        }
        self.playhead
            .step(&self.bundle.documents[self.active_doc], &mut self.ledger, self.clock_ms)
    }

    /// Resolve the currently pending choice by selecting the 0-based
    /// option index from the last [`Frame::Choices`] returned. Does
    /// nothing if no choice is pending.
    pub fn choose(&mut self, option_idx: usize) {
        self.playhead.choose(
            &self.bundle.documents[self.active_doc],
            option_idx,
            &mut self.ledger,
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
}
