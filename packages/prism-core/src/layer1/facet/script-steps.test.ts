import { describe, it, expect } from "vitest";
import {
  createStep,
  createVisualScript,
  emitStepsLua,
  emitStepsLuaWithMap,
  validateSteps,
  getStepMeta,
  getStepCategories,
  STEP_KINDS,
} from "./script-steps.js";

// ── createStep ──────────────────────────────────────────────────────────────

describe("createStep", () => {
  it("creates a step with kind and auto-generated id", () => {
    const step = createStep("set-field", { target: "status", value: '"active"' });
    expect(step.kind).toBe("set-field");
    expect(step.id).toMatch(/^step_/);
    expect(step.params.target).toBe("status");
    expect(step.params.value).toBe('"active"');
  });

  it("creates a step with empty params when not provided", () => {
    const step = createStep("commit-record");
    expect(step.params).toEqual({});
  });
});

// ── createVisualScript ──────────────────────────────────────────────────────

describe("createVisualScript", () => {
  it("creates an empty script", () => {
    const script = createVisualScript("my-script", "My Script");
    expect(script.id).toBe("my-script");
    expect(script.name).toBe("My Script");
    expect(script.steps).toEqual([]);
  });
});

// ── getStepMeta ─────────────────────────────────────────────────────────────

describe("getStepMeta", () => {
  it("returns metadata for known step kind", () => {
    const meta = getStepMeta("set-field");
    expect(meta.label).toBe("Set Field");
    expect(meta.category).toBe("data");
    expect(meta.params).toContain("target");
    expect(meta.params).toContain("value");
  });

  it("marks If as opensBlock", () => {
    expect(getStepMeta("if").opensBlock).toBe(true);
  });

  it("marks End If as closesBlock", () => {
    expect(getStepMeta("end-if").closesBlock).toBe(true);
  });

  it("marks Else If as continuesBlock", () => {
    expect(getStepMeta("else-if").continuesBlock).toBe(true);
  });
});

// ── getStepCategories ───────────────────────────────────────────────────────

describe("getStepCategories", () => {
  it("returns grouped categories", () => {
    const categories = getStepCategories();
    const names = categories.map((c) => c.category);
    expect(names).toContain("navigation");
    expect(names).toContain("data");
    expect(names).toContain("control");
    expect(names).toContain("ui");
    expect(names).toContain("find");
    expect(names).toContain("script");
    expect(names).toContain("custom");
  });

  it("total steps across all categories equals STEP_KINDS length", () => {
    const total = getStepCategories().reduce((sum, c) => sum + c.steps.length, 0);
    expect(total).toBe(STEP_KINDS.length);
  });
});

// ── emitStepsLua ────────────────────────────────────────────────────────────

