// ── NLE / Timeline System ─────────────────────────────────────────────────
export type {
  TimeSeconds,
  TimeRange,
  PPQ,
  TempoMarker,
  TimeSignature,
  MusicalPosition,
  TempoMap,
  TrackKind,
  TimelineClip,
  InterpolationMode,
  AutomationPoint,
  AutomationLane,
  TimelineTrack,
  TransportStatus,
  LoopRegion,
  TransportState,
  TimelineClock,
  TimelineMarker,
  TimelineEventKind,
  TimelineEvent,
  TimelineListener,
  TimelineEngine,
  ManualClock,
} from "./timeline-types.js";

export {
  createTimelineEngine,
  createManualClock,
  createTempoMap,
  resetIdCounter,
} from "./timeline.js";

export type { TimelineEngineOptions } from "./timeline.js";
