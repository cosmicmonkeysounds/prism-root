import { describe, it, expect } from "vitest";
import {
  createIdentity,
  resolveIdentity,
  signPayload,
  verifySignature,
  exportIdentity,
  importIdentity,
  createMultiSigConfig,
  createPartialSignature,
  assembleMultiSignature,
  verifyMultiSignature,
  encodeBase58,
  decodeBase58,
  publicKeyToDidKey,
  didKeyToPublicKey,
} from "./identity.js";
import type { DID } from "./identity-types.js";

// ── Base58 ──────────────────────────────────────────────────────────────────

describe("base58btc", () => {
  it("round-trips arbitrary bytes", () => {
    const input = new Uint8Array([0, 0, 1, 2, 3, 255, 128, 64]);
    const encoded = encodeBase58(input);
    const decoded = decodeBase58(encoded);
    expect(decoded).toEqual(input);
  });

  it("encodes empty array", () => {
    expect(encodeBase58(new Uint8Array([]))).toBe("");
    expect(decodeBase58("")).toEqual(new Uint8Array([]));
  });

  it("encodes leading zeros as '1'", () => {
    const input = new Uint8Array([0, 0, 0, 1]);
    const encoded = encodeBase58(input);
    expect(encoded.startsWith("111")).toBe(true);
  });

  it("throws on invalid base58 character", () => {
    expect(() => decodeBase58("0OIl")).toThrow("Invalid base58 character");
  });
});

// ── DID:key encoding ────────────────────────────────────────────────────────

describe("did:key encoding", () => {
  it("round-trips a public key through did:key", () => {
    const pubKey = new Uint8Array(32);
    globalThis.crypto.getRandomValues(pubKey);

    const did = publicKeyToDidKey(pubKey);
    expect(did).toMatch(/^did:key:z/);

    const recovered = didKeyToPublicKey(did);
    expect(recovered).toEqual(pubKey);
  });

  it("throws on invalid did:key format", () => {
    expect(() => didKeyToPublicKey("not-a-did" as DID)).toThrow("Invalid did:key");
  });

  it("throws on non-z multibase prefix", () => {
    expect(() => didKeyToPublicKey("did:key:m123" as DID)).toThrow("Unsupported multibase");
  });
});

// ── Identity creation ───────────────────────────────────────────────────────

describe("createIdentity", () => {
  it("creates a did:key identity by default", async () => {
    const identity = await createIdentity();

    expect(identity.did).toMatch(/^did:key:z/);
    expect(identity.document.id).toBe(identity.did);
    expect(identity.document["@context"]).toContain("https://www.w3.org/ns/did/v1");
    expect(identity.document.verificationMethod).toHaveLength(1);
    expect(identity.document.authentication).toHaveLength(1);
    expect(identity.document.assertionMethod).toHaveLength(1);
    expect(identity.keyHandle.publicKeyBytes).toHaveLength(32);
  });

  it("creates a did:web identity", async () => {
    const identity = await createIdentity({ method: "web", domain: "example.com" });

    expect(identity.did).toBe("did:web:example.com");
    expect(identity.document.id).toBe("did:web:example.com");
  });

  it("creates a did:web identity with path", async () => {
    const identity = await createIdentity({
      method: "web",
      domain: "example.com",
      path: "users/alice",
    });

    expect(identity.did).toBe("did:web:example.com:users:alice");
  });

  it("throws when did:web is missing domain", async () => {
    await expect(createIdentity({ method: "web" })).rejects.toThrow("requires a domain");
  });

  it("generates unique identities each time", async () => {
    const a = await createIdentity();
    const b = await createIdentity();
    expect(a.did).not.toBe(b.did);
  });
});

// ── Sign / Verify ───────────────────────────────────────────────────────────

