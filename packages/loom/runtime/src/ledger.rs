//! Append-only event ledger.
//!
//! The ledger is the canonical record of everything the playhead
//! emits — the same stream that drives the live-performance scope
//! filters (spec §13), the `since(…)` / `visits(…)` query helpers
//! (spec §12.2), and the booth's transcript view (spec §13.4).
//!
//! Phase-3 keeps it deliberately small: one envelope per visible
//! step. Knowledge / disposition / stat-change envelopes land when
//! Simulacra and Meridian come online.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::bundle::BeatRef;

/// One event written by the playhead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    /// The playhead entered a beat.
    BeatEntered {
        beat: String,
        file: PathBuf,
        #[serde(skip)]
        reference: Option<BeatRef>,
    },
    /// A Fountain-style scene heading was reached.
    Scene { text: String },
    /// A flush-left prose paragraph was reached.
    Action { text: String },
    /// A speaker delivered one line. Parenthetical performer
    /// direction is carried alongside for stage / booth display.
    Dialogue {
        speaker: String,
        parenthetical: Option<String>,
        text: String,
    },
    /// A production-metadata fence was reached. The runtime does
    /// nothing with it at production (spec §15) but stages /
    /// debuggers / linters may.
    Metadata { text: String },
    /// The playhead is offering a set of choices and waiting for
    /// `Playhead::choose`.
    ChoicePrompted { options: Vec<ChoiceOption> },
    /// A choice was taken — recorded after `Playhead::choose`.
    ChoiceTaken { index: usize, text: String },
    /// A `->` divert resolved and was followed.
    Diverted { target: String, beat: String },
    /// A `-> (…) ->` tunnel call entered.
    Tunneled { target: String, beat: String },
    /// A `<-` tunnel return popped back to the caller.
    Returned,
    /// A `-> END` (or otherwise exhausted) playhead halted.
    Ended,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChoiceOption {
    pub index: usize,
    pub text: String,
    pub sticky: bool,
}

/// An ordered log of [`Event`]s. Plain `Vec`-backed; mutations only
/// through `push`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Ledger {
    events: Vec<Event>,
}

impl Ledger {
    pub fn push(&mut self, event: Event) {
        self.events.push(event);
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Index of the most recent occurrence of an event matching
    /// `predicate`. Returned as the count-from-the-end so callers
    /// can compute "did X happen recently". Used by the §12.2
    /// `since(…)` query primitive once it lands.
    pub fn last_matching<F: Fn(&Event) -> bool>(&self, predicate: F) -> Option<usize> {
        self.events.iter().rposition(predicate)
    }
}
