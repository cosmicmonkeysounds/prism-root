/**
 * @prism/core — Relay Builder & Core Modules
 *
 * Composable relay runtime. Users pick modules via builder pattern:
 *
 *   createRelayBuilder({ relayDid })
 *     .use(blindMailboxModule())
 *     .use(relayRouterModule())
 *     .use(relayTimestampModule(identity))
 *     .use(webhookModule())
 *     .build();
 *
 * Each module is independent. Dependencies are declared and validated at
 * build time. Capabilities are shared via the RelayContext.
 */

import type { DID } from "../identity/identity-types.js";
import type { PrismIdentity } from "../identity/identity-types.js";
import type {
  RelayEnvelope,
  BlindMailbox,
  RelayRouter,
  RouteResult,
  RelayTimestamper,
  TimestampReceipt,
  BlindPinger,
  BlindPing,
  PingTransport,
  CapabilityToken,
  CapabilityTokenManager,
  WebhookConfig,
  WebhookPayload,
  WebhookDelivery,
  WebhookEmitter,
  PortalManifest,
  PortalRegistry,
  RelayModule,
  RelayContext,
  RelayConfig,
  RelayInstance,
  RelayBuilder,
  RelayBuilderOptions,
} from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";

// ── Helpers ─────────────────────────────────────────────────────────────────

