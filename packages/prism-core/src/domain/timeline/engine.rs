//! Timeline engine + tempo map + manual clock.
//!
//! Port of `packages/prism-core/src/domain/timeline/timeline.ts` at
//! commit 8426588. The TS module returned a `TimelineEngine`
//! interface object built from closures; the Rust port collapses
//! that into a concrete [`TimelineEngine`] struct that owns its
//! tracks, markers, listeners, and tempo map. The clock is plug-in
//! via the [`TimelineClock`](super::types::TimelineClock) trait.
//!
//! ID generation matches TS: a single `next_id` counter shared
//! across the module, resettable via [`reset_id_counter`] so tests
//! can produce deterministic IDs.

use std::cell::Cell;
use std::collections::BTreeMap;

use serde_json::{json, Value};

use super::types::{
    AutomationLane, AutomationPoint, ClipInput, InterpolationMode, LoopRegion, MusicalPosition,
    TempoMarker, TimeSeconds, TimeSignature, TimelineClip, TimelineClock, TimelineEvent,
    TimelineEventKind, TimelineMarker, TimelineTrack, TrackKind, TrackUpdates, TransportState,
    TransportStatus,
};

// ── Errors ─────────────────────────────────────────────────────────

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TimelineError {
    #[error("Track not found: {0}")]
    TrackNotFound(String),
    #[error("Clip not found: {0}")]
    ClipNotFound(String),
    #[error("Automation lane not found: {0}")]
    LaneNotFound(String),
    #[error("Marker not found: {0}")]
    MarkerNotFound(String),
    #[error("Track is locked")]
    TrackLocked,
    #[error("Target track is locked")]
    TargetTrackLocked,
    #[error("Clip is locked")]
    ClipLocked,
    #[error("Speed must be positive")]
    InvalidSpeed,
    #[error("Duration must be positive")]
    InvalidDuration,
    #[error("No tempo markers")]
    NoTempoMarkers,
    #[error("No time signatures")]
    NoTimeSignatures,
}

// ── ID Generation ──────────────────────────────────────────────────
//
// The TS port uses a module-scoped `let nextId = 1`. Mirroring that
// requires a single mutable counter — a thread-local `Cell` keeps
// the same shape without a global mutex.

thread_local! {
    static NEXT_ID: Cell<u64> = const { Cell::new(1) };
}

fn gen_id(prefix: &str) -> String {
    NEXT_ID.with(|c| {
        let id = c.get();
        c.set(id + 1);
        format!("{prefix}_{id}")
    })
}

/// Reset the ID counter — for tests that compare against literal IDs.
pub fn reset_id_counter() {
    NEXT_ID.with(|c| c.set(1));
}

// ── Tempo Map ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy)]
struct TimeSigEntry {
    time: TimeSeconds,
    sig: TimeSignature,
}

/// Mutable tempo / time-signature map. Counterpart of the TS
/// `TempoMap` interface — exposes `tempo_at`, `to_musical`,
/// `to_seconds`, `time_signature_at`, `add_tempo`, `set_time_signature`,
/// and `get_tempo_markers`.
#[derive(Debug, Clone)]
pub struct TempoMap {
    pub ppq: u32,
    tempo_markers: Vec<TempoMarker>,
    time_sigs: Vec<TimeSigEntry>,
}

