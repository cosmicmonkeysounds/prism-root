//! Phase 7 — server-hosted Loom play sessions.
//!
//! A `PlaySession` wraps a `loom_runtime::Playhead` against a bundle
//! built from `(path, source)` pairs the client uploads with
//! `play-start`. The server advances the playhead until it hits a
//! choice prompt or `Step::Ended`; the resulting ledger + pending
//! choices ride out as a `play-state` envelope to every WS subscriber
//! in the workspace.
//!
//! Co-playing semantics for v1:
//!
//!   * One session per workspace (the second `play-start` wins).
//!   * Any authenticated subscriber can submit `play-choice` — there's
//!     no "controller" role yet. Future revisions can gate this via
//!     capability-token permissions.
//!   * The transcript is server-authoritative; clients render it
//!     read-only.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use loom_runtime::bundle::Bundle;
use loom_runtime::ledger::{ChoiceOption, Event};
use loom_runtime::playhead::{Playhead, Step};
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PlayError {
    #[error("no active play session for workspace `{0}`")]
    NoSession(String),
    #[error("playhead init failed: {0}")]
    Init(String),
    #[error("playhead step failed: {0}")]
    Step(String),
}

/// JSON-friendly snapshot of a session at one moment.
#[derive(Clone, Debug, Serialize)]
pub struct PlayStateSnapshot {
    pub workspace: String,
    /// All ledger events from `play-start` through the current head.
    pub transcript: Vec<Event>,
    /// Choices currently waiting on a `play-choice`. Empty when ended
    /// or mid-advance.
    pub choices: Vec<ChoiceOption>,
    /// True once the playhead has yielded `Step::Ended`.
    pub ended: bool,
    /// User who started the session — surfaced so clients can label
    /// the host.
    pub starter: String,
}

struct PlaySession {
    workspace: String,
    starter: String,
    playhead: Playhead,
    transcript: Vec<Event>,
    pending_choices: Vec<ChoiceOption>,
    ended: bool,
}

impl PlaySession {
    fn new(
        workspace: String,
        files: Vec<(String, String)>,
        starter: String,
    ) -> Result<Self, PlayError> {
        let bundle = Arc::new(Bundle::from_sources(files));
        let playhead = Playhead::new(bundle).map_err(|e| PlayError::Init(e.to_string()))?;
        let mut s = Self {
            workspace,
            starter,
            playhead,
            transcript: Vec::new(),
            pending_choices: Vec::new(),
            ended: false,
        };
        s.advance()?;
        Ok(s)
    }

    fn advance(&mut self) -> Result<(), PlayError> {
        loop {
            match self
                .playhead
                .step()
                .map_err(|e| PlayError::Step(e.to_string()))?
            {
                Step::Event(e) => self.transcript.push(e),
                Step::Choice(options) => {
                    self.pending_choices = options;
                    return Ok(());
                }
                Step::Awaiting { .. } => {
                    // The playhead is parked on `<run:>` — surface
                    // control back to the host so it can interleave
                    // its own work; the next call resumes pumping.
                    return Ok(());
                }
                Step::Ended => {
                    self.pending_choices.clear();
                    self.transcript.push(Event::Ended);
                    self.ended = true;
                    return Ok(());
                }
            }
        }
    }

    fn choose(&mut self, index: usize) -> Result<(), PlayError> {
        self.playhead
            .choose(index)
            .map_err(|e| PlayError::Step(e.to_string()))?;
        self.pending_choices.clear();
        self.advance()
    }

    fn snapshot(&self) -> PlayStateSnapshot {
        PlayStateSnapshot {
            workspace: self.workspace.clone(),
            transcript: self.transcript.clone(),
            choices: self.pending_choices.clone(),
            ended: self.ended,
            starter: self.starter.clone(),
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

    /// Start (or replace) a session for `workspace`. Returns the
    /// snapshot the server should broadcast.
    pub fn start(
        &self,
        workspace: String,
        files: Vec<(String, String)>,
        starter: String,
    ) -> Result<PlayStateSnapshot, PlayError> {
        let session = PlaySession::new(workspace.clone(), files, starter)?;
        let snapshot = session.snapshot();
        self.sessions
            .write()
            .unwrap()
            .insert(workspace, Arc::new(RwLock::new(session)));
        Ok(snapshot)
    }

    /// Advance a session by selecting a choice. Returns the post-step
    /// snapshot.
    pub fn choose(&self, workspace: &str, index: usize) -> Result<PlayStateSnapshot, PlayError> {
        let session = self
            .sessions
            .read()
            .unwrap()
            .get(workspace)
            .cloned()
            .ok_or_else(|| PlayError::NoSession(workspace.to_string()))?;
        let mut session = session.write().unwrap();
        session.choose(index)?;
        Ok(session.snapshot())
    }

    /// Fetch the current snapshot without mutating — used when a
    /// client subscribes mid-session.
    pub fn snapshot(&self, workspace: &str) -> Option<PlayStateSnapshot> {
        self.sessions
            .read()
            .unwrap()
            .get(workspace)
            .map(|s| s.read().unwrap().snapshot())
    }

    /// Tear down the session — broadcast-end is the caller's
    /// responsibility.
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
        assert!(!snap.ended);
        assert_eq!(snap.choices.len(), 2);
        assert!(snap
            .transcript
            .iter()
            .any(|e| matches!(e, Event::Action { .. })));
    }

    #[test]
    fn choose_advances_to_end() {
        let hub = PlayHub::new();
        hub.start("ws-1".into(), files(), "alice".into()).unwrap();
        let snap = hub.choose("ws-1", 0).expect("choose");
        assert!(snap.ended);
        assert!(snap.choices.is_empty());
    }

    #[test]
    fn snapshot_returns_none_for_unknown_workspace() {
        let hub = PlayHub::new();
        assert!(hub.snapshot("missing").is_none());
    }

    #[test]
    fn stop_removes_session() {
        let hub = PlayHub::new();
        hub.start("ws-1".into(), files(), "alice".into()).unwrap();
        assert!(hub.stop("ws-1"));
        assert!(!hub.stop("ws-1"));
    }

    #[test]
    fn choose_on_missing_workspace_errors() {
        let hub = PlayHub::new();
        let err = hub.choose("nope", 0).unwrap_err();
        assert!(matches!(err, PlayError::NoSession(_)));
    }
}
