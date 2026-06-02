//! Multi-head play sessions — the engine behind both the server-hosted
//! co-play loop (`loom-server`) and the in-browser local play loop
//! (`loom-wasm`).
//!
//! A [`PlaySession`] wraps one or more [`Mesh`] heads against a bundle
//! built from `(path, source)` pairs. It advances each head until it
//! hits a choice prompt or `Step::Ended`; the resulting per-head ledger
//! / world / tracks plus the snapshot index serialize out as a
//! [`PlayStateSnapshot`] (the exact JSON the editor's runner panels
//! consume, whether it arrives over the relay's `play-state` envelope or
//! straight out of the wasm session).
//!
//! Phase 4 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md §5):
//! one session, many heads. The `primary` head is the editor's default
//! focus; additional heads come from `fork` (live) or `restore-from-
//! snapshot` (time-travel). Bundle is `Arc`-shared across every head so
//! a fork is just the playhead's mutable state.
//!
//! This module used to live in `loom-server::play`; it moved here so
//! the browser can run the identical engine client-side with no relay
//! (see `loom-wasm::play`). The server keeps its `PlayHub` registry
//! (workspace-id → session) on top of this.

use std::collections::HashMap;
use std::sync::Arc;

use serde::Serialize;
use thiserror::Error;

use crate::bundle::Bundle;
use crate::ledger::{ChoiceOption, EnvelopeMeta, Event};
use crate::mesh::{Mesh, MeshSnapshot, TrackIdentity};
use crate::playhead::Step;

pub type HeadId = String;
pub type SnapshotId = String;

const DEFAULT_HEAD: &str = "h0";

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("unknown head `{0}`")]
    UnknownHead(String),
    #[error("unknown snapshot `{0}`")]
    UnknownSnapshot(String),
    #[error("cannot drop the primary head")]
    CannotDropPrimary,
    #[error("playhead init failed: {0}")]
    Init(String),
    #[error("playhead step failed: {0}")]
    Step(String),
}

/// JSON-friendly snapshot of one session at one moment.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayStateSnapshot {
    pub workspace: String,
    pub primary: HeadId,
    pub starter: String,
    pub heads: Vec<HeadSnapshot>,
    pub snapshots: Vec<SnapshotInfo>,
}

/// Per-head view of the session. One of these per live head.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadSnapshot {
    pub id: HeadId,
    /// `Some(headId)` if this head was forked from another live head.
    pub parent: Option<HeadId>,
    /// `Some(snapId)` if this head was created from a snapshot.
    pub forked_from: Option<SnapshotId>,
    pub transcript: Vec<Event>,
    /// Parallel to `transcript`; one meta entry per event.
    pub meta: Vec<EnvelopeMeta>,
    pub choices: Vec<ChoiceOption>,
    pub ended: bool,
    pub world: Vec<(String, String)>,
    pub tracks: Vec<TrackInfo>,
}

/// Persisted snapshot pointer. The mesh state stays in the session; the
/// client just sees the (id, head, ledger index, label) tuple.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotInfo {
    pub id: SnapshotId,
    pub head_id: HeadId,
    /// Ledger length at capture time on the originating head.
    pub at: usize,
    pub label: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TrackInfo {
    pub id: u32,
    pub kind: &'static str,
    pub label: String,
}

struct HeadState {
    mesh: Mesh,
    transcript: Vec<Event>,
    transcript_meta: Vec<EnvelopeMeta>,
    pending_choices: Vec<ChoiceOption>,
    ended: bool,
    parent: Option<HeadId>,
    forked_from: Option<SnapshotId>,
}

impl HeadState {
    fn new(mesh: Mesh, parent: Option<HeadId>, forked_from: Option<SnapshotId>) -> Self {
        Self {
            mesh,
            transcript: Vec::new(),
            transcript_meta: Vec::new(),
            pending_choices: Vec::new(),
            ended: false,
            parent,
            forked_from,
        }
    }

    /// Pump the head until it pauses on a choice / awaiting / ended.
    fn advance(&mut self) -> Result<(), SessionError> {
        loop {
            let before = self.mesh.ledger().len();
            let step = self
                .mesh
                .step()
                .map_err(|e| SessionError::Step(e.to_string()))?;
            let ledger = self.mesh.ledger();
            let events = ledger.events();
            let meta = ledger.meta();
            for idx in before..events.len() {
                self.transcript.push(events[idx].clone());
                self.transcript_meta.push(meta[idx]);
            }
            match step.1 {
                Step::Event(_) => continue,
                Step::Choice(options) => {
                    self.pending_choices = options;
                    return Ok(());
                }
                Step::Awaiting { .. } => return Ok(()),
                Step::Ended => {
                    self.pending_choices.clear();
                    self.transcript.push(Event::Ended);
                    self.transcript_meta.push(EnvelopeMeta::default());
                    self.ended = true;
                    return Ok(());
                }
            }
        }
    }

