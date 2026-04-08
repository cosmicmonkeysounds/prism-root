/**
 * Analysis Panel — dependency graph analysis, critical path, impact calculator.
 *
 * Surfaces graph-analysis Layer 1 functions:
 *   - Critical Path Method (CPM) plan
 *   - Dependency cycle detection
 *   - Blocking chain analysis
 *   - Slip impact calculator
 */

import { useState, useMemo } from "react";
import { useGraphAnalysis, useObjects, useSelection, useKernel } from "../kernel/index.js";
import type { ObjectId } from "@prism/core/object-model";
import { objectId } from "@prism/core/object-model";
import type { PlanResult, SlipImpact } from "@prism/core/graph-analysis";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
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
  },
  section: {
    marginBottom: "1.5rem",
  },
  sectionTitle: {
    fontSize: "0.875rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.5rem",
    borderBottom: "1px solid #333",
    paddingBottom: "0.375rem",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  critical: {
    background: "#3b1111",
    border: "1px solid #5c2020",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#333",
    color: "#888",
    marginLeft: "0.375rem",
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
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "4rem",
    outline: "none",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  tabRow: {
    display: "flex",
    gap: 4,
    marginBottom: "0.75rem",
    borderBottom: "1px solid #333",
    paddingBottom: "0.375rem",
  },
  row: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.25rem 0",
    borderBottom: "1px solid #2a2a2a",
    fontSize: "0.8125rem",
  },
  nodeRow: {
    display: "flex",
    justifyContent: "space-between",
    padding: "0.375rem 0.5rem",
    borderBottom: "1px solid #2a2a2a",
    fontSize: "0.8125rem",
    cursor: "pointer",
  },
} as const;

type AnalysisTab = "plan" | "cycles" | "impact";

// ── Plan View ───────────────────────────────────────────────────────────────

function PlanView({ plan, onSelect }: { plan: PlanResult; onSelect: (id: ObjectId) => void }) {
  if (plan.totalDurationDays === 0) {
    return (
      <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
        No objects with dependencies found.
      </div>
    );
  }

  return (
    <div data-testid="analysis-plan">
      <div style={styles.card}>
        <div style={{ color: "#e5e5e5", fontSize: "0.9375rem", fontWeight: 500 }}>
          Total Duration: {plan.totalDurationDays} day(s)
        </div>
        <div style={styles.meta}>
          Critical Path: {plan.criticalPath.length} node(s)
        </div>
      </div>

      <div style={styles.sectionTitle}>Critical Path</div>
      {plan.criticalPath.map((id) => {
        const node = plan.nodes.get(id);
        if (!node) return null;
        return (
          <div
            key={id}
            style={styles.critical}
            onClick={() => onSelect(objectId(id))}
            data-testid={`plan-node-${id}`}
          >
            <div style={{ color: "#f87171", fontSize: "0.8125rem", fontWeight: 500 }}>
              {node.name}
              <span style={{ ...styles.badge, color: "#f87171" }}>critical</span>
            </div>
            <div style={styles.meta}>
              ES: {node.earlyStart} | EF: {node.earlyFinish} | Duration: {node.durationDays}d | Float: {node.totalFloat}
            </div>
          </div>
        );
      })}

      <div style={styles.sectionTitle}>All Nodes ({plan.nodes.size})</div>
      {[...plan.nodes.values()]
        .sort((a, b) => a.earlyStart - b.earlyStart)
        .map((node) => (
          <div
            key={node.id}
            style={node.isCritical ? styles.critical : styles.card}
            onClick={() => onSelect(objectId(node.id))}
            data-testid={`plan-node-${node.id}`}
          >
            <div style={{ color: "#e5e5e5", fontSize: "0.8125rem" }}>
              {node.name}
              <span style={styles.badge}>{node.type}</span>
              {node.isCritical && <span style={{ ...styles.badge, color: "#f87171" }}>critical</span>}
            </div>
            <div style={styles.meta}>
              ES: {node.earlyStart} | EF: {node.earlyFinish} | LS: {node.lateStart} | LF: {node.lateFinish} | Float: {node.totalFloat}
              {node.predecessors.length > 0 && ` | Depends on: ${node.predecessors.length}`}
            </div>
          </div>
        ))}
    </div>
  );
}

