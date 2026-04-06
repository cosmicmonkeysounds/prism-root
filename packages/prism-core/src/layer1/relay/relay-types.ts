/**
 * @prism/core — Relay Types
 *
 * The Relay is Prism's bridge between the Core/Daemon stack and the outside
 * world. It is NOT just a server — it's a modular, composable runtime where
 * users mix and match Web 1/2/3 features via a builder pattern.
 *
 * Any server running Relay software is a zero-knowledge router that never
 * sees unencrypted CRDT data. Optional modules add Sovereign Portals,
 * AutoREST gateways, webhooks, blind pings, and more.
 *
 * Architecture:
 *   createRelayBuilder({ identity })
 *     .use(blindMailbox())        // E2EE store-and-forward
 *     .use(relayTimestamping())   // Cryptographic timestamps
 *     .use(blindPings())          // Push notifications
 *     .use(sovereignPortals())    // Optional: SSR portals
 *     .use(autoRest())            // Optional: REST gateway
 *     .use(webhooks())            // Optional: Outgoing HTTP
 *     .build();
 */

import type { DID } from "../identity/identity-types.js";

// ── Envelope: what the Relay actually sees ──────────────────────────────────

/**
 * An encrypted envelope routed through the Relay. The Relay sees the
 * addressing metadata but never the payload content.
 */
export interface RelayEnvelope {
  /** Unique envelope ID. */
  id: string;
  /** Sender DID (may be ephemeral/anonymous). */
  from: DID;
  /** Recipient DID. */
  to: DID;
  /** Opaque encrypted payload (Loro CRDT update, message, etc.). */
  ciphertext: Uint8Array;
  /** ISO-8601 timestamp when the envelope was submitted. */
  submittedAt: string;
  /** Optional: Hashcash proof-of-work token for spam protection. */
  proofOfWork?: string;
  /** TTL in milliseconds. Relay drops the envelope after expiry. */
  ttlMs: number;
}

// ── Blind Mailbox ───────────────────────────────────────────────────────────

/**
 * E2EE store-and-forward message queue. Peers deposit encrypted envelopes;
 * recipients collect them when online. The Relay never sees plaintext.
 */
export interface BlindMailbox {
  /** Deposit an envelope for a recipient. */
  deposit(envelope: RelayEnvelope): void;
  /** Collect all pending envelopes for a recipient DID. Removes from queue. */
  collect(recipientDid: DID): RelayEnvelope[];
  /** Peek at pending count without collecting. */
  pendingCount(recipientDid: DID): number;
  /** Total envelopes across all mailboxes. */
  totalCount(): number;
  /** Evict expired envelopes. Returns number evicted. */
  evict(): number;
  /** Clear all mailboxes. */
  clear(): void;
}

// ── Relay Router ────────────────────────────────────────────────────────────

/** Result of routing an envelope. */
export type RouteResult =
  | { status: "delivered"; recipientDid: DID }
  | { status: "queued"; recipientDid: DID; mailboxSize: number }
  | { status: "rejected"; reason: string };

/**
 * Zero-knowledge router. Dispatches envelopes to connected peers or
 * deposits into blind mailboxes for offline peers.
 */
export interface RelayRouter {
  /** Route an envelope to its recipient. */
  route(envelope: RelayEnvelope): RouteResult;
  /** Register a peer as "online" with a delivery callback. */
  registerPeer(did: DID, deliver: (envelope: RelayEnvelope) => void): void;
  /** Unregister a peer (gone offline). */
  unregisterPeer(did: DID): void;
  /** Check if a peer is currently online. */
  isOnline(did: DID): boolean;
  /** List all online peer DIDs. */
  onlinePeers(): DID[];
}

// ── Relay Timestamp ─────────────────────────────────────────────────────────

/** A cryptographic timestamp receipt from a Relay. */
export interface TimestampReceipt {
  /** Hash of the data that was timestamped. */
  dataHash: string;
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** DID of the Relay that issued the timestamp. */
  relayDid: DID;
  /** Ed25519 signature of (dataHash + timestamp) by the Relay. */
  signature: Uint8Array;
}

