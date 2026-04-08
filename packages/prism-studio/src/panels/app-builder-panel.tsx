/**
 * App Builder Panel — Studio's self-replicating meta-builder.
 *
 * Lets users compose an App Profile (Flux / Lattice / Cadence / Grip /
 * Relay / Custom), pick build targets (web / Tauri / Capacitor / Relay
 * Docker), preview the resulting BuildPlan, and execute it via the
 * daemon (Tauri mode) or dry-run (browser mode).
 *
 * Lens #28 (Shift+B)
 */

import { useState, useCallback, useMemo, type CSSProperties } from "react";
import { useKernel, useBuilder } from "../kernel/index.js";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import type {
  BuildTarget,
  BuildPlan,
  BuildRun,
  BuildStepResult,
} from "../kernel/index.js";
import {
  serializeBuildPlan,
  serializeAppProfile,
} from "@prism/core/builder";

// ── Styles ────────────────────────────────────────────────────────────────

const s = {
  root: {
    padding: 16,
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    fontSize: 13,
    color: "#ccc",
    background: "#1a1a1a",
  },
  header: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: 12,
  },
  title: { fontSize: 18, fontWeight: 600, color: "#e5e5e5" },
  subtitle: { fontSize: 12, color: "#888", marginTop: 2 },
  section: { marginBottom: 20 },
  sectionTitle: {
    fontSize: 11,
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: 8,
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: 6,
    padding: 12,
    marginBottom: 8,
  },
  cardActive: {
    background: "#1e3a5f",
    border: "1px solid #4a9eff",
    borderRadius: 6,
    padding: 12,
    marginBottom: 8,
  },
  profileGrid: {
    display: "grid",
    gridTemplateColumns: "repeat(auto-fit, minmax(180px, 1fr))",
    gap: 8,
  },
  row: { display: "flex", gap: 8, alignItems: "center", marginBottom: 4 },
  label: { color: "#888", minWidth: 90 },
  badge: {
    display: "inline-block",
    padding: "2px 8px",
    borderRadius: 10,
    fontSize: 10,
    background: "#333",
    color: "#ccc",
    marginRight: 4,
  },
  pill: (active: boolean): CSSProperties => ({
    display: "inline-flex",
    alignItems: "center",
    gap: 6,
    padding: "6px 12px",
    borderRadius: 20,
    border: active ? "1px solid #4a9eff" : "1px solid #444",
    background: active ? "#1e3a5f" : "#252526",
    color: active ? "#fff" : "#ccc",
    cursor: "pointer",
    fontSize: 12,
  }),
  btn: {
    padding: "8px 16px",
    border: "none",
    borderRadius: 4,
    background: "#4a9eff",
    color: "#fff",
    cursor: "pointer",
    fontSize: 13,
    fontWeight: 500,
  },
  btnSecondary: {
    padding: "8px 16px",
    border: "1px solid #555",
    borderRadius: 4,
    background: "#333",
    color: "#ccc",
    cursor: "pointer",
    fontSize: 13,
  },
  btnDanger: {
    padding: "6px 12px",
    border: "none",
    borderRadius: 4,
    background: "#c53030",
    color: "#fff",
    cursor: "pointer",
    fontSize: 12,
  },
  pre: {
    background: "#0f0f0f",
    border: "1px solid #222",
    borderRadius: 4,
    padding: 10,
    color: "#8cc",
    fontSize: 11,
    fontFamily: "ui-monospace, monospace",
    overflow: "auto",
    maxHeight: 300,
  },
  stepItem: (status: string): CSSProperties => ({
    display: "flex",
    alignItems: "center",
    gap: 8,
    padding: "6px 10px",
    borderLeft: `3px solid ${
      status === "success"
        ? "#48bb78"
        : status === "failed"
          ? "#fc8181"
          : status === "skipped"
            ? "#a0aec0"
            : "#4a9eff"
    }`,
    background: "#1f1f1f",
    marginBottom: 4,
    fontSize: 12,
  }),
  empty: {
    color: "#666",
    fontStyle: "italic",
    textAlign: "center" as const,
    padding: 24,
  },
};

// ── Target metadata ───────────────────────────────────────────────────────

