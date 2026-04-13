# timeline/

Pure data-model NLE / timeline engine: transport, tracks, clips, automation lanes, markers, and a PPQN dual-time tempo map. Modeled on OpenDAW's primitives but contains no audio or DSP — the audio engine plugs in via `@prism/core/audio`.

```ts
import { createTimelineEngine, createTempoMap } from "@prism/core/timeline";
```

## Key exports

- `createTimelineEngine(options?)` — build a `TimelineEngine` with transport (`play`/`pause`/`stop`/`seek`), tracks, clips, automation lanes, markers, loop regions, and event subscriptions. Accepts `{ clock?, ppq?, bpm? }`.
- `createTempoMap(ppq?, initialBpm?)` — dual-time tempo map with tempo markers, time signatures, and `secondsToBeats`/`beatsToSeconds`/`tempoAt`/`timeSignatureAt`. Default PPQ is 480.
- `createManualClock()` — stepwise test clock implementing `TimelineClock`.
- `resetIdCounter()` — reset auto-generated IDs (test-only).
- Types: `TimeSeconds`, `PPQ`, `TempoMarker`, `TimeSignature`, `MusicalPosition`, `TempoMap`, `TrackKind`, `TimelineClip`, `InterpolationMode`, `AutomationPoint`, `AutomationLane`, `TimelineTrack`, `TransportStatus`, `TransportState`, `LoopRegion`, `TimelineClock`, `TimelineMarker`, `TimelineEvent`, `TimelineEventKind`, `TimelineListener`, `TimelineEngine`, `ManualClock`, `TimelineEngineOptions`.

## Usage

```ts
import {
  createTimelineEngine,
  createManualClock,
} from "@prism/core/timeline";

const clock = createManualClock();
const engine = createTimelineEngine({ clock, bpm: 128 });

const track = engine.addTrack("audio", "Vocals");
engine.addClip(track.id, {
  name: "Verse 1",
  startTime: 0,
  duration: 8,
  sourceOffset: 0,
  sourceRef: "vocals.wav",
  muted: false,
  locked: false,
  gain: 1,
});

engine.play();
```
