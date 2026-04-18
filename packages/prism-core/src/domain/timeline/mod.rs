//! `domain::timeline` — pure-data NLE / show-control engine.
//!
//! Port of `packages/prism-core/src/domain/timeline/*` at commit
//! 8426588. Splits the original two-file TS module
//! (`timeline-types.ts` + `timeline.ts`) into a `types` data module
//! and an `engine` factory module. Layer 1 only — no audio/video
//! APIs; those land in the daemon.

pub mod engine;
pub mod types;

pub use engine::{
    create_manual_clock, create_tempo_map, create_timeline_engine, reset_id_counter, ManualClock,
    TempoMap, TimelineEngine, TimelineEngineOptions, TimelineError,
};
pub use types::{
    AutomationLane, AutomationPoint, ClipInput, InterpolationMode, LoopRegion, MusicalPosition,
    TempoMarker, TimeRange, TimeSeconds, TimeSignature, TimelineClip, TimelineClock, TimelineEvent,
    TimelineEventKind, TimelineMarker, TimelineTrack, TrackKind, TrackUpdates, TransportState,
    TransportStatus,
};
