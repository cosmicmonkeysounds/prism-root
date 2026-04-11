import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createIdentity } from "@prism/core/identity";
import type { PrismIdentity } from "@prism/core/identity";
import type { DID } from "@prism/core/identity";
import {
  createRelayBuilder,
  blindMailboxModule,
  relayRouterModule,
} from "./relay.js";
import { collectionHostModule } from "./collection-host-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { RelayInstance, CollectionHost } from "./relay-types.js";
import { createRelayClient } from "./relay-client.js";

import { createRelayServer } from "@prism/relay/server";

let relay: RelayInstance;
let relayIdentity: PrismIdentity;
let alice: PrismIdentity;
let bob: PrismIdentity;
let serverPort: number;
let closeServer: () => Promise<void>;

beforeAll(async () => {
  relayIdentity = await createIdentity({ method: "key" });
  alice = await createIdentity({ method: "key" });
  bob = await createIdentity({ method: "key" });

  relay = createRelayBuilder({ relayDid: relayIdentity.did })
    .use(blindMailboxModule())
    .use(relayRouterModule())
    .use(collectionHostModule())
    .build();
  await relay.start();

  const server = createRelayServer({ relay, port: 0 });
  const info = await server.start();
  serverPort = info.port;
  closeServer = info.close;
});

afterAll(async () => {
  await closeServer();
  await relay.stop();
});

function wsUrl(): string {
  return `ws://localhost:${serverPort}/ws/relay`;
}

describe("relay-client", () => {
  it("connects and authenticates", async () => {
    const client = createRelayClient({
      url: wsUrl(),
      identity: alice,
      autoReconnect: false,
    });
    expect(client.state).toBe("disconnected");

    await client.connect();
    expect(client.state).toBe("connected");
    expect(client.relayDid).toBe(relayIdentity.did);
    expect(client.modules.length).toBeGreaterThan(0);

    client.close();
    expect(client.state).toBe("disconnected");
  });

  it("sends envelope between two clients", async () => {
    const aliceClient = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    const bobClient = createRelayClient({ url: wsUrl(), identity: bob, autoReconnect: false });

    await aliceClient.connect();
    await bobClient.connect();

    // Listen for envelope on Bob
    const received = new Promise<Uint8Array>((resolve) => {
      bobClient.on("envelope", (env) => {
        resolve(env.ciphertext);
      });
    });

    // Alice sends to Bob
    const result = await aliceClient.send({
      to: bob.did,
      ciphertext: new Uint8Array([42, 43, 44]),
      ttlMs: 60_000,
    });
    expect(result.status).toBe("delivered");

    const data = await received;
    expect(data).toEqual(new Uint8Array([42, 43, 44]));

    aliceClient.close();
    bobClient.close();
  });

  it("queues envelope for offline peer", async () => {
    const aliceClient = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    await aliceClient.connect();

    // Send to a DID that isn't connected
    const offlineDid = "did:key:zOfflinePeer" as DID;
    const result = await aliceClient.send({
      to: offlineDid,
      ciphertext: new Uint8Array([1, 2, 3]),
      ttlMs: 60_000,
    });
    expect(result.status).toBe("queued");

    aliceClient.close();
  });

  it("rejects send when disconnected", async () => {
    const client = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    await expect(
      client.send({ to: bob.did, ciphertext: new Uint8Array([1]) }),
    ).rejects.toThrow("Not connected");
  });

  it("requests collection sync snapshot", async () => {
    // Create a collection on the relay
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("client-sync-col");

    const client = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    await client.connect();

    const snapshot = await client.syncRequest("client-sync-col");
    expect(snapshot).toBeInstanceOf(Uint8Array);
    expect(snapshot.length).toBeGreaterThan(0);

    client.close();
  });

  it("sends sync-update to collection", async () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    const store = host.create("update-col");

    const client = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    await client.connect();

    // Subscribe to collection
    const snapshot = await client.syncRequest("update-col");
    expect(snapshot).toBeInstanceOf(Uint8Array);

    // Send a real CRDT update from the store itself (to get valid bytes)
    const exportedSnapshot = store.exportSnapshot();
    // syncUpdate should not throw — it's fire-and-forget
    client.syncUpdate("update-col", exportedSnapshot);

    // Small delay to let the message arrive on server
    await new Promise((r) => setTimeout(r, 50));

    client.close();
  });

  it("emits state-change events", async () => {
    const states: string[] = [];
    const client = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    client.on("state-change", ({ to }) => states.push(to));

    await client.connect();
    client.close();

    expect(states).toContain("connecting");
    expect(states).toContain("authenticating");
    expect(states).toContain("connected");
    expect(states).toContain("disconnected");
  });

  it("emits error for unknown collection sync", async () => {
    const client = createRelayClient({ url: wsUrl(), identity: alice, autoReconnect: false });
    await client.connect();

    await expect(client.syncRequest("nonexistent-col")).rejects.toThrow();

    client.close();
  });
});
