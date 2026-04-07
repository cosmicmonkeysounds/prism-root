/**
 * Relay Manager — manages connections to Prism Relays from Studio.
 *
 * Studio is a client-only SPA. It does NOT run any server code.
 * Instead, it connects to deployed Relay servers via:
 *   - WebSocket (RelayClient SDK) for real-time CRDT sync + envelopes
 *   - HTTP fetch for REST API (portals, status, tokens, collections)
 *
 * Relay servers are managed via CLI (`prism-relay --mode server`).
 * Studio only connects to, monitors, and publishes content through them.
 */

import type { PortalManifest, PortalLevel, RelayClient } from "@prism/core/relay";
import { createRelayClient } from "@prism/core/relay";
import type { PrismIdentity } from "@prism/core/identity";
import type { CollectionStore } from "@prism/core/persistence";

// ── Types ─────────────────────────────────────────────────────────────────

export type RelayConnectionStatus =
  | "disconnected"
  | "connecting"
  | "connected"
  | "error";

/** A configured relay endpoint that Studio can connect to. */
export interface RelayEntry {
  /** Unique local ID for this relay config. */
  id: string;
  /** Human-readable name (e.g. "Production", "Staging"). */
  name: string;
  /** HTTP base URL (e.g. "https://relay.example.com"). */
  url: string;
  /** Current connection status. */
  status: RelayConnectionStatus;
  /** Error message if status is "error". */
  error: string | null;
  /** Modules available on this relay (populated after connect). */
  modules: string[];
  /** DID of the relay (populated after connect). */
  relayDid: string | null;
}

/** Result of fetching relay status via HTTP. */
export interface RelayStatus {
  did: string;
  modules: string[];
  uptime: number;
  connections: number;
  mode: string;
}

/** Options for publishing a collection as a portal. */
export interface PublishPortalOptions {
  /** Relay entry ID to publish to. */
  relayId: string;
  /** Collection to expose. */
  collectionId: string;
  /** Display name for the portal. */
  name: string;
  /** Portal level (1=static, 2=live, 3=interactive, 4=app). */
  level: PortalLevel;
  /** Optional custom domain. */
  domain?: string;
  /** Path prefix (default: "/"). */
  basePath?: string;
  /** Whether publicly accessible. Default: true. */
  isPublic?: boolean;
}

/** Portal deployed on a specific relay. */
export interface DeployedPortal {
  /** The portal manifest from the relay. */
  manifest: PortalManifest;
  /** Which relay this portal lives on. */
  relayId: string;
  /** Full URL to view the portal. */
  viewUrl: string;
}

type Listener = () => void;

/** Manages Studio's connections to Prism Relay servers. */
export interface RelayManager {
  // ── Relay connections ──────────────────────────────────────────────────
  /** Add a relay endpoint configuration. */
  addRelay(name: string, url: string): RelayEntry;
  /** Remove a relay configuration (disconnects first). */
  removeRelay(id: string): boolean;
  /** Get all configured relays. */
  listRelays(): RelayEntry[];
  /** Get a specific relay by ID. */
  getRelay(id: string): RelayEntry | undefined;

  /** Connect to a relay via WebSocket. */
  connect(relayId: string, identity: PrismIdentity): Promise<void>;
  /** Disconnect from a relay. */
  disconnect(relayId: string): void;

  // ── Portal management (HTTP API) ───────────────────────────────────────
  /** Publish a collection as a portal on a relay. */
  publishPortal(options: PublishPortalOptions): Promise<DeployedPortal>;
  /** Unpublish (remove) a portal from a relay. */
  unpublishPortal(relayId: string, portalId: string): Promise<boolean>;
  /** List portals on a relay. */
  listPortals(relayId: string): Promise<DeployedPortal[]>;
  /** Fetch relay status via HTTP. */
  fetchStatus(relayId: string): Promise<RelayStatus>;

  // ── Collections management ─────────────────────────────────────────────
  /** List hosted collections on a relay. */
  listCollections(relayId: string): Promise<string[]>;
  /** Get collection metadata. */
  inspectCollection(relayId: string, collectionId: string): Promise<{ snapshot: string }>;
  /** Delete a collection from a relay. */
  deleteCollection(relayId: string, collectionId: string): Promise<boolean>;

  // ── Collection sync ────────────────────────────────────────────────────
  /** Push a collection snapshot to a connected relay. */
  syncCollection(relayId: string, collectionId: string, store: CollectionStore): Promise<void>;

  // ── Webhooks management ───────────────────────────────────────────────
  /** List webhooks on a relay. */
  listWebhooks(relayId: string): Promise<Array<{ id: string; url: string; events: string[]; active: boolean }>>;
  /** Delete a webhook. */
  deleteWebhook(relayId: string, webhookId: string): Promise<boolean>;

