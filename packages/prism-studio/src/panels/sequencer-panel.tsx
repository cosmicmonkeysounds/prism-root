/**
 * Sequencer Panel — visual automation builder.
 *
 * Compose conditions (subject → operator → value) and script steps
 * (set-variable, emit-event, call-function) via dropdown menus.
 * Live Lua preview via emitConditionLua / emitScriptLua.
 */

import { useState, useCallback } from "react";
import { useSequencer, useKernel } from "../kernel/index.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import type {
  SequencerConditionState,
  SequencerConditionClause,
  SequencerScriptState,
  SequencerScriptStep,
  SequencerOperator,
  SequencerSubjectKind,
  SequencerActionKind,
  SequencerCombinator,
} from "@prism/core/facet";

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "1rem",
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  header: {
    fontSize: "1.25rem",
    fontWeight: 600,
    marginBottom: "1rem",
    color: "#e5e5e5",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  btn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnPrimary: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  btnDanger: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#5c1010",
    border: "1px solid #7a1414",
    borderRadius: 3,
    color: "#f87171",
    cursor: "pointer",
  },
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.25rem 0.375rem",
    color: "#e5e5e5",
    fontSize: "0.75rem",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  luaPreview: {
    background: "#1a1a2e",
    border: "1px solid #333",
    borderRadius: "0.25rem",
    padding: "0.5rem",
    fontFamily: "monospace",
    fontSize: "0.75rem",
    color: "#4fc1ff",
    whiteSpace: "pre-wrap" as const,
    overflow: "auto",
    maxHeight: 200,
  },
  clauseRow: {
    display: "flex",
    gap: 6,
    alignItems: "center",
    marginBottom: 4,
  },
  stepRow: {
    display: "flex",
    gap: 6,
    alignItems: "center",
    marginBottom: 4,
    padding: "4px 0",
    borderBottom: "1px solid #2a2a2a",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  tabs: {
    display: "flex",
    gap: 0,
    borderBottom: "1px solid #333",
    marginBottom: "0.75rem",
  },
  tab: {
    padding: "0.375rem 0.75rem",
    fontSize: "0.8125rem",
    cursor: "pointer",
    border: "none",
    background: "transparent",
    color: "#888",
    borderBottom: "2px solid transparent",
  },
  tabActive: {
    color: "#e5e5e5",
    borderBottom: "2px solid #0e639c",
  },
} as const;

// ── Operators ───────────────────────────────────────────────────────────────

const OPERATORS: { value: SequencerOperator; label: string; needsValue: boolean }[] = [
  { value: "is", label: "=", needsValue: true },
  { value: "is-not", label: "!=", needsValue: true },
  { value: "gt", label: ">", needsValue: true },
  { value: "lt", label: "<", needsValue: true },
  { value: "gte", label: ">=", needsValue: true },
  { value: "lte", label: "<=", needsValue: true },
  { value: "contains", label: "contains", needsValue: true },
  { value: "starts-with", label: "starts with", needsValue: true },
  { value: "is-true", label: "is true", needsValue: false },
  { value: "is-false", label: "is false", needsValue: false },
  { value: "is-nil", label: "is nil", needsValue: false },
  { value: "is-not-nil", label: "is not nil", needsValue: false },
];

const SUBJECT_KINDS: SequencerSubjectKind[] = ["variable", "field", "event", "custom"];
const ACTION_KINDS: SequencerActionKind[] = ["set-variable", "add-variable", "emit-event", "call-function", "custom"];

let clauseCounter = 0;
let stepCounter = 0;

// ── Condition Builder ───────────────────────────────────────────────────────

