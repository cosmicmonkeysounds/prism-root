// `LoomSyncClient` — the browser-side WebSocket client that talks the
// envelope protocol defined in `docs/dev/loom-multiuser.md` against
// `loom-server`. Wraps a `LoomDoc` (from the loom-wasm bundle), forwards
// local commits as `update` envelopes, and applies remote `update` /
// `snapshot` envelopes back onto the doc.
//
// Intentionally framework-free: hosts wire it into their store (or
// directly to `presence.ts` for cursor sharing). Uses the native
// `WebSocket` global; no extra deps.

import type { LoomDoc, SubscriptionHandle } from "../loom-wasm/loom_wasm";
import type { PresenceState, PresenceTracker } from "./presence";

// ── envelope shapes (mirror `prism-core::network::relay::message`) ──

type Envelope =
    | { kind: "auth"; payload: { token: string } }
    | { kind: "auth-ok"; payload: { did: string } }
    | { kind: "error"; payload: { message: string } }
    | { kind: "subscribe"; payload: { workspace: string } }
    | { kind: "unsubscribe"; payload: { workspace: string } }
    | { kind: "snapshot"; payload: { workspace: string; bytes: string } }
    | { kind: "update"; payload: { workspace: string; bytes: string } }
    | {
          kind: "presence";
          payload: {
              workspace: string;
              state?: PresenceState;
              peers?: PresenceState[];
          };
      }
    | { kind: "ping"; payload?: Record<string, never> }
    | { kind: "pong"; payload?: Record<string, never> };

export interface LoomSyncOptions {
    /** Full WebSocket URL, e.g. `ws://127.0.0.1:7878/ws`. */
    url: string;
    /** Session token minted by `LoomAuthClient`. */
    token: string;
    /** Optional shared presence cache. */
    presence?: PresenceTracker;
    /** Connection / protocol error sink. */
    onError?: (err: Error) => void;
    /** Debounce window for outgoing update batches (default 50ms). */
    updateDebounceMs?: number;
}

type Subscription = {
    doc: LoomDoc;
    handle: SubscriptionHandle;
    /** Encoded version vector of the last update we *sent*. */
    lastSentVersion: Uint8Array | null;
    /** Pending debounce timer id, or 0 if idle. */
    flushTimer: ReturnType<typeof setTimeout> | 0;
};

export class LoomSyncClient {
    private ws: WebSocket | null = null;
    private authed = false;
    private readonly outbox: string[] = [];
    private readonly subs = new Map<string, Subscription>();
    private closed = false;
    private readonly opts: LoomSyncOptions;

    constructor(opts: LoomSyncOptions) {
        this.opts = opts;
    }

    connect(): Promise<void> {
        return new Promise((resolve, reject) => {
            const ws = new WebSocket(this.opts.url);
            this.ws = ws;
            ws.binaryType = "arraybuffer";

            ws.onopen = () => {
                this.sendEnvelope({
                    kind: "auth",
                    payload: { token: this.opts.token },
                });
            };

            ws.onmessage = (ev) => {
                let env: Envelope;
                try {
                    env = JSON.parse(String(ev.data));
                } catch (err) {
                    this.report(err);
                    return;
                }
                if (env.kind === "auth-ok") {
                    this.authed = true;
                    this.flushOutbox();
                    resolve();
                    return;
                }
                if (env.kind === "error") {
                    const err = new Error(env.payload.message);
                    this.report(err);
                    if (!this.authed) reject(err);
                    return;
                }
                this.handleEnvelope(env);
            };

            ws.onerror = () => {
                const err = new Error("websocket error");
                this.report(err);
                if (!this.authed) reject(err);
            };

            ws.onclose = () => {
                this.authed = false;
                this.ws = null;
            };
        });
    }

    /** Subscribe `doc` to live updates for `workspaceId`. */
    subscribe(workspaceId: string, doc: LoomDoc): void {
        if (this.subs.has(workspaceId)) return;

        const sub: Subscription = {
            doc,
            handle: doc.subscribe(() => this.scheduleFlush(workspaceId)),
            lastSentVersion: null,
            flushTimer: 0,
        };
        this.subs.set(workspaceId, sub);

        this.sendEnvelope({
            kind: "subscribe",
            payload: { workspace: workspaceId },
        });
    }

