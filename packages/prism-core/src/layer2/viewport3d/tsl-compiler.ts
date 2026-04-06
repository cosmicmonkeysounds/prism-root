/**
 * TSL (Three.js Shading Language) shader graph compiler.
 *
 * Takes a node-wire graph (from Builder 2's @xyflow canvas) and compiles
 * it to GLSL vertex/fragment shaders.  The node graph maps 1:1 to TSL
 * Node Objects; this module resolves the DAG, type-checks connections,
 * and emits shader code for WebGL or WebGPU targets.
 */

import type {
  TslNodeDef,
  TslNodeKind,
  TslConnection,
  TslShaderGraph,
  TslCompileTarget,
  TslCompileResult,
  TslCompileError,
  TslDataType,
  TslPort,
  MathOp,
  Vec3,
} from "./viewport3d-types.js";

// ---------------------------------------------------------------------------
// Node registry — inputs/outputs/code template per kind
// ---------------------------------------------------------------------------

type NodeSpec = {
  inputs: readonly TslPort[];
  outputs: readonly TslPort[];
  glsl(params: Record<string, number | string | Vec3>, inputVars: Record<string, string>): string;
};

const MATH_OP_GLSL: Record<MathOp, (a: string, b: string) => string> = {
  add: (a, b) => `(${a} + ${b})`,
  subtract: (a, b) => `(${a} - ${b})`,
  multiply: (a, b) => `(${a} * ${b})`,
  divide: (a, b) => `(${a} / max(${b}, 0.0001))`,
  power: (a, b) => `pow(${a}, ${b})`,
  sin: (a) => `sin(${a})`,
  cos: (a) => `cos(${a})`,
  abs: (a) => `abs(${a})`,
  fract: (a) => `fract(${a})`,
  clamp: (a, b) => `clamp(${a}, 0.0, ${b})`,
  smoothstep: (a, b) => `smoothstep(0.0, ${b}, ${a})`,
};

