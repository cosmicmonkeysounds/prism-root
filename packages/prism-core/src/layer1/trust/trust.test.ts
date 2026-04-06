import { describe, it, expect } from "vitest";
import {
  createLuaSandbox,
  createSchemaValidator,
  createHashcashMinter,
  createHashcashVerifier,
  createPeerTrustGraph,
  createShamirSplitter,
  createEscrowManager,
} from "./trust.js";
import type {
  SandboxPolicy,
  TrustGraphEvent,
} from "./trust-types.js";

// ── Lua Sandbox ────────────────────────────────────────────────────────────

describe("LuaSandbox", () => {
  function testPolicy(overrides: Partial<SandboxPolicy> = {}): SandboxPolicy {
    return {
      pluginId: "test-plugin",
      capabilities: ["crdt:read", "net:fetch"],
      maxDurationMs: 5000,
      maxMemoryBytes: 0,
      allowedUrls: ["https://api.example.com/*"],
      allowedPaths: [],
      ...overrides,
    };
  }

  it("grants listed capabilities", () => {
    const sandbox = createLuaSandbox(testPolicy());
    expect(sandbox.hasCapability("crdt:read")).toBe(true);
    expect(sandbox.hasCapability("net:fetch")).toBe(true);
  });

  it("denies unlisted capabilities", () => {
    const sandbox = createLuaSandbox(testPolicy());
    expect(sandbox.hasCapability("crdt:write")).toBe(false);
    expect(sandbox.hasCapability("fs:read")).toBe(false);
    expect(sandbox.hasCapability("process:spawn")).toBe(false);
  });

  it("allows URLs matching glob patterns", () => {
    const sandbox = createLuaSandbox(testPolicy());
    expect(sandbox.isUrlAllowed("https://api.example.com/v1/data")).toBe(true);
    expect(sandbox.isUrlAllowed("https://evil.com/attack")).toBe(false);
  });

  it("denies all URLs when net capability missing", () => {
    const sandbox = createLuaSandbox(testPolicy({ capabilities: ["crdt:read"] }));
    expect(sandbox.isUrlAllowed("https://api.example.com/v1/data")).toBe(false);
  });

  it("denies all URLs when allowedUrls is empty", () => {
    const sandbox = createLuaSandbox(testPolicy({ allowedUrls: [] }));
    expect(sandbox.isUrlAllowed("https://api.example.com/v1/data")).toBe(false);
  });

  it("allows paths matching glob patterns", () => {
    const sandbox = createLuaSandbox(testPolicy({
      capabilities: ["fs:read"],
      allowedPaths: ["/home/user/docs/*"],
    }));
    expect(sandbox.isPathAllowed("/home/user/docs/file.txt")).toBe(true);
    expect(sandbox.isPathAllowed("/etc/passwd")).toBe(false);
  });

  it("denies all paths when fs capability missing", () => {
    const sandbox = createLuaSandbox(testPolicy({
      capabilities: ["crdt:read"],
      allowedPaths: ["/home/user/*"],
    }));
    expect(sandbox.isPathAllowed("/home/user/file.txt")).toBe(false);
  });

  it("records violations", () => {
    const sandbox = createLuaSandbox(testPolicy());
    expect(sandbox.violations).toHaveLength(0);

    sandbox.recordViolation({
      capability: "crdt:write",
      message: "Write not allowed",
      timestamp: new Date().toISOString(),
      pluginId: "test-plugin",
    });

    expect(sandbox.violations).toHaveLength(1);
    expect(sandbox.violations[0].capability).toBe("crdt:write");
  });

  it("exposes the policy", () => {
    const policy = testPolicy();
    const sandbox = createLuaSandbox(policy);
    expect(sandbox.policy.pluginId).toBe("test-plugin");
    expect(sandbox.policy.maxDurationMs).toBe(5000);
  });
});

// ── Schema Poison Pill ─────────────────────────────────────────────────────

