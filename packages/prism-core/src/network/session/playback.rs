//! `network::session::playback` — transcript replay controller.
//!
//! `PlaybackController` drives replay of a `TranscriptTimeline` at
//! variable speed. Host-driven: the host calls `tick(now_ms)` from
//! its event loop and the controller yields entries that should be
//! "replayed" at the current playback position.

use serde::{Deserialize, Serialize};

use super::transcript::TranscriptEntry;

// ── Playback state ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaybackState {
    Stopped,
    Playing,
    Paused,
}

// ── PlaybackController ─────────────────────────────────────────────

pub struct PlaybackController {
    entries: Vec<TranscriptEntry>,
    state: PlaybackState,
    speed: f64,
    cursor: usize,
    origin_ms: u64,
    playback_start_ms: u64,
    pause_offset_ms: u64,
}

impl PlaybackController {
    pub fn new(entries: Vec<TranscriptEntry>) -> Self {
        let origin_ms = entries.first().map(|e| e.timestamp_ms).unwrap_or(0);
        Self {
            entries,
            state: PlaybackState::Stopped,
            speed: 1.0,
            cursor: 0,
            origin_ms,
            playback_start_ms: 0,
            pause_offset_ms: 0,
        }
    }

    pub fn state(&self) -> PlaybackState {
        self.state
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn set_speed(&mut self, speed: f64) {
        self.speed = speed.clamp(0.1, 100.0);
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn duration_ms(&self) -> u64 {
        if self.entries.len() < 2 {
            return 0;
        }
        self.entries.last().unwrap().timestamp_ms - self.origin_ms
    }

    pub fn play(&mut self, now_ms: u64) {
        match self.state {
            PlaybackState::Stopped => {
                self.cursor = 0;
                self.playback_start_ms = now_ms;
                self.pause_offset_ms = 0;
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Paused => {
                self.playback_start_ms = now_ms - self.pause_offset_ms;
                self.state = PlaybackState::Playing;
            }
            PlaybackState::Playing => {}
        }
    }

    pub fn pause(&mut self, now_ms: u64) {
        if self.state == PlaybackState::Playing {
            self.pause_offset_ms = now_ms - self.playback_start_ms;
            self.state = PlaybackState::Paused;
        }
    }

    pub fn stop(&mut self) {
        self.state = PlaybackState::Stopped;
        self.cursor = 0;
        self.pause_offset_ms = 0;
    }

    pub fn seek(&mut self, position: usize) {
        self.cursor = position.min(self.entries.len());
        if let Some(entry) = self.entries.get(self.cursor) {
            let offset = entry.timestamp_ms - self.origin_ms;
            self.pause_offset_ms = (offset as f64 / self.speed) as u64;
        }
    }

    pub fn tick(&mut self, now_ms: u64) -> Vec<&TranscriptEntry> {
        if self.state != PlaybackState::Playing || self.cursor >= self.entries.len() {
            if self.state == PlaybackState::Playing && self.cursor >= self.entries.len() {
                self.state = PlaybackState::Stopped;
            }
            return Vec::new();
        }

        let elapsed_real = now_ms.saturating_sub(self.playback_start_ms);
        let elapsed_playback = (elapsed_real as f64 * self.speed) as u64;
        let playback_pos = self.origin_ms + elapsed_playback;

        let mut yielded = Vec::new();
        while self.cursor < self.entries.len()
            && self.entries[self.cursor].timestamp_ms <= playback_pos
        {
            yielded.push(&self.entries[self.cursor]);
            self.cursor += 1;
        }

        if self.cursor >= self.entries.len() {
            self.state = PlaybackState::Stopped;
        }

        yielded
    }

    pub fn current_entry(&self) -> Option<&TranscriptEntry> {
        if self.cursor > 0 {
            self.entries.get(self.cursor - 1)
        } else {
            None
        }
    }

    pub fn progress(&self) -> f64 {
        if self.entries.is_empty() {
            return 0.0;
        }
        self.cursor as f64 / self.entries.len() as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::session::transcript::TranscriptEntryKind;

    fn make_entries() -> Vec<TranscriptEntry> {
        vec![
            TranscriptEntry {
                seq: 1,
                timestamp_ms: 1000,
                kind: TranscriptEntryKind::Join,
                peer_id: Some("alice".to_string()),
                summary: "alice joined".to_string(),
                data: None,
            },
            TranscriptEntry {
                seq: 2,
                timestamp_ms: 2000,
                kind: TranscriptEntryKind::Operation,
                peer_id: Some("alice".to_string()),
                summary: "alice edited".to_string(),
                data: None,
            },
            TranscriptEntry {
                seq: 3,
                timestamp_ms: 4000,
                kind: TranscriptEntryKind::Leave,
                peer_id: Some("alice".to_string()),
                summary: "alice left".to_string(),
                data: None,
            },
        ]
    }

    #[test]
    fn initial_state() {
        let pc = PlaybackController::new(make_entries());
        assert_eq!(pc.state(), PlaybackState::Stopped);
        assert_eq!(pc.cursor(), 0);
        assert_eq!(pc.len(), 3);
        assert_eq!(pc.duration_ms(), 3000);
    }

    #[test]
    fn play_and_tick() {
        let mut pc = PlaybackController::new(make_entries());
        pc.play(10_000);
        assert_eq!(pc.state(), PlaybackState::Playing);

        // At t=10_000, playback_pos = 1000+0 = 1000 → yields entry at 1000
        let e = pc.tick(10_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].summary, "alice joined");

        // At t=11_000, playback_pos = 1000+1000 = 2000 → yields entry at 2000
        let e = pc.tick(11_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].summary, "alice edited");

        // At t=12_500, playback_pos = 1000+2500 = 3500 → nothing at 3500
        let e = pc.tick(12_500);
        assert!(e.is_empty());

        // At t=13_000, playback_pos = 1000+3000 = 4000 → yields entry at 4000, stops
        let e = pc.tick(13_000);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].summary, "alice left");
        assert_eq!(pc.state(), PlaybackState::Stopped);
    }

