/**
 * AutoREST Routes — dynamic REST API gateway from CRDT collections.
 *
 * Exposes hosted collections as standard REST endpoints with capability
 * token authentication. External services see standard HTTP CRUD;
 * the Relay translates operations to CRDT mutations.
 *
 * Endpoints:
 *   GET    /api/rest/:collectionId                 — list objects
 *   GET    /api/rest/:collectionId/:objectId        — get object
 *   POST   /api/rest/:collectionId                 — create object
 *   PUT    /api/rest/:collectionId/:objectId        — update object
 *   DELETE /api/rest/:collectionId/:objectId        — delete (soft)
 */

import { Hono } from "hono";
import type {
  RelayInstance,
  CollectionHost,
  CapabilityTokenManager,
  WebhookEmitter,
} from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { objectId as toObjectId } from "@prism/core/object-model";
import { deserializeToken } from "./token-routes.js";

export function createAutoRestRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function getHost(): CollectionHost | undefined {
    return relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
  }

  function getTokenManager(): CapabilityTokenManager | undefined {
    return relay.getCapability<CapabilityTokenManager>(RELAY_CAPABILITIES.TOKENS);
  }

  function getWebhooks(): WebhookEmitter | undefined {
    return relay.getCapability<WebhookEmitter>(RELAY_CAPABILITIES.WEBHOOKS);
  }

  /** Verify capability token from Authorization header. */
  async function verifyAccess(
    authHeader: string | undefined,
    collectionId: string,
    permission: string,
  ): Promise<{ allowed: boolean; reason?: string }> {
    const tokenManager = getTokenManager();
    if (!tokenManager) return { allowed: true }; // No token module = open access

    if (!authHeader || !authHeader.startsWith("Bearer ")) {
      return { allowed: false, reason: "missing authorization token" };
    }

    try {
      const tokenJson = JSON.parse(
        Buffer.from(authHeader.slice(7), "base64").toString("utf-8"),
      );
      const token = deserializeToken(tokenJson);
      const result = await tokenManager.verify(token);
      if (!result.valid) {
        return { allowed: false, reason: result.reason ?? "invalid token" };
      }

      // Check scope matches collection
      if (token.scope !== "*" && token.scope !== collectionId) {
        return { allowed: false, reason: "token scope does not match collection" };
      }

      // Check permission
      if (!token.permissions.includes("*") && !token.permissions.includes(permission)) {
        return { allowed: false, reason: `token lacks '${permission}' permission` };
      }

      return { allowed: true };
    } catch {
      return { allowed: false, reason: "invalid authorization token" };
    }
  }

  // LIST — GET /api/rest/:collectionId
  app.get("/:collectionId", async (c) => {
    const host = getHost();
    if (!host) return c.json({ error: "collection hosting not available" }, 404);

    const collectionId = c.req.param("collectionId");
    const access = await verifyAccess(c.req.header("authorization"), collectionId, "read");
    if (!access.allowed) return c.json({ error: access.reason }, 403);

    const store = host.get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    // Support query params for filtering
    const type = c.req.query("type");
    const status = c.req.query("status");
    const tag = c.req.query("tag");
    const limit = parseInt(c.req.query("limit") ?? "100", 10);
    const offset = parseInt(c.req.query("offset") ?? "0", 10);

    let objects = store.allObjects().filter((o) => !o.deletedAt);
    if (type) objects = objects.filter((o) => o.type === type);
    if (status) objects = objects.filter((o) => o.status === status);
    if (tag) objects = objects.filter((o) => o.tags.includes(tag));

    const total = objects.length;
    const page = objects.slice(offset, offset + limit);

    return c.json({
      objects: page,
      total,
      limit,
      offset,
    });
  });

  // GET — GET /api/rest/:collectionId/:objectId
  app.get("/:collectionId/:objectId", async (c) => {
    const host = getHost();
    if (!host) return c.json({ error: "collection hosting not available" }, 404);

    const collectionId = c.req.param("collectionId");
    const access = await verifyAccess(c.req.header("authorization"), collectionId, "read");
    if (!access.allowed) return c.json({ error: access.reason }, 403);

    const store = host.get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const objectId = toObjectId(c.req.param("objectId"));
    const obj = store.getObject(objectId);
    if (!obj || obj.deletedAt) return c.json({ error: "object not found" }, 404);

    return c.json(obj);
  });

  // CREATE — POST /api/rest/:collectionId
  app.post("/:collectionId", async (c) => {
    const host = getHost();
    if (!host) return c.json({ error: "collection hosting not available" }, 404);

    const collectionId = c.req.param("collectionId");
    const access = await verifyAccess(c.req.header("authorization"), collectionId, "write");
    if (!access.allowed) return c.json({ error: access.reason }, 403);

    const store = host.get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const body = await c.req.json<{
      name: string;
      type?: string;
      description?: string;
      status?: string;
      tags?: string[];
      parentId?: string | null;
      data?: Record<string, unknown>;
    }>();

    if (!body.name || typeof body.name !== "string") {
      return c.json({ error: "name is required" }, 400);
    }

    const now = new Date().toISOString();
    const id = toObjectId(`rest-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`);

    store.putObject({
      id,
      type: body.type ?? "object",
      name: body.name,
      parentId: body.parentId !== undefined ? (body.parentId ? toObjectId(body.parentId) : null) : null,
      position: store.objectCount(),
      status: body.status ?? null,
      tags: body.tags ?? [],
      date: now,
      endDate: null,
      description: body.description ?? "",
      color: null,
      image: null,
      pinned: false,
      data: body.data ?? {},
      createdAt: now,
      updatedAt: now,
    });

    // Emit webhook event
    const webhooks = getWebhooks();
    if (webhooks) {
      await webhooks.emit("object.created", { collectionId, objectId: id, type: body.type ?? "object" });
    }

    return c.json({ ok: true, objectId: id }, 201);
  });

  // UPDATE — PUT /api/rest/:collectionId/:objectId
  app.put("/:collectionId/:objectId", async (c) => {
    const host = getHost();
    if (!host) return c.json({ error: "collection hosting not available" }, 404);

    const collectionId = c.req.param("collectionId");
    const access = await verifyAccess(c.req.header("authorization"), collectionId, "write");
    if (!access.allowed) return c.json({ error: access.reason }, 403);

    const store = host.get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const objectId = toObjectId(c.req.param("objectId"));
    const existing = store.getObject(objectId);
    if (!existing || existing.deletedAt) return c.json({ error: "object not found" }, 404);

    const body = await c.req.json<Record<string, unknown>>();
    const now = new Date().toISOString();

    store.putObject({
      ...existing,
      ...body,
      id: objectId, // Cannot change ID
      updatedAt: now,
    });

    const webhooks = getWebhooks();
    if (webhooks) {
      await webhooks.emit("object.updated", { collectionId, objectId, fields: Object.keys(body) });
    }

    return c.json({ ok: true, objectId });
  });

  // DELETE — DELETE /api/rest/:collectionId/:objectId
  app.delete("/:collectionId/:objectId", async (c) => {
    const host = getHost();
    if (!host) return c.json({ error: "collection hosting not available" }, 404);

    const collectionId = c.req.param("collectionId");
    const access = await verifyAccess(c.req.header("authorization"), collectionId, "delete");
    if (!access.allowed) return c.json({ error: access.reason }, 403);

    const store = host.get(collectionId);
    if (!store) return c.json({ error: "collection not found" }, 404);

    const objectId = toObjectId(c.req.param("objectId"));
    const existing = store.getObject(objectId);
    if (!existing || existing.deletedAt) return c.json({ error: "object not found" }, 404);

    // Soft delete
    store.putObject({
      ...existing,
      deletedAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    });

    const webhooks = getWebhooks();
    if (webhooks) {
      await webhooks.emit("object.deleted", { collectionId, objectId });
    }

    return c.json({ ok: true, objectId });
  });

  return app;
}
