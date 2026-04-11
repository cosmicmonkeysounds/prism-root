/**
 * @prism/core — Communication Fabric (Layer 1)
 *
 * Real-time sessions with transcription, media tracks, and playback sync.
 *
 * Features:
 *   - SessionManager — lifecycle for meetings/calls/recordings
 *   - TranscriptTimeline — ordered, searchable, time-indexed segments
 *   - PlaybackController — syncs transcript to media position
 *   - Listener Fallback — compute delegation to capable peers
 *   - TestTransport — in-memory transport for unit testing
 */

import type {
  SessionStatus,
  ParticipantRole,
  SessionParticipant,
  TranscriptSegment,
  TranscriptTimeline,
  MediaTrack,
  PlaybackController,
  PlaybackListener,
  SessionTransport,
  TransportEvent,
  TransportEventListener,
  DelegationRequest,
  SessionChangeType,
  SessionChangeListener,
  SessionManager,
  SessionManagerOptions,
  SessionConfig,
  TranscriptionProvider,
  TranscriptionOptions,
} from "./session-types.js";

// ── Helpers ────────────────────────────────────────────────────────────────

let idCounter = 0;
function uid(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${(idCounter++).toString(36)}`;
}

function makeSegment(base: TranscriptSegment, overrides: Partial<TranscriptSegment>): TranscriptSegment {
  return {
    id: overrides.id !== undefined ? overrides.id : base.id,
    speakerId: overrides.speakerId !== undefined ? overrides.speakerId : base.speakerId,
    speakerName: overrides.speakerName !== undefined ? overrides.speakerName : base.speakerName,
    text: overrides.text !== undefined ? overrides.text : base.text,
    startMs: overrides.startMs !== undefined ? overrides.startMs : base.startMs,
    endMs: overrides.endMs !== undefined ? overrides.endMs : base.endMs,
    confidence: overrides.confidence !== undefined ? overrides.confidence : base.confidence,
    language: overrides.language !== undefined ? overrides.language : base.language,
    isFinal: overrides.isFinal !== undefined ? overrides.isFinal : base.isFinal,
  };
}

function makeParticipant(base: SessionParticipant, overrides: Partial<SessionParticipant>): SessionParticipant {
  return {
    id: overrides.id !== undefined ? overrides.id : base.id,
    displayName: overrides.displayName !== undefined ? overrides.displayName : base.displayName,
    role: overrides.role !== undefined ? overrides.role : base.role,
    activeMedia: overrides.activeMedia !== undefined ? overrides.activeMedia : base.activeMedia,
    muted: overrides.muted !== undefined ? overrides.muted : base.muted,
    videoEnabled: overrides.videoEnabled !== undefined ? overrides.videoEnabled : base.videoEnabled,
    joinedAt: overrides.joinedAt !== undefined ? overrides.joinedAt : base.joinedAt,
    canDelegate: overrides.canDelegate !== undefined ? overrides.canDelegate : base.canDelegate,
  };
}

function makeTrack(base: MediaTrack, overrides: Partial<MediaTrack>): MediaTrack {
  return {
    id: overrides.id !== undefined ? overrides.id : base.id,
    participantId: overrides.participantId !== undefined ? overrides.participantId : base.participantId,
    kind: overrides.kind !== undefined ? overrides.kind : base.kind,
    state: overrides.state !== undefined ? overrides.state : base.state,
    label: overrides.label !== undefined ? overrides.label : base.label,
    muted: overrides.muted !== undefined ? overrides.muted : base.muted,
  };
}

function makeDelegation(base: DelegationRequest, overrides: Partial<DelegationRequest>): DelegationRequest {
  return {
    id: overrides.id !== undefined ? overrides.id : base.id,
    taskType: overrides.taskType !== undefined ? overrides.taskType : base.taskType,
    requesterId: overrides.requesterId !== undefined ? overrides.requesterId : base.requesterId,
    delegateeId: overrides.delegateeId !== undefined ? overrides.delegateeId : base.delegateeId,
    status: overrides.status !== undefined ? overrides.status : base.status,
    payload: overrides.payload !== undefined ? overrides.payload : base.payload,
    createdAt: overrides.createdAt !== undefined ? overrides.createdAt : base.createdAt,
  };
}

// ── Transcript Timeline ────────────────────────────────────────────────────

export function createTranscriptTimeline(): TranscriptTimeline {
  const segments: TranscriptSegment[] = [];

  function findIndex(id: string): number {
    return segments.findIndex(s => s.id === id);
  }

  function at(idx: number): TranscriptSegment {
    const s = segments[idx];
    if (!s) throw new Error(`Segment index ${idx} out of bounds`);
    return s;
  }

  function insertSorted(segment: TranscriptSegment): void {
    let lo = 0;
    let hi = segments.length;
    while (lo < hi) {
      const mid = (lo + hi) >>> 1;
      if (at(mid).startMs < segment.startMs) lo = mid + 1;
      else hi = mid;
    }
    segments.splice(lo, 0, segment);
  }

  return {
    get segments() {
      return segments;
    },

    get durationMs() {
      if (segments.length === 0) return 0;
      return Math.max(...segments.map(s => s.endMs));
    },

    addSegment(segment: TranscriptSegment): void {
      const existing = findIndex(segment.id);
      if (existing >= 0) {
        if (!at(existing).isFinal) {
          segments[existing] = segment;
        }
        return;
      }
      insertSorted(segment);
    },

    finalizeSegment(segmentId: string): void {
      const idx = findIndex(segmentId);
      if (idx < 0) return;
      segments[idx] = makeSegment(at(idx), { isFinal: true });
    },

    getRange(startMs: number, endMs: number): TranscriptSegment[] {
      return segments.filter(s => s.endMs > startMs && s.startMs < endMs);
    },

    getAtTime(timeMs: number): TranscriptSegment | null {
      for (const s of segments) {
        if (timeMs >= s.startMs && timeMs < s.endMs) return s;
      }
      return null;
    },

    search(query: string): TranscriptSegment[] {
      const lower = query.toLowerCase();
      return segments.filter(s => s.text.toLowerCase().includes(lower));
    },

    toPlainText(): string {
      return segments
        .map(s => `[${formatTime(s.startMs)}] ${s.speakerName}: ${s.text}`)
        .join("\n");
    },

    clear(): void {
      segments.length = 0;
    },
  };
}

function formatTime(ms: number): string {
  const totalSec = Math.floor(ms / 1000);
  const min = Math.floor(totalSec / 60);
  const sec = totalSec % 60;
  return `${min.toString().padStart(2, "0")}:${sec.toString().padStart(2, "0")}`;
}

// ── Playback Controller ────────────────────────────────────────────────────

export function createPlaybackController(
  timeline: TranscriptTimeline,
): PlaybackController {
  let positionMs = 0;
  let playing = false;
  let speed = 1.0;
  const listeners = new Set<PlaybackListener>();

  function notifyPosition(): void {
    for (const listener of listeners) listener(positionMs);
  }

  return {
    get positionMs() { return positionMs; },
    get playing() { return playing; },
    get speed() { return speed; },

    play(): void {
      playing = true;
    },

    pause(): void {
      playing = false;
    },

    seek(ms: number): void {
      positionMs = Math.max(0, ms);
      notifyPosition();
    },

    setSpeed(s: number): void {
      speed = Math.max(0.25, Math.min(4.0, s));
    },

    seekToSegment(segmentId: string): void {
      const seg = timeline.segments.find(s => s.id === segmentId);
      if (seg) {
        positionMs = seg.startMs;
        notifyPosition();
      }
    },

    onPositionChange(listener: PlaybackListener): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },
  };
}

// ── Test Transport ─────────────────────────────────────────────────────────

export function createTestTransport(): SessionTransport {
  let connected = false;
  const publishedTracks = new Map<string, MediaTrack>();
  const listeners = new Set<TransportEventListener>();

  function notify(event: TransportEvent): void {
    for (const listener of listeners) listener(event);
  }

  return {
    kind: "test",

    get connected() { return connected; },

    async connect(_rid: string, _token: string): Promise<void> {
      connected = true;
      notify({ type: "connected" });
    },

    async disconnect(): Promise<void> {
      connected = false;
      publishedTracks.clear();
      notify({ type: "disconnected" });
    },

    async publishTrack(track: MediaTrack): Promise<void> {
      publishedTracks.set(track.id, track);
      notify({ type: "track-published", trackId: track.id, participantId: track.participantId });
    },

    async unpublishTrack(trackId: string): Promise<void> {
      publishedTracks.delete(trackId);
      notify({ type: "track-unpublished", trackId });
    },

    onEvent(listener: TransportEventListener): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },

    async dispose(): Promise<void> {
      if (connected) {
        connected = false;
        notify({ type: "disconnected" });
      }
      listeners.clear();
      publishedTracks.clear();
    },
  };
}

// ── Test Transcription Provider ────────────────────────────────────────────

export interface TestTranscriptionProvider extends TranscriptionProvider {
  feedSegment(segment: TranscriptSegment): void;
}

export function createTestTranscriptionProvider(): TestTranscriptionProvider {
  const segmentListeners = new Set<(segment: TranscriptSegment) => void>();
  let running = false;

  return {
    name: "test",

    async isAvailable() { return true; },

    async start(_options: TranscriptionOptions): Promise<void> {
      running = true;
    },

    async stop(): Promise<void> {
      running = false;
    },

    onSegment(listener: (segment: TranscriptSegment) => void): () => void {
      segmentListeners.add(listener);
      return () => { segmentListeners.delete(listener); };
    },

    feedSegment(segment: TranscriptSegment): void {
      if (!running) return;
      for (const listener of segmentListeners) listener(segment);
    },
  };
}

// ── Session Manager ────────────────────────────────────────────────────────

export function createSessionManager(
  options: SessionManagerOptions = {},
): SessionManager {
  const { maxParticipants = 0 } = options;

  let status: SessionStatus = "idle";
  let config: SessionConfig | null = null;
  const participants: SessionParticipant[] = [];
  const tracks: MediaTrack[] = [];
  const delegations: DelegationRequest[] = [];
  const changeListeners = new Set<SessionChangeListener>();
  const transcript = createTranscriptTimeline();
  const playback = createPlaybackController(transcript);

  function notifyChange(change: SessionChangeType): void {
    for (const listener of changeListeners) listener(change);
  }

  function findParticipant(id: string): number {
    return participants.findIndex(p => p.id === id);
  }

  function findTrack(id: string): number {
    return tracks.findIndex(t => t.id === id);
  }

  function findDelegation(id: string): number {
    return delegations.findIndex(d => d.id === id);
  }

  function getParticipant(idx: number): SessionParticipant {
    const p = participants[idx];
    if (!p) throw new Error(`Participant index ${idx} out of bounds`);
    return p;
  }

  function getTrack(idx: number): MediaTrack {
    const t = tracks[idx];
    if (!t) throw new Error(`Track index ${idx} out of bounds`);
    return t;
  }

  function getDelegation(idx: number): DelegationRequest {
    const d = delegations[idx];
    if (!d) throw new Error(`Delegation index ${idx} out of bounds`);
    return d;
  }

  return {
    get status() { return status; },
    get config() { return config; },
    get participants() { return participants; },
    get transcript() { return transcript; },
    get playback() { return playback; },
    get tracks() { return tracks; },
    get delegations() { return delegations; },

    create(cfg: SessionConfig): void {
      if (status !== "idle") throw new Error("Session already active");
      config = cfg;
      status = "active";

      const local: SessionParticipant = {
        id: cfg.localParticipant.id,
        displayName: cfg.localParticipant.displayName,
        role: cfg.localParticipant.role,
        activeMedia: [],
        muted: false,
        videoEnabled: false,
        joinedAt: new Date().toISOString(),
        canDelegate: true,
      };
      participants.push(local);
      notifyChange({ type: "status-changed" });
    },

    join(cfg: SessionConfig): void {
      if (status !== "idle") throw new Error("Session already active");
      config = cfg;
      status = "active";

      const local: SessionParticipant = {
        id: cfg.localParticipant.id,
        displayName: cfg.localParticipant.displayName,
        role: cfg.localParticipant.role,
        activeMedia: [],
        muted: false,
        videoEnabled: false,
        joinedAt: new Date().toISOString(),
        canDelegate: false,
      };
      participants.push(local);
      notifyChange({ type: "status-changed" });
    },

    end(): void {
      status = "ended";
      notifyChange({ type: "status-changed" });
    },

    pause(): void {
      if (status !== "active") return;
      status = "paused";
      playback.pause();
      notifyChange({ type: "status-changed" });
    },

    resume(): void {
      if (status !== "paused") return;
      status = "active";
      notifyChange({ type: "status-changed" });
    },

    addParticipant(participant: SessionParticipant): void {
      if (maxParticipants > 0 && participants.length >= maxParticipants) {
        throw new Error(`Maximum participants (${maxParticipants}) reached`);
      }
      if (findParticipant(participant.id) >= 0) return;
      participants.push(participant);
      notifyChange({ type: "participant-joined" });
    },

    removeParticipant(participantId: string): void {
      const idx = findParticipant(participantId);
      if (idx < 0) return;
      participants.splice(idx, 1);
      for (let i = tracks.length - 1; i >= 0; i--) {
        const t = getTrack(i);
        if (t.participantId === participantId) {
          tracks.splice(i, 1);
        }
      }
      notifyChange({ type: "participant-left" });
    },

    setParticipantRole(participantId: string, role: ParticipantRole): void {
      const idx = findParticipant(participantId);
      if (idx < 0) return;
      participants[idx] = makeParticipant(getParticipant(idx), { role });
      notifyChange({ type: "participant-joined" });
    },

    setParticipantMuted(participantId: string, muted: boolean): void {
      const idx = findParticipant(participantId);
      if (idx < 0) return;
      participants[idx] = makeParticipant(getParticipant(idx), { muted });
      notifyChange({ type: "participant-joined" });
    },

    addTrack(track: MediaTrack): void {
      if (findTrack(track.id) >= 0) return;
      tracks.push(track);
      const pIdx = findParticipant(track.participantId);
      if (pIdx >= 0) {
        const p = getParticipant(pIdx);
        if (!p.activeMedia.includes(track.kind)) {
          participants[pIdx] = makeParticipant(p, {
            activeMedia: [...p.activeMedia, track.kind],
          });
        }
      }
      notifyChange({ type: "track-changed" });
    },

    removeTrack(trackId: string): void {
      const idx = findTrack(trackId);
      if (idx < 0) return;
      const removedTrack = getTrack(idx);
      tracks.splice(idx, 1);
      const pIdx = findParticipant(removedTrack.participantId);
      if (pIdx >= 0) {
        const remaining = tracks
          .filter(t => t.participantId === removedTrack.participantId)
          .map(t => t.kind);
        participants[pIdx] = makeParticipant(getParticipant(pIdx), {
          activeMedia: [...new Set(remaining)],
        });
      }
      notifyChange({ type: "track-changed" });
    },

    setTrackMuted(trackId: string, muted: boolean): void {
      const idx = findTrack(trackId);
      if (idx < 0) return;
      tracks[idx] = makeTrack(getTrack(idx), { muted });
      notifyChange({ type: "track-changed" });
    },

    requestDelegation(taskType: string, payload: Record<string, unknown>): DelegationRequest {
      const localId = config?.localParticipant.id ?? "unknown";
      const capable = participants.find(p => p.id !== localId && p.canDelegate);

      const request: DelegationRequest = {
        id: uid("deleg"),
        taskType,
        requesterId: localId,
        delegateeId: capable?.id ?? null,
        status: capable ? "pending" : "rejected",
        payload,
        createdAt: new Date().toISOString(),
      };

      delegations.push(request);
      notifyChange({ type: "delegation-updated" });
      return request;
    },

    respondToDelegation(requestId: string, accept: boolean): void {
      const idx = findDelegation(requestId);
      if (idx < 0) return;
      const req = getDelegation(idx);
      if (req.status !== "pending") return;

      delegations[idx] = makeDelegation(req, {
        status: accept ? "accepted" : "rejected",
      });
      notifyChange({ type: "delegation-updated" });
    },

    onChange(listener: SessionChangeListener): () => void {
      changeListeners.add(listener);
      return () => { changeListeners.delete(listener); };
    },

    dispose(): void {
      status = "ended";
      participants.length = 0;
      tracks.length = 0;
      delegations.length = 0;
      transcript.clear();
      changeListeners.clear();
    },
  };
}