describe("emitStepsLua", () => {
  it("emits set-field step", () => {
    const lua = emitStepsLua([createStep("set-field", { target: "status", value: '"active"' })]);
    expect(lua).toBe('Prism.setField("status", "active")');
  });

  it("emits set-variable step", () => {
    const lua = emitStepsLua([createStep("set-variable", { name: "total", value: "100" })]);
    expect(lua).toBe("local total = 100");
  });

  it("emits new-record step", () => {
    const lua = emitStepsLua([createStep("new-record", { objectType: "contact" })]);
    expect(lua).toBe('Prism.newRecord("contact")');
  });

  it("emits commit-record step", () => {
    const lua = emitStepsLua([createStep("commit-record")]);
    expect(lua).toBe("Prism.commitRecord()");
  });

  it("emits show-notification step", () => {
    const lua = emitStepsLua([
      createStep("show-notification", { title: '"Saved!"', kind: '"success"' }),
    ]);
    expect(lua).toBe('Prism.notify("Saved!", "success")');
  });

  it("emits comment as Lua comment", () => {
    const lua = emitStepsLua([createStep("comment", { text: "Initialize variables" })]);
    expect(lua).toBe("-- Initialize variables");
  });

  it("emits custom lua directly", () => {
    const lua = emitStepsLua([createStep("custom-lua", { code: 'print("hello")' })]);
    expect(lua).toBe('print("hello")');
  });

  it("handles if/else/end-if with indentation", () => {
    const steps = [
      createStep("if", { condition: "amount > 1000" }),
      createStep("set-field", { target: "priority", value: '"high"' }),
      createStep("else"),
      createStep("set-field", { target: "priority", value: '"normal"' }),
      createStep("end-if"),
    ];
    const lua = emitStepsLua(steps);
    const lines = lua.split("\n");
    expect(lines[0]).toBe("if amount > 1000 then");
    expect(lines[1]).toBe('  Prism.setField("priority", "high")');
    expect(lines[2]).toBe("else");
    expect(lines[3]).toBe('  Prism.setField("priority", "normal")');
    expect(lines[4]).toBe("end");
  });

  it("handles nested if inside loop", () => {
    const steps = [
      createStep("loop"),
      createStep("if", { condition: "done" }),
      createStep("exit-loop-if", { condition: "true" }),
      createStep("end-if"),
      createStep("end-loop"),
    ];
    const lua = emitStepsLua(steps);
    const lines = lua.split("\n");
    expect(lines[0]).toBe("while true do");
    expect(lines[1]).toBe("  if done then");
    expect(lines[2]).toBe("    if true then break end");
    expect(lines[3]).toBe("  end");
    expect(lines[4]).toBe("end");
  });

  it("marks disabled steps as comments", () => {
    const step = createStep("delete-record");
    step.disabled = true;
    const lua = emitStepsLua([step]);
    expect(lua).toContain("-- [disabled]");
  });

  it("emits multiple steps on separate lines", () => {
    const steps = [
      createStep("set-field", { target: "a", value: "1" }),
      createStep("set-field", { target: "b", value: "2" }),
      createStep("commit-record"),
    ];
    const lua = emitStepsLua(steps);
    expect(lua.split("\n")).toHaveLength(3);
  });

  it("emits perform-find step", () => {
    const lua = emitStepsLua([createStep("perform-find", { field: "status", value: '"active"' })]);
    expect(lua).toBe('Prism.performFind("status", "active")');
  });

  it("emits sort-records step", () => {
    const lua = emitStepsLua([createStep("sort-records", { field: "name", direction: "asc" })]);
    expect(lua).toBe('Prism.sortRecords("name", "asc")');
  });

  it("emits run-script step", () => {
    const lua = emitStepsLua([createStep("run-script", { scriptId: "calc-totals" })]);
    expect(lua).toBe('Prism.runScript("calc-totals")');
  });

  it("emits run-script with parameter", () => {
    const lua = emitStepsLua([
      createStep("run-script", { scriptId: "calc-totals", parameter: '"2026"' }),
    ]);
    expect(lua).toBe('Prism.runScript("calc-totals", "2026")');
  });

  it("emits exit-script with result", () => {
    const lua = emitStepsLua([createStep("exit-script", { result: '"done"' })]);
    expect(lua).toBe('return "done"');
  });

  it("emits exit-script without result", () => {
    const lua = emitStepsLua([createStep("exit-script")]);
    expect(lua).toBe("return");
  });

  it("emits go-to-layout step", () => {
    const lua = emitStepsLua([createStep("go-to-layout", { layoutId: "contact-form" })]);
    expect(lua).toBe('Prism.goToLayout("contact-form")');
  });

  it("emits else-if with proper indentation", () => {
    const steps = [
      createStep("if", { condition: "a > 1" }),
      createStep("set-field", { target: "x", value: "1" }),
      createStep("else-if", { condition: "a > 0" }),
      createStep("set-field", { target: "x", value: "0" }),
      createStep("end-if"),
    ];
    const lua = emitStepsLua(steps);
    const lines = lua.split("\n");
    expect(lines[0]).toBe("if a > 1 then");
    expect(lines[1]).toBe('  Prism.setField("x", 1)');
    expect(lines[2]).toBe("elseif a > 0 then");
    expect(lines[3]).toBe('  Prism.setField("x", 0)');
    expect(lines[4]).toBe("end");
  });
});

// ── emitStepsLuaWithMap ─────────────────────────────────────────────────────

