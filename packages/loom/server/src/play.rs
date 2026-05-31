//! Server-hosted Loom play sessions.
//!
//! A `PlaySession` wraps one or more `loom_runtime::Mesh` heads against
//! a bundle built from `(path, source)` pairs the client uploads with
//! `play-start`. The server advances each head until it hits a choice
//! prompt or `Step::Ended`; the resulting per-head ledger / world /
//! tracks plus the snapshot index ride out as a `play-state` envelope
//! to every WS subscriber in the workspace.
//!
//! Phase 4 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md §5):
//! one session, many heads. The `primary` head is the editor's default
//! focus; additional heads come from `fork` (live) or `restore-from-
//! snapshot` (time-travel). Bundle + registry are `Arc`-shared across
//! every head so a fork is just the playhead's mutable state.
//!
//! Co-playing semantics for v1:
//!
//!   * One session per workspace (the second `play-start` wins).
//!   * Any authenticated subscriber can submit `play-choice` —
//!     there's no "controller" role yet. Future revisions can gate
//!     this via capability-token permissions.
//!   * The transcript is server-authoritative; clients render it
//!     read-only.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use loom_runtime::bundle::Bundle;
use loom_runtime::ledger::{ChoiceOption, EnvelopeMeta, Event};
use loom_runtime::mesh::{Mesh, MeshSnapshot, TrackIdentity};
use loom_runtime::playhead::Step;
use serde::Serialize;
use thiserror::Error;

pub type HeadId = String;
pub type SnapshotId = String;

const DEFAULT_HEAD: &str = "h0";

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("no active play session for workspace `{0}`")]
    NoSession(String),
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

