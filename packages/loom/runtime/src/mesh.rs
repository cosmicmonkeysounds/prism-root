//! The mesh — many tracks, one world, one ledger (loom-editor.html §2).
//!
//! Phase A of the editor work: this module ships the *types* and a
//! thin wrapper around the existing [`crate::playhead::Playhead`].
//! The wrapper exposes the same `Step` stream callers already use, but
//! attaches the per-envelope [`crate::ledger::EnvelopeMeta`] metadata
//! the editor reads — track id, cause-edge, and (via cell envelopes)
//! the boundaries of each historical unit.
//!
//! Phase B will give every ROLE / PERSON / COHORT / ambient generator
//! their own `Track` with an explicit `Driver`; today, every push goes
//! through [`crate::ledger::TrackId::MAIN`].
//!
//! The Mesh does not own the world or the ledger directly — those live
//! on the wrapped Playhead so single-playhead callers and Mesh callers
//! see the same state. The Mesh is therefore cheap: it just adds a
//! track registry and the cell-folding view.

use std::collections::HashMap;
use std::sync::Arc;

use crate::bundle::Bundle;
use crate::ledger::{CellKind, Event, TrackId};
use crate::playhead::{PlayError, Playhead, Step};

/// What kind of cursor a [`Track`] is — drives how the canvas labels
/// the row and how the runtime advances the track (Phase B).
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TrackIdentity {
    /// A scripted character / NPC. The name is the ROLE / CHARACTER
    /// declaration name (`Wren`, `Bellkeeper`).
    Role(String),
    /// A real human (or audience walk-up) — name is the PERSON
    /// declaration id (`jamie_lee`, `walkup_12`).
    Person(String),
    /// A named cohort — `Initiates`, `Singers`. Renders as a folder
    /// in the canvas (loom-editor.html §12.3).
    Cohort(String),
    /// A top-level GENERATOR — `HarborChorus`, `Weather`.
    AmbientGenerator(String),
    /// The booth / director track. Operator actions ride on it.
    Booth,
    /// The implicit main track every single-playhead show runs on.
    /// Used by [`Mesh::single_playhead`] in Phase A so existing tests
    /// see the Mesh shape without any semantic change.
    Main,
}

impl TrackIdentity {
    /// Human-readable label for the canvas row.
    pub fn label(&self) -> &str {
        match self {
            Self::Role(s)
            | Self::Person(s)
            | Self::Cohort(s)
            | Self::AmbientGenerator(s) => s,
            Self::Booth => "Booth",
            Self::Main => "Main",
        }
    }
}

/// What drives a [`Track`] forward (loom-editor.html §8). Phase A
/// only uses `Scripted`; the other variants land with Phase B's
/// per-driver wiring.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Driver {
    /// Walks the bundle's body items deterministically. Goals,
    /// generators, hooks all participate. The default for every
    /// track until a live binding overrides it.
    Scripted,
    /// Awaits a human cue per dialogue line / per improv beat. The
    /// performer sees the cue on a prompter; the track advances on
    /// pedal / anchor phrase / improv completion.
    Performer { device: Option<String> },
    /// Driven by a live PERSON's physical / device actions —
    /// location entries, taps, walk-up join, biometric cues.
    Participant { person: String },
}

/// One independent cursor (loom-editor.html §2). Phase A keeps the
/// fields the editor needs to render a row but does not yet own a
/// frame stack — that comes with Phase B's multi-track scheduling.
#[derive(Clone, Debug)]
pub struct Track {
    pub id: TrackId,
    pub identity: TrackIdentity,
    pub driver: Driver,
}

/// One unit of a track's history — synthesised from the ledger by
/// [`Mesh::cells_for_track`]. Cells are *views* over the ledger; the
/// runtime does not store them separately. A cell runs from a
/// [`Event::CellEntered`] envelope to the matching
/// [`Event::CellExited`] (or the end of the ledger if it's still
/// open).
#[derive(Clone, Debug)]
pub struct Cell {
    pub track: TrackId,
    pub kind: CellKind,
    pub bundle_ref: String,
    /// Envelope index of the `CellEntered` that opened this cell.
    pub start: u32,
    /// Envelope index of the `CellExited` that closed this cell, or
    /// `None` while the cell is still open (the live tip).
    pub end: Option<u32>,
    /// Opaque snapshot handle written into `CellExited.snapshot`.
    /// `None` while the cell is open.
    pub snapshot: Option<String>,
}

impl Cell {
    /// `true` if the cell is the live tip (not yet closed).
    pub fn is_open(&self) -> bool {
        self.end.is_none()
    }
}

/// The mesh — `Track`s addressing one shared world + ledger.
///
/// Phase A: holds one Scripted track (`TrackId::MAIN`) that delegates
/// to the wrapped Playhead. The Mesh's `step` returns the same `Step`
/// the inner Playhead does; the per-envelope metadata captures the
/// track id so editor consumers see the graph shape from day one.
pub struct Mesh {
    head: Playhead,
    tracks: HashMap<TrackId, Track>,
    next_track_id: u32,
}

