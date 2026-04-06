/**
 * @prism/core — Communication Fabric Types (Layer 1)
 *
 * Real-time sessions, transcription, and A/V transport abstractions.
 *
 * Design principles:
 *   - SessionNode is a CRDT Collection with transcript + timeline
 *   - Transport is abstract: LiveKit (SFU), WebRTC (P2P), or test stubs
 *   - Self-Dictation via Whisper.cpp sidecar (Tauri provides the executor)
 *   - Hypermedia Playback syncs transcript to video seek
 *   - Listener Fallback delegates compute to capable peers
 */

// ── Session Status ─────────────────────────────────────────────────────────

export type SessionStatus = "idle" | "connecting" | "active" | "paused" | "ended";

export type ParticipantRole = "host" | "speaker" | "listener" | "observer";

export type MediaKind = "audio" | "video" | "screen";

// ── Participant ────────────────────────────────────────────────────────────

export interface SessionParticipant {
  /** Unique participant ID (typically a DID or peer ID). */
  id: string;
  /** Display name. */
  displayName: string;
  /** Role in the session. */
  role: ParticipantRole;
  /** Which media tracks are active. */
  activeMedia: MediaKind[];
  /** Whether this participant is muted. */
  muted: boolean;
  /** Whether video is enabled. */
  videoEnabled: boolean;
  /** ISO-8601 join timestamp. */
  joinedAt: string;
  /** Whether this peer can handle compute delegation. */
  canDelegate: boolean;
}

// ── Transcript ─────────────────────────────────────────────────────────────

export interface TranscriptSegment {
  /** Unique segment ID. */
  id: string;
  /** Speaker ID (participant ID). */
  speakerId: string;
  /** Speaker display name (denormalized for rendering). */
  speakerName: string;
  /** Transcribed text. */
  text: string;
  /** Start time relative to session start (ms). */
  startMs: number;
  /** End time relative to session start (ms). */
  endMs: number;
  /** Confidence score (0–1). */
  confidence: number;
  /** Language code (e.g. "en", "ja"). */
  language: string;
  /** Whether this segment is finalized or still being refined. */
  isFinal: boolean;
}

export interface TranscriptTimeline {
  /** All segments, ordered by startMs. */
  readonly segments: ReadonlyArray<TranscriptSegment>;
  /** Total duration in ms. */
  readonly durationMs: number;

  /** Add a new segment (or update an existing non-final one). */
  addSegment(segment: TranscriptSegment): void;
  /** Finalize a segment (mark as immutable). */
  finalizeSegment(segmentId: string): void;
  /** Get segments within a time range. */
  getRange(startMs: number, endMs: number): TranscriptSegment[];
  /** Get the segment at a specific time. */
  getAtTime(timeMs: number): TranscriptSegment | null;
  /** Search segments by text. */
  search(query: string): TranscriptSegment[];
  /** Export full transcript as plain text. */
  toPlainText(): string;
  /** Clear all segments. */
  clear(): void;
}

// ── Media Track ────────────────────────────────────────────────────────────

export type TrackState = "live" | "paused" | "ended";

export interface MediaTrack {
  /** Unique track ID. */
  id: string;
  /** Participant who owns this track. */
  participantId: string;
  /** Kind of media. */
  kind: MediaKind;
  /** Current track state. */
  state: TrackState;
  /** Track label (e.g. "Microphone", "Camera", "Screen Share"). */
  label: string;
  /** Whether the track is muted. */
  muted: boolean;
}

// ── Playback Sync ──────────────────────────────────────────────────────────

/**
 * Hypermedia playback controller.
 * Synchronizes transcript navigation with media seeking.
 */
export interface PlaybackController {
  /** Current playback position (ms from session start). */
  readonly positionMs: number;
  /** Whether playback is active. */
  readonly playing: boolean;
  /** Playback speed (1.0 = normal). */
  readonly speed: number;

  /** Start/resume playback. */
  play(): void;
  /** Pause playback. */
  pause(): void;
  /** Seek to a position (ms). */
  seek(positionMs: number): void;
  /** Set playback speed. */
  setSpeed(speed: number): void;
  /** Seek to a transcript segment. */
  seekToSegment(segmentId: string): void;
  /** Subscribe to position changes. */
  onPositionChange(listener: PlaybackListener): () => void;
}

export type PlaybackListener = (positionMs: number) => void;

// ── Transport ──────────────────────────────────────────────────────────────

export type TransportKind = "livekit" | "webrtc" | "test";

/**
 * Abstract session transport. Implementations provided by Layer 2/Daemon.
 *   - LiveKit: SFU-based for multi-party
 *   - WebRTC: P2P for 1:1 fallback
 *   - Test: in-memory for unit tests
 */
export interface SessionTransport {
  /** Transport identifier. */
  readonly kind: TransportKind;
  /** Whether the transport is connected. */
  readonly connected: boolean;

  /** Connect to a session room. */
  connect(roomId: string, token: string): Promise<void>;
  /** Disconnect from the current room. */
  disconnect(): Promise<void>;
  /** Publish a local media track. */
  publishTrack(track: MediaTrack): Promise<void>;
  /** Unpublish a local media track. */
  unpublishTrack(trackId: string): Promise<void>;
  /** Subscribe to transport events. */
  onEvent(listener: TransportEventListener): () => void;
  /** Dispose of transport resources. */
  dispose(): Promise<void>;
}

