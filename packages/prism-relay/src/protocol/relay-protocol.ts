/**
 * WebSocket wire protocol for Prism Relay.
 *
 * All messages are JSON with a `type` discriminator.
 * RelayEnvelope.ciphertext is base64-encoded in transit.
 */

import type { DID } from "@prism/core/identity";
import type { RelayEnvelope, RouteResult } from "@prism/core/relay";

// ── Serialized envelope (base64 ciphertext) ────────────────────────────────

export interface SerializedEnvelope {
  id: string;
  from: DID;
  to: DID;
  ciphertext: string; // base64
  submittedAt: string;
  proofOfWork?: string;
  ttlMs: number;
}

// ── Client → Relay messages ────────────────────────────────────────────────

export interface AuthMessage {
  type: "auth";
  did: DID;
}

export interface EnvelopeMessage {
  type: "envelope";
  envelope: SerializedEnvelope;
}

export interface CollectMessage {
  type: "collect";
}

export interface PingMessage {
  type: "ping";
}

export interface SyncRequestMessage {
  type: "sync-request";
  collectionId: string;
}

export interface SyncUpdateMessage {
  type: "sync-update";
  collectionId: string;
  update: string; // base64
}

export interface HashcashProofMessage {
  type: "hashcash-proof";
  proof: {
    challenge: {
      resource: string;
      bits: number;
      issuedAt: string;
      salt: string;
    };
    counter: number;
    hash: string;
  };
}

export interface PresenceUpdateMessage {
  type: "presence-update";
  peerId: string;
  cursor?: { x: number; y: number };
  selection?: { start: number; end: number };
  activeView?: string;
}

export type ClientMessage =
  | AuthMessage
  | EnvelopeMessage
  | CollectMessage
  | PingMessage
  | SyncRequestMessage
  | SyncUpdateMessage
  | HashcashProofMessage
  | PresenceUpdateMessage;

// ── Relay → Client messages ────────────────────────────────────────────────

export interface AuthOkMessage {
  type: "auth-ok";
  relayDid: DID;
  modules: string[];
}

export interface InboundEnvelopeMessage {
  type: "envelope";
  envelope: SerializedEnvelope;
}

export interface RouteResultMessage {
  type: "route-result";
  result: RouteResult;
}

export interface ErrorMessage {
  type: "error";
  message: string;
}

export interface PongMessage {
  type: "pong";
}

export interface SyncSnapshotMessage {
  type: "sync-snapshot";
  collectionId: string;
  snapshot: string; // base64
}

export interface SyncBroadcastMessage {
  type: "sync-update";
  collectionId: string;
  update: string; // base64
}

export interface HashcashChallengeMessage {
  type: "hashcash-challenge";
  challenge: {
    resource: string;
    bits: number;
    salt: string;
    issuedAt: string;
  };
}

export interface HashcashOkMessage {
  type: "hashcash-ok";
}

export interface PresenceStateMessage {
  type: "presence-state";
  peers: Array<{
    peerId: string;
    cursor?: { x: number; y: number };
    selection?: { start: number; end: number };
    activeView?: string;
  }>;
}

export interface PresenceBroadcastMessage {
  type: "presence-update";
  peerId: string;
  cursor?: { x: number; y: number };
  selection?: { start: number; end: number };
  activeView?: string;
}

export interface PresenceLeaveMessage {
  type: "presence-leave";
  peerId: string;
}

export type ServerMessage =
  | AuthOkMessage
  | InboundEnvelopeMessage
  | RouteResultMessage
  | ErrorMessage
  | PongMessage
  | SyncSnapshotMessage
  | SyncBroadcastMessage
  | HashcashChallengeMessage
  | HashcashOkMessage
  | PresenceStateMessage
  | PresenceBroadcastMessage
  | PresenceLeaveMessage;

// ── Serialization helpers ──────────────────────────────────────────────────

export function encodeBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

export function decodeBase64(str: string): Uint8Array {
  const binary = atob(str);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

export function serializeEnvelope(env: RelayEnvelope): SerializedEnvelope {
  const result: SerializedEnvelope = {
    id: env.id,
    from: env.from,
    to: env.to,
    ciphertext: encodeBase64(env.ciphertext),
    submittedAt: env.submittedAt,
    ttlMs: env.ttlMs,
  };
  if (env.proofOfWork !== undefined) result.proofOfWork = env.proofOfWork;
  return result;
}

export function deserializeEnvelope(s: SerializedEnvelope): RelayEnvelope {
  const result: RelayEnvelope = {
    id: s.id,
    from: s.from,
    to: s.to,
    ciphertext: decodeBase64(s.ciphertext),
    submittedAt: s.submittedAt,
    ttlMs: s.ttlMs,
  };
  if (s.proofOfWork !== undefined) result.proofOfWork = s.proofOfWork;
  return result;
}

const CLIENT_MESSAGE_TYPES = new Set([
  "auth", "envelope", "collect", "ping",
  "sync-request", "sync-update", "hashcash-proof",
  "presence-update",
]);

export function parseClientMessage(raw: string): ClientMessage {
  const msg = JSON.parse(raw) as Record<string, unknown>;
  const type = msg["type"];
  if (typeof type === "string" && CLIENT_MESSAGE_TYPES.has(type)) {
    return msg as unknown as ClientMessage;
  }
  throw new Error(`Unknown client message type: ${String(type)}`);
}

export function stringifyServerMessage(msg: ServerMessage): string {
  return JSON.stringify(msg);
}