function ConditionBuilder({
  state,
  onChange,
}: {
  state: SequencerConditionState;
  onChange: (state: SequencerConditionState) => void;
}) {
  const addClause = useCallback(() => {
    const clause: SequencerConditionClause = {
      id: `clause_${++clauseCounter}`,
      subjectKind: "variable",
      subject: "myVar",
      operator: "is",
      value: "",
    };
    onChange({ ...state, clauses: [...state.clauses, clause] });
  }, [state, onChange]);

  const removeClause = useCallback(
    (id: string) => {
      onChange({ ...state, clauses: state.clauses.filter((c) => c.id !== id) });
    },
    [state, onChange],
  );

  const updateClause = useCallback(
    (id: string, patch: Partial<SequencerConditionClause>) => {
      onChange({
        ...state,
        clauses: state.clauses.map((c) => (c.id === id ? { ...c, ...patch } : c)),
      });
    },
    [state, onChange],
  );

  return (
    <div data-testid="condition-builder">
      <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 8 }}>
        <span style={styles.meta}>Match</span>
        <select
          style={styles.select}
          value={state.combinator}
          onChange={(e) => onChange({ ...state, combinator: e.target.value as SequencerCombinator })}
          data-testid="combinator-select"
        >
          <option value="all">ALL (AND)</option>
          <option value="any">ANY (OR)</option>
        </select>
        <span style={styles.meta}>of the following:</span>
        <button style={styles.btn} onClick={addClause} data-testid="add-clause-btn">
          + Clause
        </button>
      </div>

      {state.clauses.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", padding: "0.5rem 0" }}>
          No conditions. Click "+ Clause" to add one.
        </div>
      )}

      {state.clauses.map((clause) => {
        const op = OPERATORS.find((o) => o.value === clause.operator);
        return (
          <div key={clause.id} style={styles.clauseRow as React.CSSProperties} data-testid={`clause-${clause.id}`}>
            <select
              style={{ ...styles.select, width: 80 }}
              value={clause.subjectKind}
              onChange={(e) => updateClause(clause.id, { subjectKind: e.target.value as SequencerSubjectKind })}
            >
              {SUBJECT_KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
            </select>
            <input
              style={{ ...styles.input, width: 120 }}
              value={clause.subject}
              onChange={(e) => updateClause(clause.id, { subject: e.target.value })}
              placeholder="subject"
            />
            <select
              style={{ ...styles.select, width: 90 }}
              value={clause.operator}
              onChange={(e) => updateClause(clause.id, { operator: e.target.value as SequencerOperator })}
            >
              {OPERATORS.map((o) => <option key={o.value} value={o.value}>{o.label}</option>)}
            </select>
            {(op?.needsValue !== false) && (
              <input
                style={{ ...styles.input, width: 100 }}
                value={clause.value}
                onChange={(e) => updateClause(clause.id, { value: e.target.value })}
                placeholder="value"
              />
            )}
            <button
              style={styles.btnDanger}
              onClick={() => removeClause(clause.id)}
              data-testid={`remove-clause-${clause.id}`}
            >
              X
            </button>
          </div>
        );
      })}
    </div>
  );
}

// ── Script Builder ──────────────────────────────────────────────────────────

