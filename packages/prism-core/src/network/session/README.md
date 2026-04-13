# session/

Communication Fabric — real-time sessions, transcription, and A/V transport.
A `SessionNode` is modelled as a CRDT collection with a transcript timeline
and media tracks; the session manager orchestrates participants (with roles
and mute state), a searchable transcript, and a playback controller that
keeps media seek in sync with transcript segments. Transport and
transcription are abstract so Whisper.cpp, LiveKit, and plain WebRTC can
drop in interchangeably, and the Listener Fallback protocol can delegate
compute to capable peers.

```ts
import { createSessionManager } from "@prism/core/session";
```

## Key exports

- `createSessionManager(options?)` — orchestrates session lifecycle,
  participants, tracks, delegations, transcript, and playback.
- `createTranscriptTimeline()` — sorted, searchable, time-indexed segments
  with `addSegment`/`finalizeSegment`/`getRange`/`getAtTime`.
- `createPlaybackController(transcript)` — transcript-synced media seek.
- `createTestTransport()` — in-memory `SessionTransport` stub for tests.
- `createTestTranscriptionProvider()` — `TranscriptionProvider` that lets
  tests call `feedSegment` directly.
- `SessionManager` / `SessionManagerOptions` / `SessionConfig` — manager
  types.
- `SessionParticipant` / `ParticipantRole` / `MediaKind` / `MediaTrack` /
  `TrackState` — participant and media types.
- `TranscriptSegment` / `TranscriptTimeline` / `PlaybackController` /
  `PlaybackListener` — transcript and playback types.
- `SessionTransport` / `TransportKind` / `TransportEvent` /
  `TransportEventType` / `TransportEventListener` — transport interface.
- `TranscriptionProvider` / `TranscriptionOptions` — STT interface.
- `DelegationRequest` / `DelegationStatus` / `DelegationListener` — Listener
  Fallback delegation types.
- `SessionStatus` / `SessionChangeType` / `SessionChangeListener` — state
  and event types.

## Usage

```ts
import { createSessionManager } from "@prism/core/session";

const session = createSessionManager({ maxParticipants: 8 });

session.create({
  name: "Design Review",
  roomId: "room-42",
  localParticipant: {
    id: "did:key:alice",
    displayName: "Alice",
    role: "host",
  },
});

session.transcript.addSegment({
  id: "seg-1",
  speakerId: "did:key:alice",
  speakerName: "Alice",
  text: "Let's start with the layout panel.",
  startMs: 0,
  endMs: 2400,
  confidence: 0.95,
  language: "en",
  isFinal: true,
});

session.playback.seekToSegment("seg-1");
```
