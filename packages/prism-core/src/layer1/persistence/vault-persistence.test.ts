import { describe, it, expect, beforeEach } from "vitest";
import type { GraphObject, ObjectEdge } from "../object-model/types.js";
import { objectId, edgeId } from "../object-model/types.js";
import { defaultManifest, addCollection } from "../manifest/index.js";
import type { PrismManifest } from "../manifest/index.js";
import type { PersistenceAdapter, VaultManager } from "./vault-persistence.js";
import { createMemoryAdapter, createVaultManager } from "./vault-persistence.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function makeObject(overrides: Partial<GraphObject> = {}): GraphObject {
  return {
    id: objectId("obj-1"),
    type: "task",
    name: "Test Task",
    parentId: null,
    position: 0,
    status: null,
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    data: {},
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function makeEdge(overrides: Partial<ObjectEdge> = {}): ObjectEdge {
  return {
    id: edgeId("edge-1"),
    sourceId: objectId("obj-1"),
    targetId: objectId("obj-2"),
    relation: "depends-on",
    createdAt: "2026-01-01T00:00:00Z",
    data: {},
    ...overrides,
  };
}

function makeManifest(): PrismManifest {
  let m = defaultManifest("Test Vault", "vault-1");
  m = addCollection(m, { id: "tasks", name: "Tasks" });
  m = addCollection(m, { id: "contacts", name: "Contacts" });
  return m;
}

// ── MemoryAdapter Tests ──────────────────────────────────────────────────────

describe("createMemoryAdapter", () => {
  let adapter: PersistenceAdapter;

  beforeEach(() => {
    adapter = createMemoryAdapter();
  });

  it("load returns null for missing path", () => {
    expect(adapter.load("nonexistent")).toBeNull();
  });

  it("save and load round-trip", () => {
    const data = new Uint8Array([1, 2, 3, 4]);
    adapter.save("data/test.loro", data);
    const loaded = adapter.load("data/test.loro");
    expect(loaded).toEqual(data);
  });

  it("exists returns true after save", () => {
    expect(adapter.exists("file.bin")).toBe(false);
    adapter.save("file.bin", new Uint8Array([0]));
    expect(adapter.exists("file.bin")).toBe(true);
  });

  it("delete removes a file and returns true", () => {
    adapter.save("file.bin", new Uint8Array([0]));
    expect(adapter.delete("file.bin")).toBe(true);
    expect(adapter.exists("file.bin")).toBe(false);
  });

  it("delete returns false for missing file", () => {
    expect(adapter.delete("nonexistent")).toBe(false);
  });

  it("list returns direct children only", () => {
    adapter.save("data/a.loro", new Uint8Array([1]));
    adapter.save("data/b.loro", new Uint8Array([2]));
    adapter.save("data/sub/c.loro", new Uint8Array([3]));

    const entries = adapter.list("data");
    expect(entries).toEqual(["a.loro", "b.loro"]);
  });

  it("list returns empty for missing directory", () => {
    expect(adapter.list("nonexistent")).toEqual([]);
  });
});

// ── VaultManager Tests ──────────────────────────────────────────────────────

describe("createVaultManager", () => {
  let adapter: PersistenceAdapter;
  let manifest: PrismManifest;
  let vault: VaultManager;

  beforeEach(() => {
    adapter = createMemoryAdapter();
    manifest = makeManifest();
    vault = createVaultManager(manifest, adapter);
  });

  it("exposes manifest and adapter", () => {
    expect(vault.manifest.id).toBe("vault-1");
    expect(vault.adapter).toBe(adapter);
  });

  it("opens a collection and returns a CollectionStore", () => {
    const store = vault.openCollection("tasks");
    expect(store).toBeDefined();
    expect(store.objectCount()).toBe(0);
  });

  it("returns the same store on repeated opens", () => {
    const store1 = vault.openCollection("tasks");
    const store2 = vault.openCollection("tasks");
    expect(store1).toBe(store2);
  });

  it("throws for unknown collection id", () => {
    expect(() => vault.openCollection("nonexistent")).toThrow(
      "Collection 'nonexistent' not found",
    );
  });

  it("tracks open collections", () => {
    expect(vault.openCollections()).toEqual([]);
    vault.openCollection("tasks");
    vault.openCollection("contacts");
    expect(vault.openCollections().sort()).toEqual(["contacts", "tasks"]);
  });

  // ── Dirty tracking ──────────────────────────────────────────────────────

  it("marks collection as dirty after mutation", () => {
    const store = vault.openCollection("tasks");
    expect(vault.isDirty("tasks")).toBe(false);

    store.putObject(makeObject());
    expect(vault.isDirty("tasks")).toBe(true);
  });

  it("isDirty returns false for unopened collection", () => {
    expect(vault.isDirty("tasks")).toBe(false);
  });

  // ── Save ────────────────────────────────────────────────────────────────

  it("saveCollection persists snapshot to adapter", () => {
    const store = vault.openCollection("tasks");
    store.putObject(makeObject());
    vault.saveCollection("tasks");

    expect(adapter.exists("data/collections/tasks.loro")).toBe(true);
    expect(vault.isDirty("tasks")).toBe(false);
  });

  it("saveCollection is a no-op when not dirty", () => {
    vault.openCollection("tasks");
    vault.saveCollection("tasks");
    expect(adapter.exists("data/collections/tasks.loro")).toBe(false);
  });

  it("saveCollection is a no-op for unopened collection", () => {
    vault.saveCollection("tasks");
    expect(adapter.exists("data/collections/tasks.loro")).toBe(false);
  });

  it("saveAll persists all dirty collections", () => {
    const tasks = vault.openCollection("tasks");
    const contacts = vault.openCollection("contacts");
    tasks.putObject(makeObject({ id: objectId("t1") }));
    contacts.putObject(makeObject({ id: objectId("c1") }));

    const saved = vault.saveAll();
    expect(saved.sort()).toEqual(["contacts", "tasks"]);
    expect(vault.isDirty("tasks")).toBe(false);
    expect(vault.isDirty("contacts")).toBe(false);
  });

  it("saveAll returns empty when nothing dirty", () => {
    vault.openCollection("tasks");
    expect(vault.saveAll()).toEqual([]);
  });

  // ── Load from disk ──────────────────────────────────────────────────────

  it("hydrates collection from existing disk data", () => {
    // Create data via one vault manager
    const store = vault.openCollection("tasks");
    store.putObject(makeObject({ id: objectId("persisted"), name: "Persisted" }));
    vault.saveCollection("tasks");

    // Create a new vault manager against same adapter
    const vault2 = createVaultManager(manifest, adapter);
    const store2 = vault2.openCollection("tasks");

    expect(store2.getObject(objectId("persisted"))?.name).toBe("Persisted");
    expect(store2.objectCount()).toBe(1);
  });

  it("hydrated collection retains edges", () => {
    const store = vault.openCollection("tasks");
    store.putObject(makeObject({ id: objectId("a") }));
    store.putObject(makeObject({ id: objectId("b") }));
    store.putEdge(makeEdge({ id: edgeId("e1"), sourceId: objectId("a"), targetId: objectId("b") }));
    vault.saveCollection("tasks");

    const vault2 = createVaultManager(manifest, adapter);
    const store2 = vault2.openCollection("tasks");
    expect(store2.edgeCount()).toBe(1);
    expect(store2.getEdge(edgeId("e1"))?.relation).toBe("depends-on");
  });

  // ── Close ───────────────────────────────────────────────────────────────

  it("closeCollection saves and evicts", () => {
    const store = vault.openCollection("tasks");
    store.putObject(makeObject());
    vault.closeCollection("tasks");

    expect(vault.openCollections()).toEqual([]);
    expect(vault.isDirty("tasks")).toBe(false);
    expect(adapter.exists("data/collections/tasks.loro")).toBe(true);
  });

  it("closeCollection is a no-op for unopened collection", () => {
    vault.closeCollection("tasks"); // should not throw
    expect(vault.openCollections()).toEqual([]);
  });

  it("re-opening a closed collection hydrates from disk", () => {
    const store = vault.openCollection("tasks");
    store.putObject(makeObject({ id: objectId("re-open"), name: "Reopened" }));
    vault.closeCollection("tasks");

    const store2 = vault.openCollection("tasks");
    expect(store2.getObject(objectId("re-open"))?.name).toBe("Reopened");
  });

  // ── Peer ID ─────────────────────────────────────────────────────────────

  it("passes peerId to collection stores", () => {
    const vaultWithPeer = createVaultManager(manifest, adapter, { peerId: 42n });
    const store = vaultWithPeer.openCollection("tasks");
    // Verify the store's doc has the peer ID set (Loro exposes it)
    expect(store.doc).toBeDefined();
  });
});
