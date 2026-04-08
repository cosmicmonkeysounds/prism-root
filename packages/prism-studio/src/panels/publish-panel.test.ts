/**
 * Tests for publish-panel pure helpers (status transitions + colors).
 */

import { describe, it, expect } from "vitest";
import { nextStatus, statusColor } from "./publish-panel.js";

describe("nextStatus", () => {
  it("advances draft → review", () => {
    expect(nextStatus("draft")).toBe("review");
  });

  it("advances review → published", () => {
    expect(nextStatus("review")).toBe("published");
  });

  it("stays at published once there", () => {
    expect(nextStatus("published")).toBe("published");
  });

  it("treats unknown status as draft baseline", () => {
    expect(nextStatus("archived")).toBe("draft");
    expect(nextStatus("")).toBe("draft");
  });
});

describe("statusColor", () => {
  it("returns a distinct color per status", () => {
    expect(statusColor("draft")).not.toBe(statusColor("review"));
    expect(statusColor("review")).not.toBe(statusColor("published"));
  });

  it("falls back to draft color for unknown", () => {
    expect(statusColor("weird")).toBe(statusColor("draft"));
  });
});
