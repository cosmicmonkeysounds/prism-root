import { describe, it, expect } from "vitest";
import type { RelayEnvelope } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import {
  encodeBase64,
  decodeBase64,
  serializeEnvelope,
  deserializeEnvelope,
  parseClientMessage,
  stringifyServerMessage,
} from "./relay-protocol.js";
import type { ServerMessage } from "./relay-protocol.js";

const ALICE = "did:key:zAlice" as DID;
const BOB = "did:key:zBob" as DID;
const RELAY = "did:key:zRelay" as DID;

describe("relay-protocol", () => {
  // ── Base64 round-trip ──────────────────────────────────────────────────

  it("round-trips base64 encoding", () => {
    const original = new Uint8Array([0, 1, 2, 255, 128, 64]);
    const encoded = encodeBase64(original);
    expect(typeof encoded).toBe("string");
    const decoded = decodeBase64(encoded);
    expect(decoded).toEqual(original);
  });

  it("handles empty Uint8Array", () => {
    const empty = new Uint8Array(0);
    expect(decodeBase64(encodeBase64(empty))).toEqual(empty);
  });

  // ── Envelope serialization ─────────────────────────────────────────────

  it("serializes and deserializes an envelope", () => {
    const env: RelayEnvelope = {
      id: "env-1",
      from: ALICE,
      to: BOB,
      ciphertext: new Uint8Array([10, 20, 30]),
      submittedAt: "2026-01-01T00:00:00Z",
      ttlMs: 60_000,
    };

    const serialized = serializeEnvelope(env);
    expect(typeof serialized.ciphertext).toBe("string");
    expect(serialized.id).toBe("env-1");

    const deserialized = deserializeEnvelope(serialized);
    expect(deserialized.ciphertext).toEqual(env.ciphertext);
    expect(deserialized.from).toBe(ALICE);
    expect(deserialized.to).toBe(BOB);
  });

  it("preserves optional proofOfWork", () => {
    const env: RelayEnvelope = {
      id: "env-2",
      from: ALICE,
      to: BOB,
      ciphertext: new Uint8Array([1]),
      submittedAt: "2026-01-01T00:00:00Z",
      proofOfWork: "pow-token-abc",
      ttlMs: 5000,
    };

    const rt = deserializeEnvelope(serializeEnvelope(env));
    expect(rt.proofOfWork).toBe("pow-token-abc");
  });

  // ── Client message parsing ─────────────────────────────────────────────

  it("parses auth message", () => {
    const msg = parseClientMessage(JSON.stringify({ type: "auth", did: ALICE }));
    expect(msg.type).toBe("auth");
    if (msg.type === "auth") {
      expect(msg.did).toBe(ALICE);
    }
  });

  it("parses envelope message", () => {
    const msg = parseClientMessage(
      JSON.stringify({
        type: "envelope",
        envelope: serializeEnvelope({
          id: "e1",
          from: ALICE,
          to: BOB,
          ciphertext: new Uint8Array([1, 2]),
          submittedAt: "2026-01-01T00:00:00Z",
          ttlMs: 1000,
        }),
      }),
    );
    expect(msg.type).toBe("envelope");
  });

  it("parses collect message", () => {
    const msg = parseClientMessage(JSON.stringify({ type: "collect" }));
    expect(msg.type).toBe("collect");
  });

  it("parses ping message", () => {
    const msg = parseClientMessage(JSON.stringify({ type: "ping" }));
    expect(msg.type).toBe("ping");
  });

  it("parses sync-request message", () => {
    const msg = parseClientMessage(JSON.stringify({ type: "sync-request", collectionId: "col-1" }));
    expect(msg.type).toBe("sync-request");
    if (msg.type === "sync-request") {
      expect(msg.collectionId).toBe("col-1");
    }
  });

  it("parses sync-update message", () => {
    const msg = parseClientMessage(JSON.stringify({ type: "sync-update", collectionId: "col-1", update: "AQID" }));
    expect(msg.type).toBe("sync-update");
  });

  it("parses hashcash-proof message", () => {
    const msg = parseClientMessage(JSON.stringify({
      type: "hashcash-proof",
      proof: {
        challenge: { resource: "test", bits: 8, issuedAt: "2026-01-01T00:00:00Z", salt: "abc" },
        counter: 42,
        hash: "00ff",
      },
    }));
    expect(msg.type).toBe("hashcash-proof");
  });

  it("rejects unknown message type", () => {
    expect(() => parseClientMessage(JSON.stringify({ type: "unknown" }))).toThrow(
      "Unknown client message type",
    );
  });

  // ── Server message stringification ─────────────────────────────────────

  it("stringifies auth-ok message", () => {
    const msg: ServerMessage = {
      type: "auth-ok",
      relayDid: RELAY,
      modules: ["blind-mailbox", "relay-router"],
    };
    const json = JSON.parse(stringifyServerMessage(msg)) as Record<string, unknown>;
    expect(json["type"]).toBe("auth-ok");
    expect(json["relayDid"]).toBe(RELAY);
  });

  it("stringifies error message", () => {
    const msg: ServerMessage = { type: "error", message: "bad request" };
    const json = JSON.parse(stringifyServerMessage(msg)) as Record<string, unknown>;
    expect(json["type"]).toBe("error");
    expect(json["message"]).toBe("bad request");
  });

  it("stringifies pong message", () => {
    const msg: ServerMessage = { type: "pong" };
    expect(JSON.parse(stringifyServerMessage(msg))).toEqual({ type: "pong" });
  });

  it("stringifies sync-snapshot message", () => {
    const msg: ServerMessage = { type: "sync-snapshot", collectionId: "col-1", snapshot: "AQID" };
    const json = JSON.parse(stringifyServerMessage(msg)) as Record<string, unknown>;
    expect(json["type"]).toBe("sync-snapshot");
    expect(json["collectionId"]).toBe("col-1");
    expect(json["snapshot"]).toBe("AQID");
  });

  it("stringifies hashcash-ok message", () => {
    const msg: ServerMessage = { type: "hashcash-ok" };
    expect(JSON.parse(stringifyServerMessage(msg))).toEqual({ type: "hashcash-ok" });
  });
});
