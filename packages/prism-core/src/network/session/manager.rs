//! `network::session::manager` — active session tracker.
//!
//! `SessionManager` tracks active collaboration sessions across
//! vaults. Each session has an associated `TranscriptTimeline` for
//! audit/replay. Host-driven: the host calls `create_session` /
//! `end_session` and feeds events through `record_*` methods.

use std::cell::RefCell;
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::transcript::{TranscriptEntryKind, TranscriptTimeline};

// ── Session ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionStatus {
    Active,
    Paused,
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub session_id: String,
    pub vault_id: String,
    pub status: SessionStatus,
    pub created_at_ms: u64,
    pub peer_count: usize,
    pub entry_count: usize,
}

pub struct Session {
    pub session_id: String,
    pub vault_id: String,
    pub status: SessionStatus,
    pub created_at_ms: u64,
    pub peers: Vec<String>,
    pub transcript: TranscriptTimeline,
}

impl Session {
    pub fn info(&self) -> SessionInfo {
        SessionInfo {
            session_id: self.session_id.clone(),
            vault_id: self.vault_id.clone(),
            status: self.status,
            created_at_ms: self.created_at_ms,
            peer_count: self.peers.len(),
            entry_count: self.transcript.len(),
        }
    }
}

// ── SessionManager ─────────────────────────────────────────────────

pub struct SessionManagerOptions {
    pub max_transcript_entries: usize,
}

impl Default for SessionManagerOptions {
    fn default() -> Self {
        Self {
            max_transcript_entries: 10_000,
        }
    }
}

pub struct SessionManager {
    sessions: RefCell<HashMap<String, Session>>,
    next_id: RefCell<u64>,
    max_transcript_entries: usize,
}

impl SessionManager {
    pub fn new(options: SessionManagerOptions) -> Self {
        Self {
            sessions: RefCell::new(HashMap::new()),
            next_id: RefCell::new(1),
            max_transcript_entries: options.max_transcript_entries,
        }
    }

    pub fn create_session(&self, vault_id: &str, now_ms: u64) -> String {
        let mut id_gen = self.next_id.borrow_mut();
        let session_id = format!("session-{}", *id_gen);
        *id_gen += 1;

        let session = Session {
            session_id: session_id.clone(),
            vault_id: vault_id.to_string(),
            status: SessionStatus::Active,
            created_at_ms: now_ms,
            peers: Vec::new(),
            transcript: TranscriptTimeline::new(&session_id, self.max_transcript_entries),
        };
        self.sessions
            .borrow_mut()
            .insert(session_id.clone(), session);
        session_id
    }

