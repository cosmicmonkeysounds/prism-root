/**
 * Presence routes — HTTP endpoint for initial presence state load.
 *
 * Clients fetch GET /api/presence on connect to hydrate the full set
 * of currently-present peers, then receive incremental updates via WS.
 */

import { Hono } from "hono";
import type { PresenceStore } from "../transport/presence-store.js";

export function createPresenceRoutes(presence: PresenceStore): Hono {
  const app = new Hono();

  app.get("/", (c) => {
    const peers = presence.getAll();
    return c.json({ peers, count: peers.length });
  });

  return app;
}