describe("SchemaValidator", () => {
  it("validates safe data", () => {
    const validator = createSchemaValidator();
    const result = validator.validate({ name: "test", value: 42 });
    expect(result.valid).toBe(true);
    expect(result.issues).toHaveLength(0);
  });

  it("rejects deeply nested data", () => {
    const validator = createSchemaValidator({ maxDepth: 3 });
    // Build 5-level deep object
    const data = { a: { b: { c: { d: { e: 1 } } } } };
    const result = validator.validate(data);
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.rule === "max-depth")).toBe(true);
  });

  it("rejects oversized strings", () => {
    const validator = createSchemaValidator({ maxStringLength: 10 });
    const result = validator.validate({ text: "a".repeat(20) });
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.rule === "max-string-length")).toBe(true);
  });

  it("rejects oversized arrays", () => {
    const validator = createSchemaValidator({ maxArrayLength: 5 });
    const result = validator.validate({ items: new Array(10).fill(0) });
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.rule === "max-array-length")).toBe(true);
  });

  it("rejects too many total keys", () => {
    const validator = createSchemaValidator({ maxTotalKeys: 5 });
    const result = validator.validate({ a: 1, b: 2, c: 3, d: 4, e: 5, f: 6 });
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.rule === "max-total-keys")).toBe(true);
  });

  it("rejects __proto__ keys (prototype pollution)", () => {
    const validator = createSchemaValidator();
    const data = JSON.parse('{"__proto__": {"isAdmin": true}}');
    const result = validator.validate(data);
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.rule === "disallowed-keys")).toBe(true);
    expect(result.issues.some(i => i.message.includes("__proto__"))).toBe(true);
  });

  it("rejects constructor keys", () => {
    const validator = createSchemaValidator();
    const data = JSON.parse('{"constructor": {"prototype": {"evil": true}}}');
    const result = validator.validate(data);
    expect(result.valid).toBe(false);
    expect(result.issues.some(i => i.message.includes("constructor"))).toBe(true);
  });

  it("adds custom validation rules", () => {
    const validator = createSchemaValidator();
    validator.addRule({
      name: "no-secret",
      description: "Rejects objects with 'secret' key",
      check(data, path) {
        if (data !== null && typeof data === "object" && !Array.isArray(data)) {
          if ("secret" in (data as Record<string, unknown>)) {
            return [{ path: `${path}.secret`, message: "Secret not allowed", severity: "error", rule: "no-secret" }];
          }
        }
        return [];
      },
    });

    expect(validator.ruleNames()).toContain("no-secret");
    const result = validator.validate({ secret: "password123" });
    expect(result.valid).toBe(false);
  });

  it("lists built-in rule names", () => {
    const validator = createSchemaValidator();
    const names = validator.ruleNames();
    expect(names).toContain("max-depth");
    expect(names).toContain("max-string-length");
    expect(names).toContain("max-array-length");
    expect(names).toContain("max-total-keys");
    expect(names).toContain("disallowed-keys");
  });

  it("validates nested arrays", () => {
    const validator = createSchemaValidator({ maxArrayLength: 3 });
    const result = validator.validate({ lists: [[1, 2, 3, 4, 5]] });
    expect(result.valid).toBe(false);
  });

  it("handles null and primitives gracefully", () => {
    const validator = createSchemaValidator();
    expect(validator.validate(null).valid).toBe(true);
    expect(validator.validate(42).valid).toBe(true);
    expect(validator.validate("hello").valid).toBe(true);
    expect(validator.validate(true).valid).toBe(true);
  });
});

// ── Hashcash ───────────────────────────────────────────────────────────────