/// Persisted snapshot pointer. The mesh state stays server-side; the
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
    fn advance(&mut self) -> Result<(), PlayError> {
        loop {
            let before = self.mesh.ledger().len();
            let step = self.mesh.step().map_err(|e| PlayError::Step(e.to_string()))?;
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

    fn choose(&mut self, index: usize) -> Result<(), PlayError> {
        self.mesh
            .choose(index)
            .map_err(|e| PlayError::Step(e.to_string()))?;
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
    fn new(
        workspace: String,
        files: Vec<(String, String)>,
        starter: String,
    ) -> Result<Self, PlayError> {
        let bundle = Arc::new(Bundle::from_sources(files));
        let mesh = Mesh::new(Arc::clone(&bundle)).map_err(|e| PlayError::Init(e.to_string()))?;
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

    fn head_mut(&mut self, id: &str) -> Result<&mut HeadState, PlayError> {
        self.heads
            .get_mut(id)
            .ok_or_else(|| PlayError::UnknownHead(id.to_string()))
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

    fn choose(&mut self, head: &str, index: usize) -> Result<(), PlayError> {
        self.head_mut(head)?.choose(index)
    }

    fn fork(
        &mut self,
        parent: &str,
        from_snapshot: Option<&str>,
    ) -> Result<HeadId, PlayError> {
        let new_id = self.fresh_head_id();
        let head = match from_snapshot {
            Some(snap_id) => {
                let stored = self
                    .snapshots
                    .get(snap_id)
                    .ok_or_else(|| PlayError::UnknownSnapshot(snap_id.to_string()))?
                    .clone();
                let mut mesh = Mesh::new(Arc::clone(&self.bundle))
                    .map_err(|e| PlayError::Init(e.to_string()))?;
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
                    .ok_or_else(|| PlayError::UnknownHead(parent.into()))?;
                parent_state.fork(parent.into())
            }
        };
        self.heads.insert(new_id.clone(), head);
        Ok(new_id)
    }

    fn snapshot(&mut self, head: &str, label: Option<String>) -> Result<SnapshotId, PlayError> {
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

    fn restore(&mut self, head: &str, snap_id: &str) -> Result<(), PlayError> {
        let stored = self
            .snapshots
            .get(snap_id)
            .ok_or_else(|| PlayError::UnknownSnapshot(snap_id.to_string()))?
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

    fn drop_head(&mut self, head: &str) -> Result<(), PlayError> {
        if head == self.primary {
            return Err(PlayError::CannotDropPrimary);
        }
        self.heads
            .remove(head)
            .ok_or_else(|| PlayError::UnknownHead(head.into()))?;
        Ok(())
    }

    fn set_primary(&mut self, head: &str) -> Result<(), PlayError> {
        if !self.heads.contains_key(head) {
            return Err(PlayError::UnknownHead(head.into()));
        }
        self.primary = head.into();
        Ok(())
    }

    fn snapshot_view(&self) -> PlayStateSnapshot {
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

/// Workspace-id → session registry. One handle lives on `LoomRelayState`.
#[derive(Default)]
pub struct PlayHub {
    sessions: RwLock<HashMap<String, Arc<RwLock<PlaySession>>>>,
}

impl PlayHub {
    pub fn new() -> Self {
        Self::default()
    }

    fn session(&self, workspace: &str) -> Result<Arc<RwLock<PlaySession>>, PlayError> {
        self.sessions
            .read()
            .unwrap()
            .get(workspace)
            .cloned()
            .ok_or_else(|| PlayError::NoSession(workspace.to_string()))
    }

    pub fn start(
        &self,
        workspace: String,
        files: Vec<(String, String)>,
        starter: String,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = PlaySession::new(workspace.clone(), files, starter)?;
        let snap = session.snapshot_view();
        self.sessions
            .write()
            .unwrap()
            .insert(workspace, Arc::new(RwLock::new(session)));
        Ok(snap)
    }

    pub fn choose(
        &self,
        workspace: &str,
        head: Option<&str>,
        index: usize,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        let head_id = head
            .map(String::from)
            .unwrap_or_else(|| session.primary.clone());
        session.choose(&head_id, index)?;
        Ok(session.snapshot_view())
    }

    pub fn fork(
        &self,
        workspace: &str,
        parent: Option<&str>,
        from_snapshot: Option<&str>,
    ) -> Result<(PlayStateSnapshot, HeadId), PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        let parent_id = parent
            .map(String::from)
            .unwrap_or_else(|| session.primary.clone());
        let new_id = session.fork(&parent_id, from_snapshot)?;
        Ok((session.snapshot_view(), new_id))
    }

    pub fn snapshot_head(
        &self,
        workspace: &str,
        head: Option<&str>,
        label: Option<String>,
    ) -> Result<(PlayStateSnapshot, SnapshotId), PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        let head_id = head
            .map(String::from)
            .unwrap_or_else(|| session.primary.clone());
        let snap_id = session.snapshot(&head_id, label)?;
        Ok((session.snapshot_view(), snap_id))
    }

    pub fn restore(
        &self,
        workspace: &str,
        head: &str,
        snapshot_id: &str,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        session.restore(head, snapshot_id)?;
        Ok(session.snapshot_view())
    }

    pub fn drop_head(&self, workspace: &str, head: &str) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        session.drop_head(head)?;
        Ok(session.snapshot_view())
    }

    pub fn set_primary(
        &self,
        workspace: &str,
        head: &str,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        session.set_primary(head)?;
        Ok(session.snapshot_view())
    }

    pub fn snapshot(&self, workspace: &str) -> Option<PlayStateSnapshot> {
        self.sessions
            .read()
            .unwrap()
            .get(workspace)
            .map(|s| s.read().unwrap().snapshot_view())
    }

    pub fn stop(&self, workspace: &str) -> bool {
        self.sessions.write().unwrap().remove(workspace).is_some()
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
        let hub = PlayHub::new();
        let snap = hub
            .start("ws-1".into(), files(), "alice".into())
            .expect("start");
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
        let hub = PlayHub::new();
        hub.start("ws-1".into(), files(), "alice".into()).unwrap();
        let snap = hub.choose("ws-1", None, 0).expect("choose");
        let h0 = snap.heads.iter().find(|h| h.id == "h0").unwrap();
        assert!(h0.ended);
        assert!(h0.choices.is_empty());
    }

    #[test]
    fn fork_creates_independent_head() {
        let hub = PlayHub::new();
        hub.start("ws-1".into(), files(), "alice".into()).unwrap();
        let (snap, h1) = hub.fork("ws-1", None, None).expect("fork");
        assert_eq!(snap.heads.len(), 2);
        assert!(snap.heads.iter().any(|h| h.id == h1));
        // Choose on the new head; primary should be untouched.
        let snap = hub.choose("ws-1", Some(&h1), 0).expect("choose alt");
        let h0 = snap.heads.iter().find(|h| h.id == "h0").unwrap();
        let h1s = snap.heads.iter().find(|h| h.id == h1).unwrap();
        assert!(!h0.ended);
        assert!(h1s.ended);
    }

    #[test]
    fn snapshot_then_fork_from_snapshot() {
        let hub = PlayHub::new();
        hub.start("ws-1".into(), files(), "alice".into()).unwrap();
        let (_, snap_id) = hub
            .snapshot_head("ws-1", None, Some("pre-choice".into()))
            .expect("snapshot");
        // Advance primary past the snapshot.
        hub.choose("ws-1", None, 0).expect("choose primary");
        // Fork a new head from the snapshot — should be back at the choice.
        let (snap, h1) = hub
            .fork("ws-1", None, Some(&snap_id))
            .expect("fork from snap");
        let alt = snap.heads.iter().find(|h| h.id == h1).unwrap();
        assert!(!alt.ended);
        assert_eq!(alt.choices.len(), 2);
        assert_eq!(alt.forked_from.as_deref(), Some(snap_id.as_str()));
    }
}
