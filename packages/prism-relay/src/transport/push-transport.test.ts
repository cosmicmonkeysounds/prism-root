import { describe, it, expect, vi, beforeEach } from "vitest";
import * as crypto from "node:crypto";
import {
  generateApnsJwt,
  generateFcmAssertion,
  buildApnsRequest,
  buildFcmRequest,
  createPushPingTransport,
} from "./push-transport.js";
import type { ApnsConfig, FcmConfig, PushTransportConfig, PushTransportDeps } from "./push-transport.js";
import type { DeviceRegistration } from "../routes/ping-routes.js";

// ── Test Keys ─────────────────────────────────────────────────────────────

const { privateKey: apnsKey } = crypto.generateKeyPairSync("ec", {
  namedCurve: "P-256",
  publicKeyEncoding: { type: "spki", format: "pem" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});

const { privateKey: rsaKey } = crypto.generateKeyPairSync("rsa", {
  modulusLength: 2048,
  publicKeyEncoding: { type: "spki", format: "pem" },
  privateKeyEncoding: { type: "pkcs8", format: "pem" },
});

const testApnsConfig: ApnsConfig = {
  keyId: "TESTKEY123",
  teamId: "TEAMID456",
  privateKey: apnsKey,
  bundleId: "com.prism.test",
};

const testServiceAccount = {
  client_email: "test@project.iam.gserviceaccount.com",
  private_key: rsaKey,
};

function decodeJwtPart(part: string): Record<string, unknown> {
  const padded = part.replace(/-/g, "+").replace(/_/g, "/");
  return JSON.parse(Buffer.from(padded, "base64").toString("utf-8")) as Record<string, unknown>;
}

// ── APNs JWT Tests ────────────────────────────────────────────────────────

describe("generateApnsJwt", () => {
  it("produces a valid 3-part JWT", () => {
    const jwt = generateApnsJwt(testApnsConfig, 1000);
    const parts = jwt.split(".");
    expect(parts).toHaveLength(3);
    // Each part should be non-empty base64url
    for (const part of parts) {
      expect(part?.length).toBeGreaterThan(0);
    }
  });

  it("header has alg=ES256 and kid", () => {
    const jwt = generateApnsJwt(testApnsConfig, 1000);
    const header = decodeJwtPart(jwt.split(".")[0] ?? "");
    expect(header.alg).toBe("ES256");
    expect(header.kid).toBe("TESTKEY123");
  });

  it("payload has iss=teamId and iat", () => {
    const jwt = generateApnsJwt(testApnsConfig, 1234567890);
    const payload = decodeJwtPart(jwt.split(".")[1] ?? "");
    expect(payload.iss).toBe("TEAMID456");
    expect(payload.iat).toBe(1234567890);
  });
});

// ── FCM Assertion Tests ───────────────────────────────────────────────────

describe("generateFcmAssertion", () => {
  it("produces a valid 3-part JWT", () => {
    const jwt = generateFcmAssertion(testServiceAccount, 1000);
    const parts = jwt.split(".");
    expect(parts).toHaveLength(3);
  });

  it("header has alg=RS256", () => {
    const jwt = generateFcmAssertion(testServiceAccount, 1000);
    const header = decodeJwtPart(jwt.split(".")[0] ?? "");
    expect(header.alg).toBe("RS256");
  });

  it("payload has correct iss, scope, aud, iat, and exp", () => {
    const jwt = generateFcmAssertion(testServiceAccount, 1000);
    const payload = decodeJwtPart(jwt.split(".")[1] ?? "");
    expect(payload.iss).toBe("test@project.iam.gserviceaccount.com");
    expect(payload.scope).toBe("https://www.googleapis.com/auth/firebase.messaging");
    expect(payload.aud).toBe("https://oauth2.googleapis.com/token");
    expect(payload.iat).toBe(1000);
    expect(payload.exp).toBe(4600); // iat + 3600
  });
});

// ── buildApnsRequest Tests ────────────────────────────────────────────────

describe("buildApnsRequest", () => {
  it("uses sandbox URL by default", () => {
    const { url } = buildApnsRequest(testApnsConfig, "jwt-token", "device-abc");
    expect(url).toBe("https://api.sandbox.push.apple.com/3/device/device-abc");
  });

  it("uses production URL when configured", () => {
    const prodConfig = { ...testApnsConfig, production: true };
    const { url } = buildApnsRequest(prodConfig, "jwt-token", "device-abc");
    expect(url).toBe("https://api.push.apple.com/3/device/device-abc");
  });

  it("includes content-available:1 in body", () => {
    const { init } = buildApnsRequest(testApnsConfig, "jwt-token", "device-abc");
    const body = JSON.parse(init.body as string) as { aps: Record<string, unknown> };
    expect(body.aps["content-available"]).toBe(1);
  });

  it("includes badge when provided", () => {
    const { init } = buildApnsRequest(testApnsConfig, "jwt-token", "device-abc", 5);
    const body = JSON.parse(init.body as string) as { aps: Record<string, unknown> };
    expect(body.aps.badge).toBe(5);
  });
});

// ── buildFcmRequest Tests ─────────────────────────────────────────────────

describe("buildFcmRequest", () => {
  it("targets correct FCM URL", () => {
    const { url } = buildFcmRequest("my-project", "access-token", "device-xyz");
    expect(url).toBe("https://fcm.googleapis.com/v1/projects/my-project/messages:send");
  });

  it("includes data.type=prism_sync", () => {
    const { init } = buildFcmRequest("my-project", "access-token", "device-xyz");
    const body = JSON.parse(init.body as string) as { message: { data: Record<string, string> } };
    expect(body.message.data.type).toBe("prism_sync");
  });

  it("includes badge in data when provided", () => {
    const { init } = buildFcmRequest("my-project", "access-token", "device-xyz", 3);
    const body = JSON.parse(init.body as string) as { message: { data: Record<string, string> } };
    expect(body.message.data.badge).toBe("3");
  });
});

// ── createPushPingTransport Tests ─────────────────────────────────────────

describe("createPushPingTransport", () => {
  let mockFetch: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    mockFetch = vi.fn().mockResolvedValue({
      ok: true,
      text: async () => "",
      json: async () => ({ access_token: "test-token" }),
    });
    vi.spyOn(process.stderr, "write").mockImplementation(() => true);
  });

  function makeDeviceRegistry(devices: DeviceRegistration[]): (did: string) => DeviceRegistration[] {
    return (did: string) => devices.filter((d) => d.did === did);
  }

  it("returns false when no devices registered", async () => {
    const config: PushTransportConfig = { apns: testApnsConfig };
    const deps: PushTransportDeps = {
      getDevices: makeDeviceRegistry([]),
      fetch: mockFetch,
    };
    const transport = createPushPingTransport(config, deps);
    const result = await transport.send({ to: "did:key:z6MkNobody", from: "did:key:z6MkSender" });
    expect(result).toBe(false);
    expect(mockFetch).not.toHaveBeenCalled();
  });

  it("sends to APNs device", async () => {
    const config: PushTransportConfig = { apns: testApnsConfig };
    const devices: DeviceRegistration[] = [
      { did: "did:key:z6MkUser" as DeviceRegistration["did"], platform: "apns", token: "apns-device-token", registeredAt: new Date().toISOString() },
    ];
    const deps: PushTransportDeps = {
      getDevices: makeDeviceRegistry(devices),
      fetch: mockFetch,
    };
    const transport = createPushPingTransport(config, deps);
    const result = await transport.send({ to: "did:key:z6MkUser", from: "did:key:z6MkSender" });

    expect(result).toBe(true);
    expect(mockFetch).toHaveBeenCalledOnce();
    const [url] = mockFetch.mock.calls[0] as [string, RequestInit];
    expect(url).toContain("api.sandbox.push.apple.com");
    expect(url).toContain("apns-device-token");
  });

  it("sends to FCM device", async () => {
    const fcmConfig: FcmConfig = {
      projectId: "test-project",
      serviceAccountKey: JSON.stringify(testServiceAccount),
    };
    const config: PushTransportConfig = { fcm: fcmConfig };
    const devices: DeviceRegistration[] = [
      { did: "did:key:z6MkUser" as DeviceRegistration["did"], platform: "fcm", token: "fcm-device-token", registeredAt: new Date().toISOString() },
    ];

    // FCM needs two fetches: one for OAuth token exchange, one for the actual push
    const tokenFetch = vi.fn()
      .mockResolvedValueOnce({
        ok: true,
        text: async () => "",
        json: async () => ({ access_token: "fcm-access-token" }),
      })
      .mockResolvedValueOnce({
        ok: true,
        text: async () => "",
        json: async () => ({}),
      });

    const deps: PushTransportDeps = {
      getDevices: makeDeviceRegistry(devices),
      fetch: tokenFetch,
    };
    const transport = createPushPingTransport(config, deps);
    const result = await transport.send({ to: "did:key:z6MkUser", from: "did:key:z6MkSender" });

    expect(result).toBe(true);
    // Should have called fetch at least for the token exchange + push send
    expect(tokenFetch).toHaveBeenCalled();
  });

  it("returns true when at least one send succeeds", async () => {
    const fcmConfig: FcmConfig = {
      projectId: "test-project",
      serviceAccountKey: JSON.stringify(testServiceAccount),
    };
    const config: PushTransportConfig = { apns: testApnsConfig, fcm: fcmConfig };
    const devices: DeviceRegistration[] = [
      { did: "did:key:z6MkUser" as DeviceRegistration["did"], platform: "apns", token: "apns-tok", registeredAt: new Date().toISOString() },
      { did: "did:key:z6MkUser" as DeviceRegistration["did"], platform: "fcm", token: "fcm-tok", registeredAt: new Date().toISOString() },
    ];

    // APNs succeeds, FCM token exchange succeeds, FCM push fails
    const mixedFetch = vi.fn()
      .mockResolvedValueOnce({ ok: true, text: async () => "" }) // APNs push
      .mockResolvedValueOnce({ ok: true, json: async () => ({ access_token: "tok" }), text: async () => "" }) // FCM token
      .mockResolvedValueOnce({ ok: false, status: 500, text: async () => "server error" }); // FCM push

    const deps: PushTransportDeps = {
      getDevices: makeDeviceRegistry(devices),
      fetch: mixedFetch,
    };
    const transport = createPushPingTransport(config, deps);
    const result = await transport.send({ to: "did:key:z6MkUser", from: "did:key:z6MkSender" });

    expect(result).toBe(true);
  });

  it("handles fetch errors gracefully", async () => {
    const config: PushTransportConfig = { apns: testApnsConfig };
    const devices: DeviceRegistration[] = [
      { did: "did:key:z6MkUser" as DeviceRegistration["did"], platform: "apns", token: "apns-tok", registeredAt: new Date().toISOString() },
    ];

    const failFetch = vi.fn().mockRejectedValue(new Error("Network error"));

    const deps: PushTransportDeps = {
      getDevices: makeDeviceRegistry(devices),
      fetch: failFetch,
    };
    const transport = createPushPingTransport(config, deps);
    const result = await transport.send({ to: "did:key:z6MkUser", from: "did:key:z6MkSender" });

    // Should not throw, returns false because no sends succeeded
    expect(result).toBe(false);
  });
});
