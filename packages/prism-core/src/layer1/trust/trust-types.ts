/**
 * @prism/core — Trust & Safety Types (Layer 1)
 *
 * The Sovereign Immune System.
 *
 * Subsystems:
 *   1. Lua Sandbox — Capability Tokens restricting API surface per plugin
 *   2. Schema Poison Pill — JSON schema validation before import
 *   3. Hashcash — proof-of-work spam protection for Relay messages
 *   4. Web of Trust — peer reputation, bans, toxic content hash gossip
 *   5. Secure Recovery — Shamir secret sharing for vault recovery
 *   6. Encrypted Escrow — blind escrow for key recovery via Relay
 */

// ── Lua Sandbox ────────────────────────────────────────────────────────────

/**
 * Capabilities a sandboxed Lua plugin may request.
 * Each capability maps to a set of API functions exposed in the sandbox.
 */
export type SandboxCapability =
  | "crdt:read"
  | "crdt:write"
  | "net:fetch"
  | "net:websocket"
  | "fs:read"
  | "fs:write"
  | "ui:notify"
  | "ui:dialog"
  | "process:spawn"
  | "ai:complete"
  | "ai:inline";

export interface SandboxPolicy {
  /** Plugin identifier. */
  pluginId: string;
  /** Granted capabilities. */
  capabilities: SandboxCapability[];
  /** Maximum execution time (ms). 0 = unlimited. */
  maxDurationMs: number;
  /** Maximum memory (bytes). 0 = unlimited. */
  maxMemoryBytes: number;
  /** Allowed URL patterns for net:fetch (glob-style). Empty = none. */
  allowedUrls: string[];
  /** Allowed filesystem paths for fs:read/write (glob-style). Empty = none. */
  allowedPaths: string[];
}

export interface SandboxViolation {
  /** Which capability was violated. */
  capability: SandboxCapability | "timeout" | "memory";
  /** Human-readable description. */
  message: string;
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** The plugin that violated. */
  pluginId: string;
}

export interface LuaSandbox {
  /** Check if a capability is granted. */
  hasCapability(capability: SandboxCapability): boolean;
  /** Check if a URL is allowed for fetch. */
  isUrlAllowed(url: string): boolean;
  /** Check if a path is allowed for fs. */
  isPathAllowed(path: string): boolean;
  /** Record a violation (for audit). */
  recordViolation(violation: SandboxViolation): void;
  /** Get all recorded violations. */
  readonly violations: ReadonlyArray<SandboxViolation>;
  /** The policy this sandbox enforces. */
  readonly policy: SandboxPolicy;
}

// ── Schema Poison Pill ─────────────────────────────────────────────────────

export type SchemaValidationSeverity = "error" | "warning";

export interface SchemaValidationIssue {
  /** JSON path (e.g. "$.data.fields[0].type"). */
  path: string;
  /** Human-readable message. */
  message: string;
  /** Severity. */
  severity: SchemaValidationSeverity;
  /** Rule that was violated. */
  rule: string;
}

export interface SchemaValidationResult {
  /** Whether the schema is safe to import. */
  valid: boolean;
  /** Issues found. */
  issues: SchemaValidationIssue[];
}

/**
 * Rules for schema validation. Each rule checks a specific threat vector.
 */
export interface SchemaValidationRule {
  /** Rule name. */
  name: string;
  /** Description of what this rule checks. */
  description: string;
  /** Check function — returns issues for the given data. */
  check(data: unknown, path: string): SchemaValidationIssue[];
}

export interface SchemaValidator {
  /** Validate arbitrary data before import. */
  validate(data: unknown): SchemaValidationResult;
  /** Register a custom validation rule. */
  addRule(rule: SchemaValidationRule): void;
  /** List all registered rule names. */
  ruleNames(): string[];
}

// ── Hashcash (Relay Spam Protection) ───────────────────────────────────────

export interface HashcashChallenge {
  /** Resource being protected (e.g. relay DID or room ID). */
  resource: string;
  /** Required number of leading zero bits. */
  bits: number;
  /** ISO-8601 timestamp when challenge was issued. */
  issuedAt: string;
  /** Random nonce added by the server. */
  salt: string;
}

