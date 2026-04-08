/**
 * Peer Cursors Overlay (Tier 8E).
 *
 * A thin reactive overlay that reads `kernel.presence` state and surfaces it
 * inside the canvas:
 *   - a `data-testid="peer-cursors-bar"` header chip listing every remote peer
 *     currently connected (colored dot + display name);
 *   - a per-block badge (`data-testid="peer-selection-{objectId}"`) whenever a
 *     remote peer has that object selected — the block-based analogue of a
 *     text-editor caret.
 *
 * Presence state is RAM-only (see `@prism/core/presence`), so this overlay is
 * entirely driven off the reactive `usePresence()` hook. The local peer is
 * filtered out so authors only see *other* people's cursors.
 */

import { useMemo } from "react";
import { usePresence } from "../kernel/index.js";
import type { PresenceState } from "@prism/core/presence";

/**
 * Pure helper: given a list of peers and an optional local peer ID, return the
 * map `objectId -> peers-with-that-object-selected` for remote peers only.
 * Exported so tests can cover the grouping logic without rendering React.
 */
export function groupPeerSelections(
  peers: ReadonlyArray<PresenceState>,
  localPeerId: string | null,
): Map<string, PresenceState[]> {
  const out = new Map<string, PresenceState[]>();
  for (const peer of peers) {
    if (localPeerId && peer.identity.peerId === localPeerId) continue;
    // Prefer explicit selections, fall back to cursor objectId.
    const objectIds = new Set<string>();
    for (const sel of peer.selections) objectIds.add(sel.objectId);
    if (peer.cursor?.objectId) objectIds.add(peer.cursor.objectId);
    for (const id of objectIds) {
      const list = out.get(id) ?? [];
      list.push(peer);
      out.set(id, list);
    }
  }
  return out;
}

/** Header chip row showing every remote peer connected to the canvas. */
export function PeerCursorsBar() {
  const { peers, localPeer } = usePresence();
  const remotePeers = useMemo(
    () => peers.filter((p) => p.identity.peerId !== localPeer.identity.peerId),
    [peers, localPeer.identity.peerId],
  );

  if (remotePeers.length === 0) {
    // Render an empty, test-visible bar so E2E tests can detect presence wiring
    // even when no remote peers are connected.
    return (
      <div
        data-testid="peer-cursors-bar"
        data-peer-count={0}
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "4px 8px",
          background: "#1e1e1e",
          borderBottom: "1px solid #333",
          fontSize: 11,
          color: "#666",
        }}
      >
        <span>No remote peers connected</span>
      </div>
    );
  }

  return (
    <div
      data-testid="peer-cursors-bar"
      data-peer-count={remotePeers.length}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 6,
        padding: "4px 8px",
        background: "#1e1e1e",
        borderBottom: "1px solid #333",
        fontSize: 11,
        color: "#ccc",
      }}
    >
      <span style={{ color: "#888" }}>Live:</span>
      {remotePeers.map((peer) => (
        <span
          key={peer.identity.peerId}
          data-testid={`peer-chip-${peer.identity.peerId}`}
          style={{
            display: "inline-flex",
            alignItems: "center",
            gap: 4,
            padding: "2px 6px",
            borderRadius: 10,
            background: "#252526",
            border: `1px solid ${peer.identity.color}`,
          }}
        >
          <span
            style={{
              width: 8,
              height: 8,
              borderRadius: "50%",
              background: peer.identity.color,
              display: "inline-block",
            }}
          />
          {peer.identity.displayName}
        </span>
      ))}
    </div>
  );
}

/**
 * Small inline badge rendered by block components whenever one or more remote
 * peers have that block selected. Renders null when nobody else has it.
 */
export function PeerSelectionBadge({ objectId }: { objectId: string }) {
  const { peers, localPeer } = usePresence();
  const selections = useMemo(
    () => groupPeerSelections(peers, localPeer.identity.peerId),
    [peers, localPeer.identity.peerId],
  );
  const peersOnBlock = selections.get(objectId);
  if (!peersOnBlock || peersOnBlock.length === 0) return null;

  return (
    <div
      data-testid={`peer-selection-${objectId}`}
      style={{
        position: "absolute",
        top: -10,
        left: -10,
        display: "flex",
        gap: 2,
        zIndex: 12,
        pointerEvents: "none",
      }}
    >
      {peersOnBlock.map((peer) => (
        <span
          key={peer.identity.peerId}
          title={peer.identity.displayName}
          style={{
            width: 14,
            height: 14,
            borderRadius: "50%",
            background: peer.identity.color,
            border: "2px solid #1e1e1e",
            display: "inline-block",
          }}
        />
      ))}
    </div>
  );
}
