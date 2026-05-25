// Remote-peer presence tracker. The server's `presence` envelope
// carries `{ workspace, peers: PresenceState[] }`; this module is
// the client-side cache + observable surface the editor reads to
// paint remote carets / status badges.
//
// Intentionally framework-free: the `subscribe` method matches the
// shape Zustand expects (`(listener) => unsubscribe`), but it also
// stands alone for components that just want a callback.

export interface CursorPosition {
    file?: string;
    line?: number;
    column?: number;
}

export interface SelectionRange {
    file?: string;
    anchorLine?: number;
    anchorColumn?: number;
    headLine?: number;
    headColumn?: number;
}

export interface PresenceState {
    peerId: string;
    displayName?: string;
    color?: string;
    cursor?: CursorPosition;
    selections?: SelectionRange[];
    activeView?: string;
    updatedAtMs?: number;
}

export type PresenceListener = (
    workspace: string,
    peers: PresenceState[],
) => void;

export class PresenceTracker {
    private peers = new Map<string, Map<string, PresenceState>>();
    private listeners = new Set<PresenceListener>();

    /** Replace the peer set for `workspace` (server envelope path). */
    setPeers(workspace: string, peers: PresenceState[]): void {
        const map = new Map<string, PresenceState>();
        for (const p of peers) map.set(p.peerId, p);
        this.peers.set(workspace, map);
        this.notify(workspace);
    }

    /** Apply a single peer update (incremental presence message). */
    updatePeer(workspace: string, peer: PresenceState): void {
        let map = this.peers.get(workspace);
        if (!map) {
            map = new Map();
            this.peers.set(workspace, map);
        }
        map.set(peer.peerId, peer);
        this.notify(workspace);
    }

    /** Remove a peer (left / TTL expired). */
    removePeer(workspace: string, peerId: string): void {
        const map = this.peers.get(workspace);
        if (!map) return;
        if (map.delete(peerId)) this.notify(workspace);
    }

    /** Current snapshot of peers in `workspace`. */
    getPeers(workspace: string): PresenceState[] {
        const map = this.peers.get(workspace);
        if (!map) return [];
        return Array.from(map.values());
    }

    /** Subscribe to every peer-set update. Returns an unsubscribe fn. */
    subscribe(listener: PresenceListener): () => void {
        this.listeners.add(listener);
        return () => this.listeners.delete(listener);
    }

    private notify(workspace: string): void {
        const peers = this.getPeers(workspace);
        for (const listener of this.listeners) listener(workspace, peers);
    }
}