  // ── Federation/peers ──────────────────────────────────────────────────
  /** List federation peers. */
  listPeers(relayId: string): Promise<Array<{ relayDid: string; url: string }>>;
  /** Ban a peer. */
  banPeer(relayId: string, did: string): Promise<boolean>;
  /** Unban a peer. */
  unbanPeer(relayId: string, did: string): Promise<boolean>;
  /** Get trust graph. */
  getTrustGraph(relayId: string): Promise<unknown[]>;

  // ── Certificates ──────────────────────────────────────────────────────
  /** List ACME certificates. */
  listCertificates(relayId: string): Promise<Array<{ domain: string; expiresAt: string; issuedAt: string }>>;

  // ── Backup/restore ────────────────────────────────────────────────────
  /** Export relay state as JSON. */
  backupRelay(relayId: string): Promise<Record<string, unknown>>;
  /** Import relay state from JSON. */
  restoreRelay(relayId: string, data: Record<string, unknown>): Promise<{ restored: Record<string, number> }>;

  // ── Health ────────────────────────────────────────────────────────────
  /** Fetch relay health (uptime, memory, connections). */
  fetchHealth(relayId: string): Promise<Record<string, unknown>>;

  // ── Discovery ─────────────────────────────────────────────────────────
  /** Probe a URL to check if it's a Prism Relay. Returns relay info or null. */
  discoverRelay(url: string): Promise<{ did: string; modules: string[]; mode: string } | null>;

  // ── Subscriptions ──────────────────────────────────────────────────────
  /** Subscribe to relay state changes. Returns unsubscribe function. */
  subscribe(listener: Listener): () => void;

  /** Dispose all connections and state. */
  dispose(): void;
}

// ── HTTP client interface (injectable for testing) ───────────────────────

export interface RelayHttpClient {
  fetch(url: string, init?: RequestInit): Promise<Response>;
}

const defaultHttpClient: RelayHttpClient = {
  fetch: (url, init) => globalThis.fetch(url, init),
};

// ── Factory options ──────────────────────────────────────────────────────

export interface RelayManagerOptions {
  /** Injectable HTTP client (defaults to global fetch). */
  httpClient?: RelayHttpClient;
  /** Injectable WebSocket factory (for testing). */
  createWsClient?: typeof createRelayClient;
}

// ── Implementation ───────────────────────────────────────────────────────

let idCounter = 0;
function genRelayId(): string {
  return `relay_${Date.now().toString(36)}_${(idCounter++).toString(36)}`;
}

