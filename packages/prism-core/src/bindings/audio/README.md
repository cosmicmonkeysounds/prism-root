# audio/

OpenDAW audio-engine bridge. Connects the Layer 1 `@prism/core/timeline` engine to OpenDAW's DSP graph: bidirectional transport sync via `AnimationFrame`, track loading (`AudioFileBox`/`AudioRegionBox` with PPQN conversion), 10 audio effects, and WAV export of mix or stems.

```ts
import { createOpenDawBridge, installOpenDawWorkers } from "@prism/core/audio";
```

## Key exports

- `installOpenDawWorkers(workersUrl, workletsUrl, offlineEngineUrl)` — one-time setup; must run before `createOpenDawBridge`.
- `createOpenDawBridge(timeline, options?)` — async factory. Wires a `TimelineEngine` to OpenDAW. Options: `{ bpm?, onStatusUpdate?, localAudioBuffers?, instrumentFactory?, effectFactories? }`.
- React hooks: `useOpenDawBridge`, `usePlaybackPosition`, `useTransportControls`, `useTrackEffects`.
- Types: `OpenDawBridge`, `OpenDawBridgeOptions`, `OpenDawEffectType`, `TrackBinding`, `ClipBinding`, `AudioExportOptions`, `StemConfig`, `UseOpenDawBridgeResult`, `PlaybackPositionResult`, `TransportControlsResult`, `TrackEffectsResult`. Re-exports `EffectFactory` and `InstrumentFactory`.

## Usage

```tsx
import { useMemo } from "react";
import { createTimelineEngine } from "@prism/core/timeline";
import { useOpenDawBridge, useTransportControls } from "@prism/core/audio";

function AudioTimeline() {
  const timeline = useMemo(() => createTimelineEngine(), []);
  const { bridge, loading, error } = useOpenDawBridge(timeline, { bpm: 120 });
  const { play, pause, stop } = useTransportControls(timeline);

  if (loading) return <p>Loading audio engine...</p>;
  if (error) return <p>Error: {error.message}</p>;
  return <button onClick={play}>Play</button>;
}
```
