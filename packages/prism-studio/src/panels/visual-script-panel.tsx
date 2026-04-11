/**
 * VisualScriptPanel — FileMaker-style step-based script editor.
 *
 * Non-programmers configure scripts by adding steps from a categorized palette.
 * Each step renders as a card with dropdowns/inputs for its parameters.
 * Live Luau preview shows the generated code. Block validation catches errors.
 *
 * Lens #24 (Shift+S)
 */

import { useState, useMemo, useCallback, type CSSProperties } from "react";
import { useKernel } from "../kernel/kernel-context.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  type ScriptStep,
  type ScriptStepKind,
  type VisualScript,
  createStep,
  createVisualScript,
  emitStepsLuau,
  emitStepsLuauWithMap,
  validateSteps,
  getStepMeta,
  getStepCategories,
} from "@prism/core/facet";
import {
  createLuauDebugger,
  type DebugRunResult,
  type TraceFrame,
} from "@prism/core/luau";

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
  breakpoint,
  onToggleBreakpoint,
  isCurrent,
}: {
  step: ScriptStep;
  index: number;
  indent: number;
  onUpdate: (params: Record<string, string>) => void;
  onRemove: () => void;
  onMove: (dir: -1 | 1) => void;
  onToggle: () => void;
  breakpoint: boolean;
  onToggleBreakpoint: () => void;
  isCurrent: boolean;
}) {
  const meta = getStepMeta(step.kind);

  return (
    <div
      data-testid={`visual-script-step-${index}`}
      data-current={isCurrent ? "true" : "false"}
      data-breakpoint={breakpoint ? "true" : "false"}
      style={{
        ...s.card,
        ...(step.disabled ? s.cardDisabled : {}),
        ...(isCurrent ? { borderColor: "#ffcc00", boxShadow: "0 0 0 1px #ffcc00" } : {}),
        marginLeft: indent * 16 + 20,
        position: "relative",
      }}
    >
      <button
        aria-label="toggle breakpoint"
        data-testid={`visual-script-bp-${index}`}
        onClick={onToggleBreakpoint}
        style={{
          position: "absolute",
          left: -18,
          top: 8,
          width: 14,
          height: 14,
          borderRadius: "50%",
          border: "1px solid #666",
          background: breakpoint ? "#e74c3c" : "transparent",
          cursor: "pointer",
          padding: 0,
        }}
        title={breakpoint ? "Clear breakpoint" : "Set breakpoint"}
      />
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
                  placeholder={param === "condition" ? "expression" : "luau code"}
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

// ── Debug Frames Panel ──────────────────────────────────────────────────────

function DebugFramesPanel({
  result,
  activeIdx,
  onSelect,
  onStepToNextBreakpoint,
  onClose,
}: {
  result: DebugRunResult;
  activeIdx: number;
  onSelect: (idx: number) => void;
  onStepToNextBreakpoint: () => void;
  onClose: () => void;
}) {
  const frame: TraceFrame | undefined = result.frames[activeIdx];
  return (
    <div
      data-testid="debug-frames-panel"
      style={{
        borderTop: "1px solid #533",
        background: "#181818",
        display: "flex",
        flexDirection: "column",
        maxHeight: 220,
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: 6,
          padding: "4px 8px",
          borderBottom: "1px solid #333",
          fontSize: 11,
        }}
      >
        <strong style={{ color: result.success ? "#6c6" : "#f88" }}>
          {result.success ? "✓ Debug" : "✗ Debug"}
        </strong>
        <span style={{ color: "#888" }}>
          {result.frames.length} frame{result.frames.length === 1 ? "" : "s"}
        </span>
        {!result.success && result.error && (
          <span style={{ color: "#f88" }}>· {result.error}</span>
        )}
        <div style={{ flex: 1 }} />
        <button
          data-testid="debug-prev-frame"
          style={s.btn}
          onClick={() => onSelect(Math.max(0, activeIdx - 1))}
          disabled={activeIdx === 0}
        >
          ◀ Prev
        </button>
        <button
          data-testid="debug-next-frame"
          style={s.btn}
          onClick={() => onSelect(Math.min(result.frames.length - 1, activeIdx + 1))}
          disabled={activeIdx >= result.frames.length - 1}
        >
          Next ▶
        </button>
        <button
          data-testid="debug-continue"
          style={s.btn}
          onClick={onStepToNextBreakpoint}
          title="Jump to next breakpoint"
        >
          ⇥ Continue
        </button>
        <button data-testid="debug-close" style={s.btn} onClick={onClose}>✕</button>
      </div>
      <div style={{ display: "flex", flex: 1, minHeight: 0 }}>
        <div
          style={{
            width: 160,
            overflow: "auto",
            borderRight: "1px solid #333",
            fontSize: 11,
          }}
          data-testid="debug-frame-list"
        >
          {result.frames.map((f, i) => (
            <div
              key={i}
              data-testid={`debug-frame-${i}`}
              onClick={() => onSelect(i)}
              style={{
                padding: "2px 8px",
                cursor: "pointer",
                background: i === activeIdx ? "#2a3040" : "transparent",
                color: f.breakpoint ? "#f88" : "#ccc",
              }}
            >
              #{i} · line {f.line}
              {f.breakpoint ? " ●" : ""}
            </div>
          ))}
        </div>
        <div
          style={{ flex: 1, overflow: "auto", padding: 8, fontSize: 11, fontFamily: "monospace" }}
          data-testid="debug-locals"
        >
          {frame === undefined ? (
            <div style={{ color: "#666" }}>no frame selected</div>
          ) : Object.keys(frame.locals).length === 0 ? (
            <div style={{ color: "#666" }}>(no locals at line {frame.line})</div>
          ) : (
            Object.entries(frame.locals).map(([k, v]) => (
              <div key={k}>
                <span style={{ color: "#9cdcfe" }}>{k}</span>
                <span style={{ color: "#888" }}> = </span>
                <span style={{ color: "#ce9178" }}>{v}</span>
              </div>
            ))
          )}
        </div>
      </div>
    </div>
  );
}

// ── Panel ───────────────────────────────────────────────────────────────────

export function VisualScriptPanel() {
  const kernel = useKernel();
  const [script, setScript] = useState<VisualScript>(() => createVisualScript("new-script", "New Script"));

  // ── Debugger state ────────────────────────────────────────────────────────
  /** Step-id → has-breakpoint. */
  const [breakpoints, setBreakpoints] = useState<Set<string>>(() => new Set());
  /** Last debug run result, if any. */
  const [debugResult, setDebugResult] = useState<DebugRunResult | null>(null);
  /** Index into debugResult.frames — drives the yellow step highlight. */
  const [activeFrameIdx, setActiveFrameIdx] = useState(0);
  const [isDebugging, setIsDebugging] = useState(false);

  const categories = useMemo(() => getStepCategories(), []);
  const luau = useMemo(() => emitStepsLuau(script.steps), [script.steps]);
  const emittedWithMap = useMemo(() => emitStepsLuauWithMap(script.steps), [script.steps]);
  const errors = useMemo(() => validateSteps(script.steps), [script.steps]);

  // Currently-focused step id (for highlighting) = owner of the line in the
  // currently-selected trace frame. emittedWithMap.lineToStep does the lookup.
  const currentStepId = useMemo(() => {
    if (!debugResult || debugResult.frames.length === 0) return null;
    const frame = debugResult.frames[activeFrameIdx];
    if (!frame) return null;
    return emittedWithMap.lineToStep.get(frame.line) ?? null;
  }, [debugResult, activeFrameIdx, emittedWithMap]);

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

  const copyLuau = useCallback(() => {
    void navigator.clipboard.writeText(luau);
    kernel.notifications.add({ title: "Luau copied to clipboard", kind: "success" });
  }, [luau, kernel]);

  const toggleBreakpoint = useCallback((stepId: string) => {
    setBreakpoints((prev) => {
      const next = new Set(prev);
      if (next.has(stepId)) next.delete(stepId);
      else next.add(stepId);
      return next;
    });
  }, []);

  const debugRun = useCallback(async () => {
    if (errors.length > 0) {
      kernel.notifications.add({
        title: `Fix ${errors.length} validation error${errors.length === 1 ? "" : "s"} first`,
        kind: "warning",
      });
      return;
    }
    if (script.steps.length === 0) {
      kernel.notifications.add({ title: "No steps to debug", kind: "warning" });
      return;
    }
    setIsDebugging(true);
    try {
      const dbg = await createLuauDebugger();
      try {
        // Apply breakpoints — translate step ids to Luau line numbers.
        for (const stepId of breakpoints) {
          const line = emittedWithMap.stepToLine.get(stepId);
          if (line !== undefined) dbg.setBreakpoint(line);
        }
        const result = await dbg.run(emittedWithMap.code);
        setDebugResult(result);
        setActiveFrameIdx(0);
        if (result.success) {
          kernel.notifications.add({
            title: `Debug: ${result.frames.length} frame${result.frames.length === 1 ? "" : "s"} captured`,
            kind: "success",
          });
        } else {
          kernel.notifications.add({
            title: `Debug error: ${result.error ?? "unknown"}`,
            kind: "error",
          });
        }
      } finally {
        await dbg.dispose();
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      kernel.notifications.add({ title: `Debug failed: ${message}`, kind: "error" });
    } finally {
      setIsDebugging(false);
    }
  }, [breakpoints, emittedWithMap, errors, script.steps.length, kernel]);

  const stepToNextBreakpoint = useCallback(() => {
    if (!debugResult) return;
    const frames = debugResult.frames;
    for (let i = activeFrameIdx + 1; i < frames.length; i++) {
      if (frames[i]?.breakpoint) {
        setActiveFrameIdx(i);
        return;
      }
    }
    // No next breakpoint — jump to end.
    setActiveFrameIdx(Math.max(0, frames.length - 1));
  }, [debugResult, activeFrameIdx]);

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
          <button style={s.btn} onClick={copyLuau}>Copy Luau</button>
          <button
            data-testid="visual-script-debug-btn"
            style={s.btn}
            onClick={() => void debugRun()}
            disabled={isDebugging}
            title="Run script under the step debugger"
          >
            {isDebugging ? "\u25B6 Running\u2026" : "\u25B6 Debug"}
          </button>
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
              breakpoint={breakpoints.has(step.id)}
              onToggleBreakpoint={() => toggleBreakpoint(step.id)}
              isCurrent={currentStepId === step.id}
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

        {/* Debugger Frames Panel */}
        {debugResult && (
          <DebugFramesPanel
            result={debugResult}
            activeIdx={activeFrameIdx}
            onSelect={setActiveFrameIdx}
            onStepToNextBreakpoint={stepToNextBreakpoint}
            onClose={() => setDebugResult(null)}
          />
        )}

        {/* Luau Preview */}
        <div style={s.preview}>
          {luau || "-- Empty script"}
        </div>
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const VISUAL_SCRIPT_LENS_ID = lensId("visual-script");

export const visualScriptLensManifest: LensManifest = {

  id: VISUAL_SCRIPT_LENS_ID,
  name: "Visual Script",
  icon: "\u{1F9E9}",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-visual-script", name: "Switch to Visual Script Editor", shortcut: ["shift+s"], section: "Navigation" }],
  },
};

export const visualScriptLensBundle: LensBundle = defineLensBundle(
  visualScriptLensManifest,
  VisualScriptPanel,
);