describe("signPayload / verifySignature", () => {
  it("signs and verifies a payload", async () => {
    const identity = await createIdentity();
    const data = new TextEncoder().encode("hello prism");

    const signature = await signPayload(identity, data);
    expect(signature).toBeInstanceOf(Uint8Array);
    expect(signature.length).toBe(64); // Ed25519 signatures are 64 bytes

    const valid = await identity.verifySignature(data, signature);
    expect(valid).toBe(true);
  });

  it("rejects tampered data", async () => {
    const identity = await createIdentity();
    const data = new TextEncoder().encode("original");
    const signature = await identity.signPayload(data);

    const tampered = new TextEncoder().encode("tampered");
    const valid = await identity.verifySignature(tampered, signature);
    expect(valid).toBe(false);
  });

  it("rejects tampered signature", async () => {
    const identity = await createIdentity();
    const data = new TextEncoder().encode("hello");
    const signature = await identity.signPayload(data);

    signature[0] = (signature[0] as number) ^ 0xff;
    const valid = await identity.verifySignature(data, signature);
    expect(valid).toBe(false);
  });

  it("verifies via standalone verifySignature with DID resolution", async () => {
    const identity = await createIdentity();
    const data = new TextEncoder().encode("verify-me");
    const sig = await identity.signPayload(data);

    const valid = await verifySignature(identity.did, data, sig);
    expect(valid).toBe(true);
  });
});

// ── resolveIdentity ─────────────────────────────────────────────────────────

describe("resolveIdentity", () => {
  it("resolves a did:key to its public key", async () => {
    const identity = await createIdentity();
    const resolved = await resolveIdentity(identity.did);

    expect(resolved.did).toBe(identity.did);
    expect(resolved.publicKey).toEqual(identity.keyHandle.publicKeyBytes);
    expect(resolved.document.verificationMethod).toHaveLength(1);
  });

  it("resolved identity can verify signatures from original", async () => {
    const identity = await createIdentity();
    const data = new TextEncoder().encode("cross-verify");
    const sig = await identity.signPayload(data);

    const resolved = await resolveIdentity(identity.did);
    const valid = await resolved.verifySignature(data, sig);
    expect(valid).toBe(true);
  });

  it("throws for did:web (not yet implemented)", async () => {
    await expect(resolveIdentity("did:web:example.com" as DID)).rejects.toThrow(
      "network resolver",
    );
  });

  it("throws for unsupported DID method", async () => {
    await expect(resolveIdentity("did:btcr:abc123" as DID)).rejects.toThrow(
      "Unsupported DID method",
    );
  });
});

// ── Identity Persistence ─────────────────────────────────────────────────────

describe("identity persistence", () => {
  it("round-trips an identity through export/import", async () => {
    const original = await createIdentity({ method: "key" });
    const exported = await exportIdentity(original);

    expect(exported.did).toBe(original.did);
    expect(exported.privateKeyJwk.kty).toBe("OKP");
    expect(exported.publicKeyJwk.kty).toBe("OKP");
    expect(typeof exported.createdAt).toBe("string");

    const restored = await importIdentity(exported);
    expect(restored.did).toBe(original.did);
    expect(restored.keyHandle.publicKeyBytes).toEqual(original.keyHandle.publicKeyBytes);
  });

  it("restored identity can sign and original can verify", async () => {
    const original = await createIdentity({ method: "key" });
    const exported = await exportIdentity(original);
    const restored = await importIdentity(exported);

    const data = new TextEncoder().encode("persistence-test");
    const sig = await restored.signPayload(data);
    const valid = await original.verifySignature(data, sig);
    expect(valid).toBe(true);
  });

  it("original identity can sign and restored can verify", async () => {
    const original = await createIdentity({ method: "key" });
    const exported = await exportIdentity(original);
    const restored = await importIdentity(exported);

    const data = new TextEncoder().encode("reverse-test");
    const sig = await original.signPayload(data);
    const valid = await restored.verifySignature(data, sig);
    expect(valid).toBe(true);
  });

  it("exported identity is JSON-serializable", async () => {
    const identity = await createIdentity({ method: "key" });
    const exported = await exportIdentity(identity);

    const json = JSON.stringify(exported);
    const parsed = JSON.parse(json);
    const restored = await importIdentity(parsed);

    expect(restored.did).toBe(identity.did);

    // Verify signing still works after JSON round-trip
    const data = new TextEncoder().encode("json-round-trip");
    const sig = await restored.signPayload(data);
    const valid = await identity.verifySignature(data, sig);
    expect(valid).toBe(true);
  });
});

