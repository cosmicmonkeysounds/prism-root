import { Hono } from "hono";
import type {
  RelayInstance,
  PortalRegistry,
  PortalManifest,
  PortalLevel,
  WebhookEmitter,
  WebhookConfig,
  CollectionHost,
  AcmeCertificateManager,
  SslCertificate,
  PortalTemplateRegistry,
  FederationRegistry,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";
import type { PeerTrustGraph } from "@prism/core/trust";
import { encodeBase64 } from "../protocol/relay-protocol.js";

export function createBackupRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  app.get("/", (c) => {
    const portals =
      relay.getCapability<PortalRegistry>(RELAY_CAPABILITIES.PORTALS)?.list() ??
      [];
    const webhooks =
      relay
        .getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS)
        ?.list() ?? [];
    const templates =
      relay
        .getCapability<PortalTemplateRegistry>(RELAY_CAPABILITIES.TEMPLATES)
        ?.list() ?? [];
    const certificates =
      relay
        .getCapability<AcmeCertificateManager>(RELAY_CAPABILITIES.ACME)
        ?.listCertificates() ?? [];
    const federationPeers =
      relay
        .getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION)
        ?.getPeers() ?? [];
    const flaggedHashes = [
      ...(relay
        .getCapability<PeerTrustGraph>(RELAY_CAPABILITIES.TRUST)
        ?.flaggedContent() ?? []),
    ];

    const collections: Record<string, string> = {};
    const collectionHost = relay.getCapability<CollectionHost>(
      RELAY_CAPABILITIES.COLLECTIONS,
    );
    if (collectionHost) {
      for (const id of collectionHost.list()) {
        const store = collectionHost.get(id);
        if (store) {
          try {
            const snapshot = store.exportSnapshot();
            collections[id] = encodeBase64(snapshot);
          } catch {
            // Skip unserializable collections
          }
        }
      }
    }

    return c.json({
      portals,
      webhooks,
      templates,
      certificates,
      federationPeers,
      flaggedHashes,
      collections,
    });
  });

  app.post("/", async (c) => {
    const body = await c.req.json<{
      portals?: PortalManifest[];
      webhooks?: WebhookConfig[];
      templates?: Array<{ name: string; description: string; css: string; headerHtml: string; footerHtml: string; objectCardHtml: string }>;
      certificates?: SslCertificate[];
      federationPeers?: Array<{ relayDid: DID; url: string }>;
      flaggedHashes?: Array<{ hash: string; category: string; reportedBy: string }>;
      collections?: Record<string, string>;
    }>();

    const restored = {
      portals: 0,
      webhooks: 0,
      templates: 0,
      certificates: 0,
      federationPeers: 0,
      flaggedHashes: 0,
      collections: 0,
    };

    // Restore portals
    const portals = relay.getCapability<PortalRegistry>(
      RELAY_CAPABILITIES.PORTALS,
    );
    if (portals && body.portals) {
      for (const p of body.portals) {
        const reg: Omit<PortalManifest, "portalId" | "createdAt"> = {
          name: p.name,
          level: p.level as PortalLevel,
          collectionId: p.collectionId,
          basePath: p.basePath,
          isPublic: p.isPublic,
        };
        if (p.domain !== undefined) reg.domain = p.domain;
        if (p.accessScope !== undefined) reg.accessScope = p.accessScope;
        portals.register(reg);
        restored.portals++;
      }
    }

    // Restore webhooks
    const webhooks = relay.getCapability<WebhookEmitter>(
      RELAY_CAPABILITIES.WEBHOOKS,
    );
    if (webhooks && body.webhooks) {
      for (const w of body.webhooks) {
        const reg: Omit<WebhookConfig, "id"> = {
          url: w.url,
          events: w.events,
          active: w.active,
        };
        if (w.secret !== undefined) reg.secret = w.secret;
        webhooks.register(reg);
        restored.webhooks++;
      }
    }

    // Restore templates
    const templates = relay.getCapability<PortalTemplateRegistry>(
      RELAY_CAPABILITIES.TEMPLATES,
    );
    if (templates && body.templates) {
      for (const t of body.templates) {
        templates.register({
          name: t.name,
          description: t.description,
          css: t.css,
          headerHtml: t.headerHtml,
          footerHtml: t.footerHtml,
          objectCardHtml: t.objectCardHtml,
        });
        restored.templates++;
      }
    }

    // Restore certificates
    const acme = relay.getCapability<AcmeCertificateManager>(
      RELAY_CAPABILITIES.ACME,
    );
    if (acme && body.certificates) {
      for (const cert of body.certificates) {
        acme.setCertificate(cert);
        restored.certificates++;
      }
    }

    // Restore federation peers
    const federation = relay.getCapability<FederationRegistry>(
      RELAY_CAPABILITIES.FEDERATION,
    );
    if (federation && body.federationPeers) {
      for (const peer of body.federationPeers) {
        federation.announce(peer.relayDid, peer.url);
        restored.federationPeers++;
      }
    }

    // Restore flagged content
    const trust = relay.getCapability<PeerTrustGraph>(
      RELAY_CAPABILITIES.TRUST,
    );
    if (trust && body.flaggedHashes) {
      for (const f of body.flaggedHashes) {
        trust.flagContent(f.hash, f.category, f.reportedBy);
        restored.flaggedHashes++;
      }
    }

    // Restore collections
    const collectionHost = relay.getCapability<CollectionHost>(
      RELAY_CAPABILITIES.COLLECTIONS,
    );
    if (collectionHost && body.collections) {
      for (const [id, snapshotBase64] of Object.entries(body.collections)) {
        const store = collectionHost.create(id);
        try {
          const binary = atob(snapshotBase64);
          const bytes = new Uint8Array(binary.length);
          for (let i = 0; i < binary.length; i++) {
            bytes[i] = binary.charCodeAt(i);
          }
          store.import(bytes);
          restored.collections++;
        } catch {
          // Skip corrupted snapshots
        }
      }
    }

    return c.json({ ok: true, restored });
  });

  return app;
}
