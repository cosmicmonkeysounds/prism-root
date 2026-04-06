/**
 * Ping Routes — device token registration and blind ping management.
 *
 * Blind Pings are content-free push notifications sent via APNs/FCM to
 * wake mobile Capacitor apps for background CRDT syncing. The payload
 * is empty — no data is exposed.
 *
 * The transport layer (APNs/FCM) is pluggable via PingTransport interface.
 * This module handles device registration and ping dispatch.
 */

import { Hono } from "hono";
import type { RelayInstance, BlindPinger } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import type { DID } from "@prism/core/identity";

/** Device token registration — maps a DID to a push notification token. */
export interface DeviceRegistration {
  did: DID;
  platform: "apns" | "fcm";
  token: string;
  registeredAt: string;
}

/**
 * APNs/FCM transport that dispatches blind pings to registered devices.
 * This is the concrete PingTransport implementation that the relay wires in.
 */
export interface PushTransportConfig {
  /** APNs team ID + key (for Apple Push Notification service). */
  apns?: { teamId: string; keyId: string; privateKey: string; bundleId: string };
  /** FCM server key (for Firebase Cloud Messaging). */
  fcm?: { serverKey: string };
}

// In-memory device registry (persisted via file store in production)
const deviceRegistry = new Map<string, DeviceRegistration>();

export function createPingRoutes(relay: RelayInstance): Hono {
  const app = new Hono();

  function getPinger(): BlindPinger | undefined {
    return relay.getCapability<BlindPinger>(RELAY_CAPABILITIES.PINGER);
  }

  // POST /api/pings/register — register a device token for push notifications
  app.post("/register", async (c) => {
    const body = await c.req.json<{
      did: string;
      platform: "apns" | "fcm";
      token: string;
    }>();

    if (!body.did || !body.platform || !body.token) {
      return c.json({ error: "did, platform, and token are required" }, 400);
    }

    if (body.platform !== "apns" && body.platform !== "fcm") {
      return c.json({ error: "platform must be 'apns' or 'fcm'" }, 400);
    }

    const registration: DeviceRegistration = {
      did: body.did as DID,
      platform: body.platform,
      token: body.token,
      registeredAt: new Date().toISOString(),
    };

    deviceRegistry.set(`${body.did}:${body.platform}`, registration);

    return c.json({ ok: true, registration }, 201);
  });

  // DELETE /api/pings/register/:did — unregister device tokens for a DID
  app.delete("/register/:did", (c) => {
    const did = c.req.param("did");
    let removed = 0;
    for (const [key, reg] of deviceRegistry) {
      if (reg.did === did) {
        deviceRegistry.delete(key);
        removed++;
      }
    }
    return c.json({ ok: true, removed });
  });

  // GET /api/pings/devices — list registered devices (admin)
  app.get("/devices", (c) => {
    const devices = [...deviceRegistry.values()];
    return c.json({ devices, count: devices.length });
  });

  // POST /api/pings/send — send a blind ping to a DID
  app.post("/send", async (c) => {
    const pinger = getPinger();
    if (!pinger) return c.json({ error: "blind ping module not installed" }, 404);

    const body = await c.req.json<{
      recipientDid: string;
      badgeCount?: number;
    }>();

    if (!body.recipientDid) {
      return c.json({ error: "recipientDid is required" }, 400);
    }

    const sent = await pinger.ping(body.recipientDid as DID, body.badgeCount);
    return c.json({ ok: true, sent });
  });

  // POST /api/pings/wake — send blind pings to all devices for a DID
  app.post("/wake", async (c) => {
    const pinger = getPinger();
    if (!pinger) return c.json({ error: "blind ping module not installed" }, 404);

    const body = await c.req.json<{ did: string; badgeCount?: number }>();
    if (!body.did) return c.json({ error: "did is required" }, 400);

    const results: Array<{ platform: string; sent: boolean }> = [];
    for (const [, reg] of deviceRegistry) {
      if (reg.did === body.did) {
        const sent = await pinger.ping(reg.did, body.badgeCount);
        results.push({ platform: reg.platform, sent });
      }
    }

    return c.json({ ok: true, pinged: results.length, results });
  });

  return app;
}

/**
 * Create a PingTransport that dispatches to APNs/FCM based on device registry.
 * This is wired into the BlindPinger module via setTransport().
 */
export function createPushPingTransport(
  config: PushTransportConfig,
): import("@prism/core/relay").PingTransport {
  return {
    async send(ping): Promise<boolean> {
      // Look up device registrations for the recipient
      const devices: DeviceRegistration[] = [];
      for (const [, reg] of deviceRegistry) {
        if (reg.did === ping.to) devices.push(reg);
      }

      if (devices.length === 0) return false;

      let anySent = false;
      for (const device of devices) {
        try {
          if (device.platform === "fcm" && config.fcm) {
            await sendFcmPing(config.fcm.serverKey, device.token, ping.badgeCount);
            anySent = true;
          } else if (device.platform === "apns" && config.apns) {
            await sendApnsPing(config.apns, device.token, ping.badgeCount);
            anySent = true;
          }
        } catch {
          // Transport error — continue to next device
        }
      }

      return anySent;
    },
  };
}

async function sendFcmPing(
  serverKey: string,
  deviceToken: string,
  badgeCount?: number,
): Promise<void> {
  await fetch("https://fcm.googleapis.com/fcm/send", {
    method: "POST",
    headers: {
      Authorization: `key=${serverKey}`,
      "Content-Type": "application/json",
    },
    body: JSON.stringify({
      to: deviceToken,
      // Content-free: only a data message to wake the app
      data: { type: "prism_sync", ...(badgeCount !== undefined ? { badge: String(badgeCount) } : {}) },
      // No "notification" key — silent background push
      content_available: true,
      priority: "high",
    }),
  });
}

async function sendApnsPing(
  config: NonNullable<PushTransportConfig["apns"]>,
  deviceToken: string,
  badgeCount?: number,
): Promise<void> {
  // APNs HTTP/2 push — minimal payload for background wake
  await fetch(`https://api.push.apple.com/3/device/${deviceToken}`, {
    method: "POST",
    headers: {
      "apns-topic": config.bundleId,
      "apns-push-type": "background",
      "apns-priority": "5",
    },
    body: JSON.stringify({
      aps: {
        "content-available": 1,
        ...(badgeCount !== undefined ? { badge: badgeCount } : {}),
      },
    }),
  });
}