function buildNodeSpecs(): Map<TslNodeKind, NodeSpec> {
  const specs = new Map<TslNodeKind, NodeSpec>();

  specs.set("float-constant", {
    inputs: [],
    outputs: [{ name: "value", dataType: "float" }],
    glsl(params) { return String(params["value"] ?? 0.0); },
  });

  specs.set("vec3-constant", {
    inputs: [],
    outputs: [{ name: "value", dataType: "vec3" }],
    glsl(params) {
      const v = (params["value"] as Vec3) ?? [0, 0, 0];
      return `vec3(${v[0]}, ${v[1]}, ${v[2]})`;
    },
  });

  specs.set("color-constant", {
    inputs: [],
    outputs: [{ name: "color", dataType: "color" }],
    glsl(params) {
      const hex = (params["value"] as string) ?? "#ffffff";
      const r = parseInt(hex.slice(1, 3), 16) / 255;
      const g = parseInt(hex.slice(3, 5), 16) / 255;
      const b = parseInt(hex.slice(5, 7), 16) / 255;
      return `vec3(${r.toFixed(4)}, ${g.toFixed(4)}, ${b.toFixed(4)})`;
    },
  });

  specs.set("math-op", {
    inputs: [{ name: "a", dataType: "float" }, { name: "b", dataType: "float" }],
    outputs: [{ name: "result", dataType: "float" }],
    glsl(params, inputs) {
      const op = (params["op"] as MathOp) ?? "add";
      const fn = MATH_OP_GLSL[op];
      return fn(inputs["a"] ?? "0.0", inputs["b"] ?? "0.0");
    },
  });

  specs.set("mix", {
    inputs: [
      { name: "a", dataType: "vec3" },
      { name: "b", dataType: "vec3" },
      { name: "factor", dataType: "float" },
    ],
    outputs: [{ name: "result", dataType: "vec3" }],
    glsl(_params, inputs) {
      return `mix(${inputs["a"] ?? "vec3(0.0)"}, ${inputs["b"] ?? "vec3(1.0)"}, ${inputs["factor"] ?? "0.5"})`;
    },
  });

  specs.set("fresnel", {
    inputs: [{ name: "power", dataType: "float" }],
    outputs: [{ name: "value", dataType: "float" }],
    glsl(_params, inputs) {
      return `pow(1.0 - max(dot(vNormal, vec3(0.0, 0.0, 1.0)), 0.0), ${inputs["power"] ?? "2.0"})`;
    },
  });

  specs.set("noise", {
    inputs: [{ name: "scale", dataType: "float" }],
    outputs: [{ name: "value", dataType: "float" }],
    glsl(_params, inputs) {
      return `fract(sin(dot(vUv * ${inputs["scale"] ?? "1.0"}, vec2(12.9898, 78.233))) * 43758.5453)`;
    },
  });

  specs.set("time", {
    inputs: [],
    outputs: [{ name: "value", dataType: "float" }],
    glsl() { return "uTime"; },
  });

  specs.set("uv", {
    inputs: [],
    outputs: [{ name: "uv", dataType: "vec2" }],
    glsl() { return "vUv"; },
  });

  specs.set("normal", {
    inputs: [],
    outputs: [{ name: "normal", dataType: "vec3" }],
    glsl() { return "vNormal"; },
  });

  specs.set("position", {
    inputs: [],
    outputs: [{ name: "position", dataType: "vec3" }],
    glsl() { return "vPosition"; },
  });

  specs.set("texture-sample", {
    inputs: [{ name: "uv", dataType: "vec2" }],
    outputs: [{ name: "color", dataType: "color" }, { name: "alpha", dataType: "float" }],
    glsl(params, inputs) {
      const texName = (params["texture"] as string) ?? "uTexture0";
      return `texture2D(${texName}, ${inputs["uv"] ?? "vUv"})`;
    },
  });

  // Output nodes
  for (const outputKind of [
    "output-color", "output-normal", "output-emissive",
    "output-roughness", "output-metalness", "output-opacity",
  ] as const) {
    const dt: TslDataType = outputKind.includes("color") || outputKind.includes("normal") || outputKind.includes("emissive")
      ? "vec3" : "float";
    specs.set(outputKind, {
      inputs: [{ name: "value", dataType: dt }],
      outputs: [],
      glsl(_params, inputs) { return inputs["value"] ?? (dt === "vec3" ? "vec3(0.0)" : "0.0"); },
    });
  }

  return specs;
}

const NODE_SPECS = buildNodeSpecs();

// ---------------------------------------------------------------------------
// Topological sort
// ---------------------------------------------------------------------------

function topologicalSort(
  graph: TslShaderGraph,
): { sorted: readonly string[]; errors: readonly TslCompileError[] } {
  const errors: TslCompileError[] = [];
  const inDegree = new Map<string, number>();
  const adjacency = new Map<string, string[]>();

  for (const [id] of graph.nodes) {
    inDegree.set(id, 0);
    adjacency.set(id, []);
  }

  for (const conn of graph.connections) {
    if (!graph.nodes.has(conn.sourceNodeId) || !graph.nodes.has(conn.targetNodeId)) {
      errors.push({ nodeId: conn.sourceNodeId, message: `Connection references missing node` });
      continue;
    }
    adjacency.get(conn.sourceNodeId)?.push(conn.targetNodeId);
    inDegree.set(conn.targetNodeId, (inDegree.get(conn.targetNodeId) ?? 0) + 1);
  }

  const queue: string[] = [];
  for (const [id, deg] of inDegree) {
    if (deg === 0) queue.push(id);
  }

  const sorted: string[] = [];
  while (queue.length > 0) {
    const id = queue.shift();
    if (id === undefined) break;
    sorted.push(id);
    for (const target of adjacency.get(id) ?? []) {
      const newDeg = (inDegree.get(target) ?? 1) - 1;
      inDegree.set(target, newDeg);
      if (newDeg === 0) queue.push(target);
    }
  }

  if (sorted.length !== graph.nodes.size) {
    const cycled = [...graph.nodes.keys()].filter((id) => !sorted.includes(id));
    for (const id of cycled) {
      errors.push({ nodeId: id, message: "Node is part of a cycle" });
    }
  }

  return { sorted, errors };
}

