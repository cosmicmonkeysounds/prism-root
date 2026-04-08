import { describe, it, expect } from "vitest";
import {
  createPrivilegeSet,
  getCollectionPermission,
  getFieldPermission,
  getLayoutPermission,
  getScriptPermission,
  canWrite,
  canRead,
} from "./privilege-set.js";

// ── createPrivilegeSet factory ──────────────────────────────────────────────

describe("createPrivilegeSet", () => {
  it("creates a privilege set with id and name", () => {
    const ps = createPrivilegeSet("admin", "Administrator", {
      collections: { "*": "full" },
    });
    expect(ps.id).toBe("admin");
    expect(ps.name).toBe("Administrator");
  });

  it("copies collection permissions", () => {
    const collections = { invoices: "read" as const, contacts: "full" as const };
    const ps = createPrivilegeSet("viewer", "Viewer", { collections });
    collections.invoices = "full";
    expect(ps.collections.invoices).toBe("read");
  });

  it("includes optional fields when provided", () => {
    const ps = createPrivilegeSet("client", "Client", {
      collections: { "*": "read" },
      fields: { "invoices.cost": "hidden" },
      layouts: { "admin-panel": "hidden" },
      scripts: { "delete-all": "none" },
      recordFilter: "record.owner == current_did",
      isDefault: true,
      canManageAccess: false,
    });
    expect(ps.fields?.["invoices.cost"]).toBe("hidden");
    expect(ps.layouts?.["admin-panel"]).toBe("hidden");
    expect(ps.scripts?.["delete-all"]).toBe("none");
    expect(ps.recordFilter).toBe("record.owner == current_did");
    expect(ps.isDefault).toBe(true);
    expect(ps.canManageAccess).toBe(false);
  });

  it("omits optional fields when not provided", () => {
    const ps = createPrivilegeSet("basic", "Basic", {
      collections: { "*": "read" },
    });
    expect(ps.fields).toBeUndefined();
    expect(ps.layouts).toBeUndefined();
    expect(ps.scripts).toBeUndefined();
    expect(ps.recordFilter).toBeUndefined();
  });
});

// ── getCollectionPermission ─────────────────────────────────────────────────

describe("getCollectionPermission", () => {
  const admin = createPrivilegeSet("admin", "Admin", {
    collections: { "*": "full" },
  });

  const mixed = createPrivilegeSet("mixed", "Mixed", {
    collections: {
      invoices: "read",
      contacts: "full",
      "*": "none",
    },
  });

  it("returns specific collection permission", () => {
    expect(getCollectionPermission(mixed, "invoices")).toBe("read");
    expect(getCollectionPermission(mixed, "contacts")).toBe("full");
  });

  it("falls back to wildcard", () => {
    expect(getCollectionPermission(admin, "anything")).toBe("full");
    expect(getCollectionPermission(mixed, "unknown")).toBe("none");
  });

  it("returns none when no wildcard and no match", () => {
    const strict = createPrivilegeSet("strict", "Strict", {
      collections: { invoices: "read" },
    });
    expect(getCollectionPermission(strict, "contacts")).toBe("none");
  });
});

// ── getFieldPermission ──────────────────────────────────────────────────────

describe("getFieldPermission", () => {
  const ps = createPrivilegeSet("client", "Client", {
    collections: { invoices: "read", contacts: "full" },
    fields: {
      "invoices.cost_breakdown": "hidden",
      "contacts.email": "readonly",
    },
  });

  it("returns explicit field permission", () => {
    expect(getFieldPermission(ps, "invoices", "cost_breakdown")).toBe("hidden");
    expect(getFieldPermission(ps, "contacts", "email")).toBe("readonly");
  });

  it("derives from collection permission when field not specified", () => {
    // invoices is read → readonly
    expect(getFieldPermission(ps, "invoices", "date")).toBe("readonly");
    // contacts is full → readwrite
    expect(getFieldPermission(ps, "contacts", "name")).toBe("readwrite");
  });

  it("returns hidden when collection is none", () => {
    const strict = createPrivilegeSet("s", "S", {
      collections: { secret: "none" },
    });
    expect(getFieldPermission(strict, "secret", "data")).toBe("hidden");
  });

  it("derives readwrite from create permission", () => {
    const creator = createPrivilegeSet("c", "C", {
      collections: { items: "create" },
    });
    expect(getFieldPermission(creator, "items", "name")).toBe("readwrite");
  });
});

// ── getLayoutPermission ─────────────────────────────────────────────────────

describe("getLayoutPermission", () => {
  it("returns specific layout permission", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: {},
      layouts: { "admin-dashboard": "hidden", "public-view": "visible" },
    });
    expect(getLayoutPermission(ps, "admin-dashboard")).toBe("hidden");
    expect(getLayoutPermission(ps, "public-view")).toBe("visible");
  });

  it("falls back to wildcard", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: {},
      layouts: { "*": "hidden" },
    });
    expect(getLayoutPermission(ps, "any-layout")).toBe("hidden");
  });

  it("defaults to visible when no layouts defined", () => {
    const ps = createPrivilegeSet("p", "P", { collections: {} });
    expect(getLayoutPermission(ps, "anything")).toBe("visible");
  });
});

// ── getScriptPermission ─────────────────────────────────────────────────────

describe("getScriptPermission", () => {
  it("returns specific script permission", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: {},
      scripts: { "safe-script": "execute", "dangerous-script": "none" },
    });
    expect(getScriptPermission(ps, "safe-script")).toBe("execute");
    expect(getScriptPermission(ps, "dangerous-script")).toBe("none");
  });

  it("falls back to wildcard", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: {},
      scripts: { "*": "execute" },
    });
    expect(getScriptPermission(ps, "any-script")).toBe("execute");
  });

  it("defaults to none when no scripts defined", () => {
    const ps = createPrivilegeSet("p", "P", { collections: {} });
    expect(getScriptPermission(ps, "anything")).toBe("none");
  });
});

// ── canWrite / canRead helpers ──────────────────────────────────────────────

describe("canWrite", () => {
  it("returns true for full and create", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: { a: "full", b: "create", c: "read", d: "none" },
    });
    expect(canWrite(ps, "a")).toBe(true);
    expect(canWrite(ps, "b")).toBe(true);
  });

  it("returns false for read and none", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: { c: "read", d: "none" },
    });
    expect(canWrite(ps, "c")).toBe(false);
    expect(canWrite(ps, "d")).toBe(false);
  });
});

describe("canRead", () => {
  it("returns true for full, create, and read", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: { a: "full", b: "create", c: "read" },
    });
    expect(canRead(ps, "a")).toBe(true);
    expect(canRead(ps, "b")).toBe(true);
    expect(canRead(ps, "c")).toBe(true);
  });

  it("returns false for none", () => {
    const ps = createPrivilegeSet("p", "P", {
      collections: { d: "none" },
    });
    expect(canRead(ps, "d")).toBe(false);
  });
});