impl TempoMap {
    fn sort_markers(&mut self) {
        self.tempo_markers.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    fn sort_time_sigs(&mut self) {
        self.time_sigs.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    fn find_active_marker(&self, time: TimeSeconds) -> Result<&TempoMarker, TimelineError> {
        let mut active = self
            .tempo_markers
            .first()
            .ok_or(TimelineError::NoTempoMarkers)?;
        for m in self.tempo_markers.iter().skip(1) {
            if m.time <= time {
                active = m;
            } else {
                break;
            }
        }
        Ok(active)
    }

    fn find_active_time_sig(&self, time: TimeSeconds) -> Result<TimeSignature, TimelineError> {
        let mut active = self
            .time_sigs
            .first()
            .ok_or(TimelineError::NoTimeSignatures)?;
        for entry in self.time_sigs.iter().skip(1) {
            if entry.time <= time {
                active = entry;
            } else {
                break;
            }
        }
        Ok(active.sig)
    }

    fn seconds_per_beat(bpm: f64) -> f64 {
        60.0 / bpm
    }

    pub fn tempo_at(&self, time: TimeSeconds) -> f64 {
        self.find_active_marker(time).map(|m| m.bpm).unwrap_or(0.0)
    }

    pub fn to_musical(&self, time: TimeSeconds) -> MusicalPosition {
        let mut total_beats = 0.0_f64;
        let mut remaining = time;

        for i in 0..self.tempo_markers.len() {
            let marker = self.tempo_markers[i];
            let next = self.tempo_markers.get(i + 1).copied();
            let spb = Self::seconds_per_beat(marker.bpm);

            if let Some(next_marker) = next {
                let region_duration = next_marker.time - marker.time;
                if remaining <= region_duration {
                    total_beats += remaining / spb;
                    break;
                }
                total_beats += region_duration / spb;
                remaining -= region_duration;
            } else {
                total_beats += remaining / spb;
                break;
            }
        }

        let sig = self.find_active_time_sig(time).unwrap_or(TimeSignature {
            numerator: 4,
            denominator: 4,
        });
        let beats_per_bar = sig.numerator as f64;
        let total_ticks = (total_beats * self.ppq as f64).round() as u64;
        let ticks_per_bar = (beats_per_bar * self.ppq as f64) as u64;
        let bar = (total_ticks / ticks_per_bar.max(1)) as u32 + 1;
        let remaining_ticks = total_ticks % ticks_per_bar.max(1);
        let beat = (remaining_ticks / self.ppq as u64) as u32 + 1;
        let tick = (remaining_ticks % self.ppq as u64) as u32;

        MusicalPosition { bar, beat, tick }
    }

    pub fn to_seconds(&self, position: MusicalPosition) -> TimeSeconds {
        let sig = match self.time_sigs.first() {
            Some(s) => s.sig,
            None => return 0.0,
        };
        let beats_per_bar = sig.numerator as f64;
        let total_beats = (position.bar as f64 - 1.0) * beats_per_bar
            + (position.beat as f64 - 1.0)
            + position.tick as f64 / self.ppq as f64;

        let mut beats_remaining = total_beats;
        let mut seconds = 0.0_f64;

        for i in 0..self.tempo_markers.len() {
            let marker = self.tempo_markers[i];
            let next = self.tempo_markers.get(i + 1).copied();
            let spb = Self::seconds_per_beat(marker.bpm);

            if let Some(next_marker) = next {
                let region_duration = next_marker.time - marker.time;
                let region_beats = region_duration / spb;
                if beats_remaining <= region_beats {
                    seconds += beats_remaining * spb;
                    return seconds;
                }
                seconds += region_duration;
                beats_remaining -= region_beats;
            } else {
                seconds += beats_remaining * spb;
                return seconds;
            }
        }

        seconds
    }

    pub fn time_signature_at(&self, time: TimeSeconds) -> TimeSignature {
        self.find_active_time_sig(time).unwrap_or(TimeSignature {
            numerator: 4,
            denominator: 4,
        })
    }

    pub fn add_tempo(&mut self, marker: TempoMarker) {
        let idx = self
            .tempo_markers
            .iter()
            .position(|m| (m.time - marker.time).abs() < 1e-9);
        if let Some(i) = idx {
            self.tempo_markers[i] = marker;
        } else {
            self.tempo_markers.push(marker);
        }
        self.sort_markers();
    }

    pub fn set_time_signature(&mut self, time: TimeSeconds, sig: TimeSignature) {
        let idx = self
            .time_sigs
            .iter()
            .position(|e| (e.time - time).abs() < 1e-9);
        if let Some(i) = idx {
            self.time_sigs[i] = TimeSigEntry { time, sig };
        } else {
            self.time_sigs.push(TimeSigEntry { time, sig });
        }
        self.sort_time_sigs();
    }

    pub fn get_tempo_markers(&self) -> Vec<TempoMarker> {
        self.tempo_markers.clone()
    }
}

pub fn create_tempo_map(ppq: u32, initial_bpm: f64) -> TempoMap {
    TempoMap {
        ppq,
        tempo_markers: vec![TempoMarker {
            time: 0.0,
            bpm: initial_bpm,
        }],
        time_sigs: vec![TimeSigEntry {
            time: 0.0,
            sig: TimeSignature {
                numerator: 4,
                denominator: 4,
            },
        }],
    }
}

// ── Manual Clock ───────────────────────────────────────────────────

struct ScheduledCallback {
    id: String,
    time: TimeSeconds,
    callback: Box<dyn FnMut()>,
}

/// In-memory, manually-advanced clock used by tests. Unlike the TS
/// version, callbacks use `Box<dyn FnMut()>` because Rust closures
/// must own their captures.
pub struct ManualClock {
    current_time: TimeSeconds,
    handle_counter: u64,
    scheduled: Vec<ScheduledCallback>,
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl ManualClock {
    pub fn new() -> Self {
        Self {
            current_time: 0.0,
            handle_counter: 0,
            scheduled: Vec::new(),
        }
    }

    fn fire_scheduled(&mut self) {
        // Collect the indices to fire, sorted by scheduled time.
        let mut to_fire: Vec<usize> = self
            .scheduled
            .iter()
            .enumerate()
            .filter(|(_, s)| s.time <= self.current_time)
            .map(|(i, _)| i)
            .collect();
        to_fire.sort_by(|a, b| {
            self.scheduled[*a]
                .time
                .partial_cmp(&self.scheduled[*b].time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // Drain in reverse-index order so removals don't shift later
        // entries, then fire callbacks in ascending-time order to
        // match the TS sort.
        let mut to_remove = to_fire.clone();
        to_remove.sort_by(|a, b| b.cmp(a));
        let mut pulled: Vec<(TimeSeconds, Box<dyn FnMut()>)> = Vec::with_capacity(to_fire.len());
        for idx in &to_remove {
            let entry = self.scheduled.remove(*idx);
            pulled.push((entry.time, entry.callback));
        }
        pulled.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        for (_, cb) in pulled.iter_mut() {
            cb();
        }
    }

    pub fn advance(&mut self, seconds: TimeSeconds) {
        self.current_time += seconds;
        self.fire_scheduled();
    }

    pub fn set_time(&mut self, time: TimeSeconds) {
        self.current_time = time;
        self.fire_scheduled();
    }
}

impl TimelineClock for ManualClock {
    fn now(&self) -> TimeSeconds {
        self.current_time
    }

    fn schedule(&mut self, time: TimeSeconds, callback: Box<dyn FnMut()>) -> String {
        self.handle_counter += 1;
        let id = format!("sched_{}", self.handle_counter);
        self.scheduled.push(ScheduledCallback {
            id: id.clone(),
            time,
            callback,
        });
        id
    }

    fn cancel(&mut self, handle: &str) {
        if let Some(idx) = self.scheduled.iter().position(|s| s.id == handle) {
            self.scheduled.remove(idx);
        }
    }

    fn start(&mut self) {}
    fn stop(&mut self) {}
}

pub fn create_manual_clock() -> ManualClock {
    ManualClock::new()
}

// ── Automation Evaluation ──────────────────────────────────────────

fn evaluate_automation(lane: &AutomationLane, time: TimeSeconds) -> f64 {
    if lane.points.is_empty() {
        return lane.default_value;
    }

    let first = lane.points[0];
    if time <= first.time {
        return first.value;
    }

    let last = lane.points[lane.points.len() - 1];
    if time >= last.time {
        return last.value;
    }

    let mut prev = first;
    let mut next = first;
    for i in 0..lane.points.len() - 1 {
        let p = lane.points[i];
        let pn = lane.points[i + 1];
        if time >= p.time && time <= pn.time {
            prev = p;
            next = pn;
            break;
        }
    }

    if (prev.time - next.time).abs() < f64::EPSILON {
        return prev.value;
    }

    match prev.interpolation {
        InterpolationMode::Step => prev.value,
        InterpolationMode::Linear => {
            let t = (time - prev.time) / (next.time - prev.time);
            prev.value + t * (next.value - prev.value)
        }
        InterpolationMode::Bezier => {
            let t = (time - prev.time) / (next.time - prev.time);
            let smooth = t * t * (3.0 - 2.0 * t);
            prev.value + smooth * (next.value - prev.value)
        }
    }
}

// ── Listener Registry ──────────────────────────────────────────────

type ListenerId = u64;

struct Listener {
    id: ListenerId,
    callback: Box<dyn FnMut(&TimelineEvent)>,
}

// ── Engine ─────────────────────────────────────────────────────────

pub struct TimelineEngineOptions {
    pub clock: Option<Box<dyn TimelineClock>>,
    pub ppq: u32,
    pub bpm: f64,
}

impl Default for TimelineEngineOptions {
    fn default() -> Self {
        Self {
            clock: None,
            ppq: 480,
            bpm: 120.0,
        }
    }
}

/// Side-effectful timeline engine. Owns its tracks, markers,
/// listeners, transport state, and tempo map. Created via
/// [`create_timeline_engine`]. The clock is plug-in: pass any
/// `Box<dyn TimelineClock>` through [`TimelineEngineOptions::clock`].
pub struct TimelineEngine {
    clock: Box<dyn TimelineClock>,
    tempo_map: TempoMap,
    tracks: Vec<TimelineTrack>,
    markers: Vec<TimelineMarker>,
    listeners: Vec<Listener>,
    next_listener_id: ListenerId,
    transport: TransportState,
}

pub fn create_timeline_engine(options: TimelineEngineOptions) -> TimelineEngine {
    let clock: Box<dyn TimelineClock> = options
        .clock
        .unwrap_or_else(|| Box::new(create_manual_clock()));
    let tempo_map = create_tempo_map(options.ppq, options.bpm);
    TimelineEngine {
        clock,
        tempo_map,
        tracks: Vec::new(),
        markers: Vec::new(),
        listeners: Vec::new(),
        next_listener_id: 0,
        transport: TransportState {
            status: TransportStatus::Stopped,
            position: 0.0,
            speed: 1.0,
            loop_region: LoopRegion {
                enabled: false,
                start: 0.0,
                end: 0.0,
            },
        },
    }
}

impl TimelineEngine {
    fn emit(&mut self, kind: TimelineEventKind, data: BTreeMap<String, Value>) {
        let event = TimelineEvent {
            kind,
            timestamp: chrono::Utc::now().timestamp_millis(),
            data,
        };
        for listener in self.listeners.iter_mut() {
            (listener.callback)(&event);
        }
    }

    fn require_track_mut(&mut self, track_id: &str) -> Result<&mut TimelineTrack, TimelineError> {
        self.tracks
            .iter_mut()
            .find(|t| t.id == track_id)
            .ok_or_else(|| TimelineError::TrackNotFound(track_id.to_string()))
    }

    fn require_track_index(&self, track_id: &str) -> Result<usize, TimelineError> {
        self.tracks
            .iter()
            .position(|t| t.id == track_id)
            .ok_or_else(|| TimelineError::TrackNotFound(track_id.to_string()))
    }

    fn find_clip_indexes(&self, clip_id: &str) -> Option<(usize, usize)> {
        for (ti, track) in self.tracks.iter().enumerate() {
            if let Some(ci) = track.clips.iter().position(|c| c.id == clip_id) {
                return Some((ti, ci));
            }
        }
        None
    }

    // ── Transport ─────────────────────────────────────────────────

    pub fn play(&mut self) {
        if self.transport.status == TransportStatus::Playing {
            return;
        }
        self.transport.status = TransportStatus::Playing;
        self.clock.start();
        let position = self.transport.position;
        let mut data = BTreeMap::new();
        data.insert("position".into(), json!(position));
        self.emit(TimelineEventKind::TransportPlay, data);
    }

    pub fn pause(&mut self) {
        if self.transport.status != TransportStatus::Playing {
            return;
        }
        self.transport.status = TransportStatus::Paused;
        self.clock.stop();
        let position = self.transport.position;
        let mut data = BTreeMap::new();
        data.insert("position".into(), json!(position));
        self.emit(TimelineEventKind::TransportPause, data);
    }

    pub fn stop(&mut self) {
        self.transport.status = TransportStatus::Stopped;
        self.transport.position = 0.0;
        self.clock.stop();
        self.emit(TimelineEventKind::TransportStop, BTreeMap::new());
    }

    pub fn seek(&mut self, mut time: TimeSeconds) {
        if time < 0.0 {
            time = 0.0;
        }
        self.transport.position = time;
        let mut data = BTreeMap::new();
        data.insert("position".into(), json!(time));
        self.emit(TimelineEventKind::TransportSeek, data);
    }

    pub fn scrub(&mut self, mut time: TimeSeconds) {
        if time < 0.0 {
            time = 0.0;
        }
        self.transport.position = time;
        let mut data = BTreeMap::new();
        data.insert("position".into(), json!(time));
        data.insert("scrub".into(), json!(true));
        self.emit(TimelineEventKind::TransportSeek, data);
    }

    pub fn set_speed(&mut self, speed: f64) -> Result<(), TimelineError> {
        if speed <= 0.0 {
            return Err(TimelineError::InvalidSpeed);
        }
        self.transport.speed = speed;
        Ok(())
    }

    pub fn set_loop(&mut self, region: LoopRegion) {
        self.transport.loop_region = region;
        let mut data = BTreeMap::new();
        data.insert("enabled".into(), json!(region.enabled));
        data.insert("start".into(), json!(region.start));
        data.insert("end".into(), json!(region.end));
        self.emit(TimelineEventKind::TransportLoop, data);
    }

    pub fn get_transport(&self) -> TransportState {
        self.transport
    }

    // ── Tracks ────────────────────────────────────────────────────

    pub fn add_track(&mut self, kind: TrackKind, name: impl Into<String>) -> TimelineTrack {
        let name = name.into();
        let track = TimelineTrack {
            id: gen_id("track"),
            name: name.clone(),
            kind,
            muted: false,
            solo: false,
            locked: false,
            gain: 1.0,
            clips: Vec::new(),
            automation_lanes: Vec::new(),
        };
        self.tracks.push(track.clone());

        let mut data = BTreeMap::new();
        data.insert("trackId".into(), json!(track.id));
        data.insert("kind".into(), json!(track.kind));
        data.insert("name".into(), json!(name));
        self.emit(TimelineEventKind::TrackAdded, data);
        track
    }

    pub fn remove_track(&mut self, track_id: &str) -> Result<(), TimelineError> {
        let idx = self.require_track_index(track_id)?;
        self.tracks.remove(idx);
        let mut data = BTreeMap::new();
        data.insert("trackId".into(), json!(track_id));
        self.emit(TimelineEventKind::TrackRemoved, data);
        Ok(())
    }

    pub fn get_track(&self, track_id: &str) -> Option<TimelineTrack> {
        self.tracks.iter().find(|t| t.id == track_id).cloned()
    }

    pub fn get_tracks(&self) -> Vec<TimelineTrack> {
        self.tracks.clone()
    }

    pub fn update_track(
        &mut self,
        track_id: &str,
        updates: TrackUpdates,
    ) -> Result<(), TimelineError> {
        {
            let track = self.require_track_mut(track_id)?;
            if let Some(name) = updates.name.clone() {
                track.name = name;
            }
            if let Some(m) = updates.muted {
                track.muted = m;
            }
            if let Some(s) = updates.solo {
                track.solo = s;
            }
            if let Some(l) = updates.locked {
                track.locked = l;
            }
            if let Some(g) = updates.gain {
                track.gain = g;
            }
        }
        let mut data = BTreeMap::new();
        data.insert("trackId".into(), json!(track_id));
        data.insert(
            "updates".into(),
            json!({
                "name": updates.name,
                "muted": updates.muted,
                "solo": updates.solo,
                "locked": updates.locked,
                "gain": updates.gain,
            }),
        );
        self.emit(TimelineEventKind::TrackUpdated, data);
        Ok(())
    }

    // ── Clips ─────────────────────────────────────────────────────

    pub fn add_clip(
        &mut self,
        track_id: &str,
        clip_data: ClipInput,
    ) -> Result<TimelineClip, TimelineError> {
        let clip;
        {
            let track = self.require_track_mut(track_id)?;
            if track.locked {
                return Err(TimelineError::TrackLocked);
            }
            clip = TimelineClip {
                id: gen_id("clip"),
                track_id: track_id.to_string(),
                name: clip_data.name,
                start_time: clip_data.start_time,
                duration: clip_data.duration,
                source_offset: clip_data.source_offset,
                source_ref: clip_data.source_ref,
                muted: clip_data.muted,
                locked: clip_data.locked,
                gain: clip_data.gain,
            };
            track.clips.push(clip.clone());
        }
        let mut data = BTreeMap::new();
        data.insert("trackId".into(), json!(track_id));
        data.insert("clipId".into(), json!(clip.id));
        self.emit(TimelineEventKind::ClipAdded, data);
        Ok(clip)
    }

    pub fn remove_clip(&mut self, track_id: &str, clip_id: &str) -> Result<(), TimelineError> {
        {
            let track = self.require_track_mut(track_id)?;
            let idx = track
                .clips
                .iter()
                .position(|c| c.id == clip_id)
                .ok_or_else(|| TimelineError::ClipNotFound(clip_id.to_string()))?;
            track.clips.remove(idx);
        }
        let mut data = BTreeMap::new();
        data.insert("trackId".into(), json!(track_id));
        data.insert("clipId".into(), json!(clip_id));
        self.emit(TimelineEventKind::ClipRemoved, data);
        Ok(())
    }

    pub fn move_clip(
        &mut self,
        clip_id: &str,
        target_track_id: &str,
        new_start_time: TimeSeconds,
    ) -> Result<(), TimelineError> {
        let (source_idx, clip_idx) = self
            .find_clip_indexes(clip_id)
            .ok_or_else(|| TimelineError::ClipNotFound(clip_id.to_string()))?;
        if self.tracks[source_idx].clips[clip_idx].locked {
            return Err(TimelineError::ClipLocked);
        }
        let target_idx = self.require_track_index(target_track_id)?;
        if self.tracks[target_idx].locked {
            return Err(TimelineError::TargetTrackLocked);
        }

        let mut clip = self.tracks[source_idx].clips.remove(clip_idx);
        let from_track_id = self.tracks[source_idx].id.clone();
        clip.track_id = target_track_id.to_string();
        clip.start_time = new_start_time;
        self.tracks[target_idx].clips.push(clip);

        let mut data = BTreeMap::new();
        data.insert("clipId".into(), json!(clip_id));
        data.insert("fromTrackId".into(), json!(from_track_id));
        data.insert("toTrackId".into(), json!(target_track_id));
        data.insert("newStartTime".into(), json!(new_start_time));
        self.emit(TimelineEventKind::ClipMoved, data);
        Ok(())
    }

    pub fn trim_clip(
        &mut self,
        clip_id: &str,
        new_start_time: TimeSeconds,
        new_duration: TimeSeconds,
    ) -> Result<(), TimelineError> {
        let (ti, ci) = self
            .find_clip_indexes(clip_id)
            .ok_or_else(|| TimelineError::ClipNotFound(clip_id.to_string()))?;
        let clip = &mut self.tracks[ti].clips[ci];
        if clip.locked {
            return Err(TimelineError::ClipLocked);
        }
        if new_duration <= 0.0 {
            return Err(TimelineError::InvalidDuration);
        }
        let old_start = clip.start_time;
        let delta = new_start_time - old_start;
        clip.start_time = new_start_time;
        clip.duration = new_duration;
        clip.source_offset += delta;

        let mut data = BTreeMap::new();
        data.insert("clipId".into(), json!(clip_id));
        data.insert("newStartTime".into(), json!(new_start_time));
        data.insert("newDuration".into(), json!(new_duration));
        self.emit(TimelineEventKind::ClipTrimmed, data);
        Ok(())
    }

    pub fn get_clip(&self, clip_id: &str) -> Option<TimelineClip> {
        let (ti, ci) = self.find_clip_indexes(clip_id)?;
        Some(self.tracks[ti].clips[ci].clone())
    }

    // ── Automation ────────────────────────────────────────────────

    pub fn add_automation_lane(
        &mut self,
        track_id: &str,
        parameter: impl Into<String>,
        default_value: f64,
    ) -> Result<AutomationLane, TimelineError> {
        let track = self.require_track_mut(track_id)?;
        let lane = AutomationLane {
            id: gen_id("lane"),
            parameter: parameter.into(),
            default_value,
            points: Vec::new(),
        };
        track.automation_lanes.push(lane.clone());
        Ok(lane)
    }

    pub fn remove_automation_lane(
        &mut self,
        track_id: &str,
        lane_id: &str,
    ) -> Result<(), TimelineError> {
        let track = self.require_track_mut(track_id)?;
        let idx = track
            .automation_lanes
            .iter()
            .position(|l| l.id == lane_id)
            .ok_or_else(|| TimelineError::LaneNotFound(lane_id.to_string()))?;
        track.automation_lanes.remove(idx);
        Ok(())
    }

    pub fn add_automation_point(
        &mut self,
        track_id: &str,
        lane_id: &str,
        point: AutomationPoint,
    ) -> Result<(), TimelineError> {
        let track = self.require_track_mut(track_id)?;
        let lane = track
            .automation_lanes
            .iter_mut()
            .find(|l| l.id == lane_id)
            .ok_or_else(|| TimelineError::LaneNotFound(lane_id.to_string()))?;
        let insert_idx = lane.points.iter().position(|p| p.time > point.time);
        match insert_idx {
            Some(i) => lane.points.insert(i, point),
            None => lane.points.push(point),
        }
        Ok(())
    }

    pub fn remove_automation_point(
        &mut self,
        track_id: &str,
        lane_id: &str,
        time: TimeSeconds,
    ) -> Result<(), TimelineError> {
        let track = self.require_track_mut(track_id)?;
        let lane = track
            .automation_lanes
            .iter_mut()
            .find(|l| l.id == lane_id)
            .ok_or_else(|| TimelineError::LaneNotFound(lane_id.to_string()))?;
        if let Some(idx) = lane
            .points
            .iter()
            .position(|p| (p.time - time).abs() < 1e-9)
        {
            lane.points.remove(idx);
        }
        Ok(())
    }

    pub fn get_automation_value(
        &self,
        track_id: &str,
        lane_id: &str,
        time: TimeSeconds,
    ) -> Result<f64, TimelineError> {
        let track = self
            .tracks
            .iter()
            .find(|t| t.id == track_id)
            .ok_or_else(|| TimelineError::TrackNotFound(track_id.to_string()))?;
        let lane = track
            .automation_lanes
            .iter()
            .find(|l| l.id == lane_id)
            .ok_or_else(|| TimelineError::LaneNotFound(lane_id.to_string()))?;
        Ok(evaluate_automation(lane, time))
    }

    // ── Markers ───────────────────────────────────────────────────

    pub fn add_marker(
        &mut self,
        time: TimeSeconds,
        label: impl Into<String>,
        color: Option<&str>,
    ) -> TimelineMarker {
        let marker = TimelineMarker {
            id: gen_id("marker"),
            time,
            label: label.into(),
            color: color.unwrap_or("#ffcc00").to_string(),
        };
        self.markers.push(marker.clone());
        self.markers.sort_by(|a, b| {
            a.time
                .partial_cmp(&b.time)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut data = BTreeMap::new();
        data.insert("markerId".into(), json!(marker.id));
        data.insert("time".into(), json!(time));
        data.insert("label".into(), json!(marker.label));
        self.emit(TimelineEventKind::MarkerAdded, data);
        marker
    }

    pub fn remove_marker(&mut self, marker_id: &str) -> Result<(), TimelineError> {
        let idx = self
            .markers
            .iter()
            .position(|m| m.id == marker_id)
            .ok_or_else(|| TimelineError::MarkerNotFound(marker_id.to_string()))?;
        self.markers.remove(idx);
        let mut data = BTreeMap::new();
        data.insert("markerId".into(), json!(marker_id));
        self.emit(TimelineEventKind::MarkerRemoved, data);
        Ok(())
    }

    pub fn get_markers(&self) -> Vec<TimelineMarker> {
        self.markers.clone()
    }

    // ── Tempo ─────────────────────────────────────────────────────

    pub fn get_tempo_map(&self) -> &TempoMap {
        &self.tempo_map
    }

    pub fn get_tempo_map_mut(&mut self) -> &mut TempoMap {
        &mut self.tempo_map
    }

    // ── Queries ───────────────────────────────────────────────────

    pub fn get_duration(&self) -> TimeSeconds {
        let mut max_end = 0.0_f64;
        for track in &self.tracks {
            for clip in &track.clips {
                let end = clip.start_time + clip.duration;
                if end > max_end {
                    max_end = end;
                }
            }
        }
        max_end
    }

    pub fn get_clips_at_time(&self, time: TimeSeconds) -> Vec<TimelineClip> {
        let mut result = Vec::new();
        for track in &self.tracks {
            for clip in &track.clips {
                if time >= clip.start_time && time < clip.start_time + clip.duration {
                    result.push(clip.clone());
                }
            }
        }
        result
    }

    // ── Events ────────────────────────────────────────────────────

    /// Subscribe a listener to timeline events. Returns a handle ID
    /// that can be passed to [`unsubscribe`](Self::unsubscribe). The
    /// TS port returned a closure that called `splice`; in Rust the
    /// closure approach would require interior mutability on the
    /// engine, so the handle-based shape is more honest.
    pub fn subscribe(&mut self, callback: Box<dyn FnMut(&TimelineEvent)>) -> ListenerId {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push(Listener { id, callback });
        id
    }

    pub fn unsubscribe(&mut self, id: ListenerId) {
        self.listeners.retain(|l| l.id != id);
    }

    // ── Lifecycle ─────────────────────────────────────────────────

    pub fn dispose(&mut self) {
        self.clock.stop();
        self.tracks.clear();
        self.markers.clear();
        self.listeners.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::rc::Rc;

    type SharedEvents = Rc<RefCell<Vec<TimelineEvent>>>;

    fn fresh() -> (TimelineEngine, SharedEvents) {
        reset_id_counter();
        let mut engine = create_timeline_engine(TimelineEngineOptions::default());
        let events: SharedEvents = Rc::new(RefCell::new(Vec::new()));
        let captured = events.clone();
        engine.subscribe(Box::new(move |e| captured.borrow_mut().push(e.clone())));
        (engine, events)
    }

    fn make_clip() -> ClipInput {
        ClipInput::default()
    }

    // ── Transport ──

    #[test]
    fn starts_in_stopped_state() {
        let (engine, _) = fresh();
        let t = engine.get_transport();
        assert_eq!(t.status, TransportStatus::Stopped);
        assert_eq!(t.position, 0.0);
        assert_eq!(t.speed, 1.0);
    }

    #[test]
    fn transitions_play_pause_stop() {
        let (mut engine, _) = fresh();
        engine.play();
        assert_eq!(engine.get_transport().status, TransportStatus::Playing);
        engine.pause();
        assert_eq!(engine.get_transport().status, TransportStatus::Paused);
        engine.stop();
        assert_eq!(engine.get_transport().status, TransportStatus::Stopped);
        assert_eq!(engine.get_transport().position, 0.0);
    }

    #[test]
    fn play_is_idempotent() {
        let (mut engine, events) = fresh();
        engine.play();
        engine.play();
        let plays = events
            .borrow()
            .iter()
            .filter(|e| e.kind == TimelineEventKind::TransportPlay)
            .count();
        assert_eq!(plays, 1);
    }

    #[test]
    fn pause_is_no_op_when_not_playing() {
        let (mut engine, events) = fresh();
        engine.pause();
        assert!(events.borrow().is_empty());
    }

    #[test]
    fn seek_updates_position() {
        let (mut engine, events) = fresh();
        engine.seek(10.0);
        assert_eq!(engine.get_transport().position, 10.0);
        assert_eq!(events.borrow()[0].kind, TimelineEventKind::TransportSeek);
    }

    #[test]
    fn seek_clamps_to_zero() {
        let (mut engine, _) = fresh();
        engine.seek(-5.0);
        assert_eq!(engine.get_transport().position, 0.0);
    }

    #[test]
    fn scrub_keeps_play_state() {
        let (mut engine, _) = fresh();
        engine.play();
        engine.scrub(15.0);
        assert_eq!(engine.get_transport().status, TransportStatus::Playing);
        assert_eq!(engine.get_transport().position, 15.0);
    }

    #[test]
    fn set_speed_validates() {
        let (mut engine, _) = fresh();
        engine.set_speed(2.0).unwrap();
        assert_eq!(engine.get_transport().speed, 2.0);
        assert!(engine.set_speed(0.0).is_err());
        assert!(engine.set_speed(-1.0).is_err());
    }

    #[test]
    fn set_loop_configures_region() {
        let (mut engine, _) = fresh();
        engine.set_loop(LoopRegion {
            enabled: true,
            start: 5.0,
            end: 15.0,
        });
        let l = engine.get_transport().loop_region;
        assert!(l.enabled);
        assert_eq!(l.start, 5.0);
        assert_eq!(l.end, 15.0);
    }

    #[test]
    fn emits_transport_events_in_order() {
        let (mut engine, events) = fresh();
        engine.play();
        engine.seek(5.0);
        engine.pause();
        engine.stop();
        let kinds: Vec<TimelineEventKind> = events.borrow().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TimelineEventKind::TransportPlay,
                TimelineEventKind::TransportSeek,
                TimelineEventKind::TransportPause,
                TimelineEventKind::TransportStop,
            ]
        );
    }

    // ── Tracks ──

    #[test]
    fn adds_and_retrieves_tracks() {
        let (mut engine, _) = fresh();
        let track = engine.add_track(TrackKind::Audio, "Vocals");
        assert_eq!(track.name, "Vocals");
        assert_eq!(track.kind, TrackKind::Audio);
        assert!(!track.muted);
        assert_eq!(track.gain, 1.0);
        let r = engine.get_track(&track.id).unwrap();
        assert_eq!(r.name, "Vocals");
    }

    #[test]
    fn lists_all_tracks() {
        let (mut engine, _) = fresh();
        engine.add_track(TrackKind::Audio, "T1");
        engine.add_track(TrackKind::Video, "T2");
        engine.add_track(TrackKind::Midi, "T3");
        assert_eq!(engine.get_tracks().len(), 3);
    }

    #[test]
    fn removes_tracks() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        engine.remove_track(&t.id).unwrap();
        assert_eq!(engine.get_tracks().len(), 0);
    }

    #[test]
    fn remove_non_existent_track_errors() {
        let (mut engine, _) = fresh();
        assert!(engine.remove_track("nonexistent").is_err());
    }

    #[test]
    fn updates_track_properties() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        engine
            .update_track(
                &t.id,
                TrackUpdates {
                    muted: Some(true),
                    gain: Some(0.5),
                    name: Some("Lead Vox".into()),
                    ..Default::default()
                },
            )
            .unwrap();
        let u = engine.get_track(&t.id).unwrap();
        assert!(u.muted);
        assert_eq!(u.gain, 0.5);
        assert_eq!(u.name, "Lead Vox");
    }

    #[test]
    fn get_non_existent_track_returns_none() {
        let (engine, _) = fresh();
        assert!(engine.get_track("nonexistent").is_none());
    }

    #[test]
    fn emits_track_events() {
        let (mut engine, events) = fresh();
        let t = engine.add_track(TrackKind::Lighting, "DMX");
        engine
            .update_track(
                &t.id,
                TrackUpdates {
                    solo: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        engine.remove_track(&t.id).unwrap();
        let kinds: Vec<TimelineEventKind> = events.borrow().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![
                TimelineEventKind::TrackAdded,
                TimelineEventKind::TrackUpdated,
                TimelineEventKind::TrackRemoved,
            ]
        );
    }

    #[test]
    fn returns_defensive_copies_for_tracks() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Test");
        let mut r = engine.get_track(&t.id).unwrap();
        r.name = "Mutated".into();
        assert_eq!(engine.get_track(&t.id).unwrap().name, "Test");
    }

    // ── Clips ──

    #[test]
    fn adds_clips_to_tracks() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let mut input = make_clip();
        input.name = "Verse 1".into();
        input.start_time = 2.0;
        input.duration = 8.0;
        let clip = engine.add_clip(&t.id, input).unwrap();
        assert_eq!(clip.name, "Verse 1");
        assert_eq!(clip.start_time, 2.0);
        assert_eq!(clip.duration, 8.0);
        assert_eq!(clip.track_id, t.id);
    }

    #[test]
    fn retrieves_clips_by_id() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let mut input = make_clip();
        input.name = "Chorus".into();
        let clip = engine.add_clip(&t.id, input).unwrap();
        let r = engine.get_clip(&clip.id).unwrap();
        assert_eq!(r.name, "Chorus");
    }

    #[test]
    fn removes_clips() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let clip = engine.add_clip(&t.id, make_clip()).unwrap();
        engine.remove_clip(&t.id, &clip.id).unwrap();
        assert!(engine.get_clip(&clip.id).is_none());
    }

    #[test]
    fn add_clip_to_locked_track_errors() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        engine
            .update_track(
                &t.id,
                TrackUpdates {
                    locked: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        let err = engine.add_clip(&t.id, make_clip()).unwrap_err();
        assert_eq!(err, TimelineError::TrackLocked);
    }

    #[test]
    fn moves_clips_between_tracks() {
        let (mut engine, _) = fresh();
        let t1 = engine.add_track(TrackKind::Audio, "T1");
        let t2 = engine.add_track(TrackKind::Audio, "T2");
        let mut input = make_clip();
        input.name = "movable".into();
        let clip = engine.add_clip(&t1.id, input).unwrap();
        engine.move_clip(&clip.id, &t2.id, 10.0).unwrap();
        assert_eq!(engine.get_track(&t1.id).unwrap().clips.len(), 0);
        assert_eq!(engine.get_track(&t2.id).unwrap().clips.len(), 1);
        let moved = engine.get_clip(&clip.id).unwrap();
        assert_eq!(moved.track_id, t2.id);
        assert_eq!(moved.start_time, 10.0);
    }

    #[test]
    fn locked_clip_cannot_be_moved() {
        let (mut engine, _) = fresh();
        let t1 = engine.add_track(TrackKind::Audio, "T1");
        let t2 = engine.add_track(TrackKind::Audio, "T2");
        let mut input = make_clip();
        input.locked = true;
        let clip = engine.add_clip(&t1.id, input).unwrap();
        assert_eq!(
            engine.move_clip(&clip.id, &t2.id, 0.0).unwrap_err(),
            TimelineError::ClipLocked
        );
    }

    #[test]
    fn locked_target_track_rejected() {
        let (mut engine, _) = fresh();
        let t1 = engine.add_track(TrackKind::Audio, "T1");
        let t2 = engine.add_track(TrackKind::Audio, "T2");
        engine
            .update_track(
                &t2.id,
                TrackUpdates {
                    locked: Some(true),
                    ..Default::default()
                },
            )
            .unwrap();
        let clip = engine.add_clip(&t1.id, make_clip()).unwrap();
        assert_eq!(
            engine.move_clip(&clip.id, &t2.id, 0.0).unwrap_err(),
            TimelineError::TargetTrackLocked
        );
    }

    #[test]
    fn trim_adjusts_source_offset() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let mut input = make_clip();
        input.start_time = 5.0;
        input.duration = 10.0;
        input.source_offset = 0.0;
        let clip = engine.add_clip(&t.id, input).unwrap();
        engine.trim_clip(&clip.id, 7.0, 6.0).unwrap();
        let trimmed = engine.get_clip(&clip.id).unwrap();
        assert_eq!(trimmed.start_time, 7.0);
        assert_eq!(trimmed.duration, 6.0);
        assert_eq!(trimmed.source_offset, 2.0);
    }

    #[test]
    fn trim_zero_duration_errors() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let clip = engine.add_clip(&t.id, make_clip()).unwrap();
        assert_eq!(
            engine.trim_clip(&clip.id, 0.0, 0.0).unwrap_err(),
            TimelineError::InvalidDuration
        );
    }

    #[test]
    fn emits_clip_events() {
        let (mut engine, events) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        events.borrow_mut().clear();
        let clip = engine.add_clip(&t.id, make_clip()).unwrap();
        engine.remove_clip(&t.id, &clip.id).unwrap();
        let kinds: Vec<TimelineEventKind> = events.borrow().iter().map(|e| e.kind).collect();
        assert_eq!(
            kinds,
            vec![TimelineEventKind::ClipAdded, TimelineEventKind::ClipRemoved,]
        );
    }

    #[test]
    fn returns_defensive_copies_for_clips() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Test");
        let mut input = make_clip();
        input.name = "original".into();
        let clip = engine.add_clip(&t.id, input).unwrap();
        let mut r = engine.get_clip(&clip.id).unwrap();
        r.name = "mutated".into();
        assert_eq!(engine.get_clip(&clip.id).unwrap().name, "original");
    }