impl Mesh {
    /// Build a mesh around a fresh single-playhead show. The main
    /// track is created automatically and bound to
    /// [`TrackIdentity::Main`] / [`Driver::Scripted`].
    pub fn new(bundle: Arc<Bundle>) -> Result<Self, PlayError> {
        let head = Playhead::new(bundle)?;
        let mut tracks = HashMap::new();
        tracks.insert(
            TrackId::MAIN,
            Track {
                id: TrackId::MAIN,
                identity: TrackIdentity::Main,
                driver: Driver::Scripted,
            },
        );
        Ok(Self {
            head,
            tracks,
            next_track_id: 1,
        })
    }

    /// Wrap an existing playhead. Use this when an integration tests
    /// or boot path needs to attach the Mesh view to a playhead it
    /// constructed itself.
    pub fn from_playhead(head: Playhead) -> Self {
        let mut tracks = HashMap::new();
        tracks.insert(
            TrackId::MAIN,
            Track {
                id: TrackId::MAIN,
                identity: TrackIdentity::Main,
                driver: Driver::Scripted,
            },
        );
        Self {
            head,
            tracks,
            next_track_id: 1,
        }
    }

    /// Advance the mesh by one step. Phase A: delegates to the wrapped
    /// playhead and reports the step alongside `TrackId::MAIN` so
    /// callers can already write code that handles per-track output.
    pub fn step(&mut self) -> Result<(TrackId, Step), PlayError> {
        let step = self.head.step()?;
        Ok((TrackId::MAIN, step))
    }

    /// Pick a choice on the main track (Phase A only has one track).
    pub fn choose(&mut self, index: usize) -> Result<(), PlayError> {
        self.head.choose(index)
    }

    pub fn world(&self) -> &crate::expr::World {
        self.head.world()
    }

    pub fn ledger(&self) -> &crate::ledger::Ledger {
        self.head.ledger()
    }

    pub fn tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.get(&id)
    }

    /// Register a new track. Returns its allocated id. Phase A
    /// callers (tests, future-proofing) can use this to mint
    /// participant rows even before the runtime drives them.
    pub fn register_track(&mut self, identity: TrackIdentity, driver: Driver) -> TrackId {
        let id = TrackId(self.next_track_id);
        self.next_track_id += 1;
        self.tracks.insert(
            id,
            Track {
                id,
                identity,
                driver,
            },
        );
        id
    }

    /// Borrow the inner playhead — present so existing callers that
    /// have a Mesh can still reach the single-playhead API while we
    /// finish the multi-track wiring.
    pub fn playhead(&self) -> &Playhead {
        &self.head
    }

    pub fn playhead_mut(&mut self) -> &mut Playhead {
        &mut self.head
    }

    /// Fold the ledger into a vector of [`Cell`]s belonging to
    /// `track`. Cells are returned in ledger order. Cells nest — a
    /// `-> ringing` divert opens a child cell on top of the
    /// `opening` cell's parent, and the first `CellExited` matches
    /// the most-recently-opened cell (LIFO). An open cell at the
    /// tail (no matching `CellExited`) is included as a
    /// `Cell { end: None, snapshot: None, … }` so the canvas can
    /// render the live tip.
    pub fn cells_for_track(&self, track: TrackId) -> Vec<Cell> {
        let mut out = Vec::new();
        let mut open: Vec<Cell> = Vec::new();
        for (idx, event, meta) in self.ledger().iter_with_meta() {
            if meta.track != track {
                continue;
            }
            match event {
                Event::CellEntered {
                    track: t,
                    kind,
                    bundle_ref,
                } => {
                    open.push(Cell {
                        track: *t,
                        kind: *kind,
                        bundle_ref: bundle_ref.clone(),
                        start: idx,
                        end: None,
                        snapshot: None,
                    });
                }
                Event::CellExited {
                    track: _t,
                    snapshot,
                } => {
                    if let Some(mut cell) = open.pop() {
                        cell.end = Some(idx);
                        cell.snapshot = Some(snapshot.clone());
                        out.push(cell);
                    }
                }
                _ => {}
            }
        }
        // Anything still on the open stack is a live tip.
        out.extend(open.into_iter().rev());
        // Return in source order — the LIFO above interleaved child
        // closes before their parents; re-sort by start for the canvas.
        out.sort_by_key(|c| c.start);
        out
    }

    /// Walk the cause-edge chain backward from `idx`, returning every
    /// envelope index reached. Used by the editor's "why did this
    /// fire?" inspector (loom-editor.html §6).
    pub fn cause_chain(&self, idx: u32) -> Vec<u32> {
        let meta = self.ledger().meta();
        let mut chain = vec![idx];
        let mut cur = meta.get(idx as usize).and_then(|m| m.cause);
        while let Some(prev) = cur {
            chain.push(prev);
            cur = meta.get(prev as usize).and_then(|m| m.cause);
        }
        chain
    }
}
