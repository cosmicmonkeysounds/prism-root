//! `network::session::transcript` — ordered event log for a session.
//!
//! `TranscriptTimeline` is an append-only ordered log of session
//! events: joins, leaves, operations, messages, and state snapshots.
//! Each entry has a timestamp and can be replayed for audit or
//! debugging.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

// ── Entry types ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TranscriptEntryKind {
    Join,
    Leave,
    Operation,
    Message,
    Snapshot,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    pub seq: u64,
    pub timestamp_ms: u64,
    pub kind: TranscriptEntryKind,
    pub peer_id: Option<String>,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<JsonValue>,
}

// ── TranscriptTimeline ─────────────────────────────────────────────

pub struct TranscriptTimeline {
    session_id: String,
    entries: Vec<TranscriptEntry>,
    next_seq: u64,
    max_entries: usize,
}

impl TranscriptTimeline {
    pub fn new(session_id: &str, max_entries: usize) -> Self {
        Self {
            session_id: session_id.to_string(),
            entries: Vec::new(),
            next_seq: 1,
            max_entries,
        }
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn append(
        &mut self,
        kind: TranscriptEntryKind,
        peer_id: Option<&str>,
        summary: &str,
        data: Option<JsonValue>,
        timestamp_ms: u64,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        self.entries.push(TranscriptEntry {
            seq,
            timestamp_ms,
            kind,
            peer_id: peer_id.map(String::from),
            summary: summary.to_string(),
            data,
        });
        if self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
        seq
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn get(&self, seq: u64) -> Option<&TranscriptEntry> {
        self.entries.iter().find(|e| e.seq == seq)
    }

    pub fn entries(&self) -> &[TranscriptEntry] {
        &self.entries
    }

    pub fn entries_since(&self, after_seq: u64) -> Vec<&TranscriptEntry> {
        self.entries.iter().filter(|e| e.seq > after_seq).collect()
    }

    pub fn entries_in_range(&self, start_ms: u64, end_ms: u64) -> Vec<&TranscriptEntry> {
        self.entries
            .iter()
            .filter(|e| e.timestamp_ms >= start_ms && e.timestamp_ms <= end_ms)
            .collect()
    }

    pub fn entries_by_peer(&self, peer_id: &str) -> Vec<&TranscriptEntry> {
        self.entries
            .iter()
            .filter(|e| e.peer_id.as_deref() == Some(peer_id))
            .collect()
    }

    pub fn entries_by_kind(&self, kind: TranscriptEntryKind) -> Vec<&TranscriptEntry> {
        self.entries.iter().filter(|e| e.kind == kind).collect()
    }

    pub fn first_timestamp(&self) -> Option<u64> {
        self.entries.first().map(|e| e.timestamp_ms)
    }

    pub fn last_timestamp(&self) -> Option<u64> {
        self.entries.last().map(|e| e.timestamp_ms)
    }

    pub fn duration_ms(&self) -> u64 {
        match (self.first_timestamp(), self.last_timestamp()) {
            (Some(first), Some(last)) => last.saturating_sub(first),
            _ => 0,
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn export(&self) -> Vec<TranscriptEntry> {
        self.entries.clone()
    }

    pub fn import(&mut self, entries: Vec<TranscriptEntry>) {
        for entry in entries {
            if entry.seq >= self.next_seq {
                self.next_seq = entry.seq + 1;
            }
            self.entries.push(entry);
        }
        self.entries.sort_by_key(|e| e.seq);
        while self.entries.len() > self.max_entries {
            self.entries.remove(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_timeline() -> TranscriptTimeline {
        TranscriptTimeline::new("session-1", 1000)
    }

    #[test]
    fn append_and_get() {
        let mut tl = make_timeline();
        let seq = tl.append(
            TranscriptEntryKind::Join,
            Some("peer-1"),
            "peer-1 joined",
            None,
            1000,
        );
        assert_eq!(seq, 1);
        assert_eq!(tl.len(), 1);
        let entry = tl.get(1).unwrap();
        assert_eq!(entry.kind, TranscriptEntryKind::Join);
        assert_eq!(entry.peer_id.as_deref(), Some("peer-1"));
    }

    #[test]
    fn sequential_sequence_numbers() {
        let mut tl = make_timeline();
        let s1 = tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        let s2 = tl.append(TranscriptEntryKind::Leave, None, "b", None, 2000);
        let s3 = tl.append(TranscriptEntryKind::Operation, None, "c", None, 3000);
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
    }

    #[test]
    fn entries_since() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        tl.append(TranscriptEntryKind::Operation, None, "b", None, 2000);
        tl.append(TranscriptEntryKind::Leave, None, "c", None, 3000);
        let since = tl.entries_since(1);
        assert_eq!(since.len(), 2);
        assert_eq!(since[0].seq, 2);
        assert_eq!(since[1].seq, 3);
    }

    #[test]
    fn entries_in_range() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        tl.append(TranscriptEntryKind::Operation, None, "b", None, 2000);
        tl.append(TranscriptEntryKind::Leave, None, "c", None, 3000);
        let range = tl.entries_in_range(1500, 2500);
        assert_eq!(range.len(), 1);
        assert_eq!(range[0].summary, "b");
    }

    #[test]
    fn entries_by_peer() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, Some("alice"), "a", None, 1000);
        tl.append(TranscriptEntryKind::Join, Some("bob"), "b", None, 2000);
        tl.append(
            TranscriptEntryKind::Operation,
            Some("alice"),
            "c",
            None,
            3000,
        );
        let alice = tl.entries_by_peer("alice");
        assert_eq!(alice.len(), 2);
    }

    #[test]
    fn entries_by_kind() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        tl.append(TranscriptEntryKind::Operation, None, "b", None, 2000);
        tl.append(TranscriptEntryKind::Operation, None, "c", None, 3000);
        let ops = tl.entries_by_kind(TranscriptEntryKind::Operation);
        assert_eq!(ops.len(), 2);
    }

    #[test]
    fn duration_ms() {
        let mut tl = make_timeline();
        assert_eq!(tl.duration_ms(), 0);
        tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        assert_eq!(tl.duration_ms(), 0);
        tl.append(TranscriptEntryKind::Leave, None, "b", None, 5000);
        assert_eq!(tl.duration_ms(), 4000);
    }

    #[test]
    fn max_entries_evicts_oldest() {
        let mut tl = TranscriptTimeline::new("s", 3);
        for i in 0..5 {
            tl.append(
                TranscriptEntryKind::Operation,
                None,
                &format!("op-{i}"),
                None,
                i * 1000,
            );
        }
        assert_eq!(tl.len(), 3);
        assert_eq!(tl.entries()[0].summary, "op-2");
    }

    #[test]
    fn clear() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, None, "a", None, 1000);
        tl.clear();
        assert!(tl.is_empty());
    }

