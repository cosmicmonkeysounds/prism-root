//! Server-hosted Loom play sessions.
//!
//! The session engine itself ([`PlaySession`] + the per-head /
//! snapshot view structs) lives in `loom_runtime::session` so the
//! browser can run the identical loop client-side with no relay. This
//! module keeps the server-only piece: a [`PlayHub`] registry mapping
//! workspace id → live session, plus the locking that lets many WS
//! connections drive one shared session.
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

use thiserror::Error;

// Re-export the runtime engine types so existing `crate::play::…`
// references (ws.rs, tests) keep resolving after the move.
pub use loom_runtime::session::{
    HeadId, HeadSnapshot, PlaySession, PlayStateSnapshot, SessionError, SnapshotId, SnapshotInfo,
    TrackInfo,
};

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("no active play session for workspace `{0}`")]
    NoSession(String),
    #[error(transparent)]
    Session(#[from] SessionError),
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
            .unwrap_or_else(|| session.primary().to_string());
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
            .unwrap_or_else(|| session.primary().to_string());
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
            .unwrap_or_else(|| session.primary().to_string());
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

    pub fn booth_skip(
        &self,
        workspace: &str,
        head: Option<&str>,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        let head_id = head
            .map(String::from)
            .unwrap_or_else(|| session.primary().to_string());
        session.booth_skip(&head_id)?;
        Ok(session.snapshot_view())
    }

    pub fn booth_force(
        &self,
        workspace: &str,
        head: Option<&str>,
        raw: String,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        let head_id = head
            .map(String::from)
            .unwrap_or_else(|| session.primary().to_string());
        session.booth_force(&head_id, raw)?;
        Ok(session.snapshot_view())
    }

    pub fn booth_hot_reload(
        &self,
        workspace: &str,
        files: Vec<(String, String)>,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = self.session(workspace)?;
        let mut session = session.write().unwrap();
        session.booth_hot_reload(files)?;
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
    use loom_runtime::ledger::Event;

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
    fn choose_without_session_errors() {
        let hub = PlayHub::new();
        let err = hub.choose("ghost", None, 0).unwrap_err();
        assert!(matches!(err, PlayError::NoSession(_)));
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
