/**
 * Push Notification Transport — real APNs and FCM implementations.
 *
 * Sends content-free "blind pings" to wake mobile Capacitor apps for
 * background CRDT syncing. No data is exposed in the push payload.
 *
 * APNs: HTTP/2 to api.push.apple.com with ES256 JWT auth.
 * FCM:  HTTP v1 API with OAuth2 service account auth.
 *
 * Zero external dependencies — uses only Node.js crypto + fetch.
 */

import * as crypto from "node:crypto";
import type { BlindPing, PingTransport } from "@prism/core/relay";
import type { DeviceRegistration } from "../routes/ping-routes.js";

// ── Config Types ───────────────────────────────────────────────────────────

export interface ApnsConfig {
  /** APNs signing key ID (from Apple Developer portal). */
  keyId: string;
  /** Apple Developer Team ID. */
  teamId: string;
  /** ES256 private key in PEM format. */
  privateKey: string;
  /** App bundle identifier (e.g. "com.prism.app"). */
  bundleId: string;
  /** Use production APNs endpoint. Default: false (sandbox). */
  production?: boolean;
}

export interface FcmConfig {
  /** Firebase project ID. */
  projectId: string;
  /** Service account key as JSON string. */
  serviceAccountKey: string;
}

export interface PushTransportConfig {
  apns?: ApnsConfig;
  fcm?: FcmConfig;
}

// ── APNs JWT Generation ────────────────────────────────────────────────────

const APNS_TOKEN_TTL_MS = 50 * 60 * 1000; // 50 minutes (Apple max is 60)

interface ApnsToken {
  jwt: string;
  issuedAt: number;
}

/**
 * Base64url encode a buffer (RFC 7515).
 */
function base64url(data: Buffer | Uint8Array): string {
  return Buffer.from(data)
    .toString("base64")
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/, "");
}

/**
 * Generate an APNs provider JWT (ES256).
 * See: https://developer.apple.com/documentation/usernotifications/establishing-a-token-based-connection-to-apns
 */
export function generateApnsJwt(config: ApnsConfig, nowSeconds?: number): string {
  const iat = nowSeconds ?? Math.floor(Date.now() / 1000);

  const header = { alg: "ES256", kid: config.keyId };
  const payload = { iss: config.teamId, iat };

  const headerB64 = base64url(Buffer.from(JSON.stringify(header)));
  const payloadB64 = base64url(Buffer.from(JSON.stringify(payload)));
  const signingInput = `${headerB64}.${payloadB64}`;

  const key = crypto.createPrivateKey(config.privateKey);
  const sig = crypto.sign("sha256", Buffer.from(signingInput), {
    key,
    dsaEncoding: "ieee-p1363",
  });

  return `${signingInput}.${base64url(sig)}`;
}

/**
 * Create an APNs token manager that caches and refreshes JWTs.
 */
function createApnsTokenManager(config: ApnsConfig): { getToken(): string } {
  let cached: ApnsToken | undefined;

  return {
    getToken(): string {
      const now = Date.now();
      if (cached && now - cached.issuedAt < APNS_TOKEN_TTL_MS) {
        return cached.jwt;
      }
      const jwt = generateApnsJwt(config);
      cached = { jwt, issuedAt: now };
      return jwt;
    },
  };
}

// ── FCM OAuth2 Token ───────────────────────────────────────────────────────

const FCM_TOKEN_TTL_MS = 55 * 60 * 1000; // 55 minutes (Google tokens last 60)
const FCM_TOKEN_URL = "https://oauth2.googleapis.com/token";
const FCM_SCOPE = "https://www.googleapis.com/auth/firebase.messaging";

interface FcmToken {
  accessToken: string;
  issuedAt: number;
}

interface ServiceAccountKey {
  client_email: string;
  private_key: string;
  token_uri?: string;
}

/**
 * Generate a Google OAuth2 JWT assertion for service account auth.
 * See: https://developers.google.com/identity/protocols/oauth2/service-account
 */
export function generateFcmAssertion(
  serviceAccount: ServiceAccountKey,
  nowSeconds?: number,
): string {
  const iat = nowSeconds ?? Math.floor(Date.now() / 1000);
  const exp = iat + 3600;

  const header = { alg: "RS256", typ: "JWT" };
  const payload = {
    iss: serviceAccount.client_email,
    scope: FCM_SCOPE,
    aud: serviceAccount.token_uri ?? FCM_TOKEN_URL,
    iat,
    exp,
  };

  const headerB64 = base64url(Buffer.from(JSON.stringify(header)));
  const payloadB64 = base64url(Buffer.from(JSON.stringify(payload)));
  const signingInput = `${headerB64}.${payloadB64}`;

  const key = crypto.createPrivateKey(serviceAccount.private_key);
  const sig = crypto.sign("sha256", Buffer.from(signingInput), key);

  return `${signingInput}.${base64url(sig)}`;
}

/**
 * Create an FCM token manager that exchanges JWT assertions for access tokens.
 */