export function createRelayManager(options?: RelayManagerOptions): RelayManager {
  const http = options?.httpClient ?? defaultHttpClient;
  const makeWsClient = options?.createWsClient ?? createRelayClient;

  const relays = new Map<string, RelayEntry>();
  const clients = new Map<string, RelayClient>();
  const listeners = new Set<Listener>();

  function notify(): void {
    for (const fn of listeners) fn();
  }

  function updateEntry(id: string, patch: Partial<RelayEntry>): void {
    const entry = relays.get(id);
    if (!entry) return;
    Object.assign(entry, patch);
    notify();
  }

  // ── Normalize URL ───────────────────────────────────────────────────────

  function normalizeUrl(url: string): string {
    return url.replace(/\/+$/, "");
  }

  function wsUrl(baseUrl: string): string {
    const base = normalizeUrl(baseUrl);
    const protocol = base.startsWith("https") ? "wss" : "ws";
    const host = base.replace(/^https?:\/\//, "");
    return `${protocol}://${host}/ws/relay`;
  }

  // ── Relay connections ──────────────────────────────────────────────────

  function addRelay(name: string, url: string): RelayEntry {
    const entry: RelayEntry = {
      id: genRelayId(),
      name,
      url: normalizeUrl(url),
      status: "disconnected",
      error: null,
      modules: [],
      relayDid: null,
    };
    relays.set(entry.id, entry);
    notify();
    return entry;
  }

  function removeRelay(id: string): boolean {
    const entry = relays.get(id);
    if (!entry) return false;
    disconnect(id);
    relays.delete(id);
    notify();
    return true;
  }

  function listRelays(): RelayEntry[] {
    return [...relays.values()];
  }

  function getRelay(id: string): RelayEntry | undefined {
    const entry = relays.get(id);
    return entry ? { ...entry } : undefined;
  }

  async function connect(relayId: string, identity: PrismIdentity): Promise<void> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);

    // Disconnect existing client if any
    disconnect(relayId);

    updateEntry(relayId, { status: "connecting", error: null });

    const client = makeWsClient({
      url: wsUrl(entry.url),
      identity,
      autoReconnect: true,
      reconnectDelayMs: 3000,
      maxReconnectAttempts: 5,
    });

    clients.set(relayId, client);

    client.on("connected", ({ relayDid, modules }) => {
      updateEntry(relayId, {
        status: "connected",
        relayDid,
        modules,
        error: null,
      });
    });

    client.on("disconnected", ({ reason }) => {
      updateEntry(relayId, {
        status: "disconnected",
        error: reason === "closed" ? null : reason,
      });
    });

    client.on("error", ({ message }) => {
      updateEntry(relayId, { status: "error", error: message });
    });

    client.on("state-change", ({ to }) => {
      if (to === "reconnecting") {
        updateEntry(relayId, { status: "connecting", error: "Reconnecting..." });
      }
    });

    try {
      await client.connect();
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      updateEntry(relayId, { status: "error", error: message });
      clients.delete(relayId);
      throw err;
    }
  }

  function disconnect(relayId: string): void {
    const client = clients.get(relayId);
    if (client) {
      client.close();
      clients.delete(relayId);
    }
    updateEntry(relayId, {
      status: "disconnected",
      error: null,
      modules: [],
      relayDid: null,
    });
  }

  // ── Portal management (HTTP) ──────────────────────────────────────────

  async function publishPortal(opts: PublishPortalOptions): Promise<DeployedPortal> {
    const entry = relays.get(opts.relayId);
    if (!entry) throw new Error(`Unknown relay: ${opts.relayId}`);

    const body = {
      name: opts.name,
      level: opts.level,
      collectionId: opts.collectionId,
      basePath: opts.basePath ?? "/",
      isPublic: opts.isPublic ?? true,
      ...(opts.domain ? { domain: opts.domain } : {}),
    };

    const res = await http.fetch(`${entry.url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const text = await res.text();
      throw new Error(`Failed to publish portal: ${res.status} ${text}`);
    }

    const manifest = (await res.json()) as PortalManifest;
    return {
      manifest,
      relayId: opts.relayId,
      viewUrl: `${entry.url}/portals/${manifest.portalId}`,
    };
  }

  async function unpublishPortal(relayId: string, portalId: string): Promise<boolean> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);

    const res = await http.fetch(`${entry.url}/api/portals/${portalId}`, {
      method: "DELETE",
    });

    return res.ok;
  }

  async function listPortals(relayId: string): Promise<DeployedPortal[]> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);

    const res = await http.fetch(`${entry.url}/api/portals`);
    if (!res.ok) {
      throw new Error(`Failed to list portals: ${res.status}`);
    }

    const manifests = (await res.json()) as PortalManifest[];
    return manifests.map((manifest) => ({
      manifest,
      relayId,
      viewUrl: `${entry.url}/portals/${manifest.portalId}`,
    }));
  }

  async function fetchStatus(relayId: string): Promise<RelayStatus> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);

    const res = await http.fetch(`${entry.url}/api/status`);
    if (!res.ok) {
      throw new Error(`Failed to fetch status: ${res.status}`);
    }

    return (await res.json()) as RelayStatus;
  }

  // ── Collections management ─────────────────────────────────────────────

  async function listCollections(relayId: string): Promise<string[]> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/collections`);
    if (!res.ok) throw new Error(`Failed to list collections: ${res.status}`);
    return (await res.json()) as string[];
  }

  async function inspectCollection(relayId: string, collectionId: string): Promise<{ snapshot: string }> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/collections/${encodeURIComponent(collectionId)}/snapshot`);
    if (!res.ok) throw new Error(`Collection not found: ${collectionId}`);
    return (await res.json()) as { snapshot: string };
  }

  async function deleteCollection(relayId: string, collectionId: string): Promise<boolean> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/collections/${encodeURIComponent(collectionId)}`, { method: "DELETE" });
    return res.ok;
  }

  // ── Collection sync ────────────────────────────────────────────────────

  async function syncCollection(
    relayId: string,
    collectionId: string,
    store: CollectionStore,
  ): Promise<void> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);

    // First, ensure the collection exists on the relay
    await http.fetch(`${entry.url}/api/collections`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: collectionId }),
    });

    // Export the local CRDT snapshot and push via HTTP
    const snapshot = store.exportSnapshot();
    await http.fetch(`${entry.url}/api/collections/${collectionId}/import`, {
      method: "POST",
      headers: { "Content-Type": "application/octet-stream" },
      body: snapshot as unknown as BodyInit,
    });

    // If we have a live WS connection, also subscribe for future updates
    const client = clients.get(relayId);
    if (client && client.state === "connected") {
      client.syncUpdate(collectionId, snapshot);
    }
  }

  // ── Webhooks management ────────────────────────────────────────────────

  async function listWebhooks(relayId: string): Promise<Array<{ id: string; url: string; events: string[]; active: boolean }>> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/webhooks`);
    if (!res.ok) throw new Error(`Failed to list webhooks: ${res.status}`);
    return (await res.json()) as Array<{ id: string; url: string; events: string[]; active: boolean }>;
  }

  async function deleteWebhook(relayId: string, webhookId: string): Promise<boolean> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/webhooks/${encodeURIComponent(webhookId)}`, { method: "DELETE" });
    return res.ok;
  }

  // ── Federation/peers ──────────────────────────────────────────────────

  async function listPeers(relayId: string): Promise<Array<{ relayDid: string; url: string }>> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/federation/peers`);
    if (!res.ok) throw new Error(`Failed to list peers: ${res.status}`);
    return (await res.json()) as Array<{ relayDid: string; url: string }>;
  }

  async function banPeer(relayId: string, did: string): Promise<boolean> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/federation/peers/${encodeURIComponent(did)}/ban`, { method: "POST" });
    return res.ok;
  }

  async function unbanPeer(relayId: string, did: string): Promise<boolean> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/federation/peers/${encodeURIComponent(did)}/unban`, { method: "POST" });
    return res.ok;
  }

  async function getTrustGraph(relayId: string): Promise<unknown[]> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/federation/trust-graph`);
    if (!res.ok) throw new Error(`Failed to get trust graph: ${res.status}`);
    return (await res.json()) as unknown[];
  }

  // ── Certificates ──────────────────────────────────────────────────────

  async function listCertificates(relayId: string): Promise<Array<{ domain: string; expiresAt: string; issuedAt: string }>> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/certificates`);
    if (!res.ok) throw new Error(`Failed to list certificates: ${res.status}`);
    return (await res.json()) as Array<{ domain: string; expiresAt: string; issuedAt: string }>;
  }

  // ── Backup/restore ────────────────────────────────────────────────────

  async function backupRelay(relayId: string): Promise<Record<string, unknown>> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/backup`);
    if (!res.ok) throw new Error(`Failed to backup relay: ${res.status}`);
    return (await res.json()) as Record<string, unknown>;
  }

  async function restoreRelay(relayId: string, data: Record<string, unknown>): Promise<{ restored: Record<string, number> }> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/restore`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    });
    if (!res.ok) throw new Error(`Failed to restore relay: ${res.status}`);
    return (await res.json()) as { restored: Record<string, number> };
  }

  // ── Health ────────────────────────────────────────────────────────────

  async function fetchHealth(relayId: string): Promise<Record<string, unknown>> {
    const entry = relays.get(relayId);
    if (!entry) throw new Error(`Unknown relay: ${relayId}`);
    const res = await http.fetch(`${entry.url}/api/health`);
    if (!res.ok) throw new Error(`Failed to fetch health: ${res.status}`);
    return (await res.json()) as Record<string, unknown>;
  }

  // ── Discovery ─────────────────────────────────────────────────────────

  async function discoverRelay(url: string): Promise<{ did: string; modules: string[]; mode: string } | null> {
    try {
      const base = url.replace(/\/+$/, "");
      const res = await http.fetch(`${base}/api/health`);
      if (!res.ok) return null;
      const data = (await res.json()) as Record<string, unknown>;
      if (!data["did"]) return null;
      return {
        did: data["did"] as string,
        modules: (data["modules"] as string[]) ?? [],
        mode: (data["mode"] as string) ?? "unknown",
      };
    } catch {
      return null;
    }
  }

  // ── Subscriptions ──────────────────────────────────────────────────────

  function subscribe(listener: Listener): () => void {
    listeners.add(listener);
    return () => listeners.delete(listener);
  }

  // ── Dispose ────────────────────────────────────────────────────────────

  function dispose(): void {
    for (const [id] of clients) {
      disconnect(id);
    }
    relays.clear();
    listeners.clear();
  }

  return {
    addRelay,
    removeRelay,
    listRelays,
    getRelay,
    connect,
    disconnect,
    publishPortal,
    unpublishPortal,
    listPortals,
    fetchStatus,
    listCollections,
    inspectCollection,
    deleteCollection,
    syncCollection,
    listWebhooks,
    deleteWebhook,
    listPeers,
    banPeer,
    unbanPeer,
    getTrustGraph,
    listCertificates,
    backupRelay,
    restoreRelay,
    fetchHealth,
    discoverRelay,
    subscribe,
    dispose,
  };
}
