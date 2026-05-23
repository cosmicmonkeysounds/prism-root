//! Append-only event ledger powering `played(X)`, `visits(X)`,
//! `chose(X)`, `since(X)`, `count(field)` queries (grammar §11.6 +
//! §6.2). The ledger is the show's history of record — every section
//! visit, every choice taken, every `fire X` action — and the
//! resolver pulls predicates against it at expression evaluation
//! time.
//!
//! Phase 1 carries `Visited`, `Chose`, and `Fired` entries. Future
//! phases add `SpeakerSpoke`, `CueFired`, and the participant /
//! cohort lifecycle events.

use serde::{Deserialize, Serialize};

/// Externally tagged so the ledger round-trips through postcard
/// (the runtime's snapshot wire format).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerEntry {
    /// The playhead entered a section.
    Visited {
        section: String,
        /// Wall-clock-or-virtual time in milliseconds since show
        /// start. The runtime is the source of truth here; tests
        /// inject monotonic counters.
        at_ms: u64,
    },
    /// A choice was taken. `id` is the section path of the choice's
    /// parent + a 0-based index within that section.
    Chose {
        section: String,
        index: usize,
        label: String,
        at_ms: u64,
    },
    /// `~ fire <name>` action ran.
    Fired { event: String, at_ms: u64 },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Ledger {
    entries: Vec<LedgerEntry>,
}

impl Ledger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn entries(&self) -> &[LedgerEntry] {
        &self.entries
    }

    pub fn push(&mut self, entry: LedgerEntry) {
        self.entries.push(entry);
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// `played(section)` — has this section been visited at least once?
    pub fn played(&self, section: &str) -> bool {
        self.entries
            .iter()
            .any(|e| matches!(e, LedgerEntry::Visited { section: s, .. } if s == section))
    }

    /// `visits(section)` — total visit count for the section.
    pub fn visits(&self, section: &str) -> u32 {
        self.entries
            .iter()
            .filter(|e| matches!(e, LedgerEntry::Visited { section: s, .. } if s == section))
            .count() as u32
    }

    /// `chose(label_or_id)` — was any choice with this label/id taken?
    /// Matches against both the rendered label text and the section
    /// path so authors can write either.
    pub fn chose(&self, key: &str) -> bool {
        self.entries.iter().any(|e| match e {
            LedgerEntry::Chose { label, section, .. } => label == key || section == key,
            _ => false,
        })
    }

    /// `since(event)` — milliseconds since the last `Fired` event of
    /// this name, or `None` if it never fired.
    pub fn since(&self, event: &str, now_ms: u64) -> Option<u64> {
        self.entries
            .iter()
            .rev()
            .find_map(|e| match e {
                LedgerEntry::Fired { event: ev, at_ms } if ev == event => {
                    Some(now_ms.saturating_sub(*at_ms))
                }
                _ => None,
            })
    }

    /// Last entry of the given kind family. Field names mirror the
    /// `LedgerField` enum in grammar §11.6: `speaker`, `choice`,
    /// `event`. Returns the entry verbatim so callers can pull the
    /// payload they care about.
    pub fn last(&self, field: LedgerField) -> Option<&LedgerEntry> {
        self.entries.iter().rev().find(|e| match field {
            LedgerField::Speaker => false, // Not yet recorded.
            LedgerField::Choice => matches!(e, LedgerEntry::Chose { .. }),
            LedgerField::Event => matches!(e, LedgerEntry::Fired { .. }),
        })
    }

    /// `count(field)` — total entries of the given kind.
    pub fn count(&self, field: LedgerField) -> u32 {
        self.entries
            .iter()
            .filter(|e| match field {
                LedgerField::Speaker => false,
                LedgerField::Choice => matches!(e, LedgerEntry::Chose { .. }),
                LedgerField::Event => matches!(e, LedgerEntry::Fired { .. }),
            })
            .count() as u32
    }
}

/// Discriminator for the `last(...)` and `count(...)` ledger
/// predicates from grammar §11.6. Mirrors the doc's `LedgerField`
/// production verbatim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LedgerField {
    Speaker,
    Choice,
    Event,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn led() -> Ledger {
        let mut l = Ledger::new();
        l.push(LedgerEntry::Visited {
            section: "start".into(),
            at_ms: 0,
        });
        l.push(LedgerEntry::Chose {
            section: "start".into(),
            index: 0,
            label: "I'll help.".into(),
            at_ms: 10,
        });
        l.push(LedgerEntry::Visited {
            section: "investigate".into(),
            at_ms: 20,
        });
        l.push(LedgerEntry::Fired {
            event: "trust_threshold_hit".into(),
            at_ms: 30,
        });
        l
    }

    #[test]
    fn played_returns_true_for_visited_section() {
        let l = led();
        assert!(l.played("start"));
        assert!(l.played("investigate"));
        assert!(!l.played("finale"));
    }

    #[test]
    fn visits_counts_repeats() {
        let mut l = Ledger::new();
        l.push(LedgerEntry::Visited {
            section: "hub".into(),
            at_ms: 0,
        });
        l.push(LedgerEntry::Visited {
            section: "hub".into(),
            at_ms: 10,
        });
        assert_eq!(l.visits("hub"), 2);
        assert_eq!(l.visits("never"), 0);
    }

    #[test]
    fn chose_matches_label_or_section() {
        let l = led();
        assert!(l.chose("I'll help."));
        assert!(l.chose("start"));
        assert!(!l.chose("Go away."));
    }

    #[test]
    fn since_computes_recency() {
        let l = led();
        assert_eq!(l.since("trust_threshold_hit", 90), Some(60));
        assert_eq!(l.since("never_fired", 90), None);
    }

    #[test]
    fn last_and_count_route_by_field() {
        let l = led();
        assert!(matches!(
            l.last(LedgerField::Choice),
            Some(LedgerEntry::Chose { .. })
        ));
        assert!(matches!(
            l.last(LedgerField::Event),
            Some(LedgerEntry::Fired { .. })
        ));
        assert_eq!(l.count(LedgerField::Choice), 1);
        assert_eq!(l.count(LedgerField::Event), 1);
    }

    #[test]
    fn serde_round_trips_through_json() {
        let l = led();
        let s = serde_json::to_string(&l).unwrap();
        let back: Ledger = serde_json::from_str(&s).unwrap();
        assert_eq!(back.entries(), l.entries());
    }
}
