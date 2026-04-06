// Layer 2 — The Renderers (Visual Implementation)
// Re-exports from each renderer module.

export {
  loroSync,
  createLoroTextDoc,
  prismEditorSetup,
  prismJSLang,
  prismJSONLang,
  useCodemirror,
} from "./codemirror/index.js";

export { createPuckLoroBridge, usePuckLoro } from "./puck/index.js";

export {
  createActionRegistry,
  PrismKBarProvider,
  usePrismKBar,
} from "./kbar/index.js";

export {
  prismNodeTypes,
  prismEdgeTypes,
  PrismGraph,
  applyElkLayout,
} from "./graph/index.js";
export type {
  CodeMirrorNode,
  MarkdownNode,
  DefaultPrismNode,
  HardRefEdge,
  WeakRefEdge,
  PrismGraphProps,
  LayoutOptions,
} from "./graph/index.js";

export {
  LensProvider,
  useLensContext,
  useShellStore,
  ActivityBar,
  TabBar,
  ShellLayout,
} from "./shell/index.js";
export type {
  LensComponentMap,
  LensContextValue,
  LensProviderProps,
} from "./shell/index.js";

export {
  installOpenDawWorkers,
  createOpenDawBridge,
  useOpenDawBridge,
  usePlaybackPosition,
  useTransportControls,
  useTrackEffects,
} from "./audio/index.js";
export type {
  TrackBinding,
  ClipBinding,
  OpenDawEffectType,
  AudioExportOptions,
  StemConfig,
  OpenDawBridgeOptions,
  OpenDawBridge,
  UseOpenDawBridgeResult,
  PlaybackPositionResult,
  TransportControlsResult,
  TrackEffectsResult,
} from "./audio/index.js";

export {
  DEFAULT_TRANSFORM,
  DEFAULT_MATERIAL,
  PRIMITIVE_DEFAULTS,
  DEFAULT_GIZMO_STATE,
  createSceneState,
  detectCadFormat,
  computeBoundingBox,
  mergeMeshes,
  createCadGeometryManager,
  createTestCadAdapter,
  compileTslGraph,
  typesCompatible,
  validateConnections,
  createTslNode,
  createTslConnection,
  createShaderGraph,
  snapValue,
  snapVec3,
  snapTransform,
  createGizmoController,
} from "./viewport3d/index.js";
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
  SceneState,
  SceneStateListener,
  CadImportOptions,
  CadWorkerAdapter,
  CadGeometryManager,
  GizmoUndoAdapter,
  GizmoListener,
  GizmoController,
} from "./viewport3d/index.js";
