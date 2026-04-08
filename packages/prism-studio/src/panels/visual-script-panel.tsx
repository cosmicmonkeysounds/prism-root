/**
 * VisualScriptPanel — FileMaker-style step-based script editor.
 *
 * Non-programmers configure scripts by adding steps from a categorized palette.
 * Each step renders as a card with dropdowns/inputs for its parameters.
 * Live Lua preview shows the generated code. Block validation catches errors.
 *
 * Lens #24 (Shift+S)
 */

import { useState, useMemo, useCallback, type CSSProperties } from "react";
import { useKernel } from "../kernel/kernel-context.js";
import {
  type ScriptStep,
  type ScriptStepKind,
  type VisualScript,
  createStep,
  createVisualScript,
  emitStepsLua,
  validateSteps,
  getStepMeta,
  getStepCategories,
} from "@prism/core/layer1";

// ── Styles ──────────────────────────────────────────────────────────────────

const s: Record<string, CSSProperties> = {
  root: { display: "flex", height: "100%", fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a" },
  sidebar: { width: 220, borderRight: "1px solid #333", overflow: "auto", padding: 8 },
  main: { flex: 1, display: "flex", flexDirection: "column", overflow: "hidden" },
  toolbar: { display: "flex", gap: 6, padding: 8, borderBottom: "1px solid #333", alignItems: "center" },
  stepList: { flex: 1, overflow: "auto", padding: 8 },
  preview: { borderTop: "1px solid #333", height: 200, overflow: "auto", padding: 8, fontFamily: "monospace", fontSize: 12, color: "#9cdcfe", background: "#111", whiteSpace: "pre" },
  card: { background: "#252525", border: "1px solid #444", borderRadius: 4, padding: "6px 8px", marginBottom: 4, cursor: "grab" },
  cardDisabled: { opacity: 0.5 },
  cardError: { borderColor: "#f44" },
  cardLabel: { fontWeight: 600, fontSize: 12, color: "#aaa", marginBottom: 4 },
  row: { display: "flex", gap: 4, alignItems: "center", flexWrap: "wrap" as const, marginBottom: 2 },
  input: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "2px 6px", color: "#ccc", fontSize: 12, flex: 1, minWidth: 80 },
  select: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "2px 4px", color: "#ccc", fontSize: 12 },
  btn: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "2px 8px", color: "#ccc", cursor: "pointer", fontSize: 11 },
  btnDanger: { background: "#442", border: "1px solid #855", borderRadius: 3, padding: "2px 8px", color: "#f88", cursor: "pointer", fontSize: 11 },
  catHeader: { fontSize: 11, fontWeight: 700, color: "#888", textTransform: "uppercase" as const, padding: "8px 0 4px", letterSpacing: "0.5px" },
  palItem: { padding: "3px 6px", borderRadius: 3, cursor: "pointer", fontSize: 12, marginBottom: 1 },
  errBox: { color: "#f88", fontSize: 11, padding: "4px 8px", borderTop: "1px solid #533" },
  scriptName: { background: "#333", border: "1px solid #555", borderRadius: 3, padding: "2px 6px", color: "#ccc", fontSize: 13, fontWeight: 600 },
};

// ── Step Card ───────────────────────────────────────────────────────────────