describe("emitStepsLuaWithMap", () => {
  it("returns code matching emitStepsLua", () => {
    const steps = [
      createStep("set-field", { target: "a", value: "1" }),
      createStep("commit-record"),
    ];
    const result = emitStepsLuaWithMap(steps);
    expect(result.code).toBe(emitStepsLua(steps));
  });

  it("maps each step id to its 1-based Lua line", () => {
    const a = createStep("set-field", { target: "a", value: "1" });
    const b = createStep("set-field", { target: "b", value: "2" });
    const c = createStep("commit-record");
    const result = emitStepsLuaWithMap([a, b, c]);
    expect(result.stepToLine.get(a.id)).toBe(1);
    expect(result.stepToLine.get(b.id)).toBe(2);
    expect(result.stepToLine.get(c.id)).toBe(3);
  });

  it("maps lines back to step ids", () => {
    const a = createStep("set-field", { target: "a", value: "1" });
    const b = createStep("commit-record");
    const result = emitStepsLuaWithMap([a, b]);
    expect(result.lineToStep.get(1)).toBe(a.id);
    expect(result.lineToStep.get(2)).toBe(b.id);
  });

  it("maps steps inside indented blocks to the correct lines", () => {
    const ifStep = createStep("if", { condition: "x > 0" });
    const inner = createStep("set-field", { target: "y", value: "1" });
    const endStep = createStep("end-if");
    const result = emitStepsLuaWithMap([ifStep, inner, endStep]);
    expect(result.code.split("\n")).toEqual([
      "if x > 0 then",
      '  Prism.setField("y", 1)',
      "end",
    ]);
    expect(result.stepToLine.get(ifStep.id)).toBe(1);
    expect(result.stepToLine.get(inner.id)).toBe(2);
    expect(result.stepToLine.get(endStep.id)).toBe(3);
  });

  it("records disabled steps as their comment line", () => {
    const a = createStep("set-field", { target: "a", value: "1" });
    const disabled = createStep("delete-record");
    disabled.disabled = true;
    const b = createStep("commit-record");
    const result = emitStepsLuaWithMap([a, disabled, b]);
    expect(result.stepToLine.get(disabled.id)).toBe(2);
    expect(result.lineToStep.get(2)).toBe(disabled.id);
    expect(result.stepToLine.get(b.id)).toBe(3);
  });
});

// ── validateSteps ───────────────────────────────────────────────────────────

describe("validateSteps", () => {
  it("returns empty for valid script", () => {
    const steps = [
      createStep("if", { condition: "true" }),
      createStep("set-field", { target: "a", value: "1" }),
      createStep("end-if"),
    ];
    expect(validateSteps(steps)).toEqual([]);
  });

  it("detects unclosed If", () => {
    const steps = [createStep("if", { condition: "true" })];
    const errors = validateSteps(steps);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain("Unclosed");
  });

  it("detects End If without If", () => {
    const errors = validateSteps([createStep("end-if")]);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain("without matching");
  });

  it("detects unclosed Loop", () => {
    const errors = validateSteps([createStep("loop")]);
    expect(errors).toHaveLength(1);
    expect(errors[0]).toContain("Unclosed");
  });

  it("detects End Loop without Loop", () => {
    const errors = validateSteps([createStep("end-loop")]);
    expect(errors[0]).toContain("without matching");
  });

  it("detects Exit Loop If outside loop", () => {
    const errors = validateSteps([createStep("exit-loop-if", { condition: "true" })]);
    expect(errors[0]).toContain("outside of a loop");
  });

  it("detects Else without If", () => {
    const errors = validateSteps([createStep("else")]);
    expect(errors[0]).toContain('without matching "If"');
  });

  it("validates nested blocks correctly", () => {
    const steps = [
      createStep("loop"),
      createStep("if", { condition: "true" }),
      createStep("exit-loop-if", { condition: "done" }),
      createStep("end-if"),
      createStep("end-loop"),
    ];
    expect(validateSteps(steps)).toEqual([]);
  });

  it("detects mismatched nesting", () => {
    const steps = [
      createStep("if", { condition: "true" }),
      createStep("loop"),
      createStep("end-if"),  // Wrong! Should be end-loop first
      createStep("end-loop"),
    ];
    const errors = validateSteps(steps);
    expect(errors.length).toBeGreaterThan(0);
  });
});