// ── Multi-sig ───────────────────────────────────────────────────────────────

describe("multi-sig", () => {
  it("creates a valid multi-sig config", () => {
    const config = createMultiSigConfig(2, [
      "did:key:z1" as DID,
      "did:key:z2" as DID,
      "did:key:z3" as DID,
    ]);
    expect(config.threshold).toBe(2);
    expect(config.signers).toHaveLength(3);
  });

  it("rejects threshold > signers", () => {
    expect(() =>
      createMultiSigConfig(3, ["did:key:z1" as DID, "did:key:z2" as DID]),
    ).toThrow("exceeds number of signers");
  });

  it("rejects threshold < 1", () => {
    expect(() => createMultiSigConfig(0, ["did:key:z1" as DID])).toThrow("at least 1");
  });

  it("rejects duplicate signers", () => {
    expect(() =>
      createMultiSigConfig(1, ["did:key:z1" as DID, "did:key:z1" as DID]),
    ).toThrow("Duplicate signers");
  });

  it("full multi-sig sign + verify flow (2-of-3)", async () => {
    const alice = await createIdentity();
    const bob = await createIdentity();
    const charlie = await createIdentity();

    const config = createMultiSigConfig(2, [alice.did, bob.did, charlie.did]);
    const data = new TextEncoder().encode("vault-action");

    // Alice and Bob sign
    const aliceSig = await createPartialSignature(alice, data);
    const bobSig = await createPartialSignature(bob, data);

    const multiSig = assembleMultiSignature(config, [aliceSig, bobSig]);
    expect(multiSig.threshold).toBe(2);
    expect(multiSig.signatures).toHaveLength(2);

    const valid = await verifyMultiSignature(config, multiSig, data);
    expect(valid).toBe(true);
  });

  it("rejects multi-sig below threshold", async () => {
    const alice = await createIdentity();
    const bob = await createIdentity();

    const config = createMultiSigConfig(2, [alice.did, bob.did]);
    const data = new TextEncoder().encode("need-two");

    const aliceSig = await createPartialSignature(alice, data);
    const multiSig = assembleMultiSignature(config, [aliceSig]);

    const valid = await verifyMultiSignature(config, multiSig, data);
    expect(valid).toBe(false);
  });

  it("rejects partial signature from non-signer", async () => {
    const alice = await createIdentity();
    const outsider = await createIdentity();

    const config = createMultiSigConfig(1, [alice.did]);
    const data = new TextEncoder().encode("restricted");

    const outsiderSig = await createPartialSignature(outsider, data);

    expect(() => assembleMultiSignature(config, [outsiderSig])).toThrow(
      "not in the multi-sig config",
    );
  });

  it("rejects duplicate partial signatures", async () => {
    const alice = await createIdentity();
    const config = createMultiSigConfig(1, [alice.did]);
    const data = new TextEncoder().encode("no-dupes");

    const sig1 = await createPartialSignature(alice, data);
    const sig2 = await createPartialSignature(alice, data);

    expect(() => assembleMultiSignature(config, [sig1, sig2])).toThrow(
      "Duplicate partial signatures",
    );
  });

  it("rejects multi-sig with tampered data", async () => {
    const alice = await createIdentity();
    const bob = await createIdentity();

    const config = createMultiSigConfig(2, [alice.did, bob.did]);
    const data = new TextEncoder().encode("original-data");
    const tampered = new TextEncoder().encode("tampered-data");

    const aliceSig = await createPartialSignature(alice, data);
    const bobSig = await createPartialSignature(bob, data);

    const multiSig = assembleMultiSignature(config, [aliceSig, bobSig]);

    const valid = await verifyMultiSignature(config, multiSig, tampered);
    expect(valid).toBe(false);
  });
});
