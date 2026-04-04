import { describe, it, expect } from "vitest";
import {
  isValidNSID,
  parseNSID,
  nsid,
  nsidAuthority,
  nsidName,
  isValidPrismAddress,
  prismAddress,
  parsePrismAddress,
  NSIDRegistry,
} from "./nsid.js";

describe("NSID", () => {
  it("validates correct NSIDs", () => {
    expect(isValidNSID("io.prismapp.task")).toBe(true);
    expect(isValidNSID("io.lattice.simulacra.character")).toBe(true);
    expect(isValidNSID("com.my-studio.game.faction")).toBe(true);
  });

  it("rejects invalid NSIDs", () => {
    expect(isValidNSID("task")).toBe(false); // too few segments
    expect(isValidNSID("io.task")).toBe(false); // only 2 segments
    expect(isValidNSID("io.UPPER.task")).toBe(false); // uppercase
    expect(isValidNSID("")).toBe(false);
  });

  it("parseNSID returns null for invalid", () => {
    expect(parseNSID("invalid")).toBeNull();
    expect(parseNSID("io.prismapp.task")).not.toBeNull();
  });

  it("nsid() constructs from parts", () => {
    expect(nsid("io.prismapp", "productivity", "task")).toBe(
      "io.prismapp.productivity.task",
    );
  });

  it("nsid() throws on invalid", () => {
    expect(() => nsid("BAD")).toThrow();
  });

  it("nsidAuthority extracts first two segments", () => {
    const id = nsid("io.prismapp", "productivity", "task");
    expect(nsidAuthority(id)).toBe("io.prismapp");
  });

  it("nsidName extracts last segment", () => {
    const id = nsid("io.prismapp", "productivity", "task");
    expect(nsidName(id)).toBe("task");
  });
});

describe("PrismAddress", () => {
  it("validates correct addresses", () => {
    expect(
      isValidPrismAddress("prism://did:web:node.example.com/objects/abc-123"),
    ).toBe(true);
  });

  it("rejects invalid addresses", () => {
    expect(isValidPrismAddress("helm://bad/objects/x")).toBe(false);
    expect(isValidPrismAddress("prism://no-objects-path")).toBe(false);
    expect(isValidPrismAddress("")).toBe(false);
  });

  it("builds addresses", () => {
    const addr = prismAddress("did:web:node.example.com", "abc-123");
    expect(addr).toBe("prism://did:web:node.example.com/objects/abc-123");
  });

  it("parses addresses", () => {
    const result = parsePrismAddress(
      "prism://did:web:node.example.com/objects/abc-123",
    );
    expect(result).toEqual({
      nodeDid: "did:web:node.example.com",
      objectId: "abc-123",
    });
  });

  it("returns null for invalid parse", () => {
    expect(parsePrismAddress("not-a-prism-address")).toBeNull();
  });
});

describe("NSIDRegistry", () => {
  it("registers and resolves mappings", () => {
    const reg = new NSIDRegistry();
    reg.register("task", "io.prismapp.productivity.task");

    expect(reg.getNSID("task")).toBe("io.prismapp.productivity.task");
    expect(reg.getLocalType("io.prismapp.productivity.task")).toBe("task");
    expect(reg.hasNSID("io.prismapp.productivity.task")).toBe(true);
  });

  it("throws on invalid NSID", () => {
    const reg = new NSIDRegistry();
    expect(() => reg.register("task", "bad")).toThrow();
  });

  it("clears all mappings", () => {
    const reg = new NSIDRegistry();
    reg.register("task", "io.prismapp.productivity.task");
    reg.clear();

    expect(reg.hasNSID("io.prismapp.productivity.task")).toBe(false);
  });
});
