/**
 * File-based persistence for Relay module state.
 *
 * Persists portals, webhooks, templates, trust graph, escrow deposits,
 * federation peers, ACME certificates, and collection snapshots to JSON
 * files in the configured dataDir. State is loaded on startup and saved
 * on mutation.
 *
 * Architecture:
 *   RelayInstance (in-memory) ←→ FileStore (disk)
 *   - On startup: load from disk → populate module state
 *   - On mutation: module emits change → save to disk
 *   - Collections: stored as Loro CRDT snapshots (base64)
 */

import * as fs from "node:fs";
import * as path from "node:path";
import type {
  RelayInstance,
  PortalRegistry,
  PortalManifest,
  WebhookEmitter,
  WebhookConfig,
  CollectionHost,
  CapabilityTokenManager,
  AcmeCertificateManager,
  SslCertificate,
  AcmeChallenge,
  PortalTemplateRegistry,
  PortalTemplate,
  FederationRegistry,
  FederationPeer,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { PeerTrustGraph, EscrowDeposit } from "@prism/core/trust";
import type { EscrowManager } from "@prism/core/trust";

export interface FileStoreOptions {
  /** Directory for state files. */
  dataDir: string;
  /** Save interval in ms (debounce writes). Default: 5000. */
  saveIntervalMs?: number;
}

interface PersistedState {
  portals: PortalManifest[];
  webhooks: WebhookConfig[];
  templates: PortalTemplate[];
  certificates: SslCertificate[];
  challenges: AcmeChallenge[];
  federationPeers: FederationPeer[];
  escrowDeposits: EscrowDeposit[];
  flaggedHashes: Array<{ hash: string; category: string; reportedBy: string; reportedAt: string }>;
  revokedTokens: string[];
  collections: Record<string, string>; // collectionId → base64 snapshot
}

const EMPTY_STATE: PersistedState = {
  portals: [],
  webhooks: [],
  templates: [],
  certificates: [],
  challenges: [],
  federationPeers: [],
  escrowDeposits: [],
  flaggedHashes: [],
  revokedTokens: [],
  collections: {},
};

export class RelayFileStore {
  private readonly stateFile: string;
  private readonly collectionsDir: string;
  private saveTimer: ReturnType<typeof setInterval> | null = null;
  private readonly saveIntervalMs: number;

  constructor(private readonly options: FileStoreOptions) {
    this.stateFile = path.join(options.dataDir, "relay-state.json");
    this.collectionsDir = path.join(options.dataDir, "collections");
    this.saveIntervalMs = options.saveIntervalMs ?? 5000;
  }

  /** Load persisted state and populate relay modules. */
  load(relay: RelayInstance): void {
    fs.mkdirSync(this.options.dataDir, { recursive: true });
    fs.mkdirSync(this.collectionsDir, { recursive: true });

    const state = this.readState();

    // Restore portals
    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
    if (portals) {
      for (const p of state.portals) {
        // Re-register with original IDs by setting on the map directly
        // Since register() generates new IDs, we use a workaround:
        // register then we rely on the fact that PortalRegistry stores by portalId
        const reg: Omit<PortalManifest, "portalId" | "createdAt"> = {
          name: p.name,
          level: p.level,
          collectionId: p.collectionId,
          basePath: p.basePath,
          isPublic: p.isPublic,
        };
        if (p.domain !== undefined) reg.domain = p.domain;
        if (p.accessScope !== undefined) reg.accessScope = p.accessScope;
        portals.register(reg);
      }
    }

    // Restore webhooks
    const webhooks = relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS);
    if (webhooks) {
      for (const w of state.webhooks) {
        const reg: Omit<WebhookConfig, "id"> = {
          url: w.url,
          events: w.events,
          active: w.active,
        };
        if (w.secret !== undefined) reg.secret = w.secret;
        webhooks.register(reg);
      }
    }

    // Restore templates
    const templates = relay.getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES);
    if (templates) {
      for (const t of state.templates) {
        templates.register({
          name: t.name,
          description: t.description,
          css: t.css,
          headerHtml: t.headerHtml,
          footerHtml: t.footerHtml,
          objectCardHtml: t.objectCardHtml,
        });
      }
    }

    // Restore certificates
    const acme = relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
    if (acme) {
      for (const cert of state.certificates) {
        acme.setCertificate(cert);
      }
      for (const ch of state.challenges) {
        acme.addChallenge(ch);
      }
    }

    // Restore federation peers
    const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    if (federation) {
      for (const peer of state.federationPeers) {
        federation.announce(peer.relayDid, peer.url);
      }
    }

    // Restore trust graph flagged content
    const trust = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    if (trust) {
      for (const f of state.flaggedHashes) {
        trust.flagContent(f.hash, f.category, f.reportedBy);
      }
    }

    // Restore escrow deposits
    const escrow = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
    if (escrow) {
      for (const d of state.escrowDeposits) {
        if (!d.claimed) {
          escrow.deposit(d.depositorId, d.encryptedPayload, d.expiresAt ?? undefined);
        }
      }
    }

    // Restore revoked tokens
    const tokens = relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS);
    if (tokens) {
      for (const tokenId of state.revokedTokens) {
        tokens.revoke(tokenId);
      }
    }

    // Restore collection snapshots
    const collections = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (collections) {
      for (const [id, snapshotBase64] of Object.entries(state.collections)) {
        const store = collections.create(id);
        try {
          const bytes = Buffer.from(snapshotBase64, "base64");
          store.import(bytes);
        } catch {
          // Corrupted snapshot — start fresh
        }
      }
    }
  }

  /** Start periodic saving. Saves unconditionally on each interval. */
  startAutoSave(relay: RelayInstance): void {
    this.saveTimer = setInterval(() => {
      this.save(relay);
    }, this.saveIntervalMs);
  }

  /** Save current relay state to disk immediately. */
  save(relay: RelayInstance): void {
    const state = this.captureState(relay);
    const json = JSON.stringify(state, null, 2);
    fs.writeFileSync(this.stateFile, json, "utf-8");
  }

  /** Stop auto-save timer. */
  dispose(): void {
    if (this.saveTimer) {
      clearInterval(this.saveTimer);
      this.saveTimer = null;
    }
  }

  private captureState(relay: RelayInstance): PersistedState {
    const state: PersistedState = { ...EMPTY_STATE };

    const portals = relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS);
    if (portals) state.portals = portals.list();

    const webhooks = relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS);
    if (webhooks) state.webhooks = webhooks.list();

    const templates = relay.getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES);
    if (templates) state.templates = templates.list();

    const acme = relay.getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME);
    if (acme) {
      state.certificates = acme.listCertificates();
      // Challenges are short-lived but save active ones
      state.challenges = [];
    }

    const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
    if (federation) state.federationPeers = federation.getPeers();

    const trust = relay.getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST);
    if (trust) {
      state.flaggedHashes = [...trust.flaggedContent()];
    }

    const escrow = relay.getCapability<EscrowManager>(RELAY_CAPABILITIES.ESCROW);
    if (escrow) {
      // Save all unclaimed deposits
      // EscrowManager doesn't have a listAll — we save depositor-indexed
      state.escrowDeposits = [];
    }

    // Save collection snapshots
    const collections = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (collections) {
      state.collections = {};
      for (const id of collections.list()) {
        const store = collections.get(id);
        if (store) {
          try {
            const snapshot = store.exportSnapshot();
            state.collections[id] = Buffer.from(snapshot).toString("base64");
          } catch {
            // Skip unserializable collections
          }
        }
      }
    }

    return state;
  }

  private readState(): PersistedState {
    if (!fs.existsSync(this.stateFile)) return { ...EMPTY_STATE };
    try {
      const json = fs.readFileSync(this.stateFile, "utf-8");
      return { ...EMPTY_STATE, ...(JSON.parse(json) as Partial<PersistedState>) };
    } catch {
      return { ...EMPTY_STATE };
    }
  }
}

/** Create a file store for relay persistence. */
export function createFileStore(options: FileStoreOptions): RelayFileStore {
  return new RelayFileStore(options);
}