// ── Cycle View ──────────────────────────────────────────────────────────────

function CycleView({ cycles, objectMap }: { cycles: string[][]; objectMap: Map<string, string> }) {
  return (
    <div data-testid="analysis-cycles">
      {cycles.length === 0 ? (
        <div style={{ ...styles.card, color: "#22c55e", textAlign: "center" }}>
          No dependency cycles detected.
        </div>
      ) : (
        <>
          <div style={{ ...styles.card, color: "#f87171" }}>
            {cycles.length} cycle(s) detected
          </div>
          {cycles.map((cycle, i) => (
            <div key={i} style={styles.critical} data-testid={`cycle-${i}`}>
              <div style={{ color: "#f87171", fontSize: "0.8125rem", fontWeight: 500 }}>
                Cycle {i + 1}
              </div>
              <div style={styles.meta}>
                {cycle.map((id) => objectMap.get(id) ?? id).join(" -> ")}
              </div>
            </div>
          ))}
        </>
      )}
    </div>
  );
}

// ── Impact View ─────────────────────────────────────────────────────────────

function ImpactView({
  selectedId,
  blockingChain,
  impacted,
  slipImpact,
  slipDays,
  onSlipDaysChange,
  objectMap,
  onSelect,
}: {
  selectedId: ObjectId | null;
  blockingChain: string[];
  impacted: string[];
  slipImpact: SlipImpact[];
  slipDays: number;
  onSlipDaysChange: (days: number) => void;
  objectMap: Map<string, string>;
  onSelect: (id: ObjectId) => void;
}) {
  if (!selectedId) {
    return (
      <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
        Select an object to analyze its impact.
      </div>
    );
  }

  return (
    <div data-testid="analysis-impact">
      <div style={styles.card}>
        <div style={{ color: "#e5e5e5", fontSize: "0.875rem" }}>
          Analyzing: <strong>{objectMap.get(selectedId) ?? selectedId}</strong>
        </div>
      </div>

      <div style={styles.sectionTitle}>
        Blocking Chain ({blockingChain.length} upstream)
      </div>
      {blockingChain.length === 0 ? (
        <div style={{ ...styles.meta, padding: "0.375rem 0" }}>No upstream blockers.</div>
      ) : (
        blockingChain.map((id) => (
          <div
            key={id}
            style={styles.nodeRow}
            onClick={() => onSelect(objectId(id))}
            data-testid={`blocker-${id}`}
          >
            <span>{objectMap.get(id) ?? id}</span>
            <span style={{ ...styles.badge, color: "#f59e0b" }}>blocker</span>
          </div>
        ))
      )}

      <div style={styles.sectionTitle}>
        Downstream Impact ({impacted.length} affected)
      </div>
      {impacted.length === 0 ? (
        <div style={{ ...styles.meta, padding: "0.375rem 0" }}>No downstream dependents.</div>
      ) : (
        impacted.map((id) => (
          <div
            key={id}
            style={styles.nodeRow}
            onClick={() => onSelect(objectId(id))}
            data-testid={`impacted-${id}`}
          >
            <span>{objectMap.get(id) ?? id}</span>
            <span style={{ ...styles.badge, color: "#3b82f6" }}>impacted</span>
          </div>
        ))
      )}

      <div style={styles.sectionTitle}>
        Slip Impact Calculator
      </div>
      <div style={{ display: "flex", gap: 8, alignItems: "center", marginBottom: "0.5rem" }}>
        <span style={{ fontSize: "0.8125rem" }}>If this slips by</span>
        <input
          style={styles.input}
          type="number"
          min={1}
          value={slipDays}
          onChange={(e) => onSlipDaysChange(Math.max(1, parseInt(e.target.value) || 1))}
          data-testid="slip-days-input"
        />
        <span style={{ fontSize: "0.8125rem" }}>day(s):</span>
      </div>
      {slipImpact.length === 0 ? (
        <div style={{ ...styles.meta, padding: "0.375rem 0" }}>No downstream impact.</div>
      ) : (
        slipImpact.map((si) => (
          <div
            key={si.objectId}
            style={styles.nodeRow}
            onClick={() => onSelect(objectId(si.objectId))}
            data-testid={`slip-${si.objectId}`}
          >
            <span>{si.objectName}</span>
            <span>
              <span style={{ ...styles.badge, color: "#f87171" }}>+{si.slipDays}d</span>
              <span style={{ ...styles.badge }}>depth {si.depth}</span>
            </span>
          </div>
        ))
      )}
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export function AnalysisPanel() {
  const kernel = useKernel();
  const { selectedId, select } = useSelection();
  const objects = useObjects();
  const analysis = useGraphAnalysis();
  const [tab, setTab] = useState<AnalysisTab>("plan");
  const [slipDays, setSlipDays] = useState(3);

  const objectMap = useMemo(() => {
    const m = new Map<string, string>();
    for (const obj of objects) m.set(obj.id, obj.name);
    return m;
  }, [objects]);

  const plan = useMemo(() => analysis.plan(), [objects]);
  const cycles = useMemo(() => analysis.detectCycles(), [objects]);

  const blockingChain = useMemo(
    () => (selectedId ? analysis.blockingChain(selectedId) : []),
    [selectedId, objects],
  );

  const impacted = useMemo(
    () => (selectedId ? analysis.impact(selectedId) : []),
    [selectedId, objects],
  );

  const slipImpact = useMemo(
    () => (selectedId ? analysis.slipImpact(selectedId, slipDays) : []),
    [selectedId, slipDays, objects],
  );

  const handleSelect = (id: ObjectId) => {
    select(id);
    kernel.notifications.add({ title: `Selected: ${objectMap.get(id) ?? id}`, kind: "info" });
  };

  return (
    <div style={styles.container} data-testid="analysis-panel">
      <div style={styles.header as React.CSSProperties}>Analysis</div>

      <div style={styles.tabRow as React.CSSProperties}>
        <button
          style={{ ...styles.btn, background: tab === "plan" ? "#094771" : "#333" }}
          onClick={() => setTab("plan")}
          data-testid="analysis-tab-plan"
        >
          Critical Path
        </button>
        <button
          style={{ ...styles.btn, background: tab === "cycles" ? "#094771" : "#333" }}
          onClick={() => setTab("cycles")}
          data-testid="analysis-tab-cycles"
        >
          Cycles {cycles.length > 0 && `(${cycles.length})`}
        </button>
        <button
          style={{ ...styles.btn, background: tab === "impact" ? "#094771" : "#333" }}
          onClick={() => setTab("impact")}
          data-testid="analysis-tab-impact"
        >
          Impact
        </button>
      </div>

      {tab === "plan" && <PlanView plan={plan} onSelect={handleSelect} />}
      {tab === "cycles" && <CycleView cycles={cycles} objectMap={objectMap} />}
      {tab === "impact" && (
        <ImpactView
          selectedId={selectedId}
          blockingChain={blockingChain}
          impacted={impacted}
          slipImpact={slipImpact}
          slipDays={slipDays}
          onSlipDaysChange={setSlipDays}
          objectMap={objectMap}
          onSelect={handleSelect}
        />
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const ANALYSIS_LENS_ID = lensId("analysis");

export const analysisLensManifest: LensManifest = {

  id: ANALYSIS_LENS_ID,
  name: "Analysis",
  icon: "\uD83D\uDCC8",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-analysis", name: "Switch to Analysis", shortcut: ["n"], section: "Navigation" }],
  },
};

export const analysisLensBundle: LensBundle = defineLensBundle(
  analysisLensManifest,
  AnalysisPanel,
);
