import { describe, it, expect } from "vitest";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { extractMarkers, computeBounds } from "./map-widget-renderer.js";

function obj(id: string, data: Record<string, unknown>): GraphObject {
  return {
    id: id as ObjectId,
    type: "place",
    name: id,
    data,
    createdAt: 0,
    updatedAt: 0,
  } as unknown as GraphObject;
}

describe("extractMarkers", () => {
  it("keeps only objects with valid numeric lat/lng", () => {
    const markers = extractMarkers(
      [
        obj("a", { lat: 40.7, lng: -74 }),
        obj("b", { lat: "invalid", lng: 0 }),
        obj("c", { lat: 0 }),
        obj("d", { lat: 91, lng: 0 }), // out of range
        obj("e", { lat: 0, lng: -181 }), // out of range
      ],
      "lat",
      "lng",
      "name",
    );
    expect(markers).toHaveLength(1);
    expect(markers[0]?.id).toBe("a");
  });

  it("uses titleField for marker labels", () => {
    const markers = extractMarkers([obj("a", { lat: 1, lng: 2, title: "NYC" })], "lat", "lng", "title");
    expect(markers[0]?.title).toBe("NYC");
  });
});

describe("computeBounds", () => {
  it("returns a padded bounding box around markers", () => {
    const bounds = computeBounds([
      { id: "a", lat: 10, lng: 20, title: "a" },
      { id: "b", lat: 30, lng: 50, title: "b" },
    ]);
    expect(bounds.minLat).toBeLessThan(10);
    expect(bounds.maxLat).toBeGreaterThan(30);
    expect(bounds.minLng).toBeLessThan(20);
    expect(bounds.maxLng).toBeGreaterThan(50);
  });

  it("pads a single-marker bounds by at least 0.5 degrees", () => {
    const bounds = computeBounds([{ id: "a", lat: 10, lng: 20, title: "a" }]);
    expect(bounds.maxLat - bounds.minLat).toBeGreaterThanOrEqual(1);
    expect(bounds.maxLng - bounds.minLng).toBeGreaterThanOrEqual(1);
  });

  it("falls back to world bounds for empty input", () => {
    const bounds = computeBounds([]);
    expect(bounds).toEqual({ minLat: -90, maxLat: 90, minLng: -180, maxLng: 180 });
  });
});
