/**
 * Presence Indicator — shows connected peers as colored dots in the header bar.
 *
 * Displays the local peer plus any remote peers tracked by PresenceManager.
 * Each peer shows as a colored circle with their initial, with a tooltip
 * showing their display name. Peer count badge shown when > 1.
 */

import { usePresence } from "../kernel/index.js";

export function PresenceIndicator() {
  const { peers, localPeer, peerCount } = usePresence();

  const allPeers = [localPeer, ...peers.filter(
    (p) => p.identity.peerId !== localPeer.identity.peerId,
  )];

  return (
    <div
      data-testid="presence-indicator"
      style={{
        display: "flex",
        alignItems: "center",
        gap: 2,
        padding: "0 4px",
      }}
    >
      {allPeers.slice(0, 5).map((peer) => (
        <div
          key={peer.identity.peerId}
          data-testid={`presence-peer-${peer.identity.peerId}`}
          title={peer.identity.displayName}
          style={{
            width: 20,
            height: 20,
            borderRadius: "50%",
            background: peer.identity.color,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            fontSize: 10,
            fontWeight: 600,
            color: "#fff",
            border: peer.identity.peerId === localPeer.identity.peerId
              ? "2px solid #fff"
              : "2px solid transparent",
            cursor: "default",
            flexShrink: 0,
          }}
        >
          {peer.identity.displayName.charAt(0).toUpperCase()}
        </div>
      ))}
      {peerCount > 0 && (
        <span
          data-testid="presence-count"
          style={{
            fontSize: 10,
            color: "#888",
            marginLeft: 2,
          }}
        >
          {peerCount + 1}
        </span>
      )}
    </div>
  );
}
