import { describe, it, expect, beforeEach } from "vitest";
import { vaultHostModule } from "./vault-host-module.js";
import type { VaultHost, RelayContext } from "./relay-types.js";
import { RELAY_CAPABILITIES } from "./relay-types.js";
import type { PrismManifest } from "@prism/core/manifest";

function makeManifest(overrides?: Partial<PrismManifest>): PrismManifest {
  return {
    id: overrides?.id ?? crypto.randomUUID(),
    name: overrides?.name ?? "Test Vault",
    version: "1",
    storage: { backend: "loro", path: "data" },
    schema: { module: "@prism/core" },
    createdAt: new Date().toISOString(),
    description: overrides?.description,
    ...overrides,
  };
}

function makeSnapshot(content: string): Uint8Array {
  return new TextEncoder().encode(content);
}

describe("vault-host-module", () => {
  let host: VaultHost;

  beforeEach(() => {
    const mod = vaultHostModule();
    let captured: VaultHost | undefined;
    const ctx = {
      setCapability(name: string, cap: unknown) {
        if (name === RELAY_CAPABILITIES.VAULT_HOST) captured = cap as VaultHost;
      },
      getCapability: () => undefined,
      onMessage: () => () => {},
      broadcast: () => {},
    } as unknown as RelayContext;
    mod.install(ctx);
    expect(captured).toBeDefined();
    host = captured as VaultHost;
  });

  // ── publish ──────────────────────────────────────────────────────────────

  describe("publish", () => {
    it("creates a hosted vault and returns metadata", () => {
      const manifest = makeManifest({ name: "My Vault" });
      const result = host.publish({
        manifest,
        ownerDid: "did:key:zOwner",
        collections: { col1: makeSnapshot("data1") },
      });

      expect(result.id).toBe(manifest.id);
      expect(result.manifest).toBe(manifest);
      expect(result.ownerDid).toBe("did:key:zOwner");
      expect(result.isPublic).toBe(true);
      expect(result.totalBytes).toBe(5); // "data1"
      expect(result.hostedAt).toBeTruthy();
      expect(result.updatedAt).toBeTruthy();
    });

    it("defaults isPublic to true", () => {
      const result = host.publish({
        manifest: makeManifest(),
        ownerDid: "did:key:z1",
        collections: {},
      });
      expect(result.isPublic).toBe(true);
    });

    it("respects isPublic=false", () => {
      const result = host.publish({
        manifest: makeManifest(),
        ownerDid: "did:key:z1",
        isPublic: false,
        collections: {},
      });
      expect(result.isPublic).toBe(false);
    });

    it("replaces an existing vault with same ID", () => {
      const manifest = makeManifest({ name: "V1" });
      host.publish({ manifest, ownerDid: "did:key:z1", collections: { a: makeSnapshot("old") } });
      const updated = host.publish({
        manifest: { ...manifest, name: "V2" },
        ownerDid: "did:key:z2",
        collections: { a: makeSnapshot("new") },
      });
      expect(updated.manifest.name).toBe("V2");
      expect(updated.ownerDid).toBe("did:key:z2");
      expect(host.list().length).toBe(1);
    });
  });

  // ── get ──────────────────────────────────────────────────────────────────

  describe("get", () => {
    it("returns published vault", () => {
      const manifest = makeManifest();
      host.publish({ manifest, ownerDid: "did:key:z1", collections: {} });
      const found = host.get(manifest.id);
      expect(found).toBeDefined();
      expect(found?.id).toBe(manifest.id);
    });

    it("returns undefined for unknown vault", () => {
      expect(host.get("nonexistent")).toBeUndefined();
    });
  });

  // ── list ─────────────────────────────────────────────────────────────────

  describe("list", () => {
    it("lists all vaults", () => {
      host.publish({ manifest: makeManifest(), ownerDid: "did:key:z1", collections: {} });
      host.publish({ manifest: makeManifest(), ownerDid: "did:key:z2", isPublic: false, collections: {} });
      expect(host.list().length).toBe(2);
    });

    it("filters to public only", () => {
      host.publish({ manifest: makeManifest(), ownerDid: "did:key:z1", collections: {} });
      host.publish({ manifest: makeManifest(), ownerDid: "did:key:z2", isPublic: false, collections: {} });
      expect(host.list({ publicOnly: true }).length).toBe(1);
    });
  });

  // ── snapshots ────────────────────────────────────────────────────────────

  describe("getSnapshot", () => {
    it("returns a specific collection snapshot", () => {
      const manifest = makeManifest();
      host.publish({
        manifest,
        ownerDid: "did:key:z1",
        collections: {
          col1: makeSnapshot("hello"),
          col2: makeSnapshot("world"),
        },
      });

      const snap = host.getSnapshot(manifest.id, "col1");
      expect(snap).toBeDefined();
      expect(new TextDecoder().decode(snap as Uint8Array)).toBe("hello");
    });

    it("returns undefined for unknown collection", () => {
      const manifest = makeManifest();
      host.publish({ manifest, ownerDid: "did:key:z1", collections: {} });
      expect(host.getSnapshot(manifest.id, "nope")).toBeUndefined();
    });

    it("returns undefined for unknown vault", () => {
      expect(host.getSnapshot("nope", "col1")).toBeUndefined();
    });
  });

  describe("getAllSnapshots", () => {
    it("returns all snapshots for a vault", () => {
      const manifest = makeManifest();
      host.publish({
        manifest,
        ownerDid: "did:key:z1",
        collections: {
          a: makeSnapshot("alpha"),
          b: makeSnapshot("beta"),
        },
      });

      const all = host.getAllSnapshots(manifest.id);
      expect(all).toBeDefined();
      expect(Object.keys(all as Record<string, Uint8Array>)).toEqual(["a", "b"]);
    });

    it("returns undefined for unknown vault", () => {
      expect(host.getAllSnapshots("nope")).toBeUndefined();
    });
  });

  // ── updateCollections ────────────────────────────────────────────────────

  describe("updateCollections", () => {
    it("updates snapshots for owner", () => {
      const manifest = makeManifest();
      host.publish({
        manifest,
        ownerDid: "did:key:zOwner",
        collections: { col1: makeSnapshot("v1") },
      });

      const ok = host.updateCollections(manifest.id, "did:key:zOwner", {
        col1: makeSnapshot("v2"),
        col2: makeSnapshot("new"),
      });
      expect(ok).toBe(true);

      expect(new TextDecoder().decode(host.getSnapshot(manifest.id, "col1") as Uint8Array)).toBe("v2");
      expect(host.getSnapshot(manifest.id, "col2")).toBeDefined();
    });

    it("rejects update from non-owner", () => {
      const manifest = makeManifest();
      host.publish({
        manifest,
        ownerDid: "did:key:zOwner",
        collections: { col1: makeSnapshot("v1") },
      });

      const ok = host.updateCollections(manifest.id, "did:key:zStranger", {
        col1: makeSnapshot("hacked"),
      });
      expect(ok).toBe(false);
      expect(new TextDecoder().decode(host.getSnapshot(manifest.id, "col1") as Uint8Array)).toBe("v1");
    });

    it("updates totalBytes after adding data", () => {
      const manifest = makeManifest();
      host.publish({
        manifest,
        ownerDid: "did:key:zOwner",
        collections: { col1: makeSnapshot("short") },
      });

      host.updateCollections(manifest.id, "did:key:zOwner", {
        col2: makeSnapshot("another collection with more data"),
      });

      const updated = host.get(manifest.id);
      // col1 ("short" = 5 bytes) + col2 ("another collection with more data" = 34 bytes)
      expect(updated?.totalBytes).toBeGreaterThan(5);
      expect(updated?.updatedAt).toBeTruthy();
    });
  });

  // ── remove ───────────────────────────────────────────────────────────────

  describe("remove", () => {
    it("removes vault for owner", () => {
      const manifest = makeManifest();
      host.publish({ manifest, ownerDid: "did:key:zOwner", collections: { a: makeSnapshot("x") } });
      expect(host.remove(manifest.id, "did:key:zOwner")).toBe(true);
      expect(host.get(manifest.id)).toBeUndefined();
      expect(host.getAllSnapshots(manifest.id)).toBeUndefined();
    });

    it("rejects removal from non-owner", () => {
      const manifest = makeManifest();
      host.publish({ manifest, ownerDid: "did:key:zOwner", collections: {} });
      expect(host.remove(manifest.id, "did:key:zStranger")).toBe(false);
      expect(host.get(manifest.id)).toBeDefined();
    });

    it("returns false for unknown vault", () => {
      expect(host.remove("nope", "did:key:z1")).toBe(false);
    });
  });

  // ── search ───────────────────────────────────────────────────────────────

  describe("search", () => {
    it("finds vaults by name", () => {
      host.publish({ manifest: makeManifest({ name: "Music Production" }), ownerDid: "did:key:z1", collections: {} });
      host.publish({ manifest: makeManifest({ name: "Task Tracker" }), ownerDid: "did:key:z2", collections: {} });

      const results = host.search("music");
      expect(results.length).toBe(1);
      expect(results[0].manifest.name).toBe("Music Production");
    });

    it("finds vaults by description", () => {
      host.publish({
        manifest: makeManifest({ name: "Generic", description: "Contains audio stems" }),
        ownerDid: "did:key:z1",
        collections: {},
      });

      const results = host.search("audio");
      expect(results.length).toBe(1);
    });

    it("search is case-insensitive", () => {
      host.publish({ manifest: makeManifest({ name: "My VAULT" }), ownerDid: "did:key:z1", collections: {} });
      expect(host.search("my vault").length).toBe(1);
      expect(host.search("MY VAULT").length).toBe(1);
    });

    it("returns empty array for no matches", () => {
      host.publish({ manifest: makeManifest({ name: "Abc" }), ownerDid: "did:key:z1", collections: {} });
      expect(host.search("xyz").length).toBe(0);
    });
  });
});
