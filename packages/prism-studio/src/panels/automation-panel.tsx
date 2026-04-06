/**
 * Automation Panel — manage trigger/condition/action rules.
 *
 * Lists all automations with enable/disable toggles, run buttons,
 * create/delete controls, and a run history log.
 */

import { useState, useCallback } from "react";
import { useAutomation, useKernel } from "../kernel/index.js";
import type { Automation, AutomationTrigger, AutomationAction } from "@prism/core/automation";

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
  cardHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: "0.375rem",
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
  btnDanger: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#3b1111",
    border: "1px solid #5c2020",
    borderRadius: 3,
    color: "#f87171",
    cursor: "pointer",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "100%",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    outline: "none",
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
} as const;

// ── Trigger label ───────────────────────────────────────────────────────────

function triggerLabel(trigger: AutomationTrigger): string {
  if (trigger.type === "cron") return `Cron: ${trigger.cron}`;
  if (trigger.type === "manual") return "Manual";
  return trigger.type;
}

// ── Status badge ────────────────────────────────────────────────────────────

function StatusBadge({ status }: { status: string }) {
  const colors: Record<string, string> = {
    success: "#22c55e",
    failed: "#ef4444",
    skipped: "#888",
    partial: "#f59e0b",
  };
  return (
    <span
      data-testid={`run-status-${status}`}
      style={{ ...styles.badge, color: colors[status] ?? "#888", borderColor: colors[status] ?? "#444" }}
    >
      {status}
    </span>
  );
}

// ── Create Automation Form ──────────────────────────────────────────────────

function CreateAutomationForm({ onSave }: { onSave: (a: Automation) => void }) {
  const [name, setName] = useState("");
  const [triggerType, setTriggerType] = useState<AutomationTrigger["type"]>("manual");
  const [actionType, setActionType] = useState<AutomationAction["type"]>("notification:send");

  const handleCreate = useCallback(() => {
    if (!name.trim()) return;
    const now = new Date().toISOString();
    const id = `auto_${Date.now().toString(36)}`;

    const trigger: AutomationTrigger =
      triggerType === "cron"
        ? { type: "cron", cron: "*/5 * * * *" }
        : triggerType === "manual"
          ? { type: "manual" }
          : { type: triggerType as "object:created" };

    const action: AutomationAction =
      actionType === "notification:send"
        ? { type: "notification:send", target: "trigger-owner", title: name, body: "Automation triggered" }
        : actionType === "object:create"
          ? { type: "object:create", objectType: "page", template: { name: `Auto: ${name}` } }
          : { type: "notification:send", target: "trigger-owner", title: name, body: "Action" };

    onSave({
      id,
      name: name.trim(),
      enabled: true,
      trigger,
      conditions: [],
      actions: [action],
      createdAt: now,
      updatedAt: now,
      runCount: 0,
    });

    setName("");
  }, [name, triggerType, actionType, onSave]);

  return (
    <div style={styles.card} data-testid="create-automation-form">
      <div style={styles.sectionTitle}>New Automation</div>
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Automation name..."
          value={name}
          onChange={(e) => setName(e.target.value)}
          data-testid="automation-name-input"
        />
      </div>
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <select
          style={styles.select}
          value={triggerType}
          onChange={(e) => setTriggerType(e.target.value as AutomationTrigger["type"])}
          data-testid="automation-trigger-select"
        >
          <option value="manual">Manual</option>
          <option value="object:created">On Create</option>
          <option value="object:updated">On Update</option>
          <option value="object:deleted">On Delete</option>
          <option value="cron">Cron</option>
        </select>
        <select
          style={styles.select}
          value={actionType}
          onChange={(e) => setActionType(e.target.value as AutomationAction["type"])}
          data-testid="automation-action-select"
        >
          <option value="notification:send">Send Notification</option>
          <option value="object:create">Create Object</option>
          <option value="object:update">Update Object</option>
          <option value="object:delete">Delete Object</option>
        </select>
        <button
          style={styles.btnPrimary}
          onClick={handleCreate}
          data-testid="create-automation-btn"
        >
          Create
        </button>
      </div>
    </div>
  );
}

// ── Automation Card ─────────────────────────────────────────────────────────

