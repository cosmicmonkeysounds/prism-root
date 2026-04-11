import { describe, it, expect, beforeEach, vi } from "vitest";
import { serialiseManifest, defaultManifest } from "@prism/core/manifest";
import type { VaultRoster } from "./vault-roster.js";
import { createVaultRoster } from "./vault-roster.js";
import type {
  MemoryDiscoveryAdapter,
  VaultDiscovery,
  DiscoveryEvent,
} from "./vault-discovery.js";
import {
  createMemoryDiscoveryAdapter,
  createVaultDiscovery,
} from "./vault-discovery.js";

// ── Helpers ───────────────────────────────────────────────────────────────────

function addVault(
  adapter: MemoryDiscoveryAdapter,
  parentDir: string,
  vaultName: string,
  manifestId: string,
  manifestName: string,
  extras: Record<string, unknown> = {},
): void {
  adapter.addDirectory(parentDir, vaultName);
  const vaultPath = `${parentDir}/${vaultName}`;
  const manifest = { ...defaultManifest(manifestName, manifestId), ...extras };
  adapter.addFile(`${vaultPath}/.prism.json`, serialiseManifest(manifest));
}

// ── Tests ────────────────────────────────────────────────────────────────────

describe("VaultDiscovery", () => {
  let adapter: MemoryDiscoveryAdapter;
  let roster: VaultRoster;
  let discovery: VaultDiscovery;

  beforeEach(() => {
    adapter = createMemoryDiscoveryAdapter();
    roster = createVaultRoster();
    discovery = createVaultDiscovery(adapter, roster);
  });

  // ── Scanning ───────────────────────────────────────────────────────────────

  describe("scan", () => {
    it("discovers vaults in search paths", () => {
      addVault(adapter, "/home/user/vaults", "project-a", "id-a", "Project A");
      addVault(adapter, "/home/user/vaults", "project-b", "id-b", "Project B");

      const results = discovery.scan({ searchPaths: ["/home/user/vaults"] });
      expect(results).toHaveLength(2);

      const names = results.map((r) => r.manifest.name).sort();
      expect(names).toEqual(["Project A", "Project B"]);
    });

    it("returns empty for empty directory", () => {
      const results = discovery.scan({ searchPaths: ["/empty"] });
      expect(results).toHaveLength(0);
    });

    it("skips directories without manifests", () => {
      adapter.addDirectory("/home/user/vaults", "no-manifest");
      addVault(adapter, "/home/user/vaults", "has-manifest", "id-1", "Valid");

      const results = discovery.scan({ searchPaths: ["/home/user/vaults"] });
      expect(results).toHaveLength(1);
      expect(results[0]?.manifest.name).toBe("Valid");
    });

    it("skips invalid manifests", () => {
      adapter.addDirectory("/home/user/vaults", "bad");
      adapter.addFile("/home/user/vaults/bad/.prism.json", "not json at all");

      addVault(adapter, "/home/user/vaults", "good", "id-1", "Good");

      const results = discovery.scan({ searchPaths: ["/home/user/vaults"] });
      expect(results).toHaveLength(1);
    });

    it("discovers manifest at the search path itself", () => {
      const manifest = defaultManifest("Root Vault", "root-id");
      adapter.addFile("/my-vault/.prism.json", serialiseManifest(manifest));

      const results = discovery.scan({ searchPaths: ["/my-vault"] });
      expect(results).toHaveLength(1);
      expect(results[0]?.manifest.name).toBe("Root Vault");
    });

    it("scans multiple search paths", () => {
      addVault(adapter, "/vaults1", "a", "id-a", "Alpha");
      addVault(adapter, "/vaults2", "b", "id-b", "Beta");

      const results = discovery.scan({
        searchPaths: ["/vaults1", "/vaults2"],
      });
      expect(results).toHaveLength(2);
    });

    it("avoids duplicates when search path itself has a manifest", () => {
      // Search path has a manifest AND is also listed as a child
      const manifest = defaultManifest("Vault", "v-id");
      adapter.addFile("/parent/child/.prism.json", serialiseManifest(manifest));
      adapter.addDirectory("/parent", "child");

      const results = discovery.scan({ searchPaths: ["/parent/child"] });
      expect(results).toHaveLength(1);
    });
  });

  // ── Roster merge ───────────────────────────────────────────────────────────

  describe("roster merge", () => {
    it("adds discovered vaults to roster", () => {
      addVault(adapter, "/vaults", "proj", "id-1", "My Project", {
        description: "A cool project",
        collections: [{ id: "col-1", name: "Tasks" }],
      });

      discovery.scan({ searchPaths: ["/vaults"] });

      expect(roster.size()).toBe(1);
      const entry = roster.get("id-1");
      expect(entry?.name).toBe("My Project");
      expect(entry?.path).toBe("/vaults/proj");
      expect(entry?.description).toBe("A cool project");
      expect(entry?.collectionCount).toBe(1);
    });

    it("updates existing roster entries on rescan", () => {
      // First scan
      addVault(adapter, "/vaults", "proj", "id-1", "Original Name");
      discovery.scan({ searchPaths: ["/vaults"] });
      expect(roster.get("id-1")?.name).toBe("Original Name");

      // Update the manifest on disk
      const updated = defaultManifest("Updated Name", "id-1");
      adapter.addFile("/vaults/proj/.prism.json", serialiseManifest(updated));

      // Rescan
      discovery.scan({ searchPaths: ["/vaults"] });
      expect(roster.get("id-1")?.name).toBe("Updated Name");
      expect(roster.size()).toBe(1); // no duplicates
    });

    it("skips roster merge when mergeToRoster is false", () => {
      addVault(adapter, "/vaults", "proj", "id-1", "Project");

      discovery.scan({ searchPaths: ["/vaults"], mergeToRoster: false });
      expect(roster.size()).toBe(0);
    });
  });

  // ── Scan state ─────────────────────────────────────────────────────────────

  describe("scan state", () => {
    it("tracks lastScanAt", () => {
      expect(discovery.lastScanAt).toBeNull();

      discovery.scan({ searchPaths: ["/empty"] });
      expect(discovery.lastScanAt).toBeTruthy();
    });

    it("tracks lastScanCount", () => {
      expect(discovery.lastScanCount).toBe(0);

      addVault(adapter, "/vaults", "a", "id-a", "A");
      addVault(adapter, "/vaults", "b", "id-b", "B");
      discovery.scan({ searchPaths: ["/vaults"] });

      expect(discovery.lastScanCount).toBe(2);
    });

    it("scanning is false after scan completes", () => {
      discovery.scan({ searchPaths: ["/empty"] });
      expect(discovery.scanning).toBe(false);
    });
  });

  // ── Events ─────────────────────────────────────────────────────────────────

  describe("events", () => {
    it("emits scan-start and scan-complete", () => {
      const events: DiscoveryEvent[] = [];
      discovery.onEvent((e) => events.push(e));

      discovery.scan({ searchPaths: ["/empty"] });

      const types = events.map((e) => e.type);
      expect(types).toContain("scan-start");
      expect(types).toContain("scan-complete");
    });

    it("emits vault-found for each discovered vault", () => {
      addVault(adapter, "/vaults", "a", "id-a", "Alpha");
      addVault(adapter, "/vaults", "b", "id-b", "Beta");

      const events: DiscoveryEvent[] = [];
      discovery.onEvent((e) => events.push(e));

      discovery.scan({ searchPaths: ["/vaults"] });

      const found = events.filter((e) => e.type === "vault-found");
      expect(found).toHaveLength(2);
      expect(found[0]?.vault?.manifest.name).toBeDefined();
    });

    it("scan-complete includes results and scannedCount", () => {
      addVault(adapter, "/vaults", "a", "id-a", "Alpha");

      const events: DiscoveryEvent[] = [];
      discovery.onEvent((e) => events.push(e));

      discovery.scan({ searchPaths: ["/vaults"] });

      const complete = events.find((e) => e.type === "scan-complete");
      expect(complete?.results).toHaveLength(1);
      expect(complete?.scannedCount).toBeGreaterThan(0);
    });

    it("unsubscribes from events", () => {
      const handler = vi.fn();
      const unsub = discovery.onEvent(handler);

      discovery.scan({ searchPaths: ["/empty"] });
      const callCount = handler.mock.calls.length;

      unsub();
      discovery.scan({ searchPaths: ["/empty"] });
      expect(handler).toHaveBeenCalledTimes(callCount);
    });
  });

  // ── MemoryDiscoveryAdapter ─────────────────────────────────────────────────

  describe("MemoryDiscoveryAdapter", () => {
    it("lists directories", () => {
      adapter.addDirectory("/root", "child-a");
      adapter.addDirectory("/root", "child-b");

      const dirs = adapter.listDirectories("/root");
      expect(dirs).toEqual(["/root/child-a", "/root/child-b"]);
    });

    it("reads files", () => {
      adapter.addFile("/root/file.txt", "hello");
      expect(adapter.readFile("/root/file.txt")).toBe("hello");
      expect(adapter.readFile("/root/nope.txt")).toBeNull();
    });

    it("checks existence", () => {
      adapter.addFile("/root/file.txt", "hello");
      expect(adapter.exists("/root/file.txt")).toBe(true);
      expect(adapter.exists("/root/nope.txt")).toBe(false);
    });

    it("joins paths", () => {
      expect(adapter.joinPath("/root", "sub", "file.txt")).toBe("/root/sub/file.txt");
    });

    it("returns empty for unknown directory", () => {
      expect(adapter.listDirectories("/unknown")).toEqual([]);
    });
  });
});
