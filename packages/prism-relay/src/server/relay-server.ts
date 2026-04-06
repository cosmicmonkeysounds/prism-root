/**
 * Hono app factory for Prism Relay.
 *
 * Wires HTTP routes and WebSocket transport into a single server.
 */

import { Hono } from "hono";
import { createNodeWebSocket } from "@hono/node-ws";
import { serve } from "@hono/node-server";
import type { RelayInstance } from "@prism/core/relay";
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
} from "../routes/index.js";
import { handleWsOpen, handleWsMessage, handleWsClose, createConnectionRegistry } from "../transport/index.js";
import type { WsConnection } from "../transport/index.js";

export interface RelayServerOptions {
  relay: RelayInstance;
  port?: number;
  host?: string;
  corsOrigins?: string[];
}

export interface RelayServer {
  app: Hono;
  start(): Promise<{ port: number; close(): Promise<void> }>;
}

export function createRelayServer(options: RelayServerOptions): RelayServer {
  const { relay, port = 4444, host = "0.0.0.0", corsOrigins } = options;
  const app = new Hono();

  // ── CORS middleware ────────────────────────────────────────────────────
  if (corsOrigins && corsOrigins.length > 0) {
    app.use("/*", async (c, next) => {
      const origin = c.req.header("origin") ?? "";
      const allowed = corsOrigins.includes("*") || corsOrigins.includes(origin);
      if (allowed) {
        c.header("Access-Control-Allow-Origin", corsOrigins.includes("*") ? "*" : origin);
        c.header("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS");
        c.header("Access-Control-Allow-Headers", "Content-Type, Authorization");
      }
      if (c.req.method === "OPTIONS") {
        return c.body(null, 204);
      }
      await next();
    });
  }

  const { injectWebSocket, upgradeWebSocket } = createNodeWebSocket({ app });
  const connectionRegistry = createConnectionRegistry();

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
          handleWsMessage(ws, conn, relay, data, connectionRegistry);
        },
        onClose(_evt, ws) {
          connectionRegistry.remove(ws);
          handleWsClose(conn, relay);
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
