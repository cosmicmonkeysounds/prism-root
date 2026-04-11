/**
 * OpenCASCADE.js abstraction for CAD geometry import and tessellation.
 *
 * In production, opencascade.js runs in a Web Worker managed by the
 * Prism Daemon.  This module defines the import pipeline and
 * tessellation interface so Layer 2 components can request meshes
 * and receive R3F-compatible geometry data.
 */

import type {
  CadFileFormat,
  TessellationQuality,
  TessellatedMesh,
  CadImportResult,
  FaceGroup,
  Vec3,
  CadWorkerRequest,
  CadWorkerResponse,
} from "./viewport3d-types.js";

// ---------------------------------------------------------------------------
// Quality → linear deflection mapping
// ---------------------------------------------------------------------------

const QUALITY_DEFLECTION: Record<TessellationQuality, number> = {
  low: 0.5,
  medium: 0.1,
  high: 0.01,
};

// ---------------------------------------------------------------------------
// Format detection
// ---------------------------------------------------------------------------

const FORMAT_SIGNATURES: Record<CadFileFormat, readonly string[]> = {
  step: ["ISO-10303-21", "STEP"],
  iges: ["IGES", "INITIAL GRAPHICS"],
  brep: ["DBREP_DRAWABLESHAPE", "CASCADE"],
};

export function detectCadFormat(header: string): CadFileFormat | null {
  const upper = header.toUpperCase();
  for (const [fmt, sigs] of Object.entries(FORMAT_SIGNATURES)) {
    for (const sig of sigs) {
      if (upper.includes(sig)) return fmt as CadFileFormat;
    }
  }
  return null;
}

// ---------------------------------------------------------------------------
// Bounding box computation
// ---------------------------------------------------------------------------

export function computeBoundingBox(
  vertices: Float32Array,
): { min: Vec3; max: Vec3 } {
  if (vertices.length === 0) {
    return { min: [0, 0, 0], max: [0, 0, 0] };
  }

  let minX = Infinity, minY = Infinity, minZ = Infinity;
  let maxX = -Infinity, maxY = -Infinity, maxZ = -Infinity;

  for (let i = 0; i < vertices.length; i += 3) {
    const x = vertices[i] ?? 0;
    const y = vertices[i + 1] ?? 0;
    const z = vertices[i + 2] ?? 0;
    if (x < minX) minX = x;
    if (y < minY) minY = y;
    if (z < minZ) minZ = z;
    if (x > maxX) maxX = x;
    if (y > maxY) maxY = y;
    if (z > maxZ) maxZ = z;
  }

  return {
    min: [minX, minY, minZ],
    max: [maxX, maxY, maxZ],
  };
}

// ---------------------------------------------------------------------------
// Mesh merging
// ---------------------------------------------------------------------------

export function mergeMeshes(meshes: readonly TessellatedMesh[]): TessellatedMesh {
  if (meshes.length === 0) {
    return {
      vertices: new Float32Array(0),
      normals: new Float32Array(0),
      indices: new Uint32Array(0),
      faceGroups: [],
    };
  }

  if (meshes.length === 1) {
    const single = meshes[0];
    if (single) return single;
  }

  let totalVerts = 0;
  let totalIdx = 0;
  for (const m of meshes) {
    totalVerts += m.vertices.length;
    totalIdx += m.indices.length;
  }

  const vertices = new Float32Array(totalVerts);
  const normals = new Float32Array(totalVerts);
  const indices = new Uint32Array(totalIdx);
  const faceGroups: FaceGroup[] = [];

  let vertOffset = 0;
  let idxOffset = 0;
  let vertexCount = 0;

  for (const mesh of meshes) {
    vertices.set(mesh.vertices, vertOffset);
    normals.set(mesh.normals, vertOffset);

    for (let i = 0; i < mesh.indices.length; i++) {
      indices[idxOffset + i] = (mesh.indices[i] ?? 0) + vertexCount;
    }

    for (const fg of mesh.faceGroups) {
      const group: FaceGroup = {
        start: fg.start + idxOffset,
        count: fg.count,
      };
      if (fg.color !== undefined) {
        (group as { color: string }).color = fg.color;
      }
      faceGroups.push(group);
    }

    vertOffset += mesh.vertices.length;
    idxOffset += mesh.indices.length;
    vertexCount += mesh.vertices.length / 3;
  }

  return { vertices, normals, indices, faceGroups };
}