describe("Hashcash", () => {
  it("creates a challenge", () => {
    const verifier = createHashcashVerifier();
    const challenge = verifier.createChallenge("did:key:z123", 4);
    expect(challenge.resource).toBe("did:key:z123");
    expect(challenge.bits).toBe(4);
    expect(challenge.salt).toBeTruthy();
    expect(challenge.issuedAt).toBeTruthy();
  });

  it("mints a valid proof for low difficulty", async () => {
    const verifier = createHashcashVerifier();
    const minter = createHashcashMinter();

    const challenge = verifier.createChallenge("relay-1", 4);
    const proof = await minter.mint(challenge);

    expect(proof.counter).toBeGreaterThanOrEqual(0);
    expect(proof.hash).toBeTruthy();
    expect(proof.hash.length).toBe(64); // SHA-256 hex

    const valid = await verifier.verify(proof);
    expect(valid).toBe(true);
  });

  it("rejects tampered proof", async () => {
    const verifier = createHashcashVerifier();
    const minter = createHashcashMinter();

    const challenge = verifier.createChallenge("relay-1", 4);
    const proof = await minter.mint(challenge);

    // Tamper with the hash
    const tampered = { ...proof, hash: "0".repeat(64) };
    const valid = await verifier.verify(tampered);
    expect(valid).toBe(false);
  });

  it("rejects proof with wrong counter", async () => {
    const verifier = createHashcashVerifier();
    const minter = createHashcashMinter();

    const challenge = verifier.createChallenge("relay-1", 4);
    const proof = await minter.mint(challenge);

    const wrong = { ...proof, counter: proof.counter + 999999 };
    const valid = await verifier.verify(wrong);
    expect(valid).toBe(false);
  });

  it("uses default bits", () => {
    const verifier = createHashcashVerifier(12);
    const challenge = verifier.createChallenge("test");
    expect(challenge.bits).toBe(12);
  });
});

// ── Web of Trust ───────────────────────────────────────────────────────────

describe("PeerTrustGraph", () => {
  it("starts empty", () => {
    const graph = createPeerTrustGraph();
    expect(graph.allPeers()).toHaveLength(0);
    expect(graph.getPeer("alice")).toBeUndefined();
  });

  it("creates peer on first interaction", () => {
    const graph = createPeerTrustGraph();
    graph.recordPositive("alice");
    expect(graph.getPeer("alice")).toBeDefined();
    expect(graph.getPeer("alice")?.trustLevel).not.toBe("unknown");
  });

  it("increases score on positive interaction", () => {
    const graph = createPeerTrustGraph({ positiveWeight: 10 });
    graph.recordPositive("alice");
    graph.recordPositive("alice");
    const peer = graph.getPeer("alice");
    expect(peer?.score).toBe(20);
    expect(peer?.positiveInteractions).toBe(2);
  });

  it("decreases score on negative interaction", () => {
    const graph = createPeerTrustGraph({ negativeWeight: -15 });
    graph.recordPositive("alice"); // start at 5 (default positive)
    graph.recordNegative("alice");
    const peer = graph.getPeer("alice");
    expect(peer?.score).toBe(-10); // 5 + (-15)
    expect(peer?.negativeInteractions).toBe(1);
  });

  it("clamps score to [-100, 100]", () => {
    const graph = createPeerTrustGraph({ positiveWeight: 200 });
    graph.recordPositive("alice");
    expect(graph.getPeer("alice")?.score).toBe(100);
  });

  it("computes trust levels correctly", () => {
    const graph = createPeerTrustGraph({
      trustedThreshold: 20,
      highlyTrustedThreshold: 50,
      positiveWeight: 15,
      negativeWeight: -25,
    });

    graph.recordPositive("alice"); // 15 → neutral
    expect(graph.getPeer("alice")?.trustLevel).toBe("neutral");

    graph.recordPositive("alice"); // 30 → trusted
    expect(graph.getPeer("alice")?.trustLevel).toBe("trusted");

    graph.recordPositive("alice"); // 45 → trusted
    graph.recordPositive("alice"); // 60 → highly-trusted
    expect(graph.getPeer("alice")?.trustLevel).toBe("highly-trusted");

    graph.recordNegative("alice"); // 35 → trusted
    expect(graph.getPeer("alice")?.trustLevel).toBe("trusted");
  });

  it("bans and unbans peers", () => {
    const graph = createPeerTrustGraph();
    graph.recordPositive("alice");
    graph.ban("alice", "spamming");

    expect(graph.isBanned("alice")).toBe(true);
    expect(graph.getPeer("alice")?.trustLevel).toBe("untrusted");
    expect(graph.getPeer("alice")?.banReason).toBe("spamming");

    graph.unban("alice");
    expect(graph.isBanned("alice")).toBe(false);
    expect(graph.getPeer("alice")?.banReason).toBeNull();
  });

  it("isBanned returns false for unknown peer", () => {
    const graph = createPeerTrustGraph();
    expect(graph.isBanned("nobody")).toBe(false);
  });

  it("getPeersAtLevel filters correctly", () => {
    const graph = createPeerTrustGraph({ positiveWeight: 50 });
    graph.recordPositive("alice"); // 50 → trusted
    graph.recordPositive("bob");   // 50 → trusted
    graph.recordPositive("bob");   // 100 → highly-trusted
    graph.recordNegative("charlie"); // -10 → untrusted

    const trusted = graph.getPeersAtLevel("trusted");
    expect(trusted.length).toBe(2); // alice + bob (both ≥ trusted)

    const highlyTrusted = graph.getPeersAtLevel("highly-trusted");
    expect(highlyTrusted.length).toBe(1); // only bob
  });

  it("flags and checks content hashes", () => {
    const graph = createPeerTrustGraph();
    graph.flagContent("abc123", "spam", "alice");

    expect(graph.isContentFlagged("abc123")).toBe(true);
    expect(graph.isContentFlagged("def456")).toBe(false);
    expect(graph.flaggedContent()).toHaveLength(1);
    expect(graph.flaggedContent()[0].category).toBe("spam");
  });

  it("emits events", () => {
    const graph = createPeerTrustGraph();
    const events: TrustGraphEvent[] = [];
    graph.onChange(e => events.push(e));

    graph.recordPositive("alice");
    graph.ban("alice", "test");
    graph.flagContent("hash1", "malware", "bob");

    const types = events.map(e => e.type);
    expect(types).toContain("peer-added");
    expect(types).toContain("peer-updated");
    expect(types).toContain("peer-banned");
    expect(types).toContain("content-flagged");
  });

  it("unsubscribe stops events", () => {
    const graph = createPeerTrustGraph();
    const events: TrustGraphEvent[] = [];
    const unsub = graph.onChange(e => events.push(e));
    unsub();

    graph.recordPositive("alice");
    expect(events).toHaveLength(0);
  });

  it("dispose clears everything", () => {
    const graph = createPeerTrustGraph();
    graph.recordPositive("alice");
    graph.flagContent("hash1", "spam", "bob");

    graph.dispose();
    expect(graph.allPeers()).toHaveLength(0);
    expect(graph.flaggedContent()).toHaveLength(0);
  });
});