function ScriptBuilder({
  state,
  onChange,
}: {
  state: SequencerScriptState;
  onChange: (state: SequencerScriptState) => void;
}) {
  const addStep = useCallback(() => {
    const step: SequencerScriptStep = {
      id: `step_${++stepCounter}`,
      actionKind: "set-variable",
      target: "scope.myVar",
      value: "value",
    };
    onChange({ steps: [...state.steps, step] });
  }, [state, onChange]);

  const removeStep = useCallback(
    (id: string) => {
      onChange({ steps: state.steps.filter((s) => s.id !== id) });
    },
    [state, onChange],
  );

  const updateStep = useCallback(
    (id: string, patch: Partial<SequencerScriptStep>) => {
      onChange({ steps: state.steps.map((s) => (s.id === id ? { ...s, ...patch } : s)) });
    },
    [state, onChange],
  );

  const moveStep = useCallback(
    (id: string, dir: -1 | 1) => {
      const idx = state.steps.findIndex((s) => s.id === id);
      if (idx < 0) return;
      const newIdx = idx + dir;
      if (newIdx < 0 || newIdx >= state.steps.length) return;
      const steps = [...state.steps];
      const a = steps[idx];
      const b = steps[newIdx];
      if (a === undefined || b === undefined) return;
      [steps[idx], steps[newIdx]] = [b, a];
      onChange({ steps });
    },
    [state, onChange],
  );

  return (
    <div data-testid="script-builder">
      <div style={{ display: "flex", gap: 6, marginBottom: 8 }}>
        <button style={styles.btn} onClick={addStep} data-testid="add-step-btn">
          + Step
        </button>
        <span style={styles.meta}>{state.steps.length} step(s)</span>
      </div>

      {state.steps.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", padding: "0.5rem 0" }}>
          No script steps. Click "+ Step" to add one.
        </div>
      )}

      {state.steps.map((step, idx) => (
        <div key={step.id} style={styles.stepRow as React.CSSProperties} data-testid={`step-${step.id}`}>
          <span style={{ ...styles.meta, width: 20 }}>{idx + 1}.</span>
          <select
            style={{ ...styles.select, width: 110 }}
            value={step.actionKind}
            onChange={(e) => updateStep(step.id, { actionKind: e.target.value as SequencerActionKind })}
          >
            {ACTION_KINDS.map((k) => <option key={k} value={k}>{k}</option>)}
          </select>
          <input
            style={{ ...styles.input, flex: 1, minWidth: 80 }}
            value={step.target}
            onChange={(e) => updateStep(step.id, { target: e.target.value })}
            placeholder="target"
          />
          <input
            style={{ ...styles.input, flex: 1, minWidth: 60 }}
            value={step.value}
            onChange={(e) => updateStep(step.id, { value: e.target.value })}
            placeholder="value"
          />
          <input
            style={{ ...styles.input, width: 80 }}
            value={(step.extraArgs ?? []).join(", ")}
            onChange={(e) =>
              updateStep(step.id, { extraArgs: e.target.value.split(",").map((s) => s.trim()).filter(Boolean) })
            }
            placeholder="extra args"
          />
          <button style={styles.btn} onClick={() => moveStep(step.id, -1)} disabled={idx === 0}>
            {"\u25B2"}
          </button>
          <button
            style={styles.btn}
            onClick={() => moveStep(step.id, 1)}
            disabled={idx === state.steps.length - 1}
          >
            {"\u25BC"}
          </button>
          <button
            style={styles.btnDanger}
            onClick={() => removeStep(step.id)}
            data-testid={`remove-step-${step.id}`}
          >
            X
          </button>
        </div>
      ))}
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export function SequencerPanel() {
  const kernel = useKernel();
  const { emitConditionLua, emitScriptLua } = useSequencer();

  const [tab, setTab] = useState<"condition" | "script">("condition");

  const [conditionState, setConditionState] = useState<SequencerConditionState>({
    combinator: "all",
    clauses: [],
  });

  const [scriptState, setScriptState] = useState<SequencerScriptState>({
    steps: [],
  });

  // Live Lua preview
  const luaOutput = tab === "condition"
    ? (conditionState.clauses.length > 0 ? emitConditionLua(conditionState) : "-- Add clauses to see Lua output")
    : (scriptState.steps.length > 0 ? emitScriptLua(scriptState) : "-- Add steps to see Lua output");

  const handleCopyLua = useCallback(() => {
    void navigator.clipboard?.writeText(luaOutput);
    kernel.notifications.add({ title: "Lua copied to clipboard", kind: "info" });
  }, [luaOutput, kernel]);

  return (
    <div style={styles.container} data-testid="sequencer-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Sequencer</span>
        <span style={styles.badge}>Lua</span>
      </div>

      {/* Tabs */}
      <div style={styles.tabs as React.CSSProperties}>
        <button
          style={{ ...styles.tab, ...(tab === "condition" ? styles.tabActive : {}) }}
          onClick={() => setTab("condition")}
          data-testid="tab-condition"
        >
          Conditions
        </button>
        <button
          style={{ ...styles.tab, ...(tab === "script" ? styles.tabActive : {}) }}
          onClick={() => setTab("script")}
          data-testid="tab-script"
        >
          Script
        </button>
      </div>

      {/* Builder */}
      <div style={styles.card}>
        {tab === "condition" ? (
          <ConditionBuilder state={conditionState} onChange={setConditionState} />
        ) : (
          <ScriptBuilder state={scriptState} onChange={setScriptState} />
        )}
      </div>

      {/* Lua preview */}
      <div style={styles.card}>
        <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
          <div style={styles.sectionTitle}>Lua Output</div>
          <button style={styles.btn} onClick={handleCopyLua} data-testid="copy-lua-btn">
            Copy
          </button>
        </div>
        <div style={styles.luaPreview} data-testid="lua-preview">
          {luaOutput}
        </div>
      </div>
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const SEQUENCER_LENS_ID = lensId("sequencer");

export const sequencerLensManifest: LensManifest = {

  id: SEQUENCER_LENS_ID,
  name: "Sequencer",
  icon: "\uD83C\uDFBC",
  category: "facet",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-sequencer", name: "Switch to Sequencer", shortcut: ["q"], section: "Navigation" }],
  },
};

export const sequencerLensBundle: LensBundle = defineLensBundle(
  sequencerLensManifest,
  SequencerPanel,
);
