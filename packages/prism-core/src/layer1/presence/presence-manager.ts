/**
 * @prism/core — PresenceManager
 *
 * RAM-only state for connected peers. No CRDT persistence.
 * Provides reactive subscriptions for cursor/selection overlay rendering.
 *
 * Features:
 *   - Track remote peers' cursor, selection, and active view
 *   - Update local peer state for broadcast
 *   - TTL-based eviction for disconnected peers
 *   - subscribe(listener) for reactive UI updates
 *
 * Usage:
 *   const pm = createPresenceManager({ localIdentity: { peerId: 'me', ... } });
 *   pm.updateLocal({ cursor: { objectId: 'obj-1' } });
 *   pm.receiveRemote(remotePeerState);
 *   pm.subscribe((change) => { ... });
 */

import type {
  PresenceState,
  CursorPosition,
  SelectionRange,
  PresenceChange,
  PresenceListener,
  PresenceManagerOptions,
  TimerProvider,
} from "./presence-types.js";

// ── Interface ────────────────────────────────────────────────────────────────

export interface PresenceManager {
  /** Get local peer's current state. */
  readonly local: PresenceState;
  /** Get a specific peer's state. */
  get(peerId: string): PresenceState | undefined;
  /** Whether a peer is currently tracked. */
  has(peerId: string): boolean;
  /** All remote peers (excludes local). */
  getPeers(): PresenceState[];
  /** All peers including local. */
  getAll(): PresenceState[];
  /** Number of remote peers. */
  readonly peerCount: number;
  /** Update local peer's cursor. */
  setCursor(cursor: CursorPosition | null): void;
  /** Update local peer's selections. */
  setSelections(selections: SelectionRange[]): void;
  /** Update local peer's active view. */
  setActiveView(view: string | null): void;
  /** Update local peer's arbitrary data. */
  setData(data: Record<string, unknown>): void;
  /** Bulk update local peer state. */
  updateLocal(partial: Partial<Pick<PresenceState, "cursor" | "selections" | "activeView" | "data">>): void;
  /** Receive a remote peer's state (from awareness protocol). */
  receiveRemote(state: PresenceState): void;
  /** Explicitly remove a remote peer. */
  removePeer(peerId: string): void;
  /** Subscribe to presence changes (joined/updated/left). */
  subscribe(listener: PresenceListener): () => void;
  /** Force an eviction sweep (normally automatic). */
  sweep(): string[];
  /** Stop the eviction timer and clear all remote peers. */
  dispose(): void;
}

// ── Defaults ─────────────────────────────────────────────────────────────────

const DEFAULT_TTL_MS = 30_000;
const DEFAULT_SWEEP_INTERVAL_MS = 5_000;

const defaultTimers: TimerProvider = {
  now: () => Date.now(),
  setInterval: (fn, ms) => globalThis.setInterval(fn, ms) as unknown as number,
  clearInterval: (id) => globalThis.clearInterval(id),
};

// ── Factory ──────────────────────────────────────────────────────────────────