export interface RelayTimestamper {
  /** Issue a timestamp receipt for a data hash. */
  stamp(dataHash: string): Promise<TimestampReceipt>;
  /** Verify a timestamp receipt's signature. */
  verify(receipt: TimestampReceipt): Promise<boolean>;
}

// ── Blind Pings ─────────────────────────────────────────────────────────────

/** A content-free notification to wake a peer. */
export interface BlindPing {
  /** Recipient DID. */
  to: DID;
  /** ISO-8601 timestamp. */
  sentAt: string;
  /** Optional: badge count hint. */
  badgeCount?: number;
}

/** Push notification transport (APNs, FCM, etc.). */
export interface PingTransport {
  /** Send a blind ping. Returns true if accepted by the transport. */
  send(ping: BlindPing): Promise<boolean>;
}

export interface BlindPinger {
  /** Send a blind ping to wake a peer. */
  ping(recipientDid: DID, badgeCount?: number): Promise<boolean>;
  /** Register a transport (APNs, FCM, in-memory for testing). */
  setTransport(transport: PingTransport): void;
}

// ── Capability Tokens ───────────────────────────────────────────────────────

/** Scoped access token for AutoREST and portal access. */
export interface CapabilityToken {
  /** Unique token ID. */
  tokenId: string;
  /** DID of the token issuer. */
  issuer: DID;
  /** DID of the token subject (who it's for). */
  subject: DID | "*";
  /** Permitted operations (e.g. ["read", "write", "list"]). */
  permissions: string[];
  /** Resource scope (e.g. collection ID, object type). */
  scope: string;
  /** ISO-8601 issued-at. */
  issuedAt: string;
  /** ISO-8601 expiry. Null = no expiry. */
  expiresAt: string | null;
  /** Ed25519 signature by the issuer. */
  signature: Uint8Array;
}

export interface CapabilityTokenManager {
  /** Issue a new capability token. */
  issue(params: {
    subject: DID | "*";
    permissions: string[];
    scope: string;
    ttlMs?: number;
  }): Promise<CapabilityToken>;
  /** Verify a token's signature and check expiry. */
  verify(token: CapabilityToken): Promise<{ valid: boolean; reason?: string }>;
  /** Revoke a token by ID. */
  revoke(tokenId: string): void;
  /** Check if a token has been revoked. */
  isRevoked(tokenId: string): boolean;
}

// ── Webhooks ────────────────────────────────────────────────────────────────

/** Webhook registration for outgoing HTTP on CRDT changes. */
export interface WebhookConfig {
  /** Unique webhook ID. */
  id: string;
  /** Target URL to POST to. */
  url: string;
  /** Events that trigger this webhook. */
  events: string[];
  /** Optional shared secret for HMAC signature verification. */
  secret?: string;
  /** Whether this webhook is active. */
  active: boolean;
}

/** Payload sent to a webhook endpoint. */
export interface WebhookPayload {
  /** Webhook ID. */
  webhookId: string;
  /** Event type that triggered the webhook. */
  event: string;
  /** ISO-8601 timestamp. */
  timestamp: string;
  /** Event-specific data (serializable). */
  data: Record<string, unknown>;
  /** HMAC-SHA256 signature of the payload (if secret configured). */
  signature?: string;
}

/** Delivery result from a webhook attempt. */
export interface WebhookDelivery {
  webhookId: string;
  event: string;
  timestamp: string;
  success: boolean;
  statusCode?: number;
  error?: string;
}

export interface WebhookEmitter {
  /** Register a webhook. */
  register(config: Omit<WebhookConfig, "id">): WebhookConfig;
  /** Unregister a webhook. */
  unregister(webhookId: string): boolean;
  /** List all registered webhooks. */
  list(): WebhookConfig[];
  /** Emit an event to all matching webhooks. Returns delivery results. */
  emit(event: string, data: Record<string, unknown>): Promise<WebhookDelivery[]>;
  /** Get delivery log for a webhook. */
  deliveries(webhookId: string): WebhookDelivery[];
}

