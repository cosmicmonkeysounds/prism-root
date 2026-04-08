import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  relayTimestampModule,
  blindPingModule,
  capabilityTokenModule,
  webhookModule,
  sovereignPortalModule,
  collectionHostModule,
  hashcashModule,
  peerTrustModule,
  escrowModule,
  federationModule,
  acmeCertificateModule,
  portalTemplateModule,
  webrtcSignalingModule,
  vaultHostModule,
} from "@prism/core/relay";
import type { RelayInstance } from "@prism/core/relay";
import { createRelayServer } from "../server/index.js";

let relay: RelayInstance;
let identity: PrismIdentity;
let url: string;
let close: () => Promise<void>;

beforeAll(async () => {
  identity = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: identity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(relayTimestampModule(identity))
    .use(blindPingModule())
    .use(capabilityTokenModule(identity))
    .use(webhookModule())
    .use(sovereignPortalModule())
    .use(collectionHostModule())
    .use(hashcashModule({ bits: 4 }))
    .use(peerTrustModule())
    .use(escrowModule())
    .use(federationModule())
    .use(acmeCertificateModule())
    .use(portalTemplateModule())
    .use(webrtcSignalingModule())
    .use(vaultHostModule())
    .build();
  await relay.start();

  const server = createRelayServer({
    relay,
    port: 0,
    publicUrl: "https://relay.example.com",
    disableCsrf: true,
  });
  const info = await server.start();
  url = `http://localhost:${info.port}`;
  close = info.close;
});

afterAll(async () => {
  await close();
  await relay.stop();
});

describe("directory-routes", () => {
  it("GET /api/directory returns relay profile", async () => {
    const res = await fetch(`${url}/api/directory`);
    expect(res.status).toBe(200);
    const body = await res.json();

    expect(body).toHaveProperty("relay");
    expect(body.relay.did).toBe(identity.did);
    expect(body.relay.publicUrl).toBe("https://relay.example.com");
    expect(body.relay.version).toBe("0.1.0");
    expect(typeof body.relay.uptime).toBe("number");
    expect(Array.isArray(body.relay.modules)).toBe(true);
    expect(body.relay.modules.length).toBe(16);
    expect(body.relay.federation).toHaveProperty("peers");
    expect(body.relay.federation).toHaveProperty("accepts");
  });

  it("includes public portals but not private ones", async () => {
    // Create a public portal
    await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "Public Portal",
        level: 1,
        collectionId: "pub-col",
        basePath: "/pub",
        isPublic: true,
      }),
    });

    // Create a non-public portal
    await fetch(`${url}/api/portals`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        name: "Private Portal",
        level: 1,
        collectionId: "priv-col",
        basePath: "/priv",
        isPublic: false,
      }),
    });

    const res = await fetch(`${url}/api/directory`);
    const body = await res.json();

    expect(body.portals.some((p: { name: string }) => p.name === "Public Portal")).toBe(true);
    expect(body.portals.some((p: { name: string }) => p.name === "Private Portal")).toBe(false);
    expect(body.portals.every((p: { isPublic: boolean }) => p.isPublic)).toBe(true);
  });

  it("includes public vaults but not private ones", async () => {
    // Publish a public vault
    await fetch(`${url}/api/vaults`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        manifest: {
          id: "dir-pub-vault",
          name: "Public Vault",
          version: "1",
          storage: { backend: "loro", path: "data" },
          schema: { module: "@prism/core" },
          createdAt: new Date().toISOString(),
        },
        ownerDid: identity.did,
        isPublic: true,
        collections: { c1: Buffer.from("data").toString("base64") },
      }),
    });

    // Publish a private vault
    await fetch(`${url}/api/vaults`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        manifest: {
          id: "dir-priv-vault",
          name: "Private Vault",
          version: "1",
          storage: { backend: "loro", path: "data" },
          schema: { module: "@prism/core" },
          createdAt: new Date().toISOString(),
        },
        ownerDid: identity.did,
        isPublic: false,
        collections: {},
      }),
    });

    const res = await fetch(`${url}/api/directory`);
    const body = await res.json();

    expect(body.vaults.some((v: { name: string }) => v.name === "Public Vault")).toBe(true);
    expect(body.vaults.some((v: { name: string }) => v.name === "Private Vault")).toBe(false);
  });

  it("sets Cache-Control header", async () => {
    const res = await fetch(`${url}/api/directory`);
    expect(res.headers.get("cache-control")).toContain("max-age=300");
  });

  it("includes generatedAt timestamp", async () => {
    const res = await fetch(`${url}/api/directory`);
    const body = await res.json();
    expect(body).toHaveProperty("generatedAt");
    expect(new Date(body.generatedAt).getTime()).toBeGreaterThan(0);
  });
});
