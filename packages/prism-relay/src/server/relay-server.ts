/**
 * Hono app factory for Prism Relay.
 *
 * Wires HTTP routes, WebSocket transport, security middleware, and
 * all feature modules into a single server.
 */

import { Hono } from "hono";
import { createNodeWebSocket } from "@hono/node-ws";
import { serve } from "@hono/node-server";
import type { RelayInstance, FederationRegistry, RelayEnvelope, ForwardResult } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { serializeEnvelope } from "../protocol/relay-protocol.js";
import {
  createStatusRoutes,
  createWebhookRoutes,
  createPortalRoutes,
  createTokenRoutes,
  createCollectionRoutes,
  createHashcashRoutes,
  createTrustRoutes,
  createEscrowRoutes,
  createFederationRoutes,
  createPortalViewRoutes,
  createAcmeRoutes,
  createAcmeManagementRoutes,
  createTemplateRoutes,
  createSeoRoutes,
  createAuthRoutes,
  createSafetyRoutes,
  createAutoRestRoutes,
  createPingRoutes,
  createSignalingRoutes,
  createPresenceRoutes,
} from "../routes/index.js";
import type { AuthRoutesOptions } from "../routes/index.js";
import { handleWsOpen, handleWsMessage, handleWsClose, createConnectionRegistry, createPresenceStore } from "../transport/index.js";
import type { WsConnection } from "../transport/index.js";
import {
  csrfMiddleware,
  bodySizeLimitMiddleware,
  bannedPeerMiddleware,
  rateLimitMiddleware,
} from "../middleware/security.js";

export interface RelayServerOptions {
  relay: RelayInstance;
  port?: number;
  host?: string;
  corsOrigins?: string[];
  /** Public-facing base URL (for WebSocket URLs in portal pages and SEO). */
  publicUrl?: string;
  /** OAuth provider configuration for auth routes. */
  auth?: AuthRoutesOptions;
  /** Maximum request body size in bytes. Default: from relay config or 1MB. */
  maxBodySize?: number;
  /** Disable CSRF protection (for testing). Default: false. */
  disableCsrf?: boolean;
}

export interface RelayServer {
  app: Hono;
  start(): Promise<{ port: number; close(): Promise<void> }>;
}

export function createRelayServer(options: RelayServerOptions): RelayServer {
  const {
    relay,
    port = 4444,
    host = "0.0.0.0",
    corsOrigins,
    publicUrl,
    auth,
    maxBodySize = 1_048_576,
    disableCsrf = false,
  } = options;
  const app = new Hono();

  // ── CORS middleware ────────────────────────────────────────────────────
  if (corsOrigins && corsOrigins.length > 0) {
    app.use("/*", async (c, next) => {
      const origin = c.req.header("origin") ?? "";
      const allowed = corsOrigins.includes("*") || corsOrigins.includes(origin);
      if (allowed) {
        c.header("Access-Control-Allow-Origin", corsOrigins.includes("*") ? "*" : origin);
        c.header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS");
        c.header("Access-Control-Allow-Headers", "Content-Type, Authorization, X-Prism-CSRF, X-Prism-DID");
      }
      if (c.req.method === "OPTIONS") {
        return c.body(null, 204);
      }
      await next();
    });
  }

  // ── Security middleware ──────────────────────────────────────────────
  // Rate limiting (token bucket per IP/DID)
  app.use("/api/*", rateLimitMiddleware());

  // Body size limit on all mutating requests
  app.use("/api/*", bodySizeLimitMiddleware(maxBodySize));

  // CSRF protection on API routes (custom header requirement)
  if (!disableCsrf) {
    app.use("/api/*", csrfMiddleware());
  }

  // Banned peer rejection
  app.use("/api/*", bannedPeerMiddleware(relay));

  const { injectWebSocket, upgradeWebSocket } = createNodeWebSocket({ app });
  const connectionRegistry = createConnectionRegistry();
  const presenceStore = createPresenceStore();

  // ── HTTP routes ──────────────────────────────────────────────────────
  app.route("/api", createStatusRoutes(relay));
  app.route("/api/webhooks", createWebhookRoutes(relay));
  app.route("/api/portals", createPortalRoutes(relay));
  app.route("/api/tokens", createTokenRoutes(relay));
  app.route("/api/collections", createCollectionRoutes(relay));
  app.route("/api/hashcash", createHashcashRoutes(relay));
  app.route("/api/trust", createTrustRoutes(relay));
  app.route("/api/escrow", createEscrowRoutes(relay));
  app.route("/api/federation", createFederationRoutes(relay));
  app.route("/api/acme", createAcmeManagementRoutes(relay));
  app.route("/api/templates", createTemplateRoutes(relay));

  // ── New feature routes ──────────────────────────────────────────────
  app.route("/api/auth", createAuthRoutes(relay, auth));
  app.route("/api/safety", createSafetyRoutes(relay));
  app.route("/api/rest", createAutoRestRoutes(relay));
  app.route("/api/pings", createPingRoutes(relay));
  app.route("/api/signaling", createSignalingRoutes(relay));
  app.route("/api/presence", createPresenceRoutes(presenceStore));

  // ── ACME HTTP-01 challenge response (Let's Encrypt) ─────────────────
  app.route("/.well-known/acme-challenge", createAcmeRoutes(relay));

  // ── SEO routes (sitemap.xml, robots.txt) ────────────────────────────
  app.route("", createSeoRoutes(relay, publicUrl));

  // ── Portal view (HTML rendering) ────────────────────────────────────
  const wsBaseUrl = publicUrl
    ? publicUrl.replace(/^http/, "ws")
    : undefined;
  app.route("/portals", createPortalViewRoutes(relay, wsBaseUrl));

  // ── Federation transport ─────────────────────────────────────────────
  const federation = relay.getCapability<FederationRegistry>(RELAY_CAPABILITIES.FEDERATION);
  if (federation) {
    federation.setTransport(async (envelope: RelayEnvelope, peerUrl: string): Promise<ForwardResult> => {
      try {
        const res = await fetch(`${peerUrl}/api/federation/forward`, {
          method: "POST",
          headers: { "Content-Type": "application/json", "X-Prism-CSRF": "1" },
          body: JSON.stringify({
            envelope: serializeEnvelope(envelope),
            targetRelay: relay.did,
          }),
        });
        if (res.ok) {
          return await res.json() as ForwardResult;
        }
        return { status: "error", message: `Peer returned ${res.status}` };
      } catch (e) {
        return { status: "error", message: String(e) };
      }
    });
  }

  // ── WebSocket ────────────────────────────────────────────────────────
  app.get(
    "/ws/relay",
    upgradeWebSocket(() => {
      const conn: WsConnection = { did: null };
      return {
        onOpen(_evt, ws) {
          connectionRegistry.add(ws);
          handleWsOpen(ws, conn);
        },
        onMessage(evt, ws) {
          const data = typeof evt.data === "string" ? evt.data : String(evt.data);
          handleWsMessage(ws, conn, relay, data, connectionRegistry, presenceStore);
        },
        onClose(_evt, ws) {
          connectionRegistry.remove(ws);
          handleWsClose(conn, relay, connectionRegistry, presenceStore);
        },
      };
    }),
  );

  return {
    app,
    start() {
      return new Promise<{ port: number; close(): Promise<void> }>((resolve) => {
        const server = serve({ fetch: app.fetch, port, hostname: host }, (info) => {
          injectWebSocket(server);
          resolve({
            port: info.port,
            close() {
              return new Promise<void>((done) => {
                server.close(() => done());
              });
            },
          });
        });
      });
    },
  };
}
