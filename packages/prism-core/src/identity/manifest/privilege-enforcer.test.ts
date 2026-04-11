import { describe, it, expect } from "vitest";
import type { GraphObject } from "@prism/core/object-model";
import { createPrivilegeSet } from "./privilege-set.js";
import { createPrivilegeEnforcer, type PrivilegeContext } from "./privilege-enforcer.js";

function makeObject(data: Record<string, unknown>, overrides?: Partial<GraphObject>): GraphObject {
  return {
    id: "obj-1",
    type: "test",
    name: "Test",
    parentId: null,
    position: 0,
    status: "active",
    tags: [],
    date: null,
    endDate: null,
    description: "",
    color: null,
    image: null,
    pinned: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    deletedAt: null,
    data,
    ...overrides,
  };
}

const ctx: PrivilegeContext = { currentDid: "did:key:alice", currentRole: "admin" };

// ── Basic permission checks ─────────────────────────────────────────────────

describe("PrivilegeEnforcer", () => {
  const adminSet = createPrivilegeSet("admin", "Admin", {
    collections: { "*": "full" },
  });

  const clientSet = createPrivilegeSet("client", "Client", {
    collections: { invoices: "read", contacts: "none", "*": "none" },
    fields: { "invoices.cost_breakdown": "hidden", "invoices.notes": "readonly" },
    layouts: { "admin-dashboard": "hidden", "*": "visible" },
  });

  describe("canRead / canWrite", () => {
    it("admin can read and write everything", () => {
      const enforcer = createPrivilegeEnforcer(adminSet);
      expect(enforcer.canRead("invoices")).toBe(true);
      expect(enforcer.canWrite("invoices")).toBe(true);
    });

    it("client can read invoices but not write", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canRead("invoices")).toBe(true);
      expect(enforcer.canWrite("invoices")).toBe(false);
    });

    it("client cannot read contacts", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canRead("contacts")).toBe(false);
    });
  });

  describe("field permissions", () => {
    it("admin has readwrite on all fields", () => {
      const enforcer = createPrivilegeEnforcer(adminSet);
      expect(enforcer.canEditField("invoices", "amount")).toBe(true);
      expect(enforcer.canSeeField("invoices", "amount")).toBe(true);
    });

    it("client cannot see hidden fields", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canSeeField("invoices", "cost_breakdown")).toBe(false);
      expect(enforcer.canEditField("invoices", "cost_breakdown")).toBe(false);
    });

    it("client can see but not edit readonly fields", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canSeeField("invoices", "notes")).toBe(true);
      expect(enforcer.canEditField("invoices", "notes")).toBe(false);
    });

    it("client gets readonly for unspecified invoice fields", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.fieldPermission("invoices", "amount")).toBe("readonly");
    });
  });

  describe("layout permissions", () => {
    it("client cannot see admin dashboard", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canSeeLayout("admin-dashboard")).toBe(false);
    });

    it("client can see other layouts via wildcard", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      expect(enforcer.canSeeLayout("invoice-detail")).toBe(true);
    });
  });

  describe("filterObjects", () => {
    it("returns empty when collection is not readable", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      const objects = [makeObject({})];
      expect(enforcer.filterObjects("contacts", objects, ctx)).toEqual([]);
    });

    it("returns all when no recordFilter", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      const objects = [makeObject({}), makeObject({})];
      expect(enforcer.filterObjects("invoices", objects, ctx)).toHaveLength(2);
    });

    it("filters by recordFilter expression", () => {
      const filteredSet = createPrivilegeSet("filtered", "Filtered", {
        collections: { "*": "read" },
        recordFilter: "record.owner_did == current_did",
      });
      const enforcer = createPrivilegeEnforcer(filteredSet);
      const objects = [
        makeObject({ owner_did: "did:key:alice" }, { id: "a" }),
        makeObject({ owner_did: "did:key:bob" }, { id: "b" }),
        makeObject({ owner_did: "did:key:alice" }, { id: "c" }),
      ];
      const filtered = enforcer.filterObjects("invoices", objects, ctx);
      expect(filtered).toHaveLength(2);
      expect(filtered.map((o) => o.id)).toEqual(["a", "c"]);
    });

    it("supports != in recordFilter", () => {
      const filteredSet = createPrivilegeSet("f", "F", {
        collections: { "*": "read" },
        recordFilter: 'record.status != "archived"',
      });
      const enforcer = createPrivilegeEnforcer(filteredSet);
      const objects = [
        makeObject({ status: "active" }),
        makeObject({ status: "archived" }),
      ];
      expect(enforcer.filterObjects("col", objects, ctx)).toHaveLength(1);
    });
  });

  describe("redactObject", () => {
    it("strips hidden fields from data", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      const obj = makeObject({ amount: 500, cost_breakdown: "internal", notes: "note" });
      const redacted = enforcer.redactObject("invoices", obj);
      expect(redacted.data).toHaveProperty("amount");
      expect(redacted.data).toHaveProperty("notes");
      expect(redacted.data).not.toHaveProperty("cost_breakdown");
    });

    it("returns unmodified when no field overrides", () => {
      const enforcer = createPrivilegeEnforcer(adminSet);
      const obj = makeObject({ a: 1, b: 2 });
      expect(enforcer.redactObject("anything", obj)).toBe(obj);
    });
  });

  describe("visibleFields", () => {
    it("filters out hidden fields", () => {
      const enforcer = createPrivilegeEnforcer(clientSet);
      const all = ["amount", "cost_breakdown", "notes", "date"];
      const visible = enforcer.visibleFields("invoices", all);
      expect(visible).toEqual(["amount", "notes", "date"]);
    });
  });
});