// ── Sovereign Portals ───────────────────────────────────────────────────────

/** Portal level as described in the spec. */
export type PortalLevel = 1 | 2 | 3 | 4;

/** Configuration for a Sovereign Portal. */
export interface PortalManifest {
  /** Portal ID (derived from collection). */
  portalId: string;
  /** Display name. */
  name: string;
  /** Portal level (1=read-only, 2=live dashboard, 3=interactive forms, 4=complex app). */
  level: PortalLevel;
  /** Collection ID being exposed. */
  collectionId: string;
  /** Custom domain, if any. */
  domain?: string;
  /** Path prefix. */
  basePath: string;
  /** Whether this portal is publicly accessible. */
  isPublic: boolean;
  /** Capability token scope for portal access (level 3+). */
  accessScope?: string;
  /** ISO-8601 created timestamp. */
  createdAt: string;
}

export interface PortalRegistry {
  /** Register a portal manifest. */
  register(manifest: Omit<PortalManifest, "portalId" | "createdAt">): PortalManifest;
  /** Unregister a portal. */
  unregister(portalId: string): boolean;
  /** Get a portal by ID. */
  get(portalId: string): PortalManifest | undefined;
  /** List all portals. */
  list(): PortalManifest[];
  /** Find a portal by domain + path. */
  resolve(domain: string, path: string): PortalManifest | undefined;
}

// ── Relay Module System ─────────────────────────────────────────────────────

/**
 * A pluggable Relay module. Modules contribute capabilities to the Relay
 * runtime via the builder pattern. Each module has a lifecycle:
 * install → start → stop.
 */
export interface RelayModule {
  /** Unique module name (e.g. "blind-mailbox", "webhooks", "sovereign-portals"). */
  readonly name: string;
  /** Human description. */
  readonly description: string;
  /** Dependencies on other module names (installed first). */
  readonly dependencies: string[];
  /** Install: register capabilities with the Relay context. */
  install(ctx: RelayContext): void;
  /** Start: begin processing (open sockets, start timers). */
  start?(ctx: RelayContext): Promise<void>;
  /** Stop: clean shutdown. */
  stop?(ctx: RelayContext): Promise<void>;
}

/**
 * Shared context passed to all modules during install/start/stop.
 * Modules use this to access other modules' capabilities.
 */
export interface RelayContext {
  /** The Relay's own DID identity. */
  readonly relayDid: DID;
  /** Get a named capability provided by any installed module. */
  getCapability<T>(name: string): T | undefined;
  /** Register a named capability for other modules to consume. */
  setCapability<T>(name: string, value: T): void;
  /** Get the Relay's configuration. */
  readonly config: RelayConfig;
}

// ── Relay Instance ──────────────────────────────────────────────────────────

export interface RelayConfig {
  /** Relay's own DID. */
  relayDid: DID;
  /** Default envelope TTL in milliseconds. Default: 7 days. */
  defaultTtlMs: number;
  /** Maximum envelope size in bytes. Default: 1MB. */
  maxEnvelopeSizeBytes: number;
  /** Eviction interval for expired mailbox envelopes. Default: 60s. */
  evictionIntervalMs: number;
}

/**
 * A built Relay instance with all modules installed.
 */
export interface RelayInstance {
  /** The Relay's DID. */
  readonly did: DID;
  /** All installed module names. */
  readonly modules: string[];
  /** Get a capability by name. */
  getCapability<T>(name: string): T | undefined;
  /** Start all modules. */
  start(): Promise<void>;
  /** Stop all modules. */
  stop(): Promise<void>;
  /** Whether the relay is currently running. */
  readonly running: boolean;
}

/**
 * Builder for composing a Relay from modules.
 *
 * Usage:
 *   createRelayBuilder({ relayDid })
 *     .use(blindMailboxModule())
 *     .use(relayTimestampModule())
 *     .use(webhookModule())
 *     .build();
 */