export function createPresenceManager(
  options: PresenceManagerOptions,
): PresenceManager {
  const {
    localIdentity,
    ttlMs = DEFAULT_TTL_MS,
    sweepIntervalMs = DEFAULT_SWEEP_INTERVAL_MS,
    timers = defaultTimers,
  } = options;

  const remotePeers = new Map<string, PresenceState>();
  const listeners = new Set<PresenceListener>();

  let localState: PresenceState = {
    identity: localIdentity,
    cursor: null,
    selections: [],
    activeView: null,
    lastSeen: new Date(timers.now()).toISOString(),
    data: {},
  };

  // ── Notification ─────────────────────────────────────────────────────────

  function notify(change: PresenceChange): void {
    for (const listener of listeners) {
      listener(change);
    }
  }

  // ── Local updates ────────────────────────────────────────────────────────

  function touchLocal(): void {
    localState = { ...localState, lastSeen: new Date(timers.now()).toISOString() };
  }

  function setCursor(cursor: CursorPosition | null): void {
    localState = { ...localState, cursor };
    touchLocal();
    notify({ type: "updated", peerId: localIdentity.peerId, state: localState });
  }

  function setSelections(selections: SelectionRange[]): void {
    localState = { ...localState, selections: [...selections] };
    touchLocal();
    notify({ type: "updated", peerId: localIdentity.peerId, state: localState });
  }

  function setActiveView(view: string | null): void {
    localState = { ...localState, activeView: view };
    touchLocal();
    notify({ type: "updated", peerId: localIdentity.peerId, state: localState });
  }

  function setData(data: Record<string, unknown>): void {
    localState = { ...localState, data: { ...data } };
    touchLocal();
    notify({ type: "updated", peerId: localIdentity.peerId, state: localState });
  }

  function updateLocal(
    partial: Partial<Pick<PresenceState, "cursor" | "selections" | "activeView" | "data">>,
  ): void {
    localState = {
      ...localState,
      ...(partial.cursor !== undefined ? { cursor: partial.cursor } : {}),
      ...(partial.selections !== undefined ? { selections: [...partial.selections] } : {}),
      ...(partial.activeView !== undefined ? { activeView: partial.activeView } : {}),
      ...(partial.data !== undefined ? { data: { ...partial.data } } : {}),
    };
    touchLocal();
    notify({ type: "updated", peerId: localIdentity.peerId, state: localState });
  }

  // ── Remote peer management ───────────────────────────────────────────────

  function receiveRemote(state: PresenceState): void {
    const peerId = state.identity.peerId;
    if (peerId === localIdentity.peerId) return; // ignore self

    const existing = remotePeers.has(peerId);
    remotePeers.set(peerId, { ...state, lastSeen: new Date(timers.now()).toISOString() });

    notify({
      type: existing ? "updated" : "joined",
      peerId,
      state: remotePeers.get(peerId) ?? null,
    });
  }

  function removePeer(peerId: string): void {
    const state = remotePeers.get(peerId);
    if (!state) return;
    remotePeers.delete(peerId);
    notify({ type: "left", peerId, state: null });
  }

  // ── TTL eviction ─────────────────────────────────────────────────────────

  function sweep(): string[] {
    const now = timers.now();
    const evicted: string[] = [];

    for (const [peerId, state] of remotePeers) {
      const lastSeenMs = new Date(state.lastSeen).getTime();
      if (now - lastSeenMs > ttlMs) {
        evicted.push(peerId);
      }
    }

    for (const peerId of evicted) {
      removePeer(peerId);
    }

    return evicted;
  }

  const sweepTimerId = sweepIntervalMs > 0
    ? timers.setInterval(sweep, sweepIntervalMs)
    : -1;

  // ── Query ────────────────────────────────────────────────────────────────

  function get(peerId: string): PresenceState | undefined {
    if (peerId === localIdentity.peerId) return localState;
    return remotePeers.get(peerId);
  }

  function has(peerId: string): boolean {
    if (peerId === localIdentity.peerId) return true;
    return remotePeers.has(peerId);
  }

  function getPeers(): PresenceState[] {
    return [...remotePeers.values()];
  }

  function getAll(): PresenceState[] {
    return [localState, ...remotePeers.values()];
  }

  // ── Subscribe ────────────────────────────────────────────────────────────

  function subscribe(listener: PresenceListener): () => void {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }

  // ── Dispose ──────────────────────────────────────────────────────────────

  function dispose(): void {
    if (sweepTimerId !== -1) {
      timers.clearInterval(sweepTimerId);
    }
    // Notify left for all remote peers
    for (const peerId of [...remotePeers.keys()]) {
      removePeer(peerId);
    }
    listeners.clear();
  }

  return {
    get local() {
      return localState;
    },
    get,
    has,
    getPeers,
    getAll,
    get peerCount() {
      return remotePeers.size;
    },
    setCursor,
    setSelections,
    setActiveView,
    setData,
    updateLocal,
    receiveRemote,
    removePeer,
    subscribe,
    sweep,
    dispose,
  };
}