// ── Shamir Secret Sharing ──────────────────────────────────────────────────

describe("ShamirSplitter", () => {
  const splitter = createShamirSplitter();

  it("splits and reconstructs a secret (2-of-3)", () => {
    const secret = new Uint8Array([72, 101, 108, 108, 111]); // "Hello"
    const config = { totalShares: 3, threshold: 2 };

    const shares = splitter.split(secret, config);
    expect(shares).toHaveLength(3);
    expect(shares[0].index).toBe(1);
    expect(shares[1].index).toBe(2);
    expect(shares[2].index).toBe(3);

    // Reconstruct with shares 1 and 2
    const recovered = splitter.combine([shares[0], shares[1]], config);
    expect(recovered).toEqual(secret);
  });

  it("reconstructs with any threshold subset", () => {
    const secret = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]);
    const config = { totalShares: 5, threshold: 3 };

    const shares = splitter.split(secret, config);

    // Try different subsets of 3
    const r1 = splitter.combine([shares[0], shares[2], shares[4]], config);
    expect(r1).toEqual(secret);

    const r2 = splitter.combine([shares[1], shares[3], shares[4]], config);
    expect(r2).toEqual(secret);

    const r3 = splitter.combine([shares[0], shares[1], shares[2]], config);
    expect(r3).toEqual(secret);
  });

  it("fails with too few shares", () => {
    const secret = new Uint8Array([42]);
    const config = { totalShares: 3, threshold: 2 };
    const shares = splitter.split(secret, config);

    expect(() => splitter.combine([shares[0]], config)).toThrow("Need at least 2 shares");
  });

  it("handles single-byte secret", () => {
    const secret = new Uint8Array([255]);
    const config = { totalShares: 3, threshold: 2 };
    const shares = splitter.split(secret, config);
    const recovered = splitter.combine([shares[0], shares[2]], config);
    expect(recovered).toEqual(secret);
  });

  it("handles empty secret", () => {
    const secret = new Uint8Array([]);
    const config = { totalShares: 3, threshold: 2 };
    const shares = splitter.split(secret, config);
    const recovered = splitter.combine(shares.slice(0, 2), config);
    expect(recovered).toEqual(secret);
  });

  it("rejects invalid configs", () => {
    const secret = new Uint8Array([1]);
    expect(() => splitter.split(secret, { totalShares: 3, threshold: 1 })).toThrow("at least 2");
    expect(() => splitter.split(secret, { totalShares: 1, threshold: 2 })).toThrow(">= threshold");
  });

  it("shares are different from each other", () => {
    const secret = new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]);
    const config = { totalShares: 3, threshold: 2 };
    const shares = splitter.split(secret, config);

    // Each share should be unique
    const dataSet = new Set(shares.map(s => s.data));
    expect(dataSet.size).toBe(3);
  });
});

