/**
 * @prism/core — NLE / Timeline System Types (Layer 1)
 *
 * Non-linear editing and show control primitives.
 *
 * Design principles:
 *   - Timeline is a collection of typed tracks (audio, video, lighting, automation, MIDI)
 *   - Clips are time-range regions on tracks with source references
 *   - Transport: play/pause/seek/loop/scrub with pluggable clock
 *   - Automation lanes: breakpoint curves for parameter control
 *   - Dual time model: seconds (absolute) and musical (PPQN ticks) — inspired by OpenDAW TimeBase
 *   - Pure Layer 1: no audio/video APIs; those are Layer 2 / daemon concerns
 */

// ── Time ──────────────────────────────────────────────────────────────────

/** Time in seconds (floating-point). */
export type TimeSeconds = number;

/** A time range [start, end) in seconds. */
export interface TimeRange {
  start: TimeSeconds;
  end: TimeSeconds;
}

// ── Musical Time (PPQN) ──────────────────────────────────────────────────

/** Pulses per quarter note — resolution for musical time. */
export type PPQ = number;

/** A tempo marker at a specific time. */
export interface TempoMarker {
  /** Position in seconds. */
  time: TimeSeconds;
  /** Beats per minute at this point. */
  bpm: number;
}

/** Time signature. */
export interface TimeSignature {
  /** Beats per bar (numerator). */
  numerator: number;
  /** Beat unit (denominator, e.g. 4 = quarter note). */
  denominator: number;
}

/** Musical position in bars/beats/ticks. */
export interface MusicalPosition {
  bar: number;
  beat: number;
  tick: number;
}

/**
 * Converts between seconds and musical time.
 * Supports tempo automation (variable BPM) via tempo map.
 */
export interface TempoMap {
  /** Pulses per quarter note resolution. */
  ppq: PPQ;
  /** Get the tempo at a given time in seconds. */
  tempoAt(time: TimeSeconds): number;
  /** Convert seconds to musical position. */
  toMusical(time: TimeSeconds): MusicalPosition;
  /** Convert musical position to seconds. */
  toSeconds(position: MusicalPosition): TimeSeconds;
  /** Get the time signature at a given time. */
  timeSignatureAt(time: TimeSeconds): TimeSignature;
  /** Add a tempo change. */
  addTempo(marker: TempoMarker): void;
  /** Set the time signature at a given time. */
  setTimeSignature(time: TimeSeconds, sig: TimeSignature): void;
  /** Get all tempo markers. */
  getTempoMarkers(): TempoMarker[];
}

// ── Track Kinds ───────────────────────────────────────────────────────────

export type TrackKind = "audio" | "video" | "lighting" | "automation" | "midi";

// ── Clips ─────────────────────────────────────────────────────────────────

export interface TimelineClip {
  /** Unique clip ID. */
  id: string;
  /** ID of the track this clip belongs to. */
  trackId: string;
  /** Display name. */
  name: string;
  /** Start time on the timeline. */
  startTime: TimeSeconds;
  /** Duration of the clip. */
  duration: TimeSeconds;
  /** Offset into the source media (for trimmed clips). */
  sourceOffset: TimeSeconds;
  /** Reference to the source asset (file path, blob hash, etc). */
  sourceRef: string;
  /** Whether this clip is muted. */
  muted: boolean;
  /** Whether this clip is locked (prevents edits). */
  locked: boolean;
  /** Gain/opacity value 0..1 for mixing. */
  gain: number;
}

// ── Automation ────────────────────────────────────────────────────────────

export type InterpolationMode = "step" | "linear" | "bezier";

export interface AutomationPoint {
  /** Time position in seconds. */
  time: TimeSeconds;
  /** Value at this point (typically 0..1 normalized). */
  value: number;
  /** Interpolation to next point. */
  interpolation: InterpolationMode;
}

export interface AutomationLane {
  /** Unique lane ID. */
  id: string;
  /** The parameter this lane controls (e.g. "volume", "pan", "intensity"). */
  parameter: string;
  /** Default value when no automation points are defined. */
  defaultValue: number;
  /** Sorted automation breakpoints. */
  points: AutomationPoint[];
}

// ── Tracks ────────────────────────────────────────────────────────────────

export interface TimelineTrack {
  /** Unique track ID. */
  id: string;
  /** Display name. */
  name: string;
  /** Track kind. */
  kind: TrackKind;
  /** Whether the track is muted. */
  muted: boolean;
  /** Whether the track is solo'd. */
  solo: boolean;
  /** Whether the track is locked. */
  locked: boolean;
  /** Track-level gain/opacity 0..1. */
  gain: number;
  /** Clips on this track. */
  clips: TimelineClip[];
  /** Automation lanes for this track. */
  automationLanes: AutomationLane[];
}

// ── Transport ─────────────────────────────────────────────────────────────

