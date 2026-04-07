/**
 * Federation CRDT sync test — proves two relay instances can replicate
 * a collection via the federation sync endpoint.
 */

import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
  collectionHostModule,
  federationModule,
} from "@prism/core/relay";
import type { RelayInstance, CollectionHost } from "@prism/core/relay";
import { RELAY_CAPABILITIES } from "@prism/core/relay";
import { objectId } from "@prism/core/object-model";
import { createRelayServer } from "../server/relay-server.js";

let relayA: RelayInstance;
let relayB: RelayInstance;
let portA: number;
let portB: number;
let closeA: () => Promise<void>;
let closeB: () => Promise<void>;

beforeAll(async () => {
  // ── Create Relay A ──────────────────────────────────────────────────────
  const identityA = await createIdentity({ method: "key" });
  relayA = createRelayBuilder({ relayDid: identityA.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(collectionHostModule())
    .use(federationModule())
    .build();
  await relayA.start();

  const serverA = createRelayServer({
    relay: relayA,
    port: 0,
    disableCsrf: true,
  });
  const infoA = await serverA.start();
  portA = infoA.port;
  closeA = infoA.close;

  // ── Create Relay B ──────────────────────────────────────────────────────
  const identityB = await createIdentity({ method: "key" });
  relayB = createRelayBuilder({ relayDid: identityB.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(collectionHostModule())
    .use(federationModule())
    .build();
  await relayB.start();

  const serverB = createRelayServer({
    relay: relayB,
    port: 0,
    disableCsrf: true,
  });
  const infoB = await serverB.start();
  portB = infoB.port;
  closeB = infoB.close;

  // ── Register federation peers (A knows B, B knows A) ───────────────────
  await fetch(`http://127.0.0.1:${portA}/api/federation/announce`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ relayDid: identityB.did, url: `http://127.0.0.1:${portB}` }),
  });
  await fetch(`http://127.0.0.1:${portB}/api/federation/announce`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ relayDid: identityA.did, url: `http://127.0.0.1:${portA}` }),
  });
});

afterAll(async () => {
  await closeA();
  await closeB();
  await relayA.stop();
  await relayB.stop();
});