    fn choose(&mut self, index: usize) -> Result<(), SessionError> {
        self.mesh
            .choose(index)
            .map_err(|e| SessionError::Step(e.to_string()))?;
        self.pending_choices.clear();
        self.advance()
    }

    /// Branch this head — the new head shares no mutable state with
    /// the parent but inherits its full transcript / world by virtue of
    /// `Mesh::fork()`.
    fn fork(&self, parent: HeadId) -> HeadState {
        let mesh = self.mesh.fork();
        let mut s = HeadState::new(mesh, Some(parent), None);
        s.transcript = self.transcript.clone();
        s.transcript_meta = self.transcript_meta.clone();
        s.pending_choices = self.pending_choices.clone();
        s.ended = self.ended;
        s
    }

    fn snapshot_view(&self, id: HeadId) -> HeadSnapshot {
        let world: Vec<(String, String)> = self
            .mesh
            .world()
            .entries()
            .map(|(k, v)| (k.clone(), v.display()))
            .collect();
        let mut tracks: Vec<TrackInfo> = self
            .mesh
            .tracks()
            .map(|t| {
                let (kind, label) = match &t.identity {
                    TrackIdentity::Role(n) => ("role", n.clone()),
                    TrackIdentity::Person(n) => ("person", n.clone()),
                    TrackIdentity::Cohort(n) => ("cohort", n.clone()),
                    TrackIdentity::AmbientGenerator(n) => ("generator", n.clone()),
                    TrackIdentity::Booth => ("booth", "Booth".into()),
                    TrackIdentity::Main => ("main", "Main".into()),
                };
                TrackInfo {
                    id: t.id.0,
                    kind,
                    label,
                }
            })
            .collect();
        tracks.sort_by_key(|t| {
            let rank = match t.kind {
                "booth" => 0,
                "main" => 1,
                "role" => 2,
                "person" => 3,
                "cohort" => 4,
                "generator" => 5,
                _ => 6,
            };
            (rank, t.id)
        });
        HeadSnapshot {
            id,
            parent: self.parent.clone(),
            forked_from: self.forked_from.clone(),
            transcript: self.transcript.clone(),
            meta: self.transcript_meta.clone(),
            choices: self.pending_choices.clone(),
            ended: self.ended,
            world,
            tracks,
        }
    }
}

/// A captured head-and-mesh state. We retain the surface state
/// (choices + ended flag) alongside the mesh snapshot because the
/// playhead at a choice prompt doesn't naturally re-yield it on the
/// next step — restoration would otherwise silently lose the pending
/// choice options.
#[derive(Clone)]
struct StoredSnapshot {
    head_id: HeadId,
    mesh: MeshSnapshot,
    at: usize,
    label: Option<String>,
    pending_choices: Vec<ChoiceOption>,
    ended: bool,
}

pub struct PlaySession {
    workspace: String,
    starter: String,
    bundle: Arc<Bundle>,
    primary: HeadId,
    heads: HashMap<HeadId, HeadState>,
    snapshots: HashMap<SnapshotId, StoredSnapshot>,
    next_head: u64,
    next_snapshot: u64,
}

impl PlaySession {
    /// Build a session from `(path, source)` pairs, advancing the
    /// primary head to its first pause point.
    pub fn new(
        workspace: String,
        files: Vec<(String, String)>,
        starter: String,
    ) -> Result<Self, SessionError> {
        let bundle = Arc::new(Bundle::from_sources(files));
        let mesh = Mesh::new(Arc::clone(&bundle)).map_err(|e| SessionError::Init(e.to_string()))?;
        let mut head = HeadState::new(mesh, None, None);
        head.advance()?;
        let mut heads = HashMap::new();
        heads.insert(DEFAULT_HEAD.into(), head);
        Ok(Self {
            workspace,
            starter,
            bundle,
            primary: DEFAULT_HEAD.into(),
            heads,
            snapshots: HashMap::new(),
            next_head: 1,
            next_snapshot: 0,
        })
    }

    /// The current primary (focused) head id.
    pub fn primary(&self) -> &str {
        &self.primary
    }