export type TransportEventType =
  | "connected"
  | "disconnected"
  | "participant-joined"
  | "participant-left"
  | "track-published"
  | "track-unpublished"
  | "track-muted"
  | "track-unmuted"
  | "data-received";

export interface TransportEvent {
  type: TransportEventType;
  participantId?: string;
  trackId?: string;
  data?: unknown;
}

export type TransportEventListener = (event: TransportEvent) => void;

// ── Transcription Provider ─────────────────────────────────────────────────

/**
 * Interface for speech-to-text providers.
 * Self-Dictation: Whisper.cpp sidecar provides this via Tauri.
 * Could also be a cloud STT service.
 */
export interface TranscriptionProvider {
  /** Provider name (e.g. "whisper", "cloud-stt"). */
  readonly name: string;
  /** Whether the provider is available. */
  isAvailable(): Promise<boolean>;
  /** Start transcribing an audio stream. */
  start(options: TranscriptionOptions): Promise<void>;
  /** Stop transcription. */
  stop(): Promise<void>;
  /** Subscribe to transcript segments as they arrive. */
  onSegment(listener: (segment: TranscriptSegment) => void): () => void;
}

export interface TranscriptionOptions {
  /** Language code for recognition. */
  language?: string;
  /** Whether to enable translation to English. */
  translate?: boolean;
  /** Model size hint (e.g. "tiny", "base", "small", "medium", "large"). */
  modelSize?: string;
}

// ── Delegation ─────────────────────────────────────────────────────────────

export type DelegationStatus = "pending" | "accepted" | "rejected" | "active" | "completed";

/**
 * Listener Fallback: delegate compute tasks to capable peers.
 * Used when a device lacks resources (e.g. mobile can't run Whisper).
 */
export interface DelegationRequest {
  /** Unique request ID. */
  id: string;
  /** The task to delegate (e.g. "transcription", "ai-inference"). */
  taskType: string;
  /** The requesting participant. */
  requesterId: string;
  /** The delegatee participant (if assigned). */
  delegateeId: string | null;
  /** Current delegation status. */
  status: DelegationStatus;
  /** Task-specific payload. */
  payload: Record<string, unknown>;
  /** ISO-8601 created timestamp. */
  createdAt: string;
}

export type DelegationListener = (request: DelegationRequest) => void;

// ── Session Node ───────────────────────────────────────────────────────────

export interface SessionChangeType {
  type: "participant-joined" | "participant-left" | "status-changed"
    | "transcript-updated" | "track-changed" | "delegation-updated";
}

export type SessionChangeListener = (change: SessionChangeType) => void;

// ── Session Manager ────────────────────────────────────────────────────────

export interface SessionManagerOptions {
  /** Default transport kind. */
  transport?: TransportKind;
  /** Default transcription language. */
  defaultLanguage?: string;
  /** Maximum participants (0 = unlimited). */
  maxParticipants?: number;
}

export interface SessionConfig {
  /** Session display name. */
  name: string;
  /** Room ID for the transport layer. */
  roomId: string;
  /** The local participant's info. */
  localParticipant: {
    id: string;
    displayName: string;
    role: ParticipantRole;
  };
}

/**
 * The SessionManager orchestrates a real-time communication session.
 * It ties together transport, transcription, media, and playback.
 */
export interface SessionManager {
  /** Current session status. */
  readonly status: SessionStatus;
  /** Session config (set after create/join). */
  readonly config: SessionConfig | null;
  /** All participants. */
  readonly participants: ReadonlyArray<SessionParticipant>;
  /** The transcript timeline. */
  readonly transcript: TranscriptTimeline;
  /** The playback controller. */
  readonly playback: PlaybackController;
  /** Active media tracks. */
  readonly tracks: ReadonlyArray<MediaTrack>;

  /** Create a new session as host. */
  create(config: SessionConfig): void;
  /** Join an existing session. */
  join(config: SessionConfig): void;
  /** End or leave the session. */
  end(): void;
  /** Pause the session (host only). */
  pause(): void;
  /** Resume a paused session (host only). */
  resume(): void;

  /** Add a participant. */
  addParticipant(participant: SessionParticipant): void;
  /** Remove a participant. */
  removeParticipant(participantId: string): void;
  /** Update a participant's role. */
  setParticipantRole(participantId: string, role: ParticipantRole): void;
  /** Mute/unmute a participant. */
  setParticipantMuted(participantId: string, muted: boolean): void;

  /** Add a media track. */
  addTrack(track: MediaTrack): void;
  /** Remove a media track. */
  removeTrack(trackId: string): void;
  /** Set track muted state. */
  setTrackMuted(trackId: string, muted: boolean): void;

  /** Request compute delegation to a capable peer. */
  requestDelegation(taskType: string, payload: Record<string, unknown>): DelegationRequest;
  /** Respond to a delegation request (as delegatee). */
  respondToDelegation(requestId: string, accept: boolean): void;
  /** List active delegation requests. */
  readonly delegations: ReadonlyArray<DelegationRequest>;

  /** Subscribe to session changes. */
  onChange(listener: SessionChangeListener): () => void;
  /** Dispose of all resources. */
  dispose(): void;
}