function AutomationCard({
  automation,
  onToggle,
  onRun,
  onDelete,
}: {
  automation: Automation;
  onToggle: () => void;
  onRun: () => void;
  onDelete: () => void;
}) {
  return (
    <div style={styles.card} data-testid={`automation-${automation.id}`}>
      <div style={styles.cardHeader as React.CSSProperties}>
        <div>
          <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
            {automation.name}
          </span>
          <span style={styles.badge}>{triggerLabel(automation.trigger)}</span>
          <span style={{ ...styles.badge, color: automation.enabled ? "#22c55e" : "#888" }}>
            {automation.enabled ? "enabled" : "disabled"}
          </span>
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            style={styles.btn}
            onClick={onToggle}
            data-testid={`toggle-automation-${automation.id}`}
          >
            {automation.enabled ? "Disable" : "Enable"}
          </button>
          <button
            style={styles.btnPrimary}
            onClick={onRun}
            data-testid={`run-automation-${automation.id}`}
          >
            Run
          </button>
          <button
            style={styles.btnDanger}
            onClick={onDelete}
            data-testid={`delete-automation-${automation.id}`}
          >
            Delete
          </button>
        </div>
      </div>
      <div style={styles.meta}>
        {automation.actions.length} action(s)
        {automation.conditions.length > 0 && ` | ${automation.conditions.length} condition(s)`}
        {automation.runCount > 0 && ` | ${automation.runCount} run(s)`}
        {automation.lastRunAt && ` | Last: ${new Date(automation.lastRunAt).toLocaleString()}`}
      </div>
    </div>
  );
}

// ── Main Panel ──────────────────────────────────────────────────────────────

export function AutomationPanel() {
  const kernel = useKernel();
  const { automations, runs, save, remove, run } = useAutomation();
  const [tab, setTab] = useState<"rules" | "history">("rules");

  const handleToggle = useCallback(
    (automation: Automation) => {
      save({ ...automation, enabled: !automation.enabled, updatedAt: new Date().toISOString() });
    },
    [save],
  );

  const handleRun = useCallback(
    (id: string) => {
      void run(id).then((result) => {
        kernel.notifications.add({
          title: `Automation ${result.status}: ${automations.find((a) => a.id === id)?.name ?? id}`,
          kind: result.status === "success" ? "success" : result.status === "skipped" ? "info" : "error",
        });
      });
    },
    [run, kernel, automations],
  );

  return (
    <div style={styles.container} data-testid="automation-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Automation</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {automations.length} rule(s)
        </span>
      </div>

      <div style={styles.tabRow as React.CSSProperties}>
        <button
          style={{ ...styles.btn, background: tab === "rules" ? "#094771" : "#333" }}
          onClick={() => setTab("rules")}
          data-testid="automation-tab-rules"
        >
          Rules
        </button>
        <button
          style={{ ...styles.btn, background: tab === "history" ? "#094771" : "#333" }}
          onClick={() => setTab("history")}
          data-testid="automation-tab-history"
        >
          History ({runs.length})
        </button>
      </div>

      {tab === "rules" && (
        <>
          <CreateAutomationForm onSave={save} />
          {automations.length === 0 && (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No automations yet. Create one above.
            </div>
          )}
          {automations.map((a) => (
            <AutomationCard
              key={a.id}
              automation={a}
              onToggle={() => handleToggle(a)}
              onRun={() => handleRun(a.id)}
              onDelete={() => remove(a.id)}
            />
          ))}
        </>
      )}

      {tab === "history" && (
        <div data-testid="automation-history">
          {runs.length === 0 && (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No runs yet.
            </div>
          )}
          {[...runs].reverse().map((r) => (
            <div key={r.id} style={styles.card}>
              <div style={styles.cardHeader as React.CSSProperties}>
                <span style={{ color: "#ccc", fontSize: "0.8125rem" }}>
                  {automations.find((a) => a.id === r.automationId)?.name ?? r.automationId}
                </span>
                <StatusBadge status={r.status} />
              </div>
              <div style={styles.meta}>
                Triggered: {new Date(r.triggeredAt).toLocaleString()}
                {r.completedAt && ` | Completed: ${new Date(r.completedAt).toLocaleString()}`}
              </div>
              {r.actionResults.length > 0 && (
                <div style={{ marginTop: 4, fontSize: "0.6875rem", color: "#555" }}>
                  {r.actionResults.map((ar, i) => (
                    <div key={i}>
                      [{ar.actionType}] {ar.status}
                      {ar.elapsedMs !== undefined && ` (${ar.elapsedMs}ms)`}
                      {ar.error && <span style={{ color: "#f87171" }}> — {ar.error}</span>}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