// ---------------------------------------------------------------------------
// CadGeometryManager — orchestrates Worker communication
// ---------------------------------------------------------------------------

export type CadImportOptions = {
  readonly format?: CadFileFormat;
  readonly quality?: TessellationQuality;
};

export type CadWorkerAdapter = {
  postMessage(request: CadWorkerRequest): void;
  onMessage(handler: (response: CadWorkerResponse) => void): () => void;
  terminate(): void;
};

export type CadGeometryManager = {
  importFile(data: ArrayBuffer, options?: CadImportOptions): Promise<CadImportResult>;
  getDeflection(quality: TessellationQuality): number;
  dispose(): void;
};

export function createCadGeometryManager(
  adapter: CadWorkerAdapter,
): CadGeometryManager {
  let disposed = false;
  let pending: {
    resolve: (result: CadImportResult) => void;
    reject: (error: Error) => void;
  } | null = null;

  const unsub = adapter.onMessage((response) => {
    if (!pending) return;
    const { resolve, reject } = pending;
    pending = null;

    if (response.kind === "error") {
      reject(new Error(response.message));
    } else {
      resolve(response.result);
    }
  });

  return {
    importFile(data, options) {
      if (disposed) return Promise.reject(new Error("CadGeometryManager disposed"));
      if (pending) return Promise.reject(new Error("Import already in progress"));

      const header = new TextDecoder().decode(data.slice(0, 256));
      const format = options?.format ?? detectCadFormat(header) ?? "step";
      const quality = options?.quality ?? "medium";

      return new Promise<CadImportResult>((resolve, reject) => {
        pending = { resolve, reject };
        adapter.postMessage({ kind: "import", data, format, quality });
      });
    },

    getDeflection(quality) {
      return QUALITY_DEFLECTION[quality];
    },

    dispose() {
      disposed = true;
      unsub();
      adapter.terminate();
    },
  };
}

// ---------------------------------------------------------------------------
// Test adapter (in-memory, no real WASM)
// ---------------------------------------------------------------------------

export function createTestCadAdapter(
  mockResult?: CadImportResult,
): CadWorkerAdapter {
  let handler: ((response: CadWorkerResponse) => void) | null = null;

  return {
    postMessage(_request) {
      if (!handler) return;
      if (mockResult) {
        queueMicrotask(() => handler?.({ kind: "import-result", result: mockResult }));
      } else {
        // Generate a simple cube mesh
        const vertices = new Float32Array([
          -0.5, -0.5, -0.5, 0.5, -0.5, -0.5, 0.5, 0.5, -0.5,
          -0.5, 0.5, -0.5, -0.5, -0.5, 0.5, 0.5, -0.5, 0.5,
          0.5, 0.5, 0.5, -0.5, 0.5, 0.5,
        ]);
        const normals = new Float32Array(vertices.length);
        const indices = new Uint32Array([
          0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7,
          0, 4, 7, 0, 7, 3, 1, 5, 6, 1, 6, 2,
          3, 2, 6, 3, 6, 7, 0, 1, 5, 0, 5, 4,
        ]);
        const result: CadImportResult = {
          meshes: [{ vertices, normals, indices, faceGroups: [{ start: 0, count: 36 }] }],
          boundingBox: { min: [-0.5, -0.5, -0.5], max: [0.5, 0.5, 0.5] },
          faceCount: 12,
          edgeCount: 18,
        };
        queueMicrotask(() => handler?.({ kind: "import-result", result }));
      }
    },
    onMessage(h) {
      handler = h;
      return () => { handler = null; };
    },
    terminate() {
      handler = null;
    },
  };
}