    #[test]
    fn pause_and_resume() {
        let mut pc = PlaybackController::new(make_entries());
        pc.play(10_000);
        pc.tick(10_000);
        assert_eq!(pc.cursor(), 1);

        pc.pause(10_500);
        assert_eq!(pc.state(), PlaybackState::Paused);

        // Tick while paused yields nothing
        let e = pc.tick(11_000);
        assert!(e.is_empty());

        // Resume at 20_000 — should pick up where we left off
        pc.play(20_000);
        assert_eq!(pc.state(), PlaybackState::Playing);

        // 500ms of real time had elapsed before pause; resume adds that offset
        // playback_pos at t=20_500 = 1000+1000 = 2000
        let e = pc.tick(20_500);
        assert_eq!(e.len(), 1);
        assert_eq!(e[0].summary, "alice edited");
    }

    #[test]
    fn stop_resets() {
        let mut pc = PlaybackController::new(make_entries());
        pc.play(10_000);
        pc.tick(11_000);
        pc.stop();
        assert_eq!(pc.state(), PlaybackState::Stopped);
        assert_eq!(pc.cursor(), 0);
    }

    #[test]
    fn speed_2x() {
        let mut pc = PlaybackController::new(make_entries());
        pc.set_speed(2.0);
        pc.play(10_000);

        // At 2x speed, 500ms real = 1000ms playback → pos = 1000+1000 = 2000
        let e = pc.tick(10_500);
        assert_eq!(e.len(), 2); // entries at 1000 and 2000
    }

    #[test]
    fn speed_clamped() {
        let mut pc = PlaybackController::new(make_entries());
        pc.set_speed(0.01);
        assert!((pc.speed() - 0.1).abs() < f64::EPSILON);
        pc.set_speed(200.0);
        assert!((pc.speed() - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn seek() {
        let mut pc = PlaybackController::new(make_entries());
        pc.seek(2);
        assert_eq!(pc.cursor(), 2);
    }

    #[test]
    fn seek_clamps_to_len() {
        let mut pc = PlaybackController::new(make_entries());
        pc.seek(100);
        assert_eq!(pc.cursor(), 3);
    }

    #[test]
    fn progress() {
        let mut pc = PlaybackController::new(make_entries());
        assert!((pc.progress() - 0.0).abs() < f64::EPSILON);
        pc.play(10_000);
        pc.tick(10_000);
        assert!((pc.progress() - 1.0 / 3.0).abs() < 0.01);
    }

    #[test]
    fn empty_entries() {
        let mut pc = PlaybackController::new(Vec::new());
        assert!(pc.is_empty());
        assert_eq!(pc.duration_ms(), 0);
        pc.play(10_000);
        let e = pc.tick(20_000);
        assert!(e.is_empty());
    }

    #[test]
    fn current_entry() {
        let mut pc = PlaybackController::new(make_entries());
        assert!(pc.current_entry().is_none());
        pc.play(10_000);
        pc.tick(10_000);
        assert_eq!(pc.current_entry().unwrap().summary, "alice joined");
    }

    #[test]
    fn tick_while_stopped_yields_nothing() {
        let mut pc = PlaybackController::new(make_entries());
        let e = pc.tick(10_000);
        assert!(e.is_empty());
    }
}
