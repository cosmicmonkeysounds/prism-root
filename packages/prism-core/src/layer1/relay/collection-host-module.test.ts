import { describe, it, expect, beforeAll } from "vitest";
import { createIdentity } from "../identity/identity.js";
import { createRelayBuilder } from "./relay.js";
import { collectionHostModule } from "./collection-host-module.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { CollectionHost } from "./relay-types.js";
import type { RelayInstance } from "./relay-types.js";

let relay: RelayInstance;

beforeAll(async () => {
  const id = await createIdentity({ method: "key" });
  relay = createRelayBuilder({ relayDid: id.did })
    .use(collectionHostModule())
    .build();
  await relay.start();
});

describe("collectionHostModule", () => {
  it("registers the collections capability", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS);
    expect(host).toBeDefined();
  });

  it("creates a collection", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    const store = host.create("col-1");
    expect(store).toBeDefined();
    expect(store.objectCount()).toBe(0);
  });

  it("returns existing collection on duplicate create", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    const a = host.create("col-dup");
    const b = host.create("col-dup");
    expect(a).toBe(b);
  });

  it("lists collection IDs", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("col-list-1");
    host.create("col-list-2");
    const ids = host.list();
    expect(ids).toContain("col-list-1");
    expect(ids).toContain("col-list-2");
  });

  it("gets a collection by ID", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("col-get");
    expect(host.get("col-get")).toBeDefined();
    expect(host.get("nonexistent")).toBeUndefined();
  });

  it("removes a collection", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    host.create("col-rm");
    expect(host.remove("col-rm")).toBe(true);
    expect(host.get("col-rm")).toBeUndefined();
    expect(host.remove("col-rm")).toBe(false);
  });

  it("supports CRDT snapshot export/import", () => {
    const host = relay.getCapability<CollectionHost>(RELAY_CAPABILITIES.COLLECTIONS) as CollectionHost;
    const store = host.create("col-sync");
    store.putObject({
      id: "obj-1" as never,
      type: "page",
      name: "Test Page",
      parentId: null,
      position: 0,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    } as never);
    expect(store.objectCount()).toBe(1);

    const snapshot = store.exportSnapshot();
    expect(snapshot).toBeInstanceOf(Uint8Array);
    expect(snapshot.length).toBeGreaterThan(0);
  });
});