// ── Encrypted Escrow ───────────────────────────────────────────────────────

describe("EscrowManager", () => {
  it("deposits and retrieves", () => {
    const escrow = createEscrowManager();
    const dep = escrow.deposit("alice", "encrypted-key-data");
    expect(dep.id).toBeTruthy();
    expect(dep.depositorId).toBe("alice");
    expect(dep.encryptedPayload).toBe("encrypted-key-data");
    expect(dep.claimed).toBe(false);
    expect(dep.expiresAt).toBeNull();

    expect(escrow.get(dep.id)).toBeDefined();
  });

  it("claims a deposit", () => {
    const escrow = createEscrowManager();
    const dep = escrow.deposit("alice", "key-data");
    const claimed = escrow.claim(dep.id);
    expect(claimed).not.toBeNull();
    expect(claimed?.claimed).toBe(true);
  });

  it("cannot claim twice", () => {
    const escrow = createEscrowManager();
    const dep = escrow.deposit("alice", "key-data");
    escrow.claim(dep.id);
    const second = escrow.claim(dep.id);
    expect(second).toBeNull();
  });

  it("cannot claim nonexistent deposit", () => {
    const escrow = createEscrowManager();
    expect(escrow.claim("fake-id")).toBeNull();
  });

  it("cannot claim expired deposit", () => {
    const escrow = createEscrowManager();
    const pastDate = new Date(Date.now() - 86400000).toISOString();
    const dep = escrow.deposit("alice", "key-data", pastDate);
    const claimed = escrow.claim(dep.id);
    expect(claimed).toBeNull();
  });

  it("lists deposits for a depositor", () => {
    const escrow = createEscrowManager();
    escrow.deposit("alice", "key-1");
    escrow.deposit("alice", "key-2");
    escrow.deposit("bob", "key-3");

    expect(escrow.listDeposits("alice")).toHaveLength(2);
    expect(escrow.listDeposits("bob")).toHaveLength(1);
    expect(escrow.listDeposits("charlie")).toHaveLength(0);
  });

  it("evicts expired deposits", () => {
    const escrow = createEscrowManager();
    const pastDate = new Date(Date.now() - 86400000).toISOString();
    const futureDate = new Date(Date.now() + 86400000).toISOString();

    escrow.deposit("alice", "expired", pastDate);
    escrow.deposit("bob", "valid", futureDate);
    escrow.deposit("charlie", "no-expiry");

    const evicted = escrow.evictExpired();
    expect(evicted).toBe(1);
    expect(escrow.listDeposits("alice")).toHaveLength(0);
    expect(escrow.listDeposits("bob")).toHaveLength(1);
    expect(escrow.listDeposits("charlie")).toHaveLength(1);
  });

  it("deposit with expiry", () => {
    const escrow = createEscrowManager();
    const futureDate = new Date(Date.now() + 86400000).toISOString();
    const dep = escrow.deposit("alice", "key-data", futureDate);
    expect(dep.expiresAt).toBe(futureDate);
    // Can still claim since not expired
    const claimed = escrow.claim(dep.id);
    expect(claimed).not.toBeNull();
  });
});