// ---------------------------------------------------------------------------
// Type compatibility
// ---------------------------------------------------------------------------

const TYPE_COMPAT: Record<TslDataType, readonly TslDataType[]> = {
  float: ["float"],
  vec2: ["vec2"],
  vec3: ["vec3", "color"],
  vec4: ["vec4"],
  color: ["color", "vec3"],
  texture: ["texture"],
};

export function typesCompatible(source: TslDataType, target: TslDataType): boolean {
  return TYPE_COMPAT[source]?.includes(target) ?? false;
}

// ---------------------------------------------------------------------------
// Connection validation
// ---------------------------------------------------------------------------

export function validateConnections(
  graph: TslShaderGraph,
): readonly TslCompileError[] {
  const errors: TslCompileError[] = [];

  for (const conn of graph.connections) {
    const sourceNode = graph.nodes.get(conn.sourceNodeId);
    const targetNode = graph.nodes.get(conn.targetNodeId);
    if (!sourceNode || !targetNode) {
      errors.push({ nodeId: conn.id, message: "Connection references missing node" });
      continue;
    }

    const sourceSpec = NODE_SPECS.get(sourceNode.kind);
    const targetSpec = NODE_SPECS.get(targetNode.kind);
    if (!sourceSpec || !targetSpec) continue;

    const sourcePort = sourceSpec.outputs.find((p) => p.name === conn.sourcePort);
    const targetPort = targetSpec.inputs.find((p) => p.name === conn.targetPort);

    if (!sourcePort) {
      errors.push({ nodeId: conn.sourceNodeId, message: `No output port "${conn.sourcePort}"` });
      continue;
    }
    if (!targetPort) {
      errors.push({ nodeId: conn.targetNodeId, message: `No input port "${conn.targetPort}"` });
      continue;
    }

    if (!typesCompatible(sourcePort.dataType, targetPort.dataType)) {
      errors.push({
        nodeId: conn.id,
        message: `Type mismatch: ${sourcePort.dataType} → ${targetPort.dataType}`,
      });
    }
  }

  return errors;
}

// ---------------------------------------------------------------------------
// Compiler
// ---------------------------------------------------------------------------

