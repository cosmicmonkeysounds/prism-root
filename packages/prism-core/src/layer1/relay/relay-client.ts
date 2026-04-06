/**
 * Prism Relay Client — connects apps to a deployed relay over WebSocket.
 *
 * Usage:
 *   const client = createRelayClient({
 *     url: "wss://relay.example.com/ws/relay",
 *     identity,
 *   });
 *   await client.connect();
 *   client.on("envelope", (env) => { ... });
 *   await client.send({ to, ciphertext, ttlMs });
 *   client.close();
 */

import type { DID } from "../identity/identity-types.js";
import type { PrismIdentity } from "../identity/identity-types.js";
import type { RelayEnvelope, RouteResult } from "./relay-types.js";

// ── Wire types (match @prism/relay protocol) ──────────────────────────────

interface WireEnvelope {
  id: string;
  from: DID;
  to: DID;
  ciphertext: string; // base64
  submittedAt: string;
  proofOfWork?: string;
  ttlMs: number;
}

// ── Base64 helpers ────────────────────────────────────────────────────────

function toBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function fromBase64(str: string): Uint8Array {
  const binary = atob(str);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

// ── Types ─────────────────────────────────────────────────────────────────

export interface RelayClientOptions {
  /** WebSocket URL (e.g. "wss://relay.example.com/ws/relay"). */
  url: string;
  /** Identity for authentication. */
  identity: PrismIdentity;
  /** Auto-reconnect on disconnect. Default: true. */
  autoReconnect?: boolean;
  /** Reconnect delay in ms. Default: 2000. */
  reconnectDelayMs?: number;
  /** Max reconnect attempts. Default: 10. 0 = unlimited. */
  maxReconnectAttempts?: number;
}

export interface SendEnvelopeOptions {
  /** Recipient DID. */
  to: DID;
  /** Encrypted payload. */
  ciphertext: Uint8Array;
  /** TTL in milliseconds. Default: 7 days. */
  ttlMs?: number;
  /** Optional proof-of-work token. */
  proofOfWork?: string;
}

export type RelayClientState = "disconnected" | "connecting" | "authenticating" | "connected" | "reconnecting";

export interface RelayClientEvents {
  connected: { relayDid: DID; modules: string[] };
  disconnected: { reason: string };
  envelope: RelayEnvelope;
  "route-result": RouteResult;
  "sync-snapshot": { collectionId: string; snapshot: Uint8Array };
  "sync-update": { collectionId: string; update: Uint8Array };
  error: { message: string };
  "state-change": { from: RelayClientState; to: RelayClientState };
}

type EventHandler<K extends keyof RelayClientEvents> = (data: RelayClientEvents[K]) => void;

export interface RelayClient {
  /** Current connection state. */
  readonly state: RelayClientState;
  /** DID of the connected relay (set after auth). */
  readonly relayDid: DID | null;
  /** Modules available on the connected relay. */
  readonly modules: string[];

  /** Connect to the relay and authenticate. */
  connect(): Promise<void>;
  /** Close the connection. */
  close(): void;

  /** Send an encrypted envelope through the relay. */
  send(options: SendEnvelopeOptions): Promise<RouteResult>;

  /** Request a collection snapshot. Subscribes to future updates. */
  syncRequest(collectionId: string): Promise<Uint8Array>;
  /** Push a CRDT update to a hosted collection. */
  syncUpdate(collectionId: string, update: Uint8Array): void;

  /** Subscribe to events. */
  on<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>): void;
  /** Unsubscribe from events. */
  off<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>): void;
}

// ── Implementation ────────────────────────────────────────────────────────

let idCounter = 0;
function uid(): string {
  return `env-${Date.now().toString(36)}-${(idCounter++).toString(36)}`;
}

const DEFAULT_TTL_MS = 7 * 24 * 60 * 60 * 1000;

