// ── OpenDAW Audio Bridge ──────────────────────────────────────────────────
export type {
  TrackBinding,
  ClipBinding,
  OpenDawEffectType,
  AudioExportOptions,
  StemConfig,
  OpenDawBridgeOptions,
  OpenDawBridge,
} from "./opendaw-types.js";

export {
  installOpenDawWorkers,
  createOpenDawBridge,
} from "./opendaw-bridge.js";

export {
  useOpenDawBridge,
  usePlaybackPosition,
  useTransportControls,
  useTrackEffects,
} from "./opendaw-hooks.js";

export type {
  UseOpenDawBridgeResult,
  PlaybackPositionResult,
  TransportControlsResult,
  TrackEffectsResult,
} from "./opendaw-hooks.js";
