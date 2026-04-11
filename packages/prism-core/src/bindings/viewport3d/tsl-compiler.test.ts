import { describe, it, expect } from "vitest";
import {
  compileTslGraph,
  createTslNode,
  createTslConnection,
  createShaderGraph,
  typesCompatible,
  validateConnections,
} from "./tsl-compiler.js";

describe("typesCompatible", () => {
  it("float is compatible with float", () => {
    expect(typesCompatible("float", "float")).toBe(true);
  });

  it("vec3 is compatible with color", () => {
    expect(typesCompatible("vec3", "color")).toBe(true);
  });

  it("color is compatible with vec3", () => {
    expect(typesCompatible("color", "vec3")).toBe(true);
  });

  it("float is not compatible with vec3", () => {
    expect(typesCompatible("float", "vec3")).toBe(false);
  });
});

describe("validateConnections", () => {
  it("returns no errors for valid connections", () => {
    const color = createTslNode("color-constant", { value: "#ff0000" }, "c1");
    const output = createTslNode("output-color", {}, "out");
    const conn = createTslConnection("c1", "color", "out", "value");
    const graph = createShaderGraph("test", [color, output], [conn]);
    expect(validateConnections(graph)).toEqual([]);
  });

  it("detects type mismatches", () => {
    const floatNode = createTslNode("float-constant", { value: 1.0 }, "f1");
    const output = createTslNode("output-color", {}, "out");
    // float → vec3 input is a mismatch
    const conn = createTslConnection("f1", "value", "out", "value");
    const graph = createShaderGraph("test", [floatNode, output], [conn]);
    const errors = validateConnections(graph);
    expect(errors.length).toBeGreaterThan(0);
    expect(errors[0]?.message).toContain("Type mismatch");
  });

  it("detects missing ports", () => {
    const color = createTslNode("color-constant", {}, "c1");
    const output = createTslNode("output-color", {}, "out");
    const conn = createTslConnection("c1", "nonexistent", "out", "value");
    const graph = createShaderGraph("test", [color, output], [conn]);
    const errors = validateConnections(graph);
    expect(errors.length).toBeGreaterThan(0);
  });
});

describe("compileTslGraph", () => {
  it("compiles a simple color output", () => {
    const color = createTslNode("color-constant", { value: "#ff0000" }, "c1");
    const output = createTslNode("output-color", {}, "out");
    const conn = createTslConnection("c1", "color", "out", "value");
    const graph = createShaderGraph("red shader", [color, output], [conn]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("gl_FragColor");
    expect(result.fragmentShader).toContain("vec3(");
    expect(result.vertexShader).toContain("gl_Position");
    expect(result.errors).toEqual([]);
  });

  it("compiles math operations", () => {
    const a = createTslNode("float-constant", { value: 0.5 }, "a");
    const b = createTslNode("float-constant", { value: 2.0 }, "b");
    const math = createTslNode("math-op", { op: "multiply" }, "mul");
    const graph = createShaderGraph("math", [a, b, math], [
      createTslConnection("a", "value", "mul", "a"),
      createTslConnection("b", "value", "mul", "b"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("*");
  });

  it("includes time uniform when time node is used", () => {
    const time = createTslNode("time", {}, "t");
    const output = createTslNode("output-opacity", {}, "out");
    const conn = createTslConnection("t", "value", "out", "value");
    const graph = createShaderGraph("animated", [time, output], [conn]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.uniforms).toContain("uTime");
    expect(result.fragmentShader).toContain("uniform float uTime");
  });

  it("compiles mix node", () => {
    const c1 = createTslNode("color-constant", { value: "#ff0000" }, "c1");
    const c2 = createTslNode("color-constant", { value: "#0000ff" }, "c2");
    const factor = createTslNode("float-constant", { value: 0.5 }, "f");
    const mix = createTslNode("mix", {}, "mix");
    const output = createTslNode("output-color", {}, "out");

    const graph = createShaderGraph("blend", [c1, c2, factor, mix, output], [
      createTslConnection("c1", "color", "mix", "a"),
      createTslConnection("c2", "color", "mix", "b"),
      createTslConnection("f", "value", "mix", "factor"),
      createTslConnection("mix", "result", "out", "value"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("mix(");
  });

  it("compiles fresnel effect", () => {
    const power = createTslNode("float-constant", { value: 3.0 }, "p");
    const fresnel = createTslNode("fresnel", {}, "fr");
    const output = createTslNode("output-opacity", {}, "out");
    const graph = createShaderGraph("fresnel", [power, fresnel, output], [
      createTslConnection("p", "value", "fr", "power"),
      createTslConnection("fr", "value", "out", "value"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("pow(");
    expect(result.fragmentShader).toContain("vNormal");
  });

  it("detects cycles and fails", () => {
    const a = createTslNode("math-op", { op: "add" }, "a");
    const b = createTslNode("math-op", { op: "add" }, "b");
    const graph = createShaderGraph("cycle", [a, b], [
      createTslConnection("a", "result", "b", "a"),
      createTslConnection("b", "result", "a", "a"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(false);
    expect(result.errors.some((e) => e.message.includes("cycle"))).toBe(true);
  });

  it("compiles noise with UV", () => {
    const scale = createTslNode("float-constant", { value: 10.0 }, "s");
    const noise = createTslNode("noise", {}, "n");
    const output = createTslNode("output-opacity", {}, "out");
    const graph = createShaderGraph("noisy", [scale, noise, output], [
      createTslConnection("s", "value", "n", "scale"),
      createTslConnection("n", "value", "out", "value"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("fract(sin(");
  });

  it("handles texture sampling with uniform", () => {
    const tex = createTslNode("texture-sample", { texture: "uMainTex" }, "tex");
    const output = createTslNode("output-color", {}, "out");
    const graph = createShaderGraph("textured", [tex, output], [
      createTslConnection("tex", "color", "out", "value"),
    ]);

    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.uniforms).toContain("sampler2D:uMainTex");
    expect(result.fragmentShader).toContain("uniform sampler2D uMainTex");
  });

  it("targets webgpu with annotation", () => {
    const color = createTslNode("color-constant", { value: "#ffffff" }, "c");
    const output = createTslNode("output-color", {}, "out");
    const graph = createShaderGraph("gpu", [color, output], [
      createTslConnection("c", "color", "out", "value"),
    ]);
    const result = compileTslGraph(graph, "webgpu");
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("Target: webgpu");
  });

  it("compiles empty graph with fallback magenta", () => {
    const graph = createShaderGraph("empty", [], []);
    const result = compileTslGraph(graph);
    expect(result.success).toBe(true);
    expect(result.fragmentShader).toContain("vec3(1.0, 0.0, 1.0)");
  });
});

describe("createTslNode", () => {
  it("creates a node with correct spec ports", () => {
    const node = createTslNode("math-op", { op: "add" });
    expect(node.kind).toBe("math-op");
    expect(node.inputs.length).toBe(2);
    expect(node.outputs.length).toBe(1);
    expect(node.inputs[0]?.name).toBe("a");
    expect(node.outputs[0]?.name).toBe("result");
  });

  it("uses provided ID", () => {
    const node = createTslNode("time", {}, "my-time");
    expect(node.id).toBe("my-time");
  });
});