    // ── Automation ──

    #[test]
    fn adds_automation_lanes() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.8).unwrap();
        assert_eq!(lane.parameter, "volume");
        assert_eq!(lane.default_value, 0.8);
        assert_eq!(lane.points.len(), 0);
    }

    #[test]
    fn removes_automation_lanes() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        engine.remove_automation_lane(&t.id, &lane.id).unwrap();
        assert_eq!(engine.get_track(&t.id).unwrap().automation_lanes.len(), 0);
    }

    #[test]
    fn adds_points_in_sorted_order() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 5.0,
                    value: 0.5,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 2.0,
                    value: 0.2,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 8.0,
                    value: 0.9,
                    interpolation: InterpolationMode::Step,
                },
            )
            .unwrap();
        let track = engine.get_track(&t.id).unwrap();
        let times: Vec<f64> = track.automation_lanes[0]
            .points
            .iter()
            .map(|p| p.time)
            .collect();
        assert_eq!(times, vec![2.0, 5.0, 8.0]);
    }

    #[test]
    fn removes_points_by_time() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 5.0,
                    value: 0.5,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 10.0,
                    value: 1.0,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        engine
            .remove_automation_point(&t.id, &lane.id, 5.0)
            .unwrap();
        let track = engine.get_track(&t.id).unwrap();
        assert_eq!(track.automation_lanes[0].points.len(), 1);
    }

    #[test]
    fn step_interpolation() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        for (time, value) in [(0.0, 0.0), (5.0, 1.0)] {
            engine
                .add_automation_point(
                    &t.id,
                    &lane.id,
                    AutomationPoint {
                        time,
                        value,
                        interpolation: InterpolationMode::Step,
                    },
                )
                .unwrap();
        }
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 0.0).unwrap(),
            0.0
        );
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 2.5).unwrap(),
            0.0
        );
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 5.0).unwrap(),
            1.0
        );
    }

    #[test]
    fn linear_interpolation() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        for (time, value) in [(0.0, 0.0), (10.0, 1.0)] {
            engine
                .add_automation_point(
                    &t.id,
                    &lane.id,
                    AutomationPoint {
                        time,
                        value,
                        interpolation: InterpolationMode::Linear,
                    },
                )
                .unwrap();
        }
        let v = engine.get_automation_value(&t.id, &lane.id, 5.0).unwrap();
        assert!((v - 0.5).abs() < 1e-9);
        let v = engine.get_automation_value(&t.id, &lane.id, 7.5).unwrap();
        assert!((v - 0.75).abs() < 1e-9);
    }

    #[test]
    fn bezier_smoothstep_interpolation() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        for (time, value) in [(0.0, 0.0), (10.0, 1.0)] {
            engine
                .add_automation_point(
                    &t.id,
                    &lane.id,
                    AutomationPoint {
                        time,
                        value,
                        interpolation: InterpolationMode::Bezier,
                    },
                )
                .unwrap();
        }
        let mid = engine.get_automation_value(&t.id, &lane.id, 5.0).unwrap();
        assert!((mid - 0.5).abs() < 1e-9);
        let q = engine.get_automation_value(&t.id, &lane.id, 2.5).unwrap();
        assert!((q - 0.15625).abs() < 1e-6);
    }

    #[test]
    fn empty_lanes_return_default() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.75).unwrap();
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 5.0).unwrap(),
            0.75
        );
    }

    #[test]
    fn clamps_before_first_after_last() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "Vocals");
        let lane = engine.add_automation_lane(&t.id, "volume", 0.0).unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 5.0,
                    value: 0.3,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        engine
            .add_automation_point(
                &t.id,
                &lane.id,
                AutomationPoint {
                    time: 15.0,
                    value: 0.9,
                    interpolation: InterpolationMode::Linear,
                },
            )
            .unwrap();
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 0.0).unwrap(),
            0.3
        );
        assert_eq!(
            engine.get_automation_value(&t.id, &lane.id, 20.0).unwrap(),
            0.9
        );
    }

    // ── Markers ──

    #[test]
    fn markers_added_in_sorted_order() {
        let (mut engine, _) = fresh();
        engine.add_marker(10.0, "Chorus", None);
        engine.add_marker(5.0, "Verse", None);
        engine.add_marker(20.0, "Bridge", None);
        let labels: Vec<String> = engine
            .get_markers()
            .iter()
            .map(|m| m.label.clone())
            .collect();
        assert_eq!(labels, vec!["Verse", "Chorus", "Bridge"]);
    }

    #[test]
    fn removes_markers() {
        let (mut engine, _) = fresh();
        let m = engine.add_marker(5.0, "Intro", None);
        engine.remove_marker(&m.id).unwrap();
        assert_eq!(engine.get_markers().len(), 0);
    }

    #[test]
    fn default_marker_color() {
        let (mut engine, _) = fresh();
        let m = engine.add_marker(0.0, "Start", None);
        assert_eq!(m.color, "#ffcc00");
    }

    #[test]
    fn custom_marker_color() {
        let (mut engine, _) = fresh();
        let m = engine.add_marker(0.0, "Start", Some("#ff0000"));
        assert_eq!(m.color, "#ff0000");
    }

    #[test]
    fn remove_non_existent_marker_errors() {
        let (mut engine, _) = fresh();
        assert!(engine.remove_marker("nonexistent").is_err());
    }

    #[test]
    fn emits_marker_events() {
        let (mut engine, events) = fresh();
        let m = engine.add_marker(5.0, "Drop", None);
        engine.remove_marker(&m.id).unwrap();
        let kinds: Vec<TimelineEventKind> = events
            .borrow()
            .iter()
            .map(|e| e.kind)
            .filter(|k| {
                matches!(
                    k,
                    TimelineEventKind::MarkerAdded | TimelineEventKind::MarkerRemoved
                )
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                TimelineEventKind::MarkerAdded,
                TimelineEventKind::MarkerRemoved,
            ]
        );
    }

    // ── Queries ──

    #[test]
    fn duration_returns_max_clip_end() {
        let (mut engine, _) = fresh();
        let t1 = engine.add_track(TrackKind::Audio, "T1");
        let t2 = engine.add_track(TrackKind::Audio, "T2");
        let mut a = make_clip();
        a.start_time = 0.0;
        a.duration = 10.0;
        let mut b = make_clip();
        b.start_time = 5.0;
        b.duration = 20.0;
        engine.add_clip(&t1.id, a).unwrap();
        engine.add_clip(&t2.id, b).unwrap();
        assert_eq!(engine.get_duration(), 25.0);
    }

    #[test]
    fn duration_zero_for_empty_timeline() {
        let (engine, _) = fresh();
        assert_eq!(engine.get_duration(), 0.0);
    }

    #[test]
    fn clips_at_time_finds_overlapping() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "T1");
        for (name, start, dur) in [("A", 0.0_f64, 10.0_f64), ("B", 8.0, 10.0), ("C", 20.0, 5.0)] {
            let mut input = make_clip();
            input.name = name.into();
            input.start_time = start;
            input.duration = dur;
            engine.add_clip(&t.id, input).unwrap();
        }
        let at9: Vec<String> = engine
            .get_clips_at_time(9.0)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(at9, vec!["A", "B"]);
        let at20: Vec<String> = engine
            .get_clips_at_time(20.0)
            .into_iter()
            .map(|c| c.name)
            .collect();
        assert_eq!(at20, vec!["C"]);
    }

    #[test]
    fn excludes_clips_at_end_boundary() {
        let (mut engine, _) = fresh();
        let t = engine.add_track(TrackKind::Audio, "T1");
        let mut input = make_clip();
        input.name = "A".into();
        input.start_time = 0.0;
        input.duration = 5.0;
        engine.add_clip(&t.id, input).unwrap();
        assert_eq!(engine.get_clips_at_time(5.0).len(), 0);
    }

    // ── Events ──

    #[test]
    fn unsubscribe_listeners() {
        let (mut engine, _) = fresh();
        let local: Rc<RefCell<Vec<TimelineEvent>>> = Rc::new(RefCell::new(Vec::new()));
        let captured = local.clone();
        let id = engine.subscribe(Box::new(move |e| captured.borrow_mut().push(e.clone())));
        engine.add_track(TrackKind::Audio, "T1");
        assert_eq!(local.borrow().len(), 1);
        engine.unsubscribe(id);
        engine.add_track(TrackKind::Audio, "T2");
        assert_eq!(local.borrow().len(), 1);
    }

    // ── Lifecycle ──

    #[test]
    fn dispose_clears_state() {
        let (mut engine, _) = fresh();
        engine.add_track(TrackKind::Audio, "T1");
        engine.add_marker(5.0, "Mark", None);
        engine.dispose();
        assert_eq!(engine.get_tracks().len(), 0);
        assert_eq!(engine.get_markers().len(), 0);
    }

    // ── Manual Clock ──

    #[test]
    fn clock_starts_at_zero() {
        let c = create_manual_clock();
        assert_eq!(c.now(), 0.0);
    }

    #[test]
    fn clock_advances_time() {
        let mut c = create_manual_clock();
        c.advance(5.0);
        assert_eq!(c.now(), 5.0);
        c.advance(3.0);
        assert_eq!(c.now(), 8.0);
    }

    #[test]
    fn clock_set_time() {
        let mut c = create_manual_clock();
        c.set_time(10.0);
        assert_eq!(c.now(), 10.0);
    }

    #[test]
    fn clock_fires_scheduled_callbacks() {
        let mut c = create_manual_clock();
        let fired = Rc::new(Cell::new(false));
        let f2 = fired.clone();
        c.schedule(5.0, Box::new(move || f2.set(true)));
        c.advance(3.0);
        assert!(!fired.get());
        c.advance(3.0);
        assert!(fired.get());
    }

    #[test]
    fn clock_cancels_callbacks() {
        let mut c = create_manual_clock();
        let fired = Rc::new(Cell::new(false));
        let f2 = fired.clone();
        let h = c.schedule(5.0, Box::new(move || f2.set(true)));
        c.cancel(&h);
        c.advance(10.0);
        assert!(!fired.get());
    }

    #[test]
    fn clock_fires_callbacks_in_time_order() {
        let mut c = create_manual_clock();
        let order: Rc<RefCell<Vec<i32>>> = Rc::new(RefCell::new(Vec::new()));
        let o1 = order.clone();
        c.schedule(3.0, Box::new(move || o1.borrow_mut().push(3)));
        let o2 = order.clone();
        c.schedule(1.0, Box::new(move || o2.borrow_mut().push(1)));
        let o3 = order.clone();
        c.schedule(5.0, Box::new(move || o3.borrow_mut().push(5)));
        c.advance(10.0);
        assert_eq!(*order.borrow(), vec![1, 3, 5]);
    }

    // ── Tempo Map ──

    #[test]
    fn tempo_map_defaults() {
        let tm = create_tempo_map(480, 120.0);
        assert_eq!(tm.tempo_at(0.0), 120.0);
        let sig = tm.time_signature_at(0.0);
        assert_eq!(sig.numerator, 4);
        assert_eq!(sig.denominator, 4);
    }

    #[test]
    fn tempo_map_custom_initial_bpm() {
        let tm = create_tempo_map(480, 140.0);
        assert_eq!(tm.tempo_at(0.0), 140.0);
    }

    #[test]
    fn tempo_map_seconds_to_musical() {
        let tm = create_tempo_map(480, 120.0);
        let pos = tm.to_musical(1.0);
        assert_eq!(pos.bar, 1);
        assert_eq!(pos.beat, 3);
        assert_eq!(pos.tick, 0);
    }

    #[test]
    fn tempo_map_musical_to_seconds() {
        let tm = create_tempo_map(480, 120.0);
        let s = tm.to_seconds(MusicalPosition {
            bar: 1,
            beat: 3,
            tick: 0,
        });
        assert!((s - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tempo_map_round_trip() {
        let tm = create_tempo_map(480, 90.0);
        let time = 7.5;
        let pos = tm.to_musical(time);
        let back = tm.to_seconds(pos);
        assert!((back - time).abs() < 1e-3);
    }

    #[test]
    fn tempo_map_handles_changes() {
        let mut tm = create_tempo_map(480, 120.0);
        tm.add_tempo(TempoMarker {
            time: 4.0,
            bpm: 60.0,
        });
        assert_eq!(tm.tempo_at(0.0), 120.0);
        assert_eq!(tm.tempo_at(4.0), 60.0);
        assert_eq!(tm.tempo_at(10.0), 60.0);
    }

    #[test]
    fn tempo_map_replaces_at_same_time() {
        let mut tm = create_tempo_map(480, 120.0);
        tm.add_tempo(TempoMarker {
            time: 0.0,
            bpm: 90.0,
        });
        assert_eq!(tm.tempo_at(0.0), 90.0);
        assert_eq!(tm.get_tempo_markers().len(), 1);
    }

    #[test]
    fn tempo_map_lists_markers_sorted() {
        let mut tm = create_tempo_map(480, 120.0);
        tm.add_tempo(TempoMarker {
            time: 10.0,
            bpm: 140.0,
        });
        tm.add_tempo(TempoMarker {
            time: 5.0,
            bpm: 100.0,
        });
        let markers = tm.get_tempo_markers();
        assert_eq!(markers.len(), 3);
        let times: Vec<f64> = markers.iter().map(|m| m.time).collect();
        assert_eq!(times, vec![0.0, 5.0, 10.0]);
    }

    #[test]
    fn tempo_map_time_signature_changes() {
        let mut tm = create_tempo_map(480, 120.0);
        tm.set_time_signature(
            8.0,
            TimeSignature {
                numerator: 3,
                denominator: 4,
            },
        );
        assert_eq!(tm.time_signature_at(0.0).numerator, 4);
        assert_eq!(tm.time_signature_at(8.0).numerator, 3);
        assert_eq!(tm.time_signature_at(20.0).numerator, 3);
    }

    #[test]
    fn tempo_map_handles_ticks_in_musical() {
        let tm = create_tempo_map(480, 120.0);
        let pos = tm.to_musical(0.25);
        assert_eq!(pos.bar, 1);
        assert_eq!(pos.beat, 1);
        assert_eq!(pos.tick, 240);
    }

    #[test]
    fn tempo_map_position_with_ticks_back_to_seconds() {
        let tm = create_tempo_map(480, 120.0);
        let s = tm.to_seconds(MusicalPosition {
            bar: 1,
            beat: 1,
            tick: 240,
        });
        assert!((s - 0.25).abs() < 1e-9);
    }
}