    unsubscribe(workspaceId: string): void {
        const sub = this.subs.get(workspaceId);
        if (!sub) return;
        if (sub.flushTimer) clearTimeout(sub.flushTimer);
        sub.handle.unsubscribe();
        this.subs.delete(workspaceId);
        this.sendEnvelope({
            kind: "unsubscribe",
            payload: { workspace: workspaceId },
        });
    }

    publishPresence(workspaceId: string, state: PresenceState): void {
        this.sendEnvelope({
            kind: "presence",
            payload: { workspace: workspaceId, state },
        });
    }

    close(): void {
        this.closed = true;
        for (const id of Array.from(this.subs.keys())) this.unsubscribe(id);
        this.ws?.close();
        this.ws = null;
    }

    // ── internals ──────────────────────────────────────────────────

    private handleEnvelope(env: Envelope): void {
        switch (env.kind) {
            case "snapshot": {
                const sub = this.subs.get(env.payload.workspace);
                if (!sub) return;
                try {
                    sub.doc.importSnapshot(base64Decode(env.payload.bytes));
                    sub.lastSentVersion = sub.doc.currentVersion();
                } catch (err) {
                    this.report(err);
                }
                return;
            }
            case "update": {
                const sub = this.subs.get(env.payload.workspace);
                if (!sub) return;
                try {
                    sub.doc.applyUpdate(base64Decode(env.payload.bytes));
                    // Remote update — bump our high-water mark so we
                    // don't bounce it straight back as our own delta.
                    sub.lastSentVersion = sub.doc.currentVersion();
                } catch (err) {
                    this.report(err);
                }
                return;
            }
            case "presence": {
                if (!this.opts.presence) return;
                if (env.payload.peers) {
                    this.opts.presence.setPeers(
                        env.payload.workspace,
                        env.payload.peers,
                    );
                } else if (env.payload.state) {
                    this.opts.presence.updatePeer(
                        env.payload.workspace,
                        env.payload.state,
                    );
                }
                return;
            }
            case "ping":
                this.sendEnvelope({ kind: "pong" });
                return;
            default:
                return;
        }
    }

    private scheduleFlush(workspaceId: string): void {
        const sub = this.subs.get(workspaceId);
        if (!sub || sub.flushTimer) return;
        const debounce = this.opts.updateDebounceMs ?? 50;
        sub.flushTimer = setTimeout(() => {
            sub.flushTimer = 0;
            this.flush(workspaceId);
        }, debounce);
    }

    private flush(workspaceId: string): void {
        const sub = this.subs.get(workspaceId);
        if (!sub) return;
        let bytes: Uint8Array;
        try {
            bytes = sub.doc.exportUpdatesSince(sub.lastSentVersion ?? undefined);
        } catch (err) {
            this.report(err);
            return;
        }
        if (bytes.length === 0) return;
        sub.lastSentVersion = sub.doc.currentVersion();
        this.sendEnvelope({
            kind: "update",
            payload: { workspace: workspaceId, bytes: base64Encode(bytes) },
        });
    }

    private sendEnvelope(env: Envelope): void {
        const text = JSON.stringify(env);
        // Auth-gate everything except the initial `auth` envelope.
        const needsAuth = env.kind !== "auth";
        if (
            !this.ws ||
            this.ws.readyState !== WebSocket.OPEN ||
            (needsAuth && !this.authed)
        ) {
            if (!this.closed) this.outbox.push(text);
            return;
        }
        this.ws.send(text);
    }

    private flushOutbox(): void {
        if (!this.ws || this.ws.readyState !== WebSocket.OPEN) return;
        while (this.outbox.length > 0) {
            const next = this.outbox.shift();
            if (next != null) this.ws.send(next);
        }
    }

    private report(err: unknown): void {
        if (!this.opts.onError) return;
        this.opts.onError(err instanceof Error ? err : new Error(String(err)));
    }
}

// ── base64 helpers ────────────────────────────────────────────────

function base64Encode(bytes: Uint8Array): string {
    let binary = "";
    const chunk = 0x8000;
    for (let i = 0; i < bytes.length; i += chunk) {
        const slice = bytes.subarray(i, Math.min(i + chunk, bytes.length));
        binary += String.fromCharCode.apply(null, Array.from(slice));
    }
    return btoa(binary);
}

function base64Decode(b64: string): Uint8Array {
    const binary = atob(b64);
    const out = new Uint8Array(binary.length);
    for (let i = 0; i < binary.length; i++) out[i] = binary.charCodeAt(i);
    return out;
}
