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

/**
 * Server-side play state. The transcript is opaque to the wire
 * layer (it's `loom_runtime::ledger::Event[]`); consumers cast or
 * re-derive as needed.
 *
 * Phase 4 of the Loom IDE redesign (docs/dev/loom-ide-redesign.md §5):
 * one session has many heads. Each head has its own ledger + world
 * + tracks + choices; the `primary` head is the editor's default
 * focus. Snapshots are server-side anchors for time-travel forks.
 */
export interface PlayEnvelopeMeta {
    track: number;
    cause: number | null;
}

export interface PlayTrackInfo {
    id: number;
    /** "booth" | "main" | "role" | "person" | "cohort" | "generator" */
    kind: string;
    label: string;
}

export interface PlayHeadState {
    id: string;
    /** `headId` if this head was forked from another live head. */
    parent: string | null;
    /** `snapId` if this head was created from a snapshot. */
    forkedFrom: string | null;
    transcript: unknown[];
    meta: PlayEnvelopeMeta[];
    choices: { index: number; text: string; sticky: boolean }[];
    ended: boolean;
    world: [string, string][];
    tracks: PlayTrackInfo[];
}

export interface PlaySnapshotInfo {
    id: string;
    headId: string;
    at: number;
    label: string | null;
}

export interface PlayStatePayload {
    workspace: string;
    primary: string;
    starter: string;
    heads: PlayHeadState[];
    snapshots: PlaySnapshotInfo[];
}

/**
 * Convenience selector — the active head's state. Returns `null`
 * when there's no session or the primary head doesn't exist (the
 * latter is defensive; the server guarantees it).
 */
export function primaryHead(play: PlayStatePayload | null): PlayHeadState | null {
    if (!play) return null;
    return play.heads.find((h) => h.id === play.primary) ?? null;
}

export interface PlayFile {
    path: string;
    source: string;
}

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
    | {
          kind: "play-start";
          payload: { workspace: string; files: PlayFile[] };
      }
    | {
          kind: "play-choice";
          payload: { workspace: string; index: number; head?: string };
      }
    | { kind: "play-stop"; payload: { workspace: string } }
    | {
          kind: "play-fork";
          payload: {
              workspace: string;
              parent?: string;
              from_snapshot?: string;
          };
      }
    | {
          kind: "play-snapshot";
          payload: { workspace: string; head?: string; label?: string };
      }
    | {
          kind: "play-restore";
          payload: { workspace: string; head: string; snapshot: string };
      }
    | {
          kind: "play-drop-head";
          payload: { workspace: string; head: string };
      }
    | {
          kind: "play-set-primary";
          payload: { workspace: string; head: string };
      }
    | {
          kind: "play-booth-skip";
          payload: { workspace: string; head?: string };
      }
    | {
          kind: "play-booth-force";
          payload: { workspace: string; head?: string; raw: string };
      }
    | {
          kind: "play-booth-reload";
          payload: { workspace: string; files: PlayFile[] };
      }
    | { kind: "play-state"; payload: PlayStatePayload }
    | { kind: "ping"; payload?: Record<string, never> }
    | { kind: "pong"; payload?: Record<string, never> };

export interface LoomSyncOptions {
    /** Full WebSocket URL, e.g. `ws://127.0.0.1:7878/ws`. */
    url: string;
    /** Session token minted by `LoomAuthClient`. */
    token: string;
    /** Optional shared presence cache. */
    presence?: PresenceTracker;
    /** Phase 7 — invoked on every `play-state` envelope. */
    onPlayState?: (state: PlayStatePayload) => void;
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

    /** Phase 7 — start (or replace) a server-side play session. */
    startPlay(workspaceId: string, files: PlayFile[]): void {
        this.sendEnvelope({
            kind: "play-start",
            payload: { workspace: workspaceId, files },
        });
    }

    /** Phase 7 — advance the server's play session by selecting a choice. */
    sendChoice(workspaceId: string, index: number, head?: string): void {
        this.sendEnvelope({
            kind: "play-choice",
            payload: { workspace: workspaceId, index, head },
        });
    }

    /** Phase 7 — tear down the active play session. */
    stopPlay(workspaceId: string): void {
        this.sendEnvelope({
            kind: "play-stop",
            payload: { workspace: workspaceId },
        });
    }

    // ── Phase 4: branching ──────────────────────────────────────────

    forkPlay(workspaceId: string, opts?: { parent?: string; from_snapshot?: string }): void {
        this.sendEnvelope({
            kind: "play-fork",
            payload: {
                workspace: workspaceId,
                parent: opts?.parent,
                from_snapshot: opts?.from_snapshot,
            },
        });
    }

    snapshotPlay(workspaceId: string, opts?: { head?: string; label?: string }): void {
        this.sendEnvelope({
            kind: "play-snapshot",
            payload: {
                workspace: workspaceId,
                head: opts?.head,
                label: opts?.label,
            },
        });
    }

    restorePlay(workspaceId: string, head: string, snapshot: string): void {
        this.sendEnvelope({
            kind: "play-restore",
            payload: { workspace: workspaceId, head, snapshot },
        });
    }

    dropHead(workspaceId: string, head: string): void {
        this.sendEnvelope({
            kind: "play-drop-head",
            payload: { workspace: workspaceId, head },
        });
    }

    setPrimaryHead(workspaceId: string, head: string): void {
        this.sendEnvelope({
            kind: "play-set-primary",
            payload: { workspace: workspaceId, head },
        });
    }

    // ── Booth live-patch (spec §13.4) ─────────────────────────────

    boothSkip(workspaceId: string, head?: string): void {
        this.sendEnvelope({
            kind: "play-booth-skip",
            payload: { workspace: workspaceId, head },
        });
    }

    boothForce(workspaceId: string, raw: string, head?: string): void {
        this.sendEnvelope({
            kind: "play-booth-force",
            payload: { workspace: workspaceId, head, raw },
        });
    }

    boothReload(workspaceId: string, files: PlayFile[]): void {
        this.sendEnvelope({
            kind: "play-booth-reload",
            payload: { workspace: workspaceId, files },
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
            case "play-state": {
                this.opts.onPlayState?.(env.payload);
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