export function compileTslGraph(
  graph: TslShaderGraph,
  target: TslCompileTarget = "webgl",
): TslCompileResult {
  const errors: TslCompileError[] = [];

  // Validate connections
  errors.push(...validateConnections(graph));

  // Topological sort
  const { sorted, errors: sortErrors } = topologicalSort(graph);
  errors.push(...sortErrors);

  if (errors.length > 0) {
    return { success: false, uniforms: [], errors };
  }

  // Build input mapping: for each node, which variable feeds each input port
  const connectionMap = new Map<string, Map<string, string>>();
  for (const [id] of graph.nodes) {
    connectionMap.set(id, new Map());
  }

  const nodeVarNames = new Map<string, string>();
  const uniforms: string[] = [];
  const lines: string[] = [];
  let varCounter = 0;

  // Collect output assignments
  const outputs = new Map<string, string>();

  for (const nodeId of sorted) {
    const node = graph.nodes.get(nodeId);
    if (!node) continue;
    const spec = NODE_SPECS.get(node.kind);
    if (!spec) {
      errors.push({ nodeId, message: `Unknown node kind: ${node.kind}` });
      continue;
    }

    // Resolve input variables
    const inputVars: Record<string, string> = {};
    for (const conn of graph.connections) {
      if (conn.targetNodeId === nodeId) {
        const sourceVar = nodeVarNames.get(conn.sourceNodeId);
        if (sourceVar) {
          inputVars[conn.targetPort] = sourceVar;
        }
      }
    }

    // Generate GLSL
    const glslExpr = spec.glsl(node.params, inputVars);
    const varName = `v${varCounter++}`;
    nodeVarNames.set(nodeId, varName);

    // Check if this is a time node — needs uniform
    if (node.kind === "time" && !uniforms.includes("uTime")) {
      uniforms.push("uTime");
    }
    if (node.kind === "texture-sample") {
      const texName = (node.params["texture"] as string) ?? "uTexture0";
      if (!uniforms.includes(`sampler2D:${texName}`)) uniforms.push(`sampler2D:${texName}`);
    }

    // Determine output type
    const outputType = spec.outputs.length > 0
      ? glslTypeName((spec.outputs[0] ?? { dataType: "vec3" as const }).dataType)
      : "vec3";

    if (node.kind.startsWith("output-")) {
      const channel = node.kind.replace("output-", "");
      outputs.set(channel, glslExpr);
    } else {
      lines.push(`  ${outputType} ${varName} = ${glslExpr};`);
    }
  }

  // Assemble fragment shader
  const fragLines: string[] = [];
  fragLines.push("// Generated by Prism TSL Compiler");
  fragLines.push(`// Target: ${target}`);
  fragLines.push("precision mediump float;");
  fragLines.push("");
  fragLines.push("varying vec2 vUv;");
  fragLines.push("varying vec3 vNormal;");
  fragLines.push("varying vec3 vPosition;");
  fragLines.push("");
  for (const u of uniforms) {
    if (u.startsWith("sampler2D:")) {
      const name = u.slice("sampler2D:".length);
      fragLines.push(`uniform sampler2D ${name};`);
    } else {
      fragLines.push(`uniform float ${u};`);
    }
  }
  fragLines.push("");
  fragLines.push("void main() {");
  fragLines.push(...lines);
  fragLines.push("");

  const color = outputs.get("color") ?? "vec3(1.0, 0.0, 1.0)";
  const opacity = outputs.get("opacity") ?? "1.0";
  fragLines.push(`  gl_FragColor = vec4(${color}, ${opacity});`);
  fragLines.push("}");

  // Vertex shader (standard pass-through)
  const vertLines: string[] = [];
  vertLines.push("// Generated by Prism TSL Compiler");
  vertLines.push("varying vec2 vUv;");
  vertLines.push("varying vec3 vNormal;");
  vertLines.push("varying vec3 vPosition;");
  vertLines.push("");
  vertLines.push("void main() {");
  vertLines.push("  vUv = uv;");
  vertLines.push("  vNormal = normalize(normalMatrix * normal);");
  vertLines.push("  vPosition = (modelMatrix * vec4(position, 1.0)).xyz;");
  vertLines.push("  gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);");
  vertLines.push("}");

  return {
    success: true,
    vertexShader: vertLines.join("\n"),
    fragmentShader: fragLines.join("\n"),
    uniforms,
    errors: [],
  };
}

function glslTypeName(dt: TslDataType): string {
  switch (dt) {
    case "float": return "float";
    case "vec2": return "vec2";
    case "vec3": return "vec3";
    case "vec4": return "vec4";
    case "color": return "vec3";
    case "texture": return "vec4";
  }
}

// ---------------------------------------------------------------------------
// Helpers for building shader graphs programmatically
// ---------------------------------------------------------------------------

let nodeCounter = 0;

export function createTslNode(
  kind: TslNodeKind,
  params: Record<string, number | string | Vec3> = {},
  id?: string,
): TslNodeDef {
  const spec = NODE_SPECS.get(kind);
  return {
    id: id ?? `tsl_${(nodeCounter++).toString(36)}`,
    kind,
    inputs: spec?.inputs ?? [],
    outputs: spec?.outputs ?? [],
    params,
  };
}

export function createTslConnection(
  sourceNodeId: string,
  sourcePort: string,
  targetNodeId: string,
  targetPort: string,
): TslConnection {
  return {
    id: `conn_${sourceNodeId}_${sourcePort}_${targetNodeId}_${targetPort}`,
    sourceNodeId,
    sourcePort,
    targetNodeId,
    targetPort,
  };
}

export function createShaderGraph(
  name: string,
  nodes: readonly TslNodeDef[],
  connections: readonly TslConnection[],
): TslShaderGraph {
  const nodeMap = new Map<string, TslNodeDef>();
  for (const n of nodes) nodeMap.set(n.id, n);
  return {
    id: `sg_${Date.now().toString(36)}`,
    name,
    nodes: nodeMap,
    connections,
  };
}