    pub fn end_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        if let Some(session) = sessions.get_mut(session_id) {
            session.status = SessionStatus::Ended;
            true
        } else {
            false
        }
    }

    pub fn pause_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        if let Some(session) = sessions.get_mut(session_id) {
            if session.status == SessionStatus::Active {
                session.status = SessionStatus::Paused;
                return true;
            }
        }
        false
    }

    pub fn resume_session(&self, session_id: &str) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        if let Some(session) = sessions.get_mut(session_id) {
            if session.status == SessionStatus::Paused {
                session.status = SessionStatus::Active;
                return true;
            }
        }
        false
    }

    pub fn get_info(&self, session_id: &str) -> Option<SessionInfo> {
        self.sessions.borrow().get(session_id).map(|s| s.info())
    }

    pub fn list_active(&self) -> Vec<SessionInfo> {
        self.sessions
            .borrow()
            .values()
            .filter(|s| s.status == SessionStatus::Active)
            .map(|s| s.info())
            .collect()
    }

    pub fn list_all(&self) -> Vec<SessionInfo> {
        self.sessions.borrow().values().map(|s| s.info()).collect()
    }

    pub fn session_count(&self) -> usize {
        self.sessions.borrow().len()
    }

    pub fn active_count(&self) -> usize {
        self.sessions
            .borrow()
            .values()
            .filter(|s| s.status == SessionStatus::Active)
            .count()
    }

    // ── Event recording ────────────────────────────────────────────

    pub fn record_join(&self, session_id: &str, peer_id: &str, now_ms: u64) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        if !session.peers.contains(&peer_id.to_string()) {
            session.peers.push(peer_id.to_string());
        }
        session.transcript.append(
            TranscriptEntryKind::Join,
            Some(peer_id),
            &format!("{peer_id} joined"),
            None,
            now_ms,
        );
        true
    }

    pub fn record_leave(&self, session_id: &str, peer_id: &str, now_ms: u64) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        session.peers.retain(|p| p != peer_id);
        session.transcript.append(
            TranscriptEntryKind::Leave,
            Some(peer_id),
            &format!("{peer_id} left"),
            None,
            now_ms,
        );
        true
    }

    pub fn record_operation(
        &self,
        session_id: &str,
        peer_id: &str,
        summary: &str,
        data: Option<serde_json::Value>,
        now_ms: u64,
    ) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        session.transcript.append(
            TranscriptEntryKind::Operation,
            Some(peer_id),
            summary,
            data,
            now_ms,
        );
        true
    }

    pub fn record_message(
        &self,
        session_id: &str,
        peer_id: &str,
        message: &str,
        now_ms: u64,
    ) -> bool {
        let mut sessions = self.sessions.borrow_mut();
        let Some(session) = sessions.get_mut(session_id) else {
            return false;
        };
        session.transcript.append(
            TranscriptEntryKind::Message,
            Some(peer_id),
            message,
            None,
            now_ms,
        );
        true
    }

    // ── Transcript access ──────────────────────────────────────────

    pub fn get_transcript(
        &self,
        session_id: &str,
    ) -> Option<Vec<super::transcript::TranscriptEntry>> {
        self.sessions
            .borrow()
            .get(session_id)
            .map(|s| s.transcript.export())
    }

    pub fn get_transcript_since(
        &self,
        session_id: &str,
        after_seq: u64,
    ) -> Option<Vec<super::transcript::TranscriptEntry>> {
        self.sessions.borrow().get(session_id).map(|s| {
            s.transcript
                .entries_since(after_seq)
                .into_iter()
                .cloned()
                .collect()
        })
    }

    pub fn remove_ended(&self) -> usize {
        let mut sessions = self.sessions.borrow_mut();
        let ended: Vec<String> = sessions
            .iter()
            .filter(|(_, s)| s.status == SessionStatus::Ended)
            .map(|(k, _)| k.clone())
            .collect();
        let count = ended.len();
        for id in ended {
            sessions.remove(&id);
        }
        count
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new(SessionManagerOptions::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manager() -> SessionManager {
        SessionManager::new(SessionManagerOptions::default())
    }

    #[test]
    fn create_and_get_session() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        let info = mgr.get_info(&id).unwrap();
        assert_eq!(info.vault_id, "vault-1");
        assert_eq!(info.status, SessionStatus::Active);
        assert_eq!(info.created_at_ms, 1000);
        assert_eq!(info.peer_count, 0);
    }

    #[test]
    fn end_session() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        assert!(mgr.end_session(&id));
        assert_eq!(mgr.get_info(&id).unwrap().status, SessionStatus::Ended);
        assert!(!mgr.end_session("unknown"));
    }

    #[test]
    fn pause_and_resume() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        assert!(mgr.pause_session(&id));
        assert_eq!(mgr.get_info(&id).unwrap().status, SessionStatus::Paused);
        assert!(mgr.resume_session(&id));
        assert_eq!(mgr.get_info(&id).unwrap().status, SessionStatus::Active);
    }

    #[test]
    fn pause_requires_active() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        mgr.end_session(&id);
        assert!(!mgr.pause_session(&id));
    }

    #[test]
    fn resume_requires_paused() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        assert!(!mgr.resume_session(&id));
    }

    #[test]
    fn record_join_and_leave() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        assert!(mgr.record_join(&id, "alice", 2000));
        assert!(mgr.record_join(&id, "bob", 3000));
        let info = mgr.get_info(&id).unwrap();
        assert_eq!(info.peer_count, 2);
        assert_eq!(info.entry_count, 2);

        assert!(mgr.record_leave(&id, "alice", 4000));
        assert_eq!(mgr.get_info(&id).unwrap().peer_count, 1);
    }

    #[test]
    fn record_join_is_idempotent_for_peers() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        mgr.record_join(&id, "alice", 2000);
        mgr.record_join(&id, "alice", 3000);
        assert_eq!(mgr.get_info(&id).unwrap().peer_count, 1);
        assert_eq!(mgr.get_info(&id).unwrap().entry_count, 2);
    }

    #[test]
    fn record_operation_and_message() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        assert!(mgr.record_operation(&id, "alice", "edited field", None, 2000));
        assert!(mgr.record_message(&id, "bob", "looks good", 3000));
        assert_eq!(mgr.get_info(&id).unwrap().entry_count, 2);
    }

    #[test]
    fn record_to_unknown_session_returns_false() {
        let mgr = make_manager();
        assert!(!mgr.record_join("unknown", "alice", 1000));
        assert!(!mgr.record_leave("unknown", "alice", 1000));
        assert!(!mgr.record_operation("unknown", "alice", "op", None, 1000));
        assert!(!mgr.record_message("unknown", "alice", "msg", 1000));
    }

    #[test]
    fn get_transcript() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        mgr.record_join(&id, "alice", 2000);
        mgr.record_operation(&id, "alice", "edit", None, 3000);
        let transcript = mgr.get_transcript(&id).unwrap();
        assert_eq!(transcript.len(), 2);
    }

    #[test]
    fn get_transcript_since() {
        let mgr = make_manager();
        let id = mgr.create_session("vault-1", 1000);
        mgr.record_join(&id, "alice", 2000);
        mgr.record_operation(&id, "alice", "edit", None, 3000);
        mgr.record_leave(&id, "alice", 4000);
        let since = mgr.get_transcript_since(&id, 1).unwrap();
        assert_eq!(since.len(), 2);
    }

    #[test]
    fn list_active_and_all() {
        let mgr = make_manager();
        let id1 = mgr.create_session("vault-1", 1000);
        let _id2 = mgr.create_session("vault-2", 2000);
        mgr.end_session(&id1);
        assert_eq!(mgr.list_active().len(), 1);
        assert_eq!(mgr.list_all().len(), 2);
        assert_eq!(mgr.session_count(), 2);
        assert_eq!(mgr.active_count(), 1);
    }

    #[test]
    fn remove_ended() {
        let mgr = make_manager();
        let id1 = mgr.create_session("vault-1", 1000);
        let _id2 = mgr.create_session("vault-2", 2000);
        mgr.end_session(&id1);
        let removed = mgr.remove_ended();
        assert_eq!(removed, 1);
        assert_eq!(mgr.session_count(), 1);
    }

    #[test]
    fn unique_session_ids() {
        let mgr = make_manager();
        let id1 = mgr.create_session("vault-1", 1000);
        let id2 = mgr.create_session("vault-1", 2000);
        assert_ne!(id1, id2);
    }
}