describe("federation-sync", () => {
  it("two relays see each other as peers", async () => {
    const resA = await fetch(`http://127.0.0.1:${portA}/api/federation/peers`);
    const peersA = (await resA.json()) as Array<{ relayDid: string }>;
    expect(peersA).toHaveLength(1);

    const resB = await fetch(`http://127.0.0.1:${portB}/api/federation/peers`);
    const peersB = (await resB.json()) as Array<{ relayDid: string }>;
    expect(peersB).toHaveLength(1);
  });

  it("collection import on relay A replicates to relay B via federation", async () => {
    const collectionId = "sync-test-collection";

    // ── Create collection on relay A and add data ─────────────────────────
    const hostA = relayA.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostA) throw new Error("collection host not available on relay A");
    const storeA = hostA.create(collectionId);

    const now = new Date().toISOString();
    storeA.putObject({
      id: objectId("obj-alpha"),
      type: "task",
      name: "Alpha Task",
      parentId: null,
      position: 0,
      status: "open",
      tags: ["federation"],
      date: now,
      endDate: null,
      description: "Created on relay A",
      color: null,
      image: null,
      pinned: false,
      data: {},
      createdAt: now,
      updatedAt: now,
    });

    // ── Create collection on relay B (empty) ──────────────────────────────
    await fetch(`http://127.0.0.1:${portB}/api/collections`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ id: collectionId }),
    });

    // ── Export snapshot from A, import via A's HTTP endpoint ──────────────
    // This triggers federation push to relay B automatically.
    const snapshotBytes = storeA.exportSnapshot();
    const snapshotBase64 = Buffer.from(snapshotBytes).toString("base64");

    await fetch(`http://127.0.0.1:${portA}/api/collections/${collectionId}/import`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ data: snapshotBase64 }),
    });

    // ── Give federation push a moment to complete (fire-and-forget) ───────
    await new Promise((r) => setTimeout(r, 200));

    // ── Verify relay B has the collection and the object ─────────────────
    const hostB = relayB.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostB) throw new Error("collection host not available on relay B");
    const storeB = hostB.get(collectionId);
    expect(storeB).toBeDefined();

    const objects = storeB?.listObjects();
    expect(objects).toHaveLength(1);
    expect(objects?.[0]?.name).toBe("Alpha Task");
    expect(objects?.[0]?.description).toBe("Created on relay A");
  });

  it("POST /api/federation/sync imports a snapshot directly", async () => {
    const collectionId = "direct-sync-collection";

    // Create a collection on relay A with data
    const hostA = relayA.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostA) throw new Error("collection host not available");
    const storeA = hostA.create(collectionId);

    const now = new Date().toISOString();
    storeA.putObject({
      id: objectId("obj-direct"),
      type: "note",
      name: "Direct Sync Note",
      parentId: null,
      position: 0,
      status: "draft",
      tags: [],
      date: now,
      endDate: null,
      description: "Sent directly via federation sync endpoint",
      color: null,
      image: null,
      pinned: false,
      data: {},
      createdAt: now,
      updatedAt: now,
    });

    const snapshotBase64 = Buffer.from(storeA.exportSnapshot()).toString("base64");

    // POST directly to relay B's federation sync endpoint
    const res = await fetch(`http://127.0.0.1:${portB}/api/federation/sync`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ collectionId, snapshot: snapshotBase64 }),
    });
    expect(res.status).toBe(200);

    const hostB = relayB.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostB) throw new Error("collection host not available on relay B");
    const storeB = hostB.get(collectionId);
    expect(storeB).toBeDefined();

    const objects = storeB?.listObjects();
    expect(objects).toHaveLength(1);
    expect(objects?.[0]?.name).toBe("Direct Sync Note");
  });

  it("federation sync is resilient to peer failures", async () => {
    const collectionId = "resilient-collection";

    // Add a fake unreachable peer to relay A's federation registry
    await fetch(`http://127.0.0.1:${portA}/api/federation/announce`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ relayDid: "did:key:zFAKEPEER", url: "http://127.0.0.1:1" }),
    });

    const hostA = relayA.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostA) throw new Error("collection host not available");
    const storeA = hostA.create(collectionId);

    const now = new Date().toISOString();
    storeA.putObject({
      id: objectId("obj-resilient"),
      type: "task",
      name: "Resilient Task",
      parentId: null,
      position: 0,
      status: "open",
      tags: [],
      date: now,
      endDate: null,
      description: "Should succeed despite unreachable peer",
      color: null,
      image: null,
      pinned: false,
      data: {},
      createdAt: now,
      updatedAt: now,
    });

    const snapshotBase64 = Buffer.from(storeA.exportSnapshot()).toString("base64");

    // Import should succeed even though one federation peer is unreachable
    const res = await fetch(`http://127.0.0.1:${portA}/api/collections/${collectionId}/import`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ data: snapshotBase64 }),
    });
    expect(res.status).toBe(200);

    // Give async federation push time to attempt (and fail for fake peer)
    await new Promise((r) => setTimeout(r, 200));

    // Relay B should still have received the data (it's a real peer)
    const hostB = relayB.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    if (!hostB) throw new Error("collection host not available on relay B");
    const storeB = hostB.get(collectionId);
    expect(storeB).toBeDefined();

    const objects = storeB?.listObjects();
    expect(objects).toHaveLength(1);
    expect(objects?.[0]?.name).toBe("Resilient Task");
  });

  it("federation sync validates required fields", async () => {
    const res = await fetch(`http://127.0.0.1:${portB}/api/federation/sync`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ collectionId: "" }),
    });
    expect(res.status).toBe(400);
  });
});
