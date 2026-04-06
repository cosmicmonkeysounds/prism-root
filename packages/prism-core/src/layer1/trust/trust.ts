/**
 * @prism/core — Trust & Safety (Layer 1)
 *
 * The Sovereign Immune System.
 *
 * All implementations are pure in-memory — no crypto dependencies.
 * Web Crypto is used only for Hashcash (SHA-256 hashing).
 */

import type {
  SandboxCapability,
  SandboxPolicy,
  SandboxViolation,
  LuaSandbox,
  SchemaValidationIssue,
  SchemaValidationResult,
  SchemaValidationRule,
  SchemaValidator,
  SchemaValidatorOptions,
  HashcashChallenge,
  HashcashProof,
  HashcashMinter,
  HashcashVerifier,
  TrustLevel,
  PeerReputation,
  ContentHash,
  TrustGraphEvent,
  TrustGraphListener,
  PeerTrustGraph,
  TrustGraphOptions,
  ShamirShare,
  ShamirConfig,
  ShamirSplitter,
  EscrowDeposit,
  EscrowManager,
} from "./trust-types.js";

// ── Helpers ────────────────────────────────────────────────────────────────

let idCounter = 0;
function uid(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${(idCounter++).toString(36)}`;
}

function buf(bytes: Uint8Array): BufferSource {
  return bytes as unknown as BufferSource;
}

function globToRegex(pattern: string): RegExp {
  const escaped = pattern
    .replace(/[.+^${}()|[\]\\]/g, "\\$&")
    .replace(/\*/g, ".*")
    .replace(/\?/g, ".");
  return new RegExp(`^${escaped}$`);
}

// ── Lua Sandbox ────────────────────────────────────────────────────────────

export function createLuaSandbox(policy: SandboxPolicy): LuaSandbox {
  const capSet = new Set<SandboxCapability>(policy.capabilities);
  const urlRegexes = policy.allowedUrls.map(globToRegex);
  const pathRegexes = policy.allowedPaths.map(globToRegex);
  const violations: SandboxViolation[] = [];

  return {
    get policy() { return policy; },
    get violations() { return violations; },

    hasCapability(capability: SandboxCapability): boolean {
      return capSet.has(capability);
    },

    isUrlAllowed(url: string): boolean {
      if (!capSet.has("net:fetch") && !capSet.has("net:websocket")) return false;
      if (urlRegexes.length === 0) return false;
      return urlRegexes.some(re => re.test(url));
    },

    isPathAllowed(path: string): boolean {
      if (!capSet.has("fs:read") && !capSet.has("fs:write")) return false;
      if (pathRegexes.length === 0) return false;
      return pathRegexes.some(re => re.test(path));
    },

    recordViolation(violation: SandboxViolation): void {
      violations.push(violation);
    },
  };
}

// ── Schema Poison Pill ─────────────────────────────────────────────────────

function createDepthRule(maxDepth: number): SchemaValidationRule {
  function checkDepth(data: unknown, path: string, depth: number, issues: SchemaValidationIssue[]): void {
    if (depth > maxDepth) {
      issues.push({
        path,
        message: `Nesting depth ${depth} exceeds maximum ${maxDepth}`,
        severity: "error",
        rule: "max-depth",
      });
      return;
    }
    if (data !== null && typeof data === "object") {
      if (Array.isArray(data)) {
        for (let i = 0; i < data.length; i++) {
          checkDepth(data[i], `${path}[${i}]`, depth + 1, issues);
        }
      } else {
        for (const key of Object.keys(data)) {
          checkDepth((data as Record<string, unknown>)[key], `${path}.${key}`, depth + 1, issues);
        }
      }
    }
  }

  return {
    name: "max-depth",
    description: `Rejects data nested deeper than ${maxDepth} levels`,
    check(data: unknown, path: string): SchemaValidationIssue[] {
      const issues: SchemaValidationIssue[] = [];
      checkDepth(data, path, 0, issues);
      return issues;
    },
  };
}

function createStringSizeRule(maxLength: number): SchemaValidationRule {
  function checkStrings(data: unknown, path: string, issues: SchemaValidationIssue[]): void {
    if (typeof data === "string") {
      if (data.length > maxLength) {
        issues.push({
          path,
          message: `String length ${data.length} exceeds maximum ${maxLength}`,
          severity: "error",
          rule: "max-string-length",
        });
      }
      return;
    }
    if (data !== null && typeof data === "object") {
      if (Array.isArray(data)) {
        for (let i = 0; i < data.length; i++) {
          checkStrings(data[i], `${path}[${i}]`, issues);
        }
      } else {
        for (const key of Object.keys(data)) {
          checkStrings((data as Record<string, unknown>)[key], `${path}.${key}`, issues);
        }
      }
    }
  }

  return {
    name: "max-string-length",
    description: `Rejects strings longer than ${maxLength} characters`,
    check(data: unknown, path: string): SchemaValidationIssue[] {
      const issues: SchemaValidationIssue[] = [];
      checkStrings(data, path, issues);
      return issues;
    },
  };
}

function createArraySizeRule(maxLength: number): SchemaValidationRule {
  function checkArrays(data: unknown, path: string, issues: SchemaValidationIssue[]): void {
    if (Array.isArray(data)) {
      if (data.length > maxLength) {
        issues.push({
          path,
          message: `Array length ${data.length} exceeds maximum ${maxLength}`,
          severity: "error",
          rule: "max-array-length",
        });
      }
      for (let i = 0; i < data.length; i++) {
        checkArrays(data[i], `${path}[${i}]`, issues);
      }
    } else if (data !== null && typeof data === "object") {
      for (const key of Object.keys(data)) {
        checkArrays((data as Record<string, unknown>)[key], `${path}.${key}`, issues);
      }
    }
  }

  return {
    name: "max-array-length",
    description: `Rejects arrays longer than ${maxLength} elements`,
    check(data: unknown, path: string): SchemaValidationIssue[] {
      const issues: SchemaValidationIssue[] = [];
      checkArrays(data, path, issues);
      return issues;
    },
  };
}

function createKeyCountRule(maxKeys: number): SchemaValidationRule {
  function countKeys(data: unknown): number {
    if (data === null || typeof data !== "object") return 0;
    if (Array.isArray(data)) {
      let count = 0;
      for (const item of data) count += countKeys(item);
      return count;
    }
    const keys = Object.keys(data);
    let count = keys.length;
    for (const key of keys) {
      count += countKeys((data as Record<string, unknown>)[key]);
    }
    return count;
  }

  return {
    name: "max-total-keys",
    description: `Rejects data with more than ${maxKeys} total object keys`,
    check(data: unknown, path: string): SchemaValidationIssue[] {
      const total = countKeys(data);
      if (total > maxKeys) {
        return [{
          path,
          message: `Total key count ${total} exceeds maximum ${maxKeys}`,
          severity: "error",
          rule: "max-total-keys",
        }];
      }
      return [];
    },
  };
}

function createDisallowedKeysRule(patterns: RegExp[]): SchemaValidationRule {
  function checkKeys(data: unknown, path: string, issues: SchemaValidationIssue[]): void {
    if (data === null || typeof data !== "object" || Array.isArray(data)) {
      if (Array.isArray(data)) {
        for (let i = 0; i < data.length; i++) {
          checkKeys(data[i], `${path}[${i}]`, issues);
        }
      }
      return;
    }
    for (const key of Object.keys(data)) {
      for (const pattern of patterns) {
        if (pattern.test(key)) {
          issues.push({
            path: `${path}.${key}`,
            message: `Key "${key}" matches disallowed pattern ${pattern.source}`,
            severity: "error",
            rule: "disallowed-keys",
          });
        }
      }
      checkKeys((data as Record<string, unknown>)[key], `${path}.${key}`, issues);
    }
  }

  return {
    name: "disallowed-keys",
    description: "Rejects objects with keys matching dangerous patterns",
    check(data: unknown, path: string): SchemaValidationIssue[] {
      const issues: SchemaValidationIssue[] = [];
      checkKeys(data, path, issues);
      return issues;
    },
  };
}

/** Default disallowed key patterns: __proto__, constructor, prototype pollution vectors. */
const DEFAULT_DISALLOWED_KEYS = [
  /^__proto__$/,
  /^constructor$/,
  /^prototype$/,
];

export function createSchemaValidator(
  options: SchemaValidatorOptions = {},
): SchemaValidator {
  const {
    maxDepth = 20,
    maxStringLength = 1_000_000,
    maxArrayLength = 10_000,
    maxTotalKeys = 50_000,
    disallowedKeyPatterns = DEFAULT_DISALLOWED_KEYS,
  } = options;

  const rules: SchemaValidationRule[] = [
    createDepthRule(maxDepth),
    createStringSizeRule(maxStringLength),
    createArraySizeRule(maxArrayLength),
    createKeyCountRule(maxTotalKeys),
    createDisallowedKeysRule(disallowedKeyPatterns),
  ];

  return {
    validate(data: unknown): SchemaValidationResult {
      const allIssues: SchemaValidationIssue[] = [];
      for (const rule of rules) {
        const issues = rule.check(data, "$");
        allIssues.push(...issues);
      }
      return {
        valid: !allIssues.some(i => i.severity === "error"),
        issues: allIssues,
      };
    },

    addRule(rule: SchemaValidationRule): void {
      rules.push(rule);
    },

    ruleNames(): string[] {
      return rules.map(r => r.name);
    },
  };
}

// ── Hashcash ───────────────────────────────────────────────────────────────

async function sha256Hex(input: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(input);
  const hash = await crypto.subtle.digest("SHA-256", buf(data));
  return Array.from(new Uint8Array(hash))
    .map(b => b.toString(16).padStart(2, "0"))
    .join("");
}

function hasLeadingZeroBits(hexHash: string, bits: number): boolean {
  const fullNibbles = Math.floor(bits / 4);
  for (let i = 0; i < fullNibbles; i++) {
    if (hexHash.charAt(i) !== "0") return false;
  }
  const remaining = bits % 4;
  if (remaining > 0) {
    const nibble = parseInt(hexHash.charAt(fullNibbles), 16);
    const mask = 0xf << (4 - remaining);
    if ((nibble & mask) !== 0) return false;
  }
  return true;
}

function hashcashString(challenge: HashcashChallenge, counter: number): string {
  return `${challenge.resource}:${challenge.bits}:${challenge.issuedAt}:${challenge.salt}:${counter}`;
}

export function createHashcashMinter(): HashcashMinter {
  return {
    async mint(challenge: HashcashChallenge): Promise<HashcashProof> {
      let counter = 0;
      while (true) {
        const input = hashcashString(challenge, counter);
        const hash = await sha256Hex(input);
        if (hasLeadingZeroBits(hash, challenge.bits)) {
          return { challenge, counter, hash };
        }
        counter++;
      }
    },
  };
}

export function createHashcashVerifier(defaultBits = 8): HashcashVerifier {
  return {
    async verify(proof: HashcashProof): Promise<boolean> {
      const input = hashcashString(proof.challenge, proof.counter);
      const hash = await sha256Hex(input);
      if (hash !== proof.hash) return false;
      return hasLeadingZeroBits(hash, proof.challenge.bits);
    },

    createChallenge(resource: string, bits?: number): HashcashChallenge {
      const randomBytes = new Uint8Array(8);
      crypto.getRandomValues(randomBytes);
      const salt = Array.from(randomBytes)
        .map(b => b.toString(16).padStart(2, "0"))
        .join("");

      return {
        resource,
        bits: bits ?? defaultBits,
        issuedAt: new Date().toISOString(),
        salt,
      };
    },
  };
}

// ── Web of Trust ───────────────────────────────────────────────────────────

function computeTrustLevel(
  score: number,
  banned: boolean,
  trustedThreshold: number,
  highlyTrustedThreshold: number,
): TrustLevel {
  if (banned) return "untrusted";
  if (score >= highlyTrustedThreshold) return "highly-trusted";
  if (score >= trustedThreshold) return "trusted";
  if (score < 0) return "untrusted";
  return "neutral";
}

export function createPeerTrustGraph(
  options: TrustGraphOptions = {},
): PeerTrustGraph {
  const {
    trustedThreshold = 30,
    highlyTrustedThreshold = 70,
    positiveWeight = 5,
    negativeWeight = -10,
  } = options;

  const peers = new Map<string, PeerReputation>();
  const flaggedHashes = new Map<string, ContentHash>();
  const listeners = new Set<TrustGraphListener>();

  function notify(event: TrustGraphEvent): void {
    for (const listener of listeners) listener(event);
  }

  function ensurePeer(peerId: string): PeerReputation {
    let peer = peers.get(peerId);
    if (!peer) {
      peer = {
        peerId,
        trustLevel: "unknown",
        score: 0,
        positiveInteractions: 0,
        negativeInteractions: 0,
        banned: false,
        banReason: null,
        lastSeenAt: new Date().toISOString(),
      };
      peers.set(peerId, peer);
      notify({ type: "peer-added", peerId });
    }
    return peer;
  }

  function updateLevel(peer: PeerReputation): void {
    peer.trustLevel = computeTrustLevel(peer.score, peer.banned, trustedThreshold, highlyTrustedThreshold);
  }

  return {
    getPeer(peerId: string): PeerReputation | undefined {
      return peers.get(peerId);
    },

    recordPositive(peerId: string): void {
      const peer = ensurePeer(peerId);
      peer.positiveInteractions++;
      peer.score = Math.min(100, peer.score + positiveWeight);
      peer.lastSeenAt = new Date().toISOString();
      updateLevel(peer);
      notify({ type: "peer-updated", peerId });
    },

    recordNegative(peerId: string): void {
      const peer = ensurePeer(peerId);
      peer.negativeInteractions++;
      peer.score = Math.max(-100, peer.score + negativeWeight);
      peer.lastSeenAt = new Date().toISOString();
      updateLevel(peer);
      notify({ type: "peer-updated", peerId });
    },

    ban(peerId: string, reason: string): void {
      const peer = ensurePeer(peerId);
      peer.banned = true;
      peer.banReason = reason;
      updateLevel(peer);
      notify({ type: "peer-banned", peerId });
    },

    unban(peerId: string): void {
      const peer = peers.get(peerId);
      if (!peer || !peer.banned) return;
      peer.banned = false;
      peer.banReason = null;
      updateLevel(peer);
      notify({ type: "peer-unbanned", peerId });
    },

    isBanned(peerId: string): boolean {
      return peers.get(peerId)?.banned ?? false;
    },

    getPeersAtLevel(level: TrustLevel): PeerReputation[] {
      const order: TrustLevel[] = ["untrusted", "unknown", "neutral", "trusted", "highly-trusted"];
      const minIdx = order.indexOf(level);
      return [...peers.values()].filter(p => order.indexOf(p.trustLevel) >= minIdx);
    },

    allPeers(): PeerReputation[] {
      return [...peers.values()];
    },

    flagContent(hash: string, category: string, reportedBy: string): void {
      flaggedHashes.set(hash, {
        hash,
        category,
        reportedBy,
        reportedAt: new Date().toISOString(),
      });
      notify({ type: "content-flagged", contentHash: hash });
    },

    isContentFlagged(hash: string): boolean {
      return flaggedHashes.has(hash);
    },

    flaggedContent(): ReadonlyArray<ContentHash> {
      return [...flaggedHashes.values()];
    },

    onChange(listener: TrustGraphListener): () => void {
      listeners.add(listener);
      return () => { listeners.delete(listener); };
    },

    dispose(): void {
      peers.clear();
      flaggedHashes.clear();
      listeners.clear();
    },
  };
}

// ── Shamir Secret Sharing ──────────────────────────────────────────────────

/**
 * GF(256) arithmetic for Shamir secret sharing.
 * Uses the AES/Rijndael irreducible polynomial x^8 + x^4 + x^3 + x + 1.
 */
const GF256_POLY = 0x11b;

function gfMul(a: number, b: number): number {
  let result = 0;
  let aa = a;
  let bb = b;
  while (bb > 0) {
    if (bb & 1) result ^= aa;
    aa <<= 1;
    if (aa & 0x100) aa ^= GF256_POLY;
    bb >>= 1;
  }
  return result;
}

function gfInv(a: number): number {
  if (a === 0) throw new Error("Cannot invert 0 in GF(256)");
  // Compute a^254 = a^(-1) in GF(256) via exponentiation
  let result = 1;
  let base = a;
  let exp = 254;
  while (exp > 0) {
    if (exp & 1) result = gfMul(result, base);
    base = gfMul(base, base);
    exp >>= 1;
  }
  return result;
}

function evaluatePolynomial(coeffs: number[], x: number): number {
  let result = 0;
  for (let i = coeffs.length - 1; i >= 0; i--) {
    result = gfMul(result, x) ^ (coeffs[i] ?? 0);
  }
  return result;
}

function shareAt(shares: Array<{ x: number; y: number }>, idx: number): { x: number; y: number } {
  const s = shares[idx];
  if (!s) throw new Error(`Share index ${idx} out of bounds`);
  return s;
}

function lagrangeInterpolate(shares: Array<{ x: number; y: number }>): number {
  let secret = 0;
  for (let i = 0; i < shares.length; i++) {
    let num = 1;
    let den = 1;
    const si = shareAt(shares, i);
    for (let j = 0; j < shares.length; j++) {
      if (i === j) continue;
      const sj = shareAt(shares, j);
      num = gfMul(num, sj.x);
      den = gfMul(den, si.x ^ sj.x);
    }
    secret ^= gfMul(si.y, gfMul(num, gfInv(den)));
  }
  return secret;
}

export function createShamirSplitter(): ShamirSplitter {
  return {
    split(secret: Uint8Array, config: ShamirConfig): ShamirShare[] {
      const { totalShares, threshold } = config;
      if (threshold < 2) throw new Error("Threshold must be at least 2");
      if (totalShares < threshold) throw new Error("Total shares must be >= threshold");
      if (threshold > 255) throw new Error("Threshold must be <= 255");
      if (totalShares > 255) throw new Error("Total shares must be <= 255");

      const shares: ShamirShare[] = [];
      for (let i = 0; i < totalShares; i++) {
        shares.push({ index: i + 1, data: "" });
      }

      // For each byte of the secret, create a random polynomial and evaluate
      for (let byteIdx = 0; byteIdx < secret.length; byteIdx++) {
        // coeffs[0] is the secret byte, coeffs[1..threshold-1] are random
        const secretByte = secret[byteIdx];
        if (secretByte === undefined) throw new Error("Secret byte out of bounds");
        const coeffs: number[] = [secretByte];
        const randomBytes = new Uint8Array(threshold - 1);
        crypto.getRandomValues(randomBytes);
        for (let k = 0; k < threshold - 1; k++) {
          const rb = randomBytes[k];
          if (rb === undefined) throw new Error("Random byte out of bounds");
          coeffs.push(rb);
        }

        for (let i = 0; i < totalShares; i++) {
          const x = i + 1; // 1-based
          const y = evaluatePolynomial(coeffs, x);
          const prev = shares[i];
          if (!prev) throw new Error("Share index out of bounds");
          shares[i] = {
            index: prev.index,
            data: prev.data + y.toString(16).padStart(2, "0"),
          };
        }
      }

      return shares;
    },

    combine(shares: ShamirShare[], config: ShamirConfig): Uint8Array {
      if (shares.length < config.threshold) {
        throw new Error(`Need at least ${config.threshold} shares, got ${shares.length}`);
      }

      const usedShares = shares.slice(0, config.threshold);
      const firstShare = usedShares[0];
      if (!firstShare) throw new Error("No shares provided");
      const byteLength = firstShare.data.length / 2;
      const result = new Uint8Array(byteLength);

      for (let byteIdx = 0; byteIdx < byteLength; byteIdx++) {
        const points = usedShares.map(s => ({
          x: s.index,
          y: parseInt(s.data.slice(byteIdx * 2, byteIdx * 2 + 2), 16),
        }));
        result[byteIdx] = lagrangeInterpolate(points);
      }

      return result;
    },
  };
}

// ── Encrypted Escrow ───────────────────────────────────────────────────────

export function createEscrowManager(): EscrowManager {
  const deposits = new Map<string, EscrowDeposit>();

  return {
    deposit(depositorId: string, encryptedPayload: string, expiresAt?: string): EscrowDeposit {
      const deposit: EscrowDeposit = {
        id: uid("escrow"),
        depositorId,
        encryptedPayload,
        depositedAt: new Date().toISOString(),
        expiresAt: expiresAt ?? null,
        claimed: false,
      };
      deposits.set(deposit.id, deposit);
      return deposit;
    },

    claim(depositId: string): EscrowDeposit | null {
      const deposit = deposits.get(depositId);
      if (!deposit) return null;
      if (deposit.claimed) return null;
      if (deposit.expiresAt && new Date(deposit.expiresAt) < new Date()) return null;

      deposit.claimed = true;
      return deposit;
    },

    listDeposits(depositorId: string): EscrowDeposit[] {
      return [...deposits.values()].filter(d => d.depositorId === depositorId);
    },

    evictExpired(): number {
      const now = new Date();
      let evicted = 0;
      for (const [id, deposit] of deposits) {
        if (deposit.expiresAt && new Date(deposit.expiresAt) < now) {
          deposits.delete(id);
          evicted++;
        }
      }
      return evicted;
    },

    get(depositId: string): EscrowDeposit | undefined {
      return deposits.get(depositId);
    },
  };
}
