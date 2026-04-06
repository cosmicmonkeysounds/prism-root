/**
 * @prism/core — Ephemeral Presence Types
 *
 * Types for real-time collaboration awareness. All presence state is RAM-only;
 * nothing is persisted to Loro CRDT.
 */

// ── Cursor & Selection ───────────────────────────────────────────────────────

export interface CursorPosition {
  /** The object the cursor is on. */
  objectId: string;
  /** Optional field within the object (e.g. "name", "description"). */
  field?: string | undefined;
  /** Character offset within a text field. */
  offset?: number | undefined;
}

export interface SelectionRange {
  /** Object being selected. */
  objectId: string;
  /** Optional field for inline selection. */
  field?: string | undefined;
  /** Start offset (for text fields). */
  anchor?: number | undefined;
  /** End offset (for text fields). */
  head?: number | undefined;
}

// ── Peer Identity ────────────────────────────────────────────────────────────

export interface PeerIdentity {
  /** Unique peer ID (e.g. DID or session ID). */
  peerId: string;
  /** Display name for overlay rendering. */
  displayName: string;
  /** Hex colour for cursor/selection rendering. */
  color: string;
  /** Optional avatar URL. */
  avatarUrl?: string | undefined;
}

// ── Presence State ───────────────────────────────────────────────────────────

export interface PresenceState {
  /** Peer identity. */
  identity: PeerIdentity;
  /** Current cursor position, if any. */
  cursor: CursorPosition | null;
  /** Current selection ranges (multi-select). */
  selections: SelectionRange[];
  /** Active view/collection the peer has open. */
  activeView: string | null;
  /** ISO-8601 timestamp of last update from this peer. */
  lastSeen: string;
  /** Arbitrary per-peer metadata (e.g. status, flags). */
  data: Record<string, unknown>;
}

// ── Events ───────────────────────────────────────────────────────────────────

export type PresenceChangeType = "joined" | "updated" | "left";

export interface PresenceChange {
  type: PresenceChangeType;
  peerId: string;
  state: PresenceState | null;
}

export type PresenceListener = (change: PresenceChange) => void;

// ── Manager Options ──────────────────────────────────────────────────────────

export interface PresenceManagerOptions {
  /** Local peer identity. */
  localIdentity: PeerIdentity;
  /** TTL in milliseconds. Peers not seen within this window are evicted. Default: 30000. */
  ttlMs?: number;
  /** Interval in milliseconds for eviction sweep. Default: 5000. */
  sweepIntervalMs?: number;
  /** Timer functions for testing. */
  timers?: TimerProvider;
}

export interface TimerProvider {
  now(): number;
  setInterval(fn: () => void, ms: number): number;
  clearInterval(id: number): void;
}