    fn head_mut(&mut self, id: &str) -> Result<&mut HeadState, SessionError> {
        self.heads
            .get_mut(id)
            .ok_or_else(|| SessionError::UnknownHead(id.to_string()))
    }

    fn fresh_head_id(&mut self) -> HeadId {
        let id = format!("h{}", self.next_head);
        self.next_head += 1;
        id
    }

    fn fresh_snapshot_id(&mut self) -> SnapshotId {
        let id = format!("s{}", self.next_snapshot);
        self.next_snapshot += 1;
        id
    }

    pub fn choose(&mut self, head: &str, index: usize) -> Result<(), SessionError> {
        self.head_mut(head)?.choose(index)
    }

    pub fn fork(
        &mut self,
        parent: &str,
        from_snapshot: Option<&str>,
    ) -> Result<HeadId, SessionError> {
        let new_id = self.fresh_head_id();
        let head = match from_snapshot {
            Some(snap_id) => {
                let stored = self
                    .snapshots
                    .get(snap_id)
                    .ok_or_else(|| SessionError::UnknownSnapshot(snap_id.to_string()))?
                    .clone();
                let mut mesh = Mesh::new(Arc::clone(&self.bundle))
                    .map_err(|e| SessionError::Init(e.to_string()))?;
                mesh.restore(&stored.mesh);
                let mut s = HeadState::new(mesh, Some(parent.into()), Some(snap_id.into()));
                if let Some(p) = self.heads.get(parent) {
                    let n = stored.at.min(p.transcript.len());
                    s.transcript = p.transcript[..n].to_vec();
                    s.transcript_meta = p.transcript_meta[..n].to_vec();
                }
                s.pending_choices = stored.pending_choices;
                s.ended = stored.ended;
                s
            }
            None => {
                let parent_state = self
                    .heads
                    .get(parent)
                    .ok_or_else(|| SessionError::UnknownHead(parent.into()))?;
                parent_state.fork(parent.into())
            }
        };
        self.heads.insert(new_id.clone(), head);
        Ok(new_id)
    }

    pub fn snapshot(&mut self, head: &str, label: Option<String>) -> Result<SnapshotId, SessionError> {
        let id = self.fresh_snapshot_id();
        let state = self.head_mut(head)?;
        let stored = StoredSnapshot {
            head_id: head.into(),
            mesh: state.mesh.snapshot(),
            at: state.transcript.len(),
            label,
            pending_choices: state.pending_choices.clone(),
            ended: state.ended,
        };
        self.snapshots.insert(id.clone(), stored);
        Ok(id)
    }

    pub fn restore(&mut self, head: &str, snap_id: &str) -> Result<(), SessionError> {
        let stored = self
            .snapshots
            .get(snap_id)
            .ok_or_else(|| SessionError::UnknownSnapshot(snap_id.to_string()))?
            .clone();
        let state = self.head_mut(head)?;
        state.mesh.restore(&stored.mesh);
        let n = stored.at.min(state.transcript.len());
        state.transcript.truncate(n);
        state.transcript_meta.truncate(n);
        state.pending_choices = stored.pending_choices;
        state.ended = stored.ended;
        Ok(())
    }

    /// Booth live-patch (spec §13.4) — skip the current beat on
    /// `head`, then advance until the next pause point.
    pub fn booth_skip(&mut self, head: &str) -> Result<(), SessionError> {
        let state = self.head_mut(head)?;
        state.mesh.playhead_mut().booth_skip_beat();
        state.advance()
    }

    /// Booth live-patch — inject a directive at the head of the
    /// playhead's queue and resume.
    pub fn booth_force(&mut self, head: &str, raw: String) -> Result<(), SessionError> {
        let state = self.head_mut(head)?;
        state.mesh.playhead_mut().booth_force_directive(raw);
        state.advance()
    }

    /// Booth live-patch — reload the bundle from caller-supplied
    /// sources and hot-swap it under every head. Ledger + world are
    /// preserved on every head per the runtime contract; the playheads
    /// re-enter the new bundle's entry beat. The bundle replaces
    /// `self.bundle` so future forks-from-snapshot use the new sources.
    pub fn booth_hot_reload(&mut self, files: Vec<(String, String)>) -> Result<(), SessionError> {
        let bundle = Arc::new(Bundle::from_sources(files));
        for state in self.heads.values_mut() {
            state
                .mesh
                .playhead_mut()
                .booth_hot_reload(Arc::clone(&bundle))
                .map_err(|e| SessionError::Step(e.to_string()))?;
            // Re-seed in case the new bundle introduced new
            // ROLEs / PERSONs / generators.
            state.mesh.seed_from_bundle(&bundle);
            state.pending_choices.clear();
            state.ended = false;
            state.advance()?;
        }
        self.bundle = bundle;
        Ok(())
    }