function StepCard({
  step,
  index,
  indent,
  onUpdate,
  onRemove,
  onMove,
  onToggle,
}: {
  step: ScriptStep;
  index: number;
  indent: number;
  onUpdate: (params: Record<string, string>) => void;
  onRemove: () => void;
  onMove: (dir: -1 | 1) => void;
  onToggle: () => void;
}) {
  const meta = getStepMeta(step.kind);

  return (
    <div
      style={{
        ...s.card,
        ...(step.disabled ? s.cardDisabled : {}),
        marginLeft: indent * 16,
      }}
    >
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
        <div style={s.cardLabel}>
          <span style={{ color: "#666", marginRight: 4 }}>{index + 1}.</span>
          {meta.label}
        </div>
        <div style={{ display: "flex", gap: 2 }}>
          <button style={s.btn} onClick={() => onMove(-1)} title="Move up">{"\u2191"}</button>
          <button style={s.btn} onClick={() => onMove(1)} title="Move down">{"\u2193"}</button>
          <button style={s.btn} onClick={onToggle} title={step.disabled ? "Enable" : "Disable"}>
            {step.disabled ? "\u25CB" : "\u25CF"}
          </button>
          <button style={s.btnDanger} onClick={onRemove} title="Remove">{"\u2715"}</button>
        </div>
      </div>
      {meta.params.length > 0 && (
        <div style={s.row}>
          {meta.params.map((param) => (
            <label key={param} style={{ display: "flex", alignItems: "center", gap: 4, flex: 1 }}>
              <span style={{ color: "#777", fontSize: 11, minWidth: 40 }}>{param}:</span>
              {param === "condition" || param === "code" ? (
                <input
                  style={{ ...s.input, fontFamily: "monospace" }}
                  value={step.params[param] ?? ""}
                  onChange={(e) => onUpdate({ ...step.params, [param]: e.target.value })}
                  placeholder={param === "condition" ? "expression" : "lua code"}
                />
              ) : param === "direction" ? (
                <select
                  style={s.select}
                  value={step.params[param] ?? "asc"}
                  onChange={(e) => onUpdate({ ...step.params, [param]: e.target.value })}
                >
                  <option value="asc">Ascending</option>
                  <option value="desc">Descending</option>
                </select>
              ) : param === "position" ? (
                <select
                  style={s.select}
                  value={step.params[param] ?? "next"}
                  onChange={(e) => onUpdate({ ...step.params, [param]: e.target.value })}
                >
                  <option value="first">First</option>
                  <option value="last">Last</option>
                  <option value="next">Next</option>
                  <option value="previous">Previous</option>
                </select>
              ) : param === "kind" ? (
                <select
                  style={s.select}
                  value={step.params[param] ?? "info"}
                  onChange={(e) => onUpdate({ ...step.params, [param]: e.target.value })}
                >
                  <option value='"info"'>Info</option>
                  <option value='"success"'>Success</option>
                  <option value='"warning"'>Warning</option>
                  <option value='"error"'>Error</option>
                </select>
              ) : (
                <input
                  style={s.input}
                  value={step.params[param] ?? ""}
                  onChange={(e) => onUpdate({ ...step.params, [param]: e.target.value })}
                  placeholder={param}
                />
              )}
            </label>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Panel ───────────────────────────────────────────────────────────────────

export function VisualScriptPanel() {
  const kernel = useKernel();
  const [script, setScript] = useState<VisualScript>(() => createVisualScript("new-script", "New Script"));

  const categories = useMemo(() => getStepCategories(), []);
  const lua = useMemo(() => emitStepsLua(script.steps), [script.steps]);
  const errors = useMemo(() => validateSteps(script.steps), [script.steps]);

  // Compute indentation levels
  const indents = useMemo(() => {
    const levels: number[] = [];
    let indent = 0;
    for (const step of script.steps) {
      const meta = getStepMeta(step.kind);
      if (meta.closesBlock || meta.continuesBlock) indent = Math.max(0, indent - 1);
      levels.push(indent);
      if (meta.opensBlock || meta.continuesBlock) indent++;
    }
    return levels;
  }, [script.steps]);

  const addStep = useCallback((kind: ScriptStepKind) => {
    setScript((prev) => ({
      ...prev,
      steps: [...prev.steps, createStep(kind)],
    }));
  }, []);

  const updateStep = useCallback((index: number, params: Record<string, string>) => {
    setScript((prev) => ({
      ...prev,
      steps: prev.steps.map((s, i) => (i === index ? { ...s, params } : s)),
    }));
  }, []);

  const removeStep = useCallback((index: number) => {
    setScript((prev) => ({
      ...prev,
      steps: prev.steps.filter((_, i) => i !== index),
    }));
  }, []);

  const moveStep = useCallback((index: number, dir: -1 | 1) => {
    setScript((prev) => {
      const newIndex = index + dir;
      if (newIndex < 0 || newIndex >= prev.steps.length) return prev;
      const steps = [...prev.steps];
      [steps[index], steps[newIndex]] = [steps[newIndex] as ScriptStep, steps[index] as ScriptStep];
      return { ...prev, steps };
    });
  }, []);

  const toggleStep = useCallback((index: number) => {
    setScript((prev) => ({
      ...prev,
      steps: prev.steps.map((s, i) => (i === index ? { ...s, disabled: !s.disabled } : s)),
    }));
  }, []);

  const copyLua = useCallback(() => {
    void navigator.clipboard.writeText(lua);
    kernel.notifications.add({ title: "Lua copied to clipboard", kind: "success" });
  }, [lua, kernel]);

  return (
    <div style={s.root} data-testid="visual-script-panel">
      {/* Step Palette */}
      <div style={s.sidebar}>
        <div style={{ fontWeight: 700, marginBottom: 8, color: "#aaa" }}>Step Palette</div>
        {categories.map(({ category, steps }) => (
          <div key={category}>
            <div style={s.catHeader}>{category}</div>
            {steps.map((meta) => (
              <div
                key={meta.kind}
                style={s.palItem}
                onClick={() => addStep(meta.kind)}
                onMouseEnter={(e) => { (e.target as HTMLElement).style.background = "#333"; }}
                onMouseLeave={(e) => { (e.target as HTMLElement).style.background = "transparent"; }}
                title={meta.description}
              >
                {meta.label}
              </div>
            ))}
          </div>
        ))}
      </div>

      {/* Main area */}
      <div style={s.main}>
        {/* Toolbar */}
        <div style={s.toolbar}>
          <input
            style={s.scriptName}
            value={script.name}
            onChange={(e) => setScript((p) => ({ ...p, name: e.target.value }))}
          />
          <div style={{ flex: 1 }} />
          <span style={{ color: "#666", fontSize: 11 }}>
            {script.steps.length} step{script.steps.length !== 1 ? "s" : ""}
          </span>
          <button style={s.btn} onClick={copyLua}>Copy Lua</button>
        </div>

        {/* Step List */}
        <div style={s.stepList}>
          {script.steps.length === 0 && (
            <div style={{ color: "#666", textAlign: "center", padding: 40 }}>
              Click steps in the palette to build your script
            </div>
          )}
          {script.steps.map((step, i) => (
            <StepCard
              key={step.id}
              step={step}
              index={i}
              indent={indents[i] ?? 0}
              onUpdate={(params) => updateStep(i, params)}
              onRemove={() => removeStep(i)}
              onMove={(dir) => moveStep(i, dir)}
              onToggle={() => toggleStep(i)}
            />
          ))}
        </div>

        {/* Errors */}
        {errors.length > 0 && (
          <div style={s.errBox}>
            {errors.map((err, i) => (
              <div key={i}>{"\u26A0"} {err}</div>
            ))}
          </div>
        )}

        {/* Lua Preview */}
        <div style={s.preview}>
          {lua || "-- Empty script"}
        </div>
      </div>
    </div>
  );
}
