// @prism/core/viewport3d — 3D Viewport (Builder 3) barrel exports

export type {
  Vec3,
  Vec4,
  Mat4,
  Euler,
  EulerOrder,
  Transform,
  MaterialKind,
  MaterialDef,
  GeometryKind,
  GeometryParams,
  SceneNodeKind,
  LightParams,
  CameraParams,
  SceneNode,
  SceneGraph,
  CadFileFormat,
  TessellationQuality,
  TessellatedMesh,
  FaceGroup,
  CadImportResult,
  CadWorkerRequest,
  CadWorkerResponse,
  TslNodeKind,
  TslDataType,
  MathOp,
  TslPort,
  TslNodeDef,
  TslConnection,
  TslShaderGraph,
  TslCompileTarget,
  TslCompileResult,
  TslCompileError,
  GizmoMode,
  GizmoSpace,
  GizmoAxis,
  GizmoState,
  GizmoTransformEvent,
} from "./viewport3d-types.js";

export {
  DEFAULT_TRANSFORM,
  DEFAULT_MATERIAL,
  PRIMITIVE_DEFAULTS,
  DEFAULT_GIZMO_STATE,
} from "./viewport3d-types.js";

export { createSceneState } from "./scene-state.js";
export type { SceneState, SceneStateListener } from "./scene-state.js";

export {
  detectCadFormat,
  computeBoundingBox,
  mergeMeshes,
  createCadGeometryManager,
  createTestCadAdapter,
} from "./cad-geometry.js";
export type { CadImportOptions, CadWorkerAdapter, CadGeometryManager } from "./cad-geometry.js";

export {
  compileTslGraph,
  typesCompatible,
  validateConnections,
  createTslNode,
  createTslConnection,
  createShaderGraph,
} from "./tsl-compiler.js";

export {
  snapValue,
  snapVec3,
  snapTransform,
  createGizmoController,
} from "./gizmo-controls.js";
export type { GizmoUndoAdapter, GizmoListener, GizmoController } from "./gizmo-controls.js";