export interface HashcashProof {
  /** The original challenge. */
  challenge: HashcashChallenge;
  /** Counter that produces the required hash. */
  counter: number;
  /** The resulting hash (hex string). */
  hash: string;
}

export interface HashcashMinter {
  /** Mint a proof-of-work for the given challenge. */
  mint(challenge: HashcashChallenge): Promise<HashcashProof>;
}

export interface HashcashVerifier {
  /** Verify a proof-of-work. */
  verify(proof: HashcashProof): Promise<boolean>;
  /** Create a new challenge for a resource. */
  createChallenge(resource: string, bits?: number): HashcashChallenge;
}

// ── Web of Trust ───────────────────────────────────────────────────────────

export type TrustLevel = "unknown" | "untrusted" | "neutral" | "trusted" | "highly-trusted";

export interface PeerReputation {
  /** Peer DID or ID. */
  peerId: string;
  /** Current trust level. */
  trustLevel: TrustLevel;
  /** Numeric score (-100 to 100). */
  score: number;
  /** Number of positive interactions. */
  positiveInteractions: number;
  /** Number of negative interactions (violations, spam). */
  negativeInteractions: number;
  /** Whether this peer is banned. */
  banned: boolean;
  /** Ban reason (if banned). */
  banReason: string | null;
  /** ISO-8601 last interaction timestamp. */
  lastSeenAt: string;
}

export interface ContentHash {
  /** SHA-256 hash of the toxic/spam content. */
  hash: string;
  /** Category of the content (e.g. "spam", "malware", "phishing"). */
  category: string;
  /** Who reported this hash. */
  reportedBy: string;
  /** ISO-8601 report timestamp. */
  reportedAt: string;
}

export interface TrustGraphEvent {
  type: "peer-added" | "peer-updated" | "peer-banned" | "peer-unbanned" | "content-flagged";
  peerId?: string;
  contentHash?: string;
}

export type TrustGraphListener = (event: TrustGraphEvent) => void;

export interface PeerTrustGraph {
  /** Get reputation for a peer. */
  getPeer(peerId: string): PeerReputation | undefined;
  /** Record a positive interaction. */
  recordPositive(peerId: string): void;
  /** Record a negative interaction. */
  recordNegative(peerId: string): void;
  /** Ban a peer. */
  ban(peerId: string, reason: string): void;
  /** Unban a peer. */
  unban(peerId: string): void;
  /** Check if a peer is banned. */
  isBanned(peerId: string): boolean;
  /** Get all peers at or above a trust level. */
  getPeersAtLevel(level: TrustLevel): PeerReputation[];
  /** List all known peers. */
  allPeers(): PeerReputation[];

  /** Flag content as toxic (hash gossip). */
  flagContent(hash: string, category: string, reportedBy: string): void;
  /** Check if content hash is flagged. */
  isContentFlagged(hash: string): boolean;
  /** Get all flagged content hashes. */
  flaggedContent(): ReadonlyArray<ContentHash>;

  /** Subscribe to trust graph events. */
  onChange(listener: TrustGraphListener): () => void;
  /** Dispose of resources. */
  dispose(): void;
}

// ── Shamir Secret Sharing ──────────────────────────────────────────────────

export interface ShamirShare {
  /** Share index (1-based). */
  index: number;
  /** The share data (hex-encoded). */
  data: string;
}

export interface ShamirConfig {
  /** Total number of shares to generate. */
  totalShares: number;
  /** Minimum shares needed to reconstruct. */
  threshold: number;
}

export interface ShamirSplitter {
  /** Split a secret into shares. */
  split(secret: Uint8Array, config: ShamirConfig): ShamirShare[];
  /** Reconstruct a secret from shares. */
  combine(shares: ShamirShare[], config: ShamirConfig): Uint8Array;
}

// ── Encrypted Escrow ───────────────────────────────────────────────────────

export interface EscrowDeposit {
  /** Unique deposit ID. */
  id: string;
  /** Who deposited (DID). */
  depositorId: string;
  /** Encrypted payload (the escrowed key material). */
  encryptedPayload: string;
  /** ISO-8601 deposit timestamp. */
  depositedAt: string;
  /** ISO-8601 expiry timestamp (null = no expiry). */
  expiresAt: string | null;
  /** Whether this deposit has been claimed. */
  claimed: boolean;
}