export interface RelayBuilder {
  /** Add a module to the relay. Chainable. */
  use(module: RelayModule): RelayBuilder;
  /** Override default config. Chainable. */
  configure(partial: Partial<RelayConfig>): RelayBuilder;
  /** Build the relay instance. Validates dependencies and installs modules. */
  build(): RelayInstance;
}

// ── Builder Options ─────────────────────────────────────────────────────────

export interface RelayBuilderOptions {
  /** Relay's own DID. */
  relayDid: DID;
  /** Override default config values. */
  config?: Partial<Omit<RelayConfig, "relayDid">>;
}

// ── Well-known capability names ─────────────────────────────────────────────

/** Standard capability names that modules register. */
export const RELAY_CAPABILITIES = {
  MAILBOX: "relay:mailbox",
  ROUTER: "relay:router",
  TIMESTAMPER: "relay:timestamper",
  PINGER: "relay:pinger",
  TOKENS: "relay:tokens",
  WEBHOOKS: "relay:webhooks",
  PORTALS: "relay:portals",
  COLLECTIONS: "relay:collections",
  HASHCASH: "relay:hashcash",
  TRUST: "relay:trust",
  ESCROW: "relay:escrow",
  FEDERATION: "relay:federation",
} as const;

// ── Collection Hosting ─────────────────────────────────────────────────────

import type { CollectionStore } from "../persistence/collection-store.js";

/** Hosts CRDT collections on the Relay for remote sync. */
export interface CollectionHost {
  /** Create a new hosted collection. Returns the store. */
  create(id: string): CollectionStore;
  /** Get an existing hosted collection. */
  get(id: string): CollectionStore | undefined;
  /** List all hosted collection IDs. */
  list(): string[];
  /** Remove a hosted collection. */
  remove(id: string): boolean;
}

// ── Hashcash Gate ──────────────────────────────────────────────────────────

import type { HashcashChallenge, HashcashProof } from "../trust/trust-types.js";

/** Spam protection gate using proof-of-work challenges. */
export interface HashcashGate {
  /** Issue a challenge for a resource (e.g. relay DID). */
  createChallenge(resource: string): HashcashChallenge;
  /** Verify a proof-of-work submission. */
  verifyProof(proof: HashcashProof): Promise<boolean>;
  /** Check if a DID has already passed a challenge. */
  isVerified(did: DID): boolean;
  /** Mark a DID as having passed a challenge. */
  markVerified(did: DID): void;
}

/** Options for the hashcash module. */
export interface HashcashModuleOptions {
  /** Number of leading zero bits required (higher = harder). Default: 16. */
  bits?: number;
}

// ── Federation ─────────────────────────────────────────────────────────────

/** A known relay peer in the federation mesh. */
export interface FederationPeer {
  /** The peer relay's DID. */
  relayDid: DID;
  /** HTTP URL where the peer relay listens. */
  url: string;
  /** ISO-8601 time the peer announced itself. */
  announcedAt: string;
  /** ISO-8601 time the peer was last seen. */
  lastSeenAt: string;
}

/** Result of forwarding an envelope to another relay. */
export type ForwardResult =
  | { status: "forwarded"; targetRelay: DID }
  | { status: "no-transport" }
  | { status: "unknown-relay"; targetRelay: DID }
  | { status: "error"; message: string };

/** Callback for the runtime to provide actual HTTP forwarding. */
export type ForwardTransport = (
  envelope: RelayEnvelope,
  peerUrl: string,
) => Promise<ForwardResult>;

/** Registry of federated relay peers. */
export interface FederationRegistry {
  /** Announce a relay peer's presence. */
  announce(relayDid: DID, url: string): void;
  /** Get all known relay peers. */
  getPeers(): FederationPeer[];
  /** Get a specific peer by DID. */
  getPeer(relayDid: DID): FederationPeer | undefined;
  /** Remove a peer. */
  removePeer(relayDid: DID): boolean;
  /** Forward an envelope to a specific relay. */
  forwardEnvelope(envelope: RelayEnvelope, targetRelay: DID): Promise<ForwardResult>;
  /** Set the transport used for forwarding. */
  setTransport(transport: ForwardTransport): void;
}
