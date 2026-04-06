/**
 * @prism/core — OpenDAW Audio Bridge Types (Layer 2)
 *
 * Bridges Prism's Layer 1 Timeline primitives to the OpenDAW audio engine.
 * OpenDAW is the audio backend; Prism timeline is the data model.
 *
 * Design principles:
 *   - OpenDAW Project is the audio truth; Prism timeline is the state model
 *   - Bridge syncs bidirectionally: timeline edits → OpenDAW, OpenDAW playback → timeline
 *   - AnimationFrame drives position updates (OpenDAW requirement)
 *   - PPQN ↔ seconds conversion via OpenDAW's PPQN module
 *   - All audio operations require browser context (AudioContext, SharedArrayBuffer)
 */

import type { Project } from "@opendaw/studio-core";
import type { AudioUnitBox, TrackBox, AudioRegionBox } from "@opendaw/studio-boxes";
import type { UUID } from "@opendaw/lib-std";
import type { TimelineEngine, TrackKind } from "../../layer1/timeline/timeline-types.js";

// ── Track Binding ─────────────────────────────────────────────────────────

/** Links a Prism TimelineTrack to its OpenDAW counterparts. */
export interface TrackBinding {
  /** Prism track ID. */
  trackId: string;
  /** OpenDAW TrackBox (clip container). */
  trackBox: TrackBox;
  /** OpenDAW AudioUnitBox (mixer channel: volume/pan/mute/solo/effects). */
  audioUnitBox: AudioUnitBox;
  /** UUID for sample identification. */
  uuid: UUID.Bytes;
  /** Display name. */
  name: string;
}

/** Links a Prism TimelineClip to its OpenDAW AudioRegionBox. */
export interface ClipBinding {
  /** Prism clip ID. */
  clipId: string;
  /** Prism track ID. */
  trackId: string;
  /** OpenDAW AudioRegionBox. */
  regionBox: AudioRegionBox;
  /** UUID of the audio file. */
  fileUuid: UUID.Bytes;
}

// ── Effect Types ──────────────────────────────────────────────────────────

/** Available OpenDAW effect types. */
export type OpenDawEffectType =
  | "Reverb"
  | "DattorroReverb"
  | "Compressor"
  | "Delay"
  | "Crusher"
  | "StereoWidth"
  | "EQ"
  | "Fold"
  | "Tidal"
  | "Maximizer";

// ── Export ─────────────────────────────────────────────────────────────────

export interface AudioExportOptions {
  /** Sample rate (default: 48000). */
  sampleRate?: number;
  /** Output filename without extension. */
  fileName?: string;
  /** Progress callback (0..1). */
  onProgress?: (progress: number) => void;
  /** Status message callback. */
  onStatus?: (status: string) => void;
  /** Abort signal for cancellation. */
  abortSignal?: AbortSignal;
}

export interface StemConfig {
  /** Include audio effects on this stem. */
  includeAudioEffects: boolean;
  /** Include send effects on this stem. */
  includeSends: boolean;
  /** Output filename for this stem. */
  fileName: string;
}

// ── Bridge ────────────────────────────────────────────────────────────────

export interface OpenDawBridgeOptions {
  /** BPM for the project (default: from Prism TempoMap). */
  bpm?: number;
  /** Status callback during initialization. */
  onStatusUpdate?: (status: string) => void;
  /** Local audio buffers keyed by UUID string. */
  localAudioBuffers?: Map<string, AudioBuffer>;
  /** Instrument factory for creating audio tracks (e.g. InstrumentFactories.Tape). */
  instrumentFactory?: unknown;
  /** Effect factory map keyed by OpenDawEffectType name. */
  effectFactories?: Partial<Record<string, unknown>>;
}

/**
 * The main bridge between Prism Timeline and OpenDAW audio engine.
 *
 * Lifecycle: create → loadTracks → play/pause/stop → dispose
 */
export interface OpenDawBridge {
  /** The underlying OpenDAW project. */
  readonly project: Project;
  /** The AudioContext used by the engine. */
  readonly audioContext: AudioContext;
  /** The connected Prism timeline engine. */
  readonly timeline: TimelineEngine;

  // ── Track Management ────────────────────────────────────
  /** Load an audio file into a new track, synced to the timeline. */
  loadTrack(name: string, audioBuffer: AudioBuffer, kind?: TrackKind): Promise<TrackBinding>;
  /** Get all track bindings. */
  getTrackBindings(): TrackBinding[];
  /** Get the binding for a specific Prism track. */
  getTrackBinding(trackId: string): TrackBinding | undefined;

  // ── Transport Sync ──────────────────────────────────────
  /** Start the AnimationFrame-based position sync loop. */
  startSync(): void;
  /** Stop the sync loop. */
  stopSync(): void;

  // ── Effects ─────────────────────────────────────────────
  /** Insert an effect on a track's audio chain. */
  insertEffect(trackId: string, effectType: OpenDawEffectType): void;

  // ── Export ──────────────────────────────────────────────
  /** Export the full mix to WAV. */
  exportMix(options?: AudioExportOptions): Promise<ArrayBuffer>;
  /** Export individual stems to WAV. */
  exportStems(stemsConfig: Record<string, StemConfig>, options?: AudioExportOptions): Promise<Map<string, ArrayBuffer>>;

  // ── Volume/Pan ──────────────────────────────────────────
  /** Set track volume in dB. */
  setVolume(trackId: string, volumeDb: number): void;
  /** Set track pan (-1 to 1). */
  setPan(trackId: string, pan: number): void;
  /** Set track mute state. */
  setMute(trackId: string, muted: boolean): void;
  /** Set track solo state. */
  setSolo(trackId: string, solo: boolean): void;

  // ── Lifecycle ───────────────────────────────────────────
  /** Dispose the bridge and release all audio resources. */
  dispose(): void;
}