export function createRelayClient(options: RelayClientOptions): RelayClient {
  const {
    url,
    identity,
    autoReconnect = true,
    reconnectDelayMs = 2000,
    maxReconnectAttempts = 10,
  } = options;

  let state: RelayClientState = "disconnected";
  let ws: WebSocket | null = null;
  let relayDid: DID | null = null;
  let modules: string[] = [];
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let intentionalClose = false;

  // Event system
  const listeners = new Map<string, Set<(...args: unknown[]) => void>>();

  function emit<K extends keyof RelayClientEvents>(event: K, data: RelayClientEvents[K]): void {
    const set = listeners.get(event);
    if (set) {
      for (const handler of set) handler(data);
    }
  }

  function setState(next: RelayClientState): void {
    if (state === next) return;
    const from = state;
    state = next;
    emit("state-change", { from, to: next });
  }

  // Pending promises for request-response patterns
  type PendingResolve = { resolve: (value: unknown) => void; reject: (err: Error) => void };
  const pendingRouteResults: PendingResolve[] = [];
  const pendingSyncs = new Map<string, PendingResolve>();

  function handleMessage(raw: string): void {
    let msg: Record<string, unknown>;
    try {
      msg = JSON.parse(raw) as Record<string, unknown>;
    } catch {
      return;
    }

    const type = msg["type"] as string;

    switch (type) {
      case "auth-ok": {
        relayDid = msg["relayDid"] as DID;
        modules = msg["modules"] as string[];
        setState("connected");
        reconnectAttempts = 0;
        emit("connected", { relayDid, modules });
        break;
      }

      case "envelope": {
        const wireEnv = msg["envelope"] as WireEnvelope;
        const envelope: RelayEnvelope = {
          id: wireEnv.id,
          from: wireEnv.from,
          to: wireEnv.to,
          ciphertext: fromBase64(wireEnv.ciphertext),
          submittedAt: wireEnv.submittedAt,
          ttlMs: wireEnv.ttlMs,
        };
        if (wireEnv.proofOfWork !== undefined) {
          envelope.proofOfWork = wireEnv.proofOfWork;
        }
        emit("envelope", envelope);
        break;
      }

      case "route-result": {
        const result = msg["result"] as RouteResult;
        emit("route-result", result);
        const pending = pendingRouteResults.shift();
        if (pending) pending.resolve(result);
        break;
      }

      case "sync-snapshot": {
        const collectionId = msg["collectionId"] as string;
        const snapshot = fromBase64(msg["snapshot"] as string);
        emit("sync-snapshot", { collectionId, snapshot });
        const pending = pendingSyncs.get(collectionId);
        if (pending) {
          pendingSyncs.delete(collectionId);
          pending.resolve(snapshot);
        }
        break;
      }

      case "sync-update": {
        const collectionId = msg["collectionId"] as string;
        const update = fromBase64(msg["update"] as string);
        emit("sync-update", { collectionId, update });
        break;
      }

      case "error": {
        const message = msg["message"] as string;
        emit("error", { message });
        // Reject the oldest pending request if it's likely related
        const pending = pendingRouteResults.shift() ?? [...pendingSyncs.values()].shift();
        if (pending) pending.reject(new Error(message));
        break;
      }

      case "pong": {
        // Heartbeat response, no action needed
        break;
      }
    }
  }

  function doConnect(): Promise<void> {
    return new Promise((resolve, reject) => {
      setState("connecting");
      intentionalClose = false;

      ws = new WebSocket(url);

      ws.addEventListener("open", () => {
        setState("authenticating");
        wsSend({ type: "auth", did: identity.did });
      });

      // Wait for auth-ok to resolve the connect promise
      const onAuthOk: EventHandler<"connected"> = () => {
        resolve();
        cleanup();
      };
      const onErrorHandler: EventHandler<"error"> = (data) => {
        reject(new Error(data.message));
        cleanup();
      };
      const cleanup = () => {
        off("connected", onAuthOk);
        off("error", onErrorHandler);
      };

      // Temporarily listen for auth completion
      on("connected", onAuthOk);
      on("error", onErrorHandler);

      ws.addEventListener("message", (evt) => {
        handleMessage(typeof evt.data === "string" ? evt.data : String(evt.data));
      });

      ws.addEventListener("close", () => {
        const wasConnected = state === "connected";
        setState("disconnected");
        ws = null;

        if (wasConnected) {
          emit("disconnected", { reason: intentionalClose ? "closed" : "connection lost" });
        }

        if (!intentionalClose && autoReconnect && (maxReconnectAttempts === 0 || reconnectAttempts < maxReconnectAttempts)) {
          setState("reconnecting");
          reconnectAttempts++;
          reconnectTimer = setTimeout(() => {
            reconnectTimer = null;
            doConnect().catch(() => {
              // Reconnect attempt failed, will retry via close handler
            });
          }, reconnectDelayMs);
        }
      });

      ws.addEventListener("error", () => {
        // Error will be followed by close, handled there
      });
    });
  }

  function wsSend(msg: Record<string, unknown>): void {
    if (ws && ws.readyState === WebSocket.OPEN) {
      ws.send(JSON.stringify(msg));
    }
  }

  // ── Public API ─────────────────────────────────────────────────────────

  function on<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>): void {
    let set = listeners.get(event);
    if (!set) {
      set = new Set();
      listeners.set(event, set);
    }
    set.add(handler as (...args: unknown[]) => void);
  }

  function off<K extends keyof RelayClientEvents>(event: K, handler: EventHandler<K>): void {
    listeners.get(event)?.delete(handler as (...args: unknown[]) => void);
  }

  function connect(): Promise<void> {
    if (state === "connected") return Promise.resolve();
    return doConnect();
  }

  function close(): void {
    intentionalClose = true;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    if (ws) {
      ws.close();
      ws = null;
    }
    setState("disconnected");
  }

  function send(sendOptions: SendEnvelopeOptions): Promise<RouteResult> {
    if (state !== "connected") {
      return Promise.reject(new Error("Not connected to relay"));
    }

    const envelope: WireEnvelope = {
      id: uid(),
      from: identity.did,
      to: sendOptions.to,
      ciphertext: toBase64(sendOptions.ciphertext),
      submittedAt: new Date().toISOString(),
      ttlMs: sendOptions.ttlMs ?? DEFAULT_TTL_MS,
    };
    if (sendOptions.proofOfWork !== undefined) {
      envelope.proofOfWork = sendOptions.proofOfWork;
    }

    return new Promise((resolve, reject) => {
      pendingRouteResults.push({ resolve: resolve as (v: unknown) => void, reject });
      wsSend({ type: "envelope", envelope });
    });
  }

  function syncRequest(collectionId: string): Promise<Uint8Array> {
    if (state !== "connected") {
      return Promise.reject(new Error("Not connected to relay"));
    }

    return new Promise((resolve, reject) => {
      pendingSyncs.set(collectionId, { resolve: resolve as (v: unknown) => void, reject });
      wsSend({ type: "sync-request", collectionId });
    });
  }

  function syncUpdate(collectionId: string, update: Uint8Array): void {
    if (state !== "connected") return;
    wsSend({ type: "sync-update", collectionId, update: toBase64(update) });
  }

  return {
    get state() { return state; },
    get relayDid() { return relayDid; },
    get modules() { return [...modules]; },
    connect,
    close,
    send,
    syncRequest,
    syncUpdate,
    on,
    off,
  };
}
