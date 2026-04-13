# viewport3d/

3D Viewport / "Builder 3" primitives: a Loro-backed scene graph, OpenCASCADE.js CAD import pipeline, TSL shader-graph compiler (WebGL/WebGPU targets), and a gizmo controller with snapping and undo integration. Data-model only — the actual R3F/three.js renderer lives in app code and reads projected state from here.

```ts
import { createSceneState, createGizmoController } from "@prism/core/viewport3d";
```

## Key exports

- `createSceneState(existingDoc?)` — Loro-backed `SceneState` with nodes, materials, hierarchy, transforms, and CRDT sync. Listener-subscribable.
- `createCadGeometryManager(adapter)` — STEP / IGES / BREP import via a `CadWorkerAdapter` running OpenCASCADE.js in a worker. Tessellation quality: `low` / `medium` / `high`.
- `createTestCadAdapter()` — in-process adapter for tests.
- `detectCadFormat(header)` / `computeBoundingBox(vertices)` / `mergeMeshes(...)` — geometry helpers.
- `compileTslGraph(graph, target?)` — compile a TSL node-wire graph to GLSL (WebGL) or WGSL (WebGPU) with topological sort, type checking, and cycle detection.
- `createTslNode`, `createTslConnection`, `createShaderGraph`, `validateConnections`, `typesCompatible` — shader-graph construction helpers.
- `createGizmoController(sceneState, undoAdapter?)` — translate/rotate/scale gizmo state with per-axis snapping and undoable commit/cancel.
- `snapValue`, `snapVec3`, `snapTransform` — snapping math helpers.
- Defaults: `DEFAULT_TRANSFORM`, `DEFAULT_MATERIAL`, `PRIMITIVE_DEFAULTS`, `DEFAULT_GIZMO_STATE`.
- Types: `Vec3`, `Vec4`, `Mat4`, `Euler`, `EulerOrder`, `Transform`, `MaterialKind`, `MaterialDef`, `GeometryKind`, `GeometryParams`, `SceneNodeKind`, `LightParams`, `CameraParams`, `SceneNode`, `SceneGraph`, `SceneState`, `SceneStateListener`, `CadFileFormat`, `TessellationQuality`, `TessellatedMesh`, `FaceGroup`, `CadImportResult`, `CadImportOptions`, `CadWorkerAdapter`, `CadWorkerRequest`, `CadWorkerResponse`, `CadGeometryManager`, `TslNodeKind`, `TslDataType`, `MathOp`, `TslPort`, `TslNodeDef`, `TslConnection`, `TslShaderGraph`, `TslCompileTarget`, `TslCompileResult`, `TslCompileError`, `GizmoMode`, `GizmoSpace`, `GizmoAxis`, `GizmoState`, `GizmoTransformEvent`, `GizmoUndoAdapter`, `GizmoListener`, `GizmoController`.

## Usage

```ts
import {
  createSceneState,
  createGizmoController,
} from "@prism/core/viewport3d";

const scene = createSceneState();
const gizmo = createGizmoController(scene);

const cubeId = scene.addNode({ name: "Cube", kind: "mesh" });
gizmo.setMode("translate");
gizmo.select([cubeId]);
gizmo.beginTransform();
// ...drag from the renderer, then commit
gizmo.commitTransform();
```
