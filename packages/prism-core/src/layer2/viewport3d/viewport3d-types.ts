/**
 * Types for the Prism 3D Viewport (Builder 3).
 *
 * R3F-based 3D editor for spatial content, CAD geometry,
 * and TSL shader compilation — all backed by Loro CRDT.
 */

// ---------------------------------------------------------------------------
// Math primitives
// ---------------------------------------------------------------------------

export type Vec3 = readonly [x: number, y: number, z: number];
export type Vec4 = readonly [x: number, y: number, z: number, w: number];
export type Mat4 = readonly [
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
  number, number, number, number,
];
export type Euler = readonly [x: number, y: number, z: number, order: EulerOrder];
export type EulerOrder = "XYZ" | "XZY" | "YXZ" | "YZX" | "ZXY" | "ZYX";

// ---------------------------------------------------------------------------
// Transform
// ---------------------------------------------------------------------------

export type Transform = {
  readonly position: Vec3;
  readonly rotation: Euler;
  readonly scale: Vec3;
};

export const DEFAULT_TRANSFORM: Transform = {
  position: [0, 0, 0],
  rotation: [0, 0, 0, "XYZ"],
  scale: [1, 1, 1],
};

// ---------------------------------------------------------------------------
// Materials
// ---------------------------------------------------------------------------

export type MaterialKind =
  | "standard"
  | "physical"
  | "basic"
  | "tsl-custom";

export type MaterialDef = {
  readonly id: string;
  readonly kind: MaterialKind;
  readonly color: string;
  readonly opacity: number;
  readonly metalness: number;
  readonly roughness: number;
  readonly emissive: string;
  readonly emissiveIntensity: number;
  readonly tslShaderId?: string;
};

export const DEFAULT_MATERIAL: Omit<MaterialDef, "id"> = {
  kind: "standard",
  color: "#cccccc",
  opacity: 1,
  metalness: 0,
  roughness: 0.5,
  emissive: "#000000",
  emissiveIntensity: 0,
};

// ---------------------------------------------------------------------------
// Geometry
// ---------------------------------------------------------------------------

export type GeometryKind =
  | "box"
  | "sphere"
  | "cylinder"
  | "plane"
  | "torus"
  | "cad-mesh"
  | "imported-gltf";

export type GeometryParams = {
  readonly kind: GeometryKind;
  readonly args: readonly number[];
  readonly cadAssetId?: string;
};

export const PRIMITIVE_DEFAULTS: Record<
  Exclude<GeometryKind, "cad-mesh" | "imported-gltf">,
  readonly number[]
> = {
  box: [1, 1, 1],
  sphere: [0.5, 32, 32],
  cylinder: [0.5, 0.5, 1, 32],
  plane: [1, 1],
  torus: [0.5, 0.2, 16, 32],
};

// ---------------------------------------------------------------------------
// Scene nodes
// ---------------------------------------------------------------------------

export type SceneNodeKind =
  | "mesh"
  | "group"
  | "light-directional"
  | "light-point"
  | "light-spot"
  | "light-ambient"
  | "camera-perspective"
  | "camera-orthographic";

export type LightParams = {
  readonly color: string;
  readonly intensity: number;
  readonly distance?: number;
  readonly angle?: number;
  readonly penumbra?: number;
  readonly castShadow: boolean;
};

export type CameraParams = {
  readonly fov?: number;
  readonly near: number;
  readonly far: number;
  readonly zoom?: number;
};

export type SceneNode = {
  readonly id: string;
  readonly name: string;
  readonly kind: SceneNodeKind;
  readonly transform: Transform;
  readonly parentId: string | null;
  readonly visible: boolean;
  readonly locked: boolean;
  readonly geometry?: GeometryParams;
  readonly materialId?: string;
  readonly light?: LightParams;
  readonly camera?: CameraParams;
};

// ---------------------------------------------------------------------------
// Scene graph (what gets stored in Loro)
// ---------------------------------------------------------------------------

export type SceneGraph = {
  readonly nodes: ReadonlyMap<string, SceneNode>;
  readonly materials: ReadonlyMap<string, MaterialDef>;
  readonly rootIds: readonly string[];
};

