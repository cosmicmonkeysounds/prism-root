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

function b64(s: string): string {
  return Buffer.from(s).toString("base64");
}

function makeManifest(id: string, name: string, opts?: { description?: string }) {
  return {
    id,
    name,
    version: "1",
    storage: { backend: "loro", path: "data" },
    schema: { module: "@prism/core" },
    createdAt: new Date().toISOString(),
    description: opts?.description,
  };
}

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

  const server = createRelayServer({ relay, port: 0, publicUrl: "http://localhost:0", disableCsrf: true });
  const info = await server.start();
  url = `http://localhost:${info.port}`;
  close = info.close;
});

afterAll(async () => {
  await close();
  await relay.stop();
});

describe("vault-host-routes", () => {
  // ── Publish + Get ──────────────────────────────────────────────────────

  describe("POST /api/vaults + GET /api/vaults/:id", () => {
    it("publishes a vault and retrieves it", async () => {
      const manifest = makeManifest("v-publish", "Publish Test");
      const res = await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest,
          ownerDid: identity.did,
          isPublic: true,
          collections: { col1: b64("snapshot-data") },
        }),
      });
      expect(res.status).toBe(201);
      const vault = await res.json();
      expect(vault.id).toBe("v-publish");
      expect(vault.ownerDid).toBe(identity.did);
      expect(vault.isPublic).toBe(true);
      expect(vault.totalBytes).toBeGreaterThan(0);

      // Get it back
      const getRes = await fetch(`${url}/api/vaults/v-publish`);
      expect(getRes.status).toBe(200);
      const got = await getRes.json();
      expect(got.manifest.name).toBe("Publish Test");
    });

    it("returns 400 without manifest", async () => {
      const res = await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ownerDid: "did:key:z1" }),
      });
      expect(res.status).toBe(400);
    });

    it("returns 404 for unknown vault", async () => {
      const res = await fetch(`${url}/api/vaults/nonexistent`);
      expect(res.status).toBe(404);
    });
  });

  // ── List ───────────────────────────────────────────────────────────────

  describe("GET /api/vaults", () => {
    it("lists all vaults", async () => {
      // Publish a second vault
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest("v-list-pub", "Public Vault"),
          ownerDid: identity.did,
          isPublic: true,
          collections: {},
        }),
      });
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest("v-list-priv", "Private Vault"),
          ownerDid: identity.did,
          isPublic: false,
          collections: {},
        }),
      });

      const res = await fetch(`${url}/api/vaults`);
      const vaults = await res.json();
      expect(vaults.length).toBeGreaterThanOrEqual(3);
    });

    it("filters to public only", async () => {
      const res = await fetch(`${url}/api/vaults?public=true`);
      const vaults = await res.json();
      expect(vaults.every((v: { isPublic: boolean }) => v.isPublic)).toBe(true);
    });

    it("searches by name", async () => {
      const res = await fetch(`${url}/api/vaults?search=Private`);
      const vaults = await res.json();
      expect(vaults.some((v: { manifest: { name: string } }) => v.manifest.name === "Private Vault")).toBe(true);
    });
  });

  // ── Collection snapshots ───────────────────────────────────────────────

  describe("collection snapshot endpoints", () => {
    const vaultId = "v-snap-test";

    beforeAll(async () => {
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Snap Test"),
          ownerDid: identity.did,
          collections: {
            col1: b64("alpha-data"),
            col2: b64("beta-data"),
          },
        }),
      });
    });

    it("GET /:id/collections lists collection sizes", async () => {
      const res = await fetch(`${url}/api/vaults/${vaultId}/collections`);
      expect(res.status).toBe(200);
      const collections = await res.json();
      expect(collections.length).toBe(2);
      expect(collections[0]).toHaveProperty("id");
      expect(collections[0]).toHaveProperty("bytes");
    });

    it("GET /:id/collections/:cid returns single snapshot", async () => {
      const res = await fetch(`${url}/api/vaults/${vaultId}/collections/col1`);
      expect(res.status).toBe(200);
      const body = await res.json();
      expect(body.snapshot).toBe(b64("alpha-data"));
    });

    it("GET /:id/collections/:cid returns 404 for unknown", async () => {
      const res = await fetch(`${url}/api/vaults/${vaultId}/collections/nope`);
      expect(res.status).toBe(404);
    });
  });

  // ── Bulk download ──────────────────────────────────────────────────────

  describe("GET /api/vaults/:id/download", () => {
    it("returns manifest + all collection snapshots", async () => {
      const vaultId = "v-download";
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Download Test"),
          ownerDid: identity.did,
          collections: {
            c1: b64("data-1"),
            c2: b64("data-2"),
          },
        }),
      });

      const res = await fetch(`${url}/api/vaults/${vaultId}/download`);
      expect(res.status).toBe(200);
      const body = await res.json();
      expect(body.manifest.name).toBe("Download Test");
      expect(body.collections.c1).toBe(b64("data-1"));
      expect(body.collections.c2).toBe(b64("data-2"));
    });

    it("returns 404 for unknown vault", async () => {
      const res = await fetch(`${url}/api/vaults/nope/download`);
      expect(res.status).toBe(404);
    });
  });

  // ── Update collections ─────────────────────────────────────────────────

  describe("PUT /api/vaults/:id/collections", () => {
    it("updates snapshots for owner", async () => {
      const vaultId = "v-update";
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Update Test"),
          ownerDid: identity.did,
          collections: { col1: b64("v1") },
        }),
      });

      const res = await fetch(`${url}/api/vaults/${vaultId}/collections`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ownerDid: identity.did,
          collections: { col1: b64("v2"), col2: b64("new") },
        }),
      });
      expect(res.status).toBe(200);

      // Verify updated
      const snap = await fetch(`${url}/api/vaults/${vaultId}/collections/col1`).then((r) => r.json());
      expect(snap.snapshot).toBe(b64("v2"));

      // Verify new collection added
      const snap2 = await fetch(`${url}/api/vaults/${vaultId}/collections/col2`).then((r) => r.json());
      expect(snap2.snapshot).toBe(b64("new"));
    });

    it("rejects update from non-owner", async () => {
      const vaultId = "v-update-reject";
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Owner Test"),
          ownerDid: identity.did,
          collections: {},
        }),
      });

      const res = await fetch(`${url}/api/vaults/${vaultId}/collections`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          ownerDid: "did:key:zStranger",
          collections: { hack: b64("evil") },
        }),
      });
      expect(res.status).toBe(403);
    });
  });

  // ── Delete vault ───────────────────────────────────────────────────────

  describe("DELETE /api/vaults/:id", () => {
    it("removes vault for owner", async () => {
      const vaultId = "v-delete";
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Delete Me"),
          ownerDid: identity.did,
          collections: {},
        }),
      });

      const res = await fetch(`${url}/api/vaults/${vaultId}`, {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ownerDid: identity.did }),
      });
      expect(res.status).toBe(200);

      const getRes = await fetch(`${url}/api/vaults/${vaultId}`);
      expect(getRes.status).toBe(404);
    });

    it("rejects deletion from non-owner", async () => {
      const vaultId = "v-delete-reject";
      await fetch(`${url}/api/vaults`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          manifest: makeManifest(vaultId, "Keep Me"),
          ownerDid: identity.did,
          collections: {},
        }),
      });

      const res = await fetch(`${url}/api/vaults/${vaultId}`, {
        method: "DELETE",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ownerDid: "did:key:zStranger" }),
      });
      expect(res.status).toBe(403);
    });
  });
});