    pub fn drop_head(&mut self, head: &str) -> Result<(), SessionError> {
        if head == self.primary {
            return Err(SessionError::CannotDropPrimary);
        }
        self.heads
            .remove(head)
            .ok_or_else(|| SessionError::UnknownHead(head.into()))?;
        Ok(())
    }

    pub fn set_primary(&mut self, head: &str) -> Result<(), SessionError> {
        if !self.heads.contains_key(head) {
            return Err(SessionError::UnknownHead(head.into()));
        }
        self.primary = head.into();
        Ok(())
    }

    pub fn snapshot_view(&self) -> PlayStateSnapshot {
        let mut heads: Vec<HeadSnapshot> = self
            .heads
            .iter()
            .map(|(id, s)| s.snapshot_view(id.clone()))
            .collect();
        heads.sort_by(|a, b| a.id.cmp(&b.id));
        let mut snapshots: Vec<SnapshotInfo> = self
            .snapshots
            .iter()
            .map(|(id, s)| SnapshotInfo {
                id: id.clone(),
                head_id: s.head_id.clone(),
                at: s.at,
                label: s.label.clone(),
            })
            .collect();
        snapshots.sort_by(|a, b| a.id.cmp(&b.id));
        PlayStateSnapshot {
            workspace: self.workspace.clone(),
            primary: self.primary.clone(),
            starter: self.starter.clone(),
            heads,
            snapshots,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAIN: &str = "\
# Smoke
entry: opening

== opening
  cast: Wren

A bell rope swings in the gloom.

WREN
  Three days quiet.

* Ring the bell.
  -> END
* Walk away.
  -> END
";

    fn files() -> Vec<(String, String)> {
        vec![("main.loom".into(), MAIN.into())]
    }

    #[test]
    fn start_pauses_on_first_choice() {
        let session = PlaySession::new("ws-1".into(), files(), "alice".into()).expect("start");
        let snap = session.snapshot_view();
        assert_eq!(snap.workspace, "ws-1");
        assert_eq!(snap.starter, "alice");
        assert_eq!(snap.primary, "h0");
        assert_eq!(snap.heads.len(), 1);
        let h0 = &snap.heads[0];
        assert!(!h0.ended);
        assert_eq!(h0.choices.len(), 2);
        assert!(h0
            .transcript
            .iter()
            .any(|e| matches!(e, Event::Action { .. })));
    }

    #[test]
    fn choose_advances_to_end() {
        let mut session = PlaySession::new("ws-1".into(), files(), "alice".into()).unwrap();
        session.choose("h0", 0).expect("choose");
        let snap = session.snapshot_view();
        let h0 = snap.heads.iter().find(|h| h.id == "h0").unwrap();
        assert!(h0.ended);
        assert!(h0.choices.is_empty());
    }

    #[test]
    fn fork_creates_independent_head() {
        let mut session = PlaySession::new("ws-1".into(), files(), "alice".into()).unwrap();
        let h1 = session.fork("h0", None).expect("fork");
        let snap = session.snapshot_view();
        assert_eq!(snap.heads.len(), 2);
        assert!(snap.heads.iter().any(|h| h.id == h1));
        // Choose on the new head; primary should be untouched.
        session.choose(&h1, 0).expect("choose alt");
        let snap = session.snapshot_view();
        let h0 = snap.heads.iter().find(|h| h.id == "h0").unwrap();
        let h1s = snap.heads.iter().find(|h| h.id == h1).unwrap();
        assert!(!h0.ended);
        assert!(h1s.ended);
    }

    #[test]
    fn snapshot_then_fork_from_snapshot() {
        let mut session = PlaySession::new("ws-1".into(), files(), "alice".into()).unwrap();
        let snap_id = session
            .snapshot("h0", Some("pre-choice".into()))
            .expect("snapshot");
        // Advance primary past the snapshot.
        session.choose("h0", 0).expect("choose primary");
        // Fork a new head from the snapshot — should be back at the choice.
        let h1 = session.fork("h0", Some(&snap_id)).expect("fork from snap");
        let snap = session.snapshot_view();
        let alt = snap.heads.iter().find(|h| h.id == h1).unwrap();
        assert!(!alt.ended);
        assert_eq!(alt.choices.len(), 2);
        assert_eq!(alt.forked_from.as_deref(), Some(snap_id.as_str()));
    }
}