export type TransportStatus = "stopped" | "playing" | "paused" | "recording";

export interface LoopRegion {
  /** Whether looping is enabled. */
  enabled: boolean;
  /** Loop start time. */
  start: TimeSeconds;
  /** Loop end time. */
  end: TimeSeconds;
}

export interface TransportState {
  /** Current transport status. */
  status: TransportStatus;
  /** Current playhead position in seconds. */
  position: TimeSeconds;
  /** Playback speed multiplier (1.0 = normal). */
  speed: number;
  /** Loop region configuration. */
  loop: LoopRegion;
}

// ── Clock ─────────────────────────────────────────────────────────────────

/**
 * Abstract clock interface. Layer 2 provides a real clock
 * (tone.js / requestAnimationFrame). Tests use a manual clock.
 */
export interface TimelineClock {
  /** Current time in seconds. */
  now(): TimeSeconds;
  /** Schedule a callback at a future time. Returns cancel handle. */
  schedule(time: TimeSeconds, callback: () => void): string;
  /** Cancel a scheduled callback. */
  cancel(handle: string): void;
  /** Start the clock. */
  start(): void;
  /** Stop the clock. */
  stop(): void;
}

// ── Markers ───────────────────────────────────────────────────────────────

export interface TimelineMarker {
  /** Unique marker ID. */
  id: string;
  /** Position on the timeline. */
  time: TimeSeconds;
  /** Label text. */
  label: string;
  /** Color for UI rendering. */
  color: string;
}

// ── Events ────────────────────────────────────────────────────────────────

export type TimelineEventKind =
  | "transport:play"
  | "transport:pause"
  | "transport:stop"
  | "transport:seek"
  | "transport:loop"
  | "track:added"
  | "track:removed"
  | "track:updated"
  | "clip:added"
  | "clip:removed"
  | "clip:moved"
  | "clip:trimmed"
  | "marker:added"
  | "marker:removed";

export interface TimelineEvent {
  kind: TimelineEventKind;
  timestamp: number;
  data: Record<string, unknown>;
}

export type TimelineListener = (event: TimelineEvent) => void;

// ── Timeline Engine ───────────────────────────────────────────────────────

export interface TimelineEngine {
  // ── Transport ─────────────────────────────────────────────
  play(): void;
  pause(): void;
  stop(): void;
  seek(time: TimeSeconds): void;
  scrub(time: TimeSeconds): void;
  setSpeed(speed: number): void;
  setLoop(region: LoopRegion): void;
  getTransport(): TransportState;

  // ── Tracks ────────────────────────────────────────────────
  addTrack(kind: TrackKind, name: string): TimelineTrack;
  removeTrack(trackId: string): void;
  getTrack(trackId: string): TimelineTrack | undefined;
  getTracks(): TimelineTrack[];
  updateTrack(trackId: string, updates: Partial<Pick<TimelineTrack, "name" | "muted" | "solo" | "locked" | "gain">>): void;

  // ── Clips ─────────────────────────────────────────────────
  addClip(trackId: string, clip: Omit<TimelineClip, "id" | "trackId">): TimelineClip;
  removeClip(trackId: string, clipId: string): void;
  moveClip(clipId: string, targetTrackId: string, newStartTime: TimeSeconds): void;
  trimClip(clipId: string, newStartTime: TimeSeconds, newDuration: TimeSeconds): void;
  getClip(clipId: string): TimelineClip | undefined;

  // ── Automation ────────────────────────────────────────────
  addAutomationLane(trackId: string, parameter: string, defaultValue?: number): AutomationLane;
  removeAutomationLane(trackId: string, laneId: string): void;
  addAutomationPoint(trackId: string, laneId: string, point: AutomationPoint): void;
  removeAutomationPoint(trackId: string, laneId: string, time: TimeSeconds): void;
  getAutomationValue(trackId: string, laneId: string, time: TimeSeconds): number;

  // ── Markers ───────────────────────────────────────────────
  addMarker(time: TimeSeconds, label: string, color?: string): TimelineMarker;
  removeMarker(markerId: string): void;
  getMarkers(): TimelineMarker[];

  // ── Tempo ──────────────────────────────────────────────────
  getTempoMap(): TempoMap;

  // ── Queries ───────────────────────────────────────────────
  getDuration(): TimeSeconds;
  getClipsAtTime(time: TimeSeconds): TimelineClip[];

  // ── Events ────────────────────────────────────────────────
  subscribe(listener: TimelineListener): () => void;

  // ── Lifecycle ─────────────────────────────────────────────
  dispose(): void;
}

// ── Test Clock ────────────────────────────────────────────────────────────

export interface ManualClock extends TimelineClock {
  /** Advance the clock by the given number of seconds. */
  advance(seconds: TimeSeconds): void;
  /** Set the clock to an exact time. */
  setTime(time: TimeSeconds): void;
}