    #[test]
    fn export_and_import() {
        let mut tl = make_timeline();
        tl.append(TranscriptEntryKind::Join, Some("p1"), "joined", None, 1000);
        tl.append(
            TranscriptEntryKind::Operation,
            Some("p1"),
            "edited",
            None,
            2000,
        );
        let exported = tl.export();

        let mut tl2 = TranscriptTimeline::new("session-1", 1000);
        tl2.import(exported);
        assert_eq!(tl2.len(), 2);
        assert_eq!(tl2.get(1).unwrap().summary, "joined");
        // next_seq should be beyond the imported range
        let s = tl2.append(TranscriptEntryKind::Leave, None, "left", None, 3000);
        assert_eq!(s, 3);
    }

    #[test]
    fn session_id_accessor() {
        let tl = make_timeline();
        assert_eq!(tl.session_id(), "session-1");
    }

    #[test]
    fn get_unknown_seq_returns_none() {
        let tl = make_timeline();
        assert!(tl.get(999).is_none());
    }

    #[test]
    fn entry_with_data() {
        let mut tl = make_timeline();
        let data = serde_json::json!({"field": "name", "old": "A", "new": "B"});
        tl.append(
            TranscriptEntryKind::Operation,
            Some("p1"),
            "field change",
            Some(data.clone()),
            1000,
        );
        let entry = tl.get(1).unwrap();
        assert_eq!(entry.data, Some(data));
    }
}