const TARGET_META: Record<BuildTarget, { label: string; desc: string }> = {
  web: { label: "Web", desc: "Static Vite build (dist/)" },
  tauri: { label: "Tauri Desktop", desc: ".dmg / .msi / .AppImage" },
  "capacitor-ios": { label: "Capacitor iOS", desc: ".ipa" },
  "capacitor-android": { label: "Capacitor Android", desc: ".apk / .aab" },
  "relay-node": { label: "Relay (Node)", desc: "PM2/systemd bundle" },
  "relay-docker": { label: "Relay (Docker)", desc: "OCI image" },
};

// ── Component ─────────────────────────────────────────────────────────────

export function AppBuilderPanel() {
  const kernel = useKernel();
  const { profiles, activeProfile, runs, manager } = useBuilder();

  const [selectedProfileId, setSelectedProfileId] = useState<string>(
    profiles[0]?.id ?? "studio",
  );
  const [selectedTargets, setSelectedTargets] = useState<Set<BuildTarget>>(
    () => new Set<BuildTarget>(["web"]),
  );
  const [planPreview, setPlanPreview] = useState<BuildPlan | null>(null);
  const [lastRun, setLastRun] = useState<BuildRun | null>(null);
  const [isBuilding, setIsBuilding] = useState(false);

  const selectedProfile = useMemo(
    () => manager.getProfile(selectedProfileId),
    [manager, selectedProfileId],
  );

  const toggleTarget = useCallback((target: BuildTarget) => {
    setSelectedTargets((prev) => {
      const next = new Set(prev);
      if (next.has(target)) next.delete(target);
      else next.add(target);
      return next;
    });
  }, []);

  const handlePreview = useCallback(() => {
    if (!selectedProfile) return;
    const firstTarget = [...selectedTargets][0] ?? "web";
    const plan = manager.planBuild(selectedProfile.id, firstTarget, true);
    setPlanPreview(plan);
  }, [manager, selectedProfile, selectedTargets]);

  const handleBuild = useCallback(async () => {
    if (!selectedProfile || selectedTargets.size === 0) return;
    setIsBuilding(true);
    try {
      const plans = manager.planBuilds(
        selectedProfile.id,
        [...selectedTargets],
        true,
      );
      let finalRun: BuildRun | null = null;
      for (const plan of plans) {
        finalRun = await manager.runPlan(plan);
      }
      setLastRun(finalRun);
      kernel.notifications.add({
        title: `Built ${selectedProfile.name}`,
        kind: "success",
        body: `Dry-run produced ${plans.length} plan${plans.length === 1 ? "" : "s"}`,
      });
    } catch (err) {
      kernel.notifications.add({
        title: "Build failed",
        kind: "error",
        body: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsBuilding(false);
    }
  }, [manager, kernel, selectedProfile, selectedTargets]);

  const handleSetActive = useCallback(() => {
    if (!selectedProfile) return;
    const wasActive = activeProfile?.id === selectedProfile.id;
    manager.setActiveProfile(wasActive ? null : selectedProfile.id);
  }, [manager, selectedProfile, activeProfile]);

  return (
    <div style={s.root} data-testid="app-builder-panel">
      <div style={s.header}>
        <div>
          <div style={s.title}>App Builder</div>
          <div style={s.subtitle}>
            Compose, build, and deploy focused Prism apps and Relays from Studio.
          </div>
        </div>
        <div style={{ fontSize: 11, color: "#888" }} data-testid="builder-executor-mode">
          {manager.listRuns().length === 0
            ? "mode: dry-run"
            : `mode: dry-run · ${manager.listRuns().length} run(s)`}
        </div>
      </div>

      {/* ── Profile selection ─────────────────────────────────────────── */}
      <div style={s.section}>
        <div style={s.sectionTitle}>1. App Profile</div>
        <div style={s.profileGrid} data-testid="builder-profile-grid">
          {profiles.map((p) => (
            <div
              key={p.id}
              style={selectedProfileId === p.id ? s.cardActive : s.card}
              onClick={() => setSelectedProfileId(p.id)}
              data-testid={`builder-profile-${p.id}`}
            >
              <div style={{ fontWeight: 600, color: "#e5e5e5", marginBottom: 4 }}>
                {p.name}
                {activeProfile?.id === p.id && (
                  <span style={{ ...s.badge, background: "#4a9eff", color: "#fff", marginLeft: 6 }}>
                    active
                  </span>
                )}
              </div>
              <div style={{ fontSize: 11, color: "#888", marginBottom: 6 }}>
                v{p.version}
              </div>
              <div>
                {(p.plugins ?? ["*"]).slice(0, 4).map((id) => (
                  <span key={id} style={s.badge}>
                    {id}
                  </span>
                ))}
                {(p.plugins?.length ?? 0) > 4 && (
                  <span style={s.badge}>+{(p.plugins?.length ?? 0) - 4}</span>
                )}
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* ── Profile details ───────────────────────────────────────────── */}
      {selectedProfile && (
        <div style={s.section}>
          <div style={s.sectionTitle}>2. Profile Details</div>
          <div style={s.card} data-testid="builder-profile-details">
            <div style={s.row}>
              <span style={s.label}>ID:</span>
              <code style={{ color: "#8cc" }}>{selectedProfile.id}</code>
            </div>
            <div style={s.row}>
              <span style={s.label}>Lenses:</span>
              <span style={{ color: "#ccc" }}>
                {selectedProfile.lenses?.length ?? "all (universal host)"}
              </span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Plugins:</span>
              <span style={{ color: "#ccc" }}>
                {selectedProfile.plugins?.length ?? "all (6 built-ins)"}
              </span>
            </div>
            {selectedProfile.relayModules && (
              <div style={s.row}>
                <span style={s.label}>Relay modules:</span>
                <span style={{ color: "#ccc" }}>
                  {selectedProfile.relayModules.length}
                </span>
              </div>
            )}
            <div style={s.row}>
              <span style={s.label}>Glass Flip:</span>
              <span style={{ color: "#ccc" }}>
                {selectedProfile.allowGlassFlip === false ? "disabled" : "enabled"}
              </span>
            </div>
            <button
              style={{ ...s.btnSecondary, marginTop: 8 }}
              onClick={handleSetActive}
              data-testid="builder-toggle-active-profile"
            >
              {activeProfile?.id === selectedProfile.id
                ? "Clear active profile"
                : "Set as active profile"}
            </button>
          </div>
        </div>
      )}

      {/* ── Target selection ──────────────────────────────────────────── */}
      <div style={s.section}>
        <div style={s.sectionTitle}>3. Build Targets</div>
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }} data-testid="builder-target-list">
          {manager.targets.map((target) => (
            <div
              key={target}
              style={s.pill(selectedTargets.has(target))}
              onClick={() => toggleTarget(target)}
              data-testid={`builder-target-${target}`}
            >
              <span>{TARGET_META[target].label}</span>
              <span style={{ color: "#888", fontSize: 10 }}>
                {TARGET_META[target].desc}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* ── Actions ───────────────────────────────────────────────────── */}
      <div style={s.section}>
        <div style={{ display: "flex", gap: 8 }}>
          <button
            style={s.btnSecondary}
            onClick={handlePreview}
            data-testid="builder-preview-plan"
            disabled={!selectedProfile || selectedTargets.size === 0}
          >
            Preview Build Plan
          </button>
          <button
            style={s.btn}
            onClick={handleBuild}
            data-testid="builder-run-build"
            disabled={
              !selectedProfile || selectedTargets.size === 0 || isBuilding
            }
          >
            {isBuilding ? "Building…" : `Dry-run Build (${selectedTargets.size})`}
          </button>
        </div>
      </div>

      {/* ── Plan preview ──────────────────────────────────────────────── */}
      {planPreview && (
        <div style={s.section}>
          <div style={s.sectionTitle}>4. BuildPlan Preview</div>
          <div style={s.card} data-testid="builder-plan-preview">
            <div style={s.row}>
              <span style={s.label}>Profile:</span>
              <span>{planPreview.profileName}</span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Target:</span>
              <span style={s.badge}>{planPreview.target}</span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Steps:</span>
              <span>{planPreview.steps.length}</span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Artifacts:</span>
              <span>{planPreview.artifacts.length}</span>
            </div>
            <div
              style={{ ...s.sectionTitle, marginTop: 12, marginBottom: 4 }}
            >
              Step Details
            </div>
            {planPreview.steps.map((step, i) => (
              <div
                key={i}
                style={s.stepItem("pending")}
                data-testid={`builder-plan-step-${i}`}
              >
                <span style={s.badge}>{step.kind}</span>
                <span>{step.description}</span>
              </div>
            ))}
            <div style={{ ...s.sectionTitle, marginTop: 12, marginBottom: 4 }}>
              Expected Artifacts
            </div>
            {planPreview.artifacts.map((a, i) => (
              <div
                key={i}
                style={{ fontSize: 11, color: "#8cc", marginBottom: 2 }}
                data-testid={`builder-plan-artifact-${i}`}
              >
                <span style={s.badge}>{a.kind}</span>
                {a.path}
              </div>
            ))}
            <details style={{ marginTop: 12 }}>
              <summary style={{ cursor: "pointer", fontSize: 11, color: "#888" }}>
                Raw JSON (for CI / manual execution)
              </summary>
              <pre style={s.pre} data-testid="builder-plan-json">
                {serializeBuildPlan(planPreview)}
              </pre>
            </details>
            <details style={{ marginTop: 8 }}>
              <summary style={{ cursor: "pointer", fontSize: 11, color: "#888" }}>
                Profile JSON (.prism-app.json)
              </summary>
              <pre style={s.pre} data-testid="builder-profile-json">
                {selectedProfile ? serializeAppProfile(selectedProfile) : ""}
              </pre>
            </details>
          </div>
        </div>
      )}

      {/* ── Run output ────────────────────────────────────────────────── */}
      {lastRun && (
        <div style={s.section}>
          <div style={s.sectionTitle}>
            5. Last Build Run —{" "}
            <span
              style={{
                color:
                  lastRun.status === "success"
                    ? "#48bb78"
                    : lastRun.status === "failed"
                      ? "#fc8181"
                      : "#a0aec0",
              }}
            >
              {lastRun.status}
            </span>
          </div>
          <div style={s.card} data-testid="builder-last-run">
            <div style={s.row}>
              <span style={s.label}>Run ID:</span>
              <code style={{ color: "#8cc" }}>{lastRun.id}</code>
            </div>
            <div style={s.row}>
              <span style={s.label}>Target:</span>
              <span>{lastRun.plan.target}</span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Steps:</span>
              <span>{lastRun.steps.length}</span>
            </div>
            <div style={s.row}>
              <span style={s.label}>Artifacts:</span>
              <span>{lastRun.producedArtifacts.length}</span>
            </div>
            <div
              style={{ ...s.sectionTitle, marginTop: 12, marginBottom: 4 }}
            >
              Step Log
            </div>
            {lastRun.steps.map((result: BuildStepResult, i) => (
              <div
                key={i}
                style={s.stepItem(result.status)}
                data-testid={`builder-run-step-${i}`}
              >
                <span style={s.badge}>{result.status}</span>
                <span>{result.step.description}</span>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ── Run history ───────────────────────────────────────────────── */}
      {runs.length > 0 && (
        <div style={s.section}>
          <div style={s.sectionTitle}>Run History</div>
          <div data-testid="builder-run-history">
            {runs.slice(0, 10).map((run) => (
              <div
                key={run.id}
                style={s.card}
                data-testid={`builder-history-${run.id}`}
              >
                <div style={s.row}>
                  <span style={s.badge}>{run.plan.target}</span>
                  <span style={{ fontWeight: 600 }}>{run.plan.profileName}</span>
                  <span
                    style={{
                      ...s.badge,
                      background:
                        run.status === "success"
                          ? "#22543d"
                          : run.status === "failed"
                            ? "#742a2a"
                            : "#2d3748",
                      color:
                        run.status === "success"
                          ? "#9ae6b4"
                          : run.status === "failed"
                            ? "#feb2b2"
                            : "#cbd5e0",
                      marginLeft: "auto",
                    }}
                  >
                    {run.status}
                  </span>
                </div>
              </div>
            ))}
            <button
              style={{ ...s.btnSecondary, marginTop: 8 }}
              onClick={() => {
                manager.clearRuns();
                setLastRun(null);
              }}
              data-testid="builder-clear-runs"
            >
              Clear Run History
            </button>
          </div>
        </div>
      )}

      {profiles.length === 0 && (
        <div style={s.empty}>No profiles registered.</div>
      )}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const APP_BUILDER_LENS_ID = lensId("app-builder");

export const appBuilderLensManifest: LensManifest = {

  id: APP_BUILDER_LENS_ID,
  name: "App Builder",
  icon: "\u{1F3ED}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-app-builder", name: "Switch to App Builder", shortcut: ["shift+b"], section: "Navigation" }],
  },
};

export const appBuilderLensBundle: LensBundle = defineLensBundle(
  appBuilderLensManifest,
  AppBuilderPanel,
);