function createFcmTokenManager(
  config: FcmConfig,
  fetchFn: typeof globalThis.fetch = globalThis.fetch,
): { getToken(): Promise<string> } {
  const serviceAccount = JSON.parse(config.serviceAccountKey) as ServiceAccountKey;
  let cached: FcmToken | undefined;

  return {
    async getToken(): Promise<string> {
      const now = Date.now();
      if (cached && now - cached.issuedAt < FCM_TOKEN_TTL_MS) {
        return cached.accessToken;
      }

      const assertion = generateFcmAssertion(serviceAccount);
      const body = new URLSearchParams({
        grant_type: "urn:ietf:params:oauth:grant-type:jwt-bearer",
        assertion,
      });

      const res = await fetchFn(FCM_TOKEN_URL, {
        method: "POST",
        headers: { "Content-Type": "application/x-www-form-urlencoded" },
        body: body.toString(),
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(`FCM token exchange failed (${res.status}): ${text}`);
      }

      const data = (await res.json()) as { access_token: string };
      cached = { accessToken: data.access_token, issuedAt: now };
      return data.access_token;
    },
  };
}

// ── Push Sending ───────────────────────────────────────────────────────────

/** APNs endpoint URLs. */
const APNS_PRODUCTION = "https://api.push.apple.com";
const APNS_SANDBOX = "https://api.sandbox.push.apple.com";

/**
 * Build the APNs request for a blind ping (content-available:1, no visible content).
 */
export function buildApnsRequest(
  config: ApnsConfig,
  token: string,
  deviceToken: string,
  badgeCount?: number,
): { url: string; init: RequestInit } {
  const host = config.production ? APNS_PRODUCTION : APNS_SANDBOX;
  return {
    url: `${host}/3/device/${deviceToken}`,
    init: {
      method: "POST",
      headers: {
        authorization: `bearer ${token}`,
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
    },
  };
}

/**
 * Build the FCM v1 API request for a blind ping (data-only, no notification body).
 */
export function buildFcmRequest(
  projectId: string,
  accessToken: string,
  deviceToken: string,
  badgeCount?: number,
): { url: string; init: RequestInit } {
  return {
    url: `https://fcm.googleapis.com/v1/projects/${projectId}/messages:send`,
    init: {
      method: "POST",
      headers: {
        authorization: `Bearer ${accessToken}`,
        "content-type": "application/json",
      },
      body: JSON.stringify({
        message: {
          token: deviceToken,
          data: {
            type: "prism_sync",
            ...(badgeCount !== undefined ? { badge: String(badgeCount) } : {}),
          },
          android: {
            priority: "high",
          },
          apns: {
            headers: {
              "apns-priority": "5",
            },
            payload: {
              aps: {
                "content-available": 1,
              },
            },
          },
        },
      }),
    },
  };
}

// ── Transport Factory ──────────────────────────────────────────────────────

export interface PushTransportDeps {
  /** Device registry lookup. Returns all registrations for a DID. */
  getDevices(did: string): DeviceRegistration[];
  /** Optional: inject fetch for testing. */
  fetch?: typeof globalThis.fetch;
}

/**
 * Create a PingTransport that dispatches blind pings to APNs and FCM.
 *
 * This is the production transport wired into the BlindPinger module.
 * It looks up device registrations for the recipient DID and sends
 * a content-free push notification to each registered device.
 */
export function createPushPingTransport(
  config: PushTransportConfig,
  deps: PushTransportDeps,
): PingTransport {
  const fetchFn = deps.fetch ?? globalThis.fetch;

  const apnsTokenManager = config.apns
    ? createApnsTokenManager(config.apns)
    : undefined;
  const fcmTokenManager = config.fcm
    ? createFcmTokenManager(config.fcm, fetchFn)
    : undefined;

  return {
    async send(ping: BlindPing): Promise<boolean> {
      const devices = deps.getDevices(ping.to);
      if (devices.length === 0) return false;

      let anySent = false;

      for (const device of devices) {
        try {
          if (device.platform === "apns" && config.apns && apnsTokenManager) {
            const token = apnsTokenManager.getToken();
            const req = buildApnsRequest(
              config.apns,
              token,
              device.token,
              ping.badgeCount,
            );
            const res = await fetchFn(req.url, req.init);
            if (res.ok) {
              anySent = true;
            } else {
              const text = await res.text();
              // Log but continue — don't break on one device failure
              process.stderr.write(
                `[push-transport] APNs error for ${device.token}: ${res.status} ${text}\n`,
              );
            }
          } else if (device.platform === "fcm" && config.fcm && fcmTokenManager) {
            const accessToken = await fcmTokenManager.getToken();
            const req = buildFcmRequest(
              config.fcm.projectId,
              accessToken,
              device.token,
              ping.badgeCount,
            );
            const res = await fetchFn(req.url, req.init);
            if (res.ok) {
              anySent = true;
            } else {
              const text = await res.text();
              process.stderr.write(
                `[push-transport] FCM error for ${device.token}: ${res.status} ${text}\n`,
              );
            }
          }
        } catch (err: unknown) {
          // Transport error — log and continue to next device
          process.stderr.write(
            `[push-transport] Error sending to ${device.platform}/${device.token}: ${String(err)}\n`,
          );
        }
      }

      return anySent;
    },
  };
}