// ---------------------------------------------------------------------------
// CAD Geometry (OpenCASCADE.js)
// ---------------------------------------------------------------------------

export type CadFileFormat = "step" | "iges" | "brep";

export type TessellationQuality = "low" | "medium" | "high";

export type TessellatedMesh = {
  readonly vertices: Float32Array;
  readonly normals: Float32Array;
  readonly indices: Uint32Array;
  readonly faceGroups: readonly FaceGroup[];
};

export type FaceGroup = {
  readonly start: number;
  readonly count: number;
  readonly color?: string;
};

export type CadImportResult = {
  readonly meshes: readonly TessellatedMesh[];
  readonly boundingBox: { min: Vec3; max: Vec3 };
  readonly faceCount: number;
  readonly edgeCount: number;
};

export type CadWorkerRequest =
  | { kind: "import"; data: ArrayBuffer; format: CadFileFormat; quality: TessellationQuality }
  | { kind: "tessellate"; quality: TessellationQuality };

export type CadWorkerResponse =
  | { kind: "import-result"; result: CadImportResult }
  | { kind: "error"; message: string };

// ---------------------------------------------------------------------------
// TSL Shader Compilation
// ---------------------------------------------------------------------------

export type TslNodeKind =
  | "float-constant"
  | "vec3-constant"
  | "color-constant"
  | "math-op"
  | "mix"
  | "texture-sample"
  | "fresnel"
  | "noise"
  | "time"
  | "uv"
  | "normal"
  | "position"
  | "output-color"
  | "output-normal"
  | "output-emissive"
  | "output-roughness"
  | "output-metalness"
  | "output-opacity";

export type TslDataType = "float" | "vec2" | "vec3" | "vec4" | "color" | "texture";

export type MathOp = "add" | "subtract" | "multiply" | "divide" | "power" | "sin" | "cos" | "abs" | "fract" | "clamp" | "smoothstep";

export type TslPort = {
  readonly name: string;
  readonly dataType: TslDataType;
};

export type TslNodeDef = {
  readonly id: string;
  readonly kind: TslNodeKind;
  readonly inputs: readonly TslPort[];
  readonly outputs: readonly TslPort[];
  readonly params: Record<string, number | string | Vec3>;
};

export type TslConnection = {
  readonly id: string;
  readonly sourceNodeId: string;
  readonly sourcePort: string;
  readonly targetNodeId: string;
  readonly targetPort: string;
};

export type TslShaderGraph = {
  readonly id: string;
  readonly name: string;
  readonly nodes: ReadonlyMap<string, TslNodeDef>;
  readonly connections: readonly TslConnection[];
};

export type TslCompileTarget = "webgl" | "webgpu";

export type TslCompileResult = {
  readonly success: boolean;
  readonly vertexShader?: string;
  readonly fragmentShader?: string;
  readonly uniforms: readonly string[];
  readonly errors: readonly TslCompileError[];
};

export type TslCompileError = {
  readonly nodeId: string;
  readonly message: string;
};

// ---------------------------------------------------------------------------
// Gizmo controls
// ---------------------------------------------------------------------------

export type GizmoMode = "translate" | "rotate" | "scale";
export type GizmoSpace = "local" | "world";
export type GizmoAxis = "x" | "y" | "z" | "xy" | "xz" | "yz" | "xyz";

export type GizmoState = {
  readonly mode: GizmoMode;
  readonly space: GizmoSpace;
  readonly selectedNodeIds: readonly string[];
  readonly snapping: boolean;
  readonly snapTranslate: number;
  readonly snapRotate: number;
  readonly snapScale: number;
};

export const DEFAULT_GIZMO_STATE: GizmoState = {
  mode: "translate",
  space: "world",
  selectedNodeIds: [],
  snapping: false,
  snapTranslate: 1,
  snapRotate: 15,
  snapScale: 0.1,
};

export type GizmoTransformEvent = {
  readonly nodeId: string;
  readonly axis: GizmoAxis;
  readonly before: Transform;
  readonly after: Transform;
};
