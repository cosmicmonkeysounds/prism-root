import { describe, it, expect } from "vitest";
import {
  detectCadFormat,
  computeBoundingBox,
  mergeMeshes,
  createCadGeometryManager,
  createTestCadAdapter,
} from "./cad-geometry.js";
import type { TessellatedMesh, CadImportResult } from "./viewport3d-types.js";

describe("detectCadFormat", () => {
  it("detects STEP files", () => {
    expect(detectCadFormat("ISO-10303-21;\nHEADER;")).toBe("step");
  });

  it("detects IGES files", () => {
    expect(detectCadFormat("                                                                        S      1\nIGES")).toBe("iges");
  });

  it("detects BREP files", () => {
    expect(detectCadFormat("DBRep_DrawableShape\n")).toBe("brep");
  });

  it("returns null for unknown format", () => {
    expect(detectCadFormat("hello world")).toBeNull();
  });
});

describe("computeBoundingBox", () => {
  it("returns zero box for empty vertices", () => {
    const bb = computeBoundingBox(new Float32Array(0));
    expect(bb.min).toEqual([0, 0, 0]);
    expect(bb.max).toEqual([0, 0, 0]);
  });

  it("computes correct bounding box", () => {
    const verts = new Float32Array([
      -1, -2, -3,
      4, 5, 6,
      0, 0, 0,
    ]);
    const bb = computeBoundingBox(verts);
    expect(bb.min).toEqual([-1, -2, -3]);
    expect(bb.max).toEqual([4, 5, 6]);
  });

  it("handles single vertex", () => {
    const verts = new Float32Array([7, 8, 9]);
    const bb = computeBoundingBox(verts);
    expect(bb.min).toEqual([7, 8, 9]);
    expect(bb.max).toEqual([7, 8, 9]);
  });
});

describe("mergeMeshes", () => {
  it("returns empty mesh for empty input", () => {
    const merged = mergeMeshes([]);
    expect(merged.vertices.length).toBe(0);
    expect(merged.indices.length).toBe(0);
  });

  it("returns same mesh for single input", () => {
    const mesh: TessellatedMesh = {
      vertices: new Float32Array([0, 0, 0, 1, 1, 1]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 1]),
      faceGroups: [{ start: 0, count: 2 }],
    };
    const merged = mergeMeshes([mesh]);
    expect(merged).toBe(mesh);
  });

  it("merges two meshes with correct index offsets", () => {
    const m1: TessellatedMesh = {
      vertices: new Float32Array([0, 0, 0, 1, 0, 0]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 1]),
      faceGroups: [{ start: 0, count: 2 }],
    };
    const m2: TessellatedMesh = {
      vertices: new Float32Array([2, 0, 0, 3, 0, 0]),
      normals: new Float32Array([0, 1, 0, 0, 1, 0]),
      indices: new Uint32Array([0, 1]),
      faceGroups: [{ start: 0, count: 2, color: "#ff0000" }],
    };

    const merged = mergeMeshes([m1, m2]);
    expect(merged.vertices.length).toBe(12);
    expect(merged.indices.length).toBe(4);
    // Second mesh indices should be offset by vertex count of first
    expect(merged.indices[2]).toBe(2); // 0 + 2
    expect(merged.indices[3]).toBe(3); // 1 + 2
    expect(merged.faceGroups.length).toBe(2);
  });
});

describe("CadGeometryManager", () => {
  it("imports a file via the test adapter", async () => {
    const adapter = createTestCadAdapter();
    const manager = createCadGeometryManager(adapter);
    const data = new TextEncoder().encode("ISO-10303-21; dummy").buffer as ArrayBuffer;
    const result = await manager.importFile(data);
    expect(result.meshes.length).toBe(1);
    expect(result.faceCount).toBe(12);
    expect(result.edgeCount).toBe(18);
    manager.dispose();
  });

  it("imports with a custom mock result", async () => {
    const custom: CadImportResult = {
      meshes: [],
      boundingBox: { min: [0, 0, 0], max: [10, 10, 10] },
      faceCount: 42,
      edgeCount: 100,
    };
    const adapter = createTestCadAdapter(custom);
    const manager = createCadGeometryManager(adapter);
    const data = new ArrayBuffer(10);
    const result = await manager.importFile(data);
    expect(result.faceCount).toBe(42);
    manager.dispose();
  });

  it("rejects when disposed", async () => {
    const adapter = createTestCadAdapter();
    const manager = createCadGeometryManager(adapter);
    manager.dispose();
    await expect(manager.importFile(new ArrayBuffer(10))).rejects.toThrow("disposed");
  });

  it("rejects concurrent imports", async () => {
    const adapter = createTestCadAdapter();
    const manager = createCadGeometryManager(adapter);
    const data = new ArrayBuffer(10);
    const p1 = manager.importFile(data);
    await expect(manager.importFile(data)).rejects.toThrow("already in progress");
    await p1;
    manager.dispose();
  });

  it("returns correct deflection values", () => {
    const adapter = createTestCadAdapter();
    const manager = createCadGeometryManager(adapter);
    expect(manager.getDeflection("low")).toBe(0.5);
    expect(manager.getDeflection("medium")).toBe(0.1);
    expect(manager.getDeflection("high")).toBe(0.01);
    manager.dispose();
  });
});
