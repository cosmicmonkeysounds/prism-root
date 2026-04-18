//! Pure data types for the timeline engine.
//!
//! Port of `packages/prism-core/src/domain/timeline/timeline-types.ts`
//! at commit 8426588. The TS file declared a single `TimelineEngine`
//! interface with method signatures; in Rust we expose a trait-free
//! struct on top of these data types in [`super::engine`]. The
//! `TimelineClock` interface stays as a trait so callers can plug in
//! either the manual clock used in tests or a realtime clock layered
//! by the host.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

// ── Time ───────────────────────────────────────────────────────────

/// Time in seconds (floating-point).
pub type TimeSeconds = f64;

/// A time range `[start, end)` in seconds.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeRange {
    pub start: TimeSeconds,
    pub end: TimeSeconds,
}

// ── Musical Time ───────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TempoMarker {
    pub time: TimeSeconds,
    pub bpm: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TimeSignature {
    pub numerator: u32,
    pub denominator: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MusicalPosition {
    pub bar: u32,
    pub beat: u32,
    pub tick: u32,
}

// ── Track Kinds ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TrackKind {
    Audio,
    Video,
    Lighting,
    Automation,
    Midi,
}

// ── Clips ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineClip {
    pub id: String,
    #[serde(rename = "trackId")]
    pub track_id: String,
    pub name: String,
    #[serde(rename = "startTime")]
    pub start_time: TimeSeconds,
    pub duration: TimeSeconds,
    #[serde(rename = "sourceOffset")]
    pub source_offset: TimeSeconds,
    #[serde(rename = "sourceRef")]
    pub source_ref: String,
    pub muted: bool,
    pub locked: bool,
    pub gain: f64,
}

/// Input shape passed to `add_clip` — same fields as
/// [`TimelineClip`] minus `id` / `track_id`.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipInput {
    pub name: String,
    pub start_time: TimeSeconds,
    pub duration: TimeSeconds,
    pub source_offset: TimeSeconds,
    pub source_ref: String,
    pub muted: bool,
    pub locked: bool,
    pub gain: f64,
}

impl Default for ClipInput {
    fn default() -> Self {
        Self {
            name: "clip".into(),
            start_time: 0.0,
            duration: 5.0,
            source_offset: 0.0,
            source_ref: "audio.wav".into(),
            muted: false,
            locked: false,
            gain: 1.0,
        }
    }
}

// ── Automation ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum InterpolationMode {
    Step,
    Linear,
    Bezier,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct AutomationPoint {
    pub time: TimeSeconds,
    pub value: f64,
    pub interpolation: InterpolationMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AutomationLane {
    pub id: String,
    pub parameter: String,
    #[serde(rename = "defaultValue")]
    pub default_value: f64,
    pub points: Vec<AutomationPoint>,
}

// ── Tracks ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineTrack {
    pub id: String,
    pub name: String,
    pub kind: TrackKind,
    pub muted: bool,
    pub solo: bool,
    pub locked: bool,
    pub gain: f64,
    pub clips: Vec<TimelineClip>,
    #[serde(rename = "automationLanes")]
    pub automation_lanes: Vec<AutomationLane>,
}

/// Partial-update payload for `update_track` — every `Some` field is
/// applied, every `None` field is left untouched.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackUpdates {
    pub name: Option<String>,
    pub muted: Option<bool>,
    pub solo: Option<bool>,
    pub locked: Option<bool>,
    pub gain: Option<f64>,
}

// ── Transport ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TransportStatus {
    Stopped,
    Playing,
    Paused,
    Recording,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LoopRegion {
    pub enabled: bool,
    pub start: TimeSeconds,
    pub end: TimeSeconds,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct TransportState {
    pub status: TransportStatus,
    pub position: TimeSeconds,
    pub speed: f64,
    pub loop_region: LoopRegion,
}

// ── Clock ──────────────────────────────────────────────────────────

/// Abstract clock interface. Layer 2 provides a real clock; the
/// default manual clock used in tests lives in
/// [`super::engine::create_manual_clock`].
pub trait TimelineClock {
    fn now(&self) -> TimeSeconds;
    fn schedule(&mut self, time: TimeSeconds, callback: Box<dyn FnMut()>) -> String;
    fn cancel(&mut self, handle: &str);
    fn start(&mut self);
    fn stop(&mut self);
}

// ── Markers ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineMarker {
    pub id: String,
    pub time: TimeSeconds,
    pub label: String,
    pub color: String,
}

// ── Events ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TimelineEventKind {
    #[serde(rename = "transport:play")]
    TransportPlay,
    #[serde(rename = "transport:pause")]
    TransportPause,
    #[serde(rename = "transport:stop")]
    TransportStop,
    #[serde(rename = "transport:seek")]
    TransportSeek,
    #[serde(rename = "transport:loop")]
    TransportLoop,
    #[serde(rename = "track:added")]
    TrackAdded,
    #[serde(rename = "track:removed")]
    TrackRemoved,
    #[serde(rename = "track:updated")]
    TrackUpdated,
    #[serde(rename = "clip:added")]
    ClipAdded,
    #[serde(rename = "clip:removed")]
    ClipRemoved,
    #[serde(rename = "clip:moved")]
    ClipMoved,
    #[serde(rename = "clip:trimmed")]
    ClipTrimmed,
    #[serde(rename = "marker:added")]
    MarkerAdded,
    #[serde(rename = "marker:removed")]
    MarkerRemoved,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub kind: TimelineEventKind,
    pub timestamp: i64,
    pub data: BTreeMap<String, Value>,
}