export interface EscrowManager {
  /** Deposit encrypted key material for recovery. */
  deposit(depositorId: string, encryptedPayload: string, expiresAt?: string): EscrowDeposit;
  /** Claim a deposit (requires Shamir threshold proof). */
  claim(depositId: string): EscrowDeposit | null;
  /** List deposits for a depositor. */
  listDeposits(depositorId: string): EscrowDeposit[];
  /** Evict expired deposits. */
  evictExpired(): number;
  /** Get a deposit by ID. */
  get(depositId: string): EscrowDeposit | undefined;
  /** List every deposit (claimed or unclaimed) — used for persistence. */
  listAll(): EscrowDeposit[];
}

// ── Password Authentication ────────────────────────────────────────────────

/**
 * Persistent record for a password-authenticated user.
 *
 * Password material is stored as a PBKDF2-SHA256 hash with a per-user salt.
 * The relay never stores the plaintext password, and the manager exposes no
 * way to recover it — only verify(username, password) and replaceHash().
 */
export interface PasswordAuthRecord {
  /** Unique username (lowercased). */
  username: string;
  /** DID associated with this user (e.g. did:password:<username>). */
  did: string;
  /** Base64-encoded random salt used for PBKDF2. */
  salt: string;
  /** Base64-encoded PBKDF2-SHA256 hash of (password + salt). */
  passwordHash: string;
  /** PBKDF2 iteration count used to derive `passwordHash`. */
  iterations: number;
  /** ISO-8601 timestamp when the user was created. */
  createdAt: string;
  /** ISO-8601 timestamp when the password was last changed. */
  updatedAt: string;
  /** Optional caller-supplied metadata (display name, email, etc.). */
  metadata?: Record<string, string>;
}

/** Result of attempting to authenticate a username/password pair. */
export type PasswordAuthResult =
  | { ok: true; record: PasswordAuthRecord }
  | { ok: false; reason: "unknown-user" | "wrong-password" };

/**
 * Manager for password-authenticated users on a relay.
 *
 * Storage is in-memory; persistence is the relay's job (file store).
 * All hashing is PBKDF2-SHA256 with a configurable iteration count.
 */
export interface PasswordAuthManager {
  /** Register a new user. Throws if `username` already exists. */
  register(input: {
    username: string;
    password: string;
    did?: string;
    metadata?: Record<string, string>;
  }): Promise<PasswordAuthRecord>;
  /** Verify a username/password pair. */
  verify(username: string, password: string): Promise<PasswordAuthResult>;
  /** Change a user's password (requires the existing password). */
  changePassword(
    username: string,
    oldPassword: string,
    newPassword: string,
  ): Promise<PasswordAuthResult>;
  /** Look up a user record by username (without verifying credentials). */
  get(username: string): PasswordAuthRecord | undefined;
  /** List every user record — used for persistence. */
  list(): PasswordAuthRecord[];
  /** Restore a previously serialised record (used by file-store on load). */
  restore(record: PasswordAuthRecord): void;
  /** Delete a user record. Returns true if it existed. */
  remove(username: string): boolean;
  /** Number of registered users. */
  size(): number;
}

export interface PasswordAuthManagerOptions {
  /** PBKDF2 iteration count. Default: 600_000. */
  iterations?: number;
  /** Salt length in bytes. Default: 16. */
  saltBytes?: number;
}

// ── Options ────────────────────────────────────────────────────────────────

export interface TrustGraphOptions {
  /** Score threshold for "trusted" level. Default: 30. */
  trustedThreshold?: number;
  /** Score threshold for "highly-trusted" level. Default: 70. */
  highlyTrustedThreshold?: number;
  /** Score change per positive interaction. Default: 5. */
  positiveWeight?: number;
  /** Score change per negative interaction. Default: -10. */
  negativeWeight?: number;
}

export interface SchemaValidatorOptions {
  /** Maximum nesting depth allowed. Default: 20. */
  maxDepth?: number;
  /** Maximum string length allowed. Default: 1_000_000. */
  maxStringLength?: number;
  /** Maximum array length allowed. Default: 10_000. */
  maxArrayLength?: number;
  /** Maximum total keys across all objects. Default: 50_000. */
  maxTotalKeys?: number;
  /** Disallowed key patterns (regex). */
  disallowedKeyPatterns?: RegExp[];
}