let idCounter = 0;
function uid(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${(idCounter++).toString(36)}`;
}

function hexEncode(bytes: Uint8Array): string {
  let hex = "";
  for (const b of bytes) hex += b.toString(16).padStart(2, "0");
  return hex;
}

async function sha256Hex(data: string): Promise<string> {
  const enc = new TextEncoder();
  const buf = await globalThis.crypto.subtle.digest(
    "SHA-256",
    enc.encode(data) as unknown as BufferSource,
  );
  return hexEncode(new Uint8Array(buf));
}

// ── Relay Builder ───────────────────────────────────────────────────────────

const DEFAULT_TTL_MS = 7 * 24 * 60 * 60 * 1000; // 7 days
const DEFAULT_MAX_ENVELOPE_BYTES = 1_048_576; // 1 MB
const DEFAULT_EVICTION_INTERVAL_MS = 60_000;

export function createRelayBuilder(options: RelayBuilderOptions): RelayBuilder {
  const modules: RelayModule[] = [];
  let configOverrides: Partial<RelayConfig> = options.config ?? {};

  const builder: RelayBuilder = {
    use(module: RelayModule): RelayBuilder {
      modules.push(module);
      return builder;
    },

    configure(partial: Partial<RelayConfig>): RelayBuilder {
      configOverrides = { ...configOverrides, ...partial };
      return builder;
    },

    build(): RelayInstance {
      const config: RelayConfig = {
        relayDid: options.relayDid,
        defaultTtlMs: DEFAULT_TTL_MS,
        maxEnvelopeSizeBytes: DEFAULT_MAX_ENVELOPE_BYTES,
        evictionIntervalMs: DEFAULT_EVICTION_INTERVAL_MS,
        ...configOverrides,
      };

      // Validate dependencies
      const installed = new Set(modules.map(m => m.name));
      for (const mod of modules) {
        for (const dep of mod.dependencies) {
          if (!installed.has(dep)) {
            throw new Error(
              `Module "${mod.name}" depends on "${dep}" which is not installed`,
            );
          }
        }
      }

      // Check for duplicate module names
      if (installed.size !== modules.length) {
        const names = modules.map(m => m.name);
        const dupes = names.filter((n, i) => names.indexOf(n) !== i);
        throw new Error(`Duplicate module names: ${dupes.join(", ")}`);
      }

      // Build context
      const capabilities = new Map<string, unknown>();
      const ctx: RelayContext = {
        relayDid: config.relayDid,
        getCapability<T>(name: string): T | undefined {
          return capabilities.get(name) as T | undefined;
        },
        setCapability<T>(name: string, value: T): void {
          capabilities.set(name, value);
        },
        config,
      };

      // Install all modules (order-preserving)
      for (const mod of modules) {
        mod.install(ctx);
      }

      let running = false;

      return {
        get did() {
          return config.relayDid;
        },
        get modules() {
          return modules.map(m => m.name);
        },
        getCapability<T>(name: string): T | undefined {
          return ctx.getCapability<T>(name);
        },
        async start(): Promise<void> {
          if (running) return;
          for (const mod of modules) {
            if (mod.start) await mod.start(ctx);
          }
          running = true;
        },
        async stop(): Promise<void> {
          if (!running) return;
          // Stop in reverse order
          for (let i = modules.length - 1; i >= 0; i--) {
            const mod = modules[i] as RelayModule;
            if (mod.stop) await mod.stop(ctx);
          }
          running = false;
        },
        get running() {
          return running;
        },
      };
    },
  };

  return builder;
}

// ── Module: Blind Mailbox ───────────────────────────────────────────────────

export function blindMailboxModule(): RelayModule {
  return {
    name: "blind-mailbox",
    description: "E2EE store-and-forward message queue for offline peers",
    dependencies: [],

    install(ctx: RelayContext): void {
      const mailbox = createBlindMailbox(ctx.config.defaultTtlMs);
      ctx.setCapability(RELAY_CAPABILITIES.MAILBOX, mailbox);
    },
  };
}

function createBlindMailbox(_defaultTtlMs: number): BlindMailbox {
  // Map<recipientDID, envelope[]>
  const boxes = new Map<string, RelayEnvelope[]>();

  return {
    deposit(envelope: RelayEnvelope): void {
      const key = envelope.to;
      const queue = boxes.get(key) ?? [];
      queue.push({ ...envelope });
      boxes.set(key, queue);
    },

    collect(recipientDid: DID): RelayEnvelope[] {
      const queue = boxes.get(recipientDid);
      if (!queue || queue.length === 0) return [];
      boxes.delete(recipientDid);
      return queue;
    },

    pendingCount(recipientDid: DID): number {
      return boxes.get(recipientDid)?.length ?? 0;
    },

    totalCount(): number {
      let total = 0;
      for (const queue of boxes.values()) total += queue.length;
      return total;
    },

    evict(): number {
      const now = Date.now();
      let evicted = 0;
      for (const [did, queue] of boxes) {
        const before = queue.length;
        const remaining = queue.filter(env => {
          const submitted = new Date(env.submittedAt).getTime();
          return now - submitted < env.ttlMs;
        });
        evicted += before - remaining.length;
        if (remaining.length === 0) {
          boxes.delete(did);
        } else {
          boxes.set(did, remaining);
        }
      }
      return evicted;
    },

    clear(): void {
      boxes.clear();
    },
  };
}

// ── Module: Relay Router ────────────────────────────────────────────────────

export function relayRouterModule(): RelayModule {
  return {
    name: "relay-router",
    description: "Zero-knowledge envelope routing to online peers or blind mailbox",
    dependencies: ["blind-mailbox"],

    install(ctx: RelayContext): void {
      const mailbox = ctx.getCapability<BlindMailbox>(RELAY_CAPABILITIES.MAILBOX);
      if (!mailbox) throw new Error("Blind mailbox not available");
      const router = createRelayRouter(mailbox, ctx.config.maxEnvelopeSizeBytes);
      ctx.setCapability(RELAY_CAPABILITIES.ROUTER, router);
    },
  };
}

function createRelayRouter(
  mailbox: BlindMailbox,
  maxEnvelopeBytes: number,
): RelayRouter {
  const peers = new Map<string, (envelope: RelayEnvelope) => void>();

  return {
    route(envelope: RelayEnvelope): RouteResult {
      if (envelope.ciphertext.length > maxEnvelopeBytes) {
        return { status: "rejected", reason: "envelope exceeds maximum size" };
      }

      const deliver = peers.get(envelope.to);
      if (deliver) {
        deliver(envelope);
        return { status: "delivered", recipientDid: envelope.to };
      }

      mailbox.deposit(envelope);
      return {
        status: "queued",
        recipientDid: envelope.to,
        mailboxSize: mailbox.pendingCount(envelope.to),
      };
    },

    registerPeer(did: DID, deliver: (envelope: RelayEnvelope) => void): void {
      peers.set(did, deliver);
      // Flush any queued envelopes
      const queued = mailbox.collect(did);
      for (const env of queued) deliver(env);
    },

    unregisterPeer(did: DID): void {
      peers.delete(did);
    },

    isOnline(did: DID): boolean {
      return peers.has(did);
    },

    onlinePeers(): DID[] {
      return [...peers.keys()] as DID[];
    },
  };
}

// ── Module: Relay Timestamper ───────────────────────────────────────────────

export function relayTimestampModule(identity: PrismIdentity): RelayModule {
  return {
    name: "relay-timestamp",
    description: "Cryptographic timestamp receipts for institutional consensus",
    dependencies: [],

    install(ctx: RelayContext): void {
      const timestamper = createRelayTimestamper(identity, ctx.relayDid);
      ctx.setCapability(RELAY_CAPABILITIES.TIMESTAMPER, timestamper);
    },
  };
}

function createRelayTimestamper(
  identity: PrismIdentity,
  relayDid: DID,
): RelayTimestamper {
  const encoder = new TextEncoder();

  return {
    async stamp(dataHash: string): Promise<TimestampReceipt> {
      const timestamp = new Date().toISOString();
      const message = encoder.encode(`${dataHash}:${timestamp}`);
      const signature = await identity.signPayload(message);

      return { dataHash, timestamp, relayDid, signature };
    },

    async verify(receipt: TimestampReceipt): Promise<boolean> {
      const message = encoder.encode(`${receipt.dataHash}:${receipt.timestamp}`);
      return identity.verifySignature(message, receipt.signature);
    },
  };
}

// ── Module: Blind Pinger ────────────────────────────────────────────────────

export function blindPingModule(): RelayModule {
  return {
    name: "blind-pings",
    description: "Content-free push notifications to wake offline peers",
    dependencies: [],

    install(ctx: RelayContext): void {
      const pinger = createBlindPinger();
      ctx.setCapability(RELAY_CAPABILITIES.PINGER, pinger);
    },
  };
}

function createBlindPinger(): BlindPinger {
  let transport: PingTransport | null = null;

  return {
    async ping(recipientDid: DID, badgeCount?: number): Promise<boolean> {
      if (!transport) return false;
      const ping: BlindPing = {
        to: recipientDid,
        sentAt: new Date().toISOString(),
        ...(badgeCount !== undefined ? { badgeCount } : {}),
      };
      return transport.send(ping);
    },

    setTransport(t: PingTransport): void {
      transport = t;
    },
  };
}

// ── Module: Capability Tokens ───────────────────────────────────────────────

export function capabilityTokenModule(identity: PrismIdentity): RelayModule {
  return {
    name: "capability-tokens",
    description: "Scoped access tokens for AutoREST and portal access",
    dependencies: [],

    install(ctx: RelayContext): void {
      const manager = createCapabilityTokenManager(identity);
      ctx.setCapability(RELAY_CAPABILITIES.TOKENS, manager);
    },
  };
}

function createCapabilityTokenManager(
  identity: PrismIdentity,
): CapabilityTokenManager {
  const revoked = new Set<string>();
  const encoder = new TextEncoder();

  return {
    async issue(params): Promise<CapabilityToken> {
      const tokenId = uid("cap");
      const issuedAt = new Date().toISOString();
      const expiresAt = params.ttlMs
        ? new Date(Date.now() + params.ttlMs).toISOString()
        : null;

      const payload = `${tokenId}:${identity.did}:${params.subject}:${params.permissions.join(",")}:${params.scope}:${issuedAt}:${expiresAt ?? "null"}`;
      const signature = await identity.signPayload(encoder.encode(payload));

      return {
        tokenId,
        issuer: identity.did,
        subject: params.subject,
        permissions: [...params.permissions],
        scope: params.scope,
        issuedAt,
        expiresAt,
        signature,
      };
    },

    async verify(token: CapabilityToken): Promise<{ valid: boolean; reason?: string }> {
      if (revoked.has(token.tokenId)) {
        return { valid: false, reason: "token revoked" };
      }

      if (token.expiresAt && new Date(token.expiresAt).getTime() < Date.now()) {
        return { valid: false, reason: "token expired" };
      }

      const payload = `${token.tokenId}:${token.issuer}:${token.subject}:${token.permissions.join(",")}:${token.scope}:${token.issuedAt}:${token.expiresAt ?? "null"}`;
      const valid = await identity.verifySignature(
        encoder.encode(payload),
        token.signature,
      );

      if (!valid) {
        return { valid: false, reason: "invalid signature" };
      }

      return { valid: true };
    },

    revoke(tokenId: string): void {
      revoked.add(tokenId);
    },

    isRevoked(tokenId: string): boolean {
      return revoked.has(tokenId);
    },
  };
}

// ── Module: Webhooks ────────────────────────────────────────────────────────

export interface WebhookHttpClient {
  post(url: string, body: string, headers: Record<string, string>): Promise<{ status: number }>;
}

export function webhookModule(httpClient?: WebhookHttpClient): RelayModule {
  return {
    name: "webhooks",
    description: "Outgoing HTTP on CRDT changes (Zapier/Slack integration)",
    dependencies: [],

    install(ctx: RelayContext): void {
      const emitter = createWebhookEmitter(httpClient);
      ctx.setCapability(RELAY_CAPABILITIES.WEBHOOKS, emitter);
    },
  };
}

function createWebhookEmitter(httpClient?: WebhookHttpClient): WebhookEmitter {
  const webhooks = new Map<string, WebhookConfig>();
  const deliveryLog = new Map<string, WebhookDelivery[]>();

  return {
    register(config: Omit<WebhookConfig, "id">): WebhookConfig {
      const id = uid("wh");
      const full: WebhookConfig = { ...config, id };
      webhooks.set(id, full);
      deliveryLog.set(id, []);
      return full;
    },

    unregister(webhookId: string): boolean {
      deliveryLog.delete(webhookId);
      return webhooks.delete(webhookId);
    },

    list(): WebhookConfig[] {
      return [...webhooks.values()];
    },

    async emit(
      event: string,
      data: Record<string, unknown>,
    ): Promise<WebhookDelivery[]> {
      const results: WebhookDelivery[] = [];
      const timestamp = new Date().toISOString();

      for (const wh of webhooks.values()) {
        if (!wh.active) continue;
        if (!wh.events.includes(event) && !wh.events.includes("*")) continue;

        const payload: WebhookPayload = {
          webhookId: wh.id,
          event,
          timestamp,
          data,
        };

        if (wh.secret) {
          payload.signature = await sha256Hex(`${wh.secret}:${JSON.stringify(data)}`);
        }

        let delivery: WebhookDelivery;

        if (httpClient) {
          try {
            const resp = await httpClient.post(
              wh.url,
              JSON.stringify(payload),
              { "Content-Type": "application/json" },
            );
            delivery = {
              webhookId: wh.id,
              event,
              timestamp,
              success: resp.status >= 200 && resp.status < 300,
              statusCode: resp.status,
            };
          } catch (err) {
            delivery = {
              webhookId: wh.id,
              event,
              timestamp,
              success: false,
              error: err instanceof Error ? err.message : String(err),
            };
          }
        } else {
          // No HTTP client — record as successful (testing/dry-run mode)
          delivery = { webhookId: wh.id, event, timestamp, success: true };
        }

        const log = deliveryLog.get(wh.id);
        if (log) log.push(delivery);
        results.push(delivery);
      }

      return results;
    },

    deliveries(webhookId: string): WebhookDelivery[] {
      return deliveryLog.get(webhookId) ?? [];
    },
  };
}

// ── Module: Sovereign Portals ───────────────────────────────────────────────

export function sovereignPortalModule(): RelayModule {
  return {
    name: "sovereign-portals",
    description: "Portal registry for sovereign web presence (Level 1-4)",
    dependencies: [],

    install(ctx: RelayContext): void {
      const registry = createPortalRegistry();
      ctx.setCapability(RELAY_CAPABILITIES.PORTALS, registry);
    },
  };
}

function createPortalRegistry(): PortalRegistry {
  const portals = new Map<string, PortalManifest>();

  return {
    register(
      manifest: Omit<PortalManifest, "portalId" | "createdAt">,
    ): PortalManifest {
      const portalId = uid("portal");
      const full: PortalManifest = {
        ...manifest,
        portalId,
        createdAt: new Date().toISOString(),
      };
      portals.set(portalId, full);
      return full;
    },

    unregister(portalId: string): boolean {
      return portals.delete(portalId);
    },

    get(portalId: string): PortalManifest | undefined {
      return portals.get(portalId);
    },

    list(): PortalManifest[] {
      return [...portals.values()];
    },

    resolve(domain: string, path: string): PortalManifest | undefined {
      for (const portal of portals.values()) {
        if (portal.domain === domain && path.startsWith(portal.basePath)) {
          return portal;
        }
      }
      return undefined;
    },
  };
}

// ── In-memory Ping Transport (for testing) ──────────────────────────────────

export function createMemoryPingTransport(): PingTransport & { sent: BlindPing[] } {
  const sent: BlindPing[] = [];
  return {
    sent,
    async send(ping: BlindPing): Promise<boolean> {
      sent.push(ping);
      return true;
    },
  };
}
