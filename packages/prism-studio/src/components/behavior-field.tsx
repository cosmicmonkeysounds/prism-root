/**
 * `behaviorField` — Puck custom field for attaching Luau behaviors
 * to the currently-selected component.
 *
 * Behaviors are first-class GraphObjects of type "behavior" with
 * `{ targetObjectId, trigger, source, enabled }` in their data payload.
 * This field lists every behavior bound to the current component,
 * exposes add / edit / remove / toggle controls, and drives
 * `kernel.behaviors.fire(...)` via the button-renderer boundary so
 * edit-mode preview and published runtime share one code path.
 *
 * The field value is the `targetObjectId` the field owns (Puck hands
 * us the component id via its own render context). We store it on
 * the component so behaviors can be moved between components without
 * losing their association when Puck regenerates auto-ids.
 */

import { useCallback, useMemo, useState, useSyncExternalStore, type ReactElement } from "react";
import { FieldLabel, type Field } from "@measured/puck";
import type { ObjectId } from "@prism/core/object-model";
import type { StudioKernel } from "../kernel/studio-kernel.js";
import type { BehaviorTrigger } from "../kernel/behavior-dispatcher.js";

// ── Triggers ────────────────────────────────────────────────────────────────

const TRIGGERS: Array<{ value: BehaviorTrigger; label: string }> = [
  { value: "onClick", label: "On Click" },
  { value: "onMount", label: "On Mount" },
  { value: "onChange", label: "On Change" },
  { value: "onRouteEnter", label: "On Route Enter" },
  { value: "onRouteLeave", label: "On Route Leave" },
];

// ── Styles ──────────────────────────────────────────────────────────────────

const baseInput = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 12,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#ffffff",
  color: "#0f172a",
  boxSizing: "border-box" as const,
};

const btn = {
  padding: "4px 10px",
  fontSize: 11,
  border: "1px solid #cbd5e1",
  borderRadius: 4,
  background: "#f8fafc",
  color: "#334155",
  cursor: "pointer",
};

const btnPrimary = {
  ...btn,
  background: "#6366f1",
  borderColor: "#6366f1",
  color: "#ffffff",
};

const btnDanger = {
  ...btn,
  color: "#dc2626",
};

const row = {
  display: "flex",
  gap: 6,
  alignItems: "flex-start",
  padding: 8,
  borderRadius: 4,
  border: "1px solid #e2e8f0",
  background: "#f8fafc",
  marginBottom: 6,
};

// ── Inner component ─────────────────────────────────────────────────────────

export interface BehaviorFieldInnerProps {
  kernel: StudioKernel;
  targetObjectId: ObjectId;
  readOnly: boolean;
  label: string | undefined;
}

export function BehaviorFieldInner(props: BehaviorFieldInnerProps): ReactElement {
  const { kernel, targetObjectId, readOnly, label } = props;
  const [tick, setTick] = useState(0);

  const behaviors = useMemo(
    () => kernel.behaviors.list(targetObjectId),
    // Re-read whenever `tick` or `targetObjectId` changes
    [kernel, targetObjectId, tick],
  );

  const addBehavior = useCallback(() => {
    const created = kernel.createObject({
      type: "behavior",
      name: "New Behavior",
      parentId: null,
      position: 0,
      data: {
        targetObjectId,
        trigger: "onClick",
        source: "ui.notify(\"Clicked!\")",
        enabled: true,
      },
    });
    setTick((t) => t + 1);
    void created;
  }, [kernel, targetObjectId]);

  const updateBehavior = useCallback(
    (id: ObjectId, patch: Record<string, unknown>) => {
      const existing = kernel.store.getObject(id);
      if (!existing) return;
      const nextData = { ...(existing.data as Record<string, unknown>), ...patch };
      kernel.updateObject(id, { data: nextData });
      setTick((t) => t + 1);
    },
    [kernel],
  );

  const removeBehavior = useCallback(
    (id: ObjectId) => {
      kernel.deleteObject(id);
      setTick((t) => t + 1);
    },
    [kernel],
  );

  const runBehavior = useCallback(
    async (id: ObjectId, trigger: BehaviorTrigger) => {
      await kernel.behaviors.fire(targetObjectId, trigger);
      void id;
    },
    [kernel, targetObjectId],
  );

  return (
    <FieldLabel label={label ?? "Behaviors"} el="div" readOnly={readOnly}>
      <div style={{ display: "flex", flexDirection: "column", gap: 4 }}>
        {behaviors.length === 0 ? (
          <div style={{ fontSize: 11, color: "#94a3b8", fontStyle: "italic" }}>
            No behaviors attached.
          </div>
        ) : null}
        {behaviors.map((b) => (
          <div key={b.id} style={row} data-testid="behavior-row">
            <div style={{ flex: 1, display: "flex", flexDirection: "column", gap: 4 }}>
              <div style={{ display: "flex", gap: 6 }}>
                <select
                  value={b.trigger}
                  disabled={readOnly}
                  onChange={(e) =>
                    updateBehavior(b.id, { trigger: e.target.value as BehaviorTrigger })
                  }
                  style={{ ...baseInput, flex: 1 }}
                  aria-label="Trigger"
                >
                  {TRIGGERS.map((t) => (
                    <option key={t.value} value={t.value}>
                      {t.label}
                    </option>
                  ))}
                </select>
                <label
                  style={{
                    display: "flex",
                    alignItems: "center",
                    gap: 4,
                    fontSize: 11,
                    color: "#334155",
                  }}
                >
                  <input
                    type="checkbox"
                    checked={b.enabled}
                    disabled={readOnly}
                    onChange={(e) => updateBehavior(b.id, { enabled: e.target.checked })}
                  />
                  Enabled
                </label>
              </div>
              <textarea
                value={b.source}
                disabled={readOnly}
                rows={3}
                onChange={(e) => updateBehavior(b.id, { source: e.target.value })}
                style={{ ...baseInput, fontFamily: "monospace", fontSize: 11, resize: "vertical" }}
                aria-label="Luau source"
                placeholder="ui.notify(&quot;Clicked!&quot;)"
              />
              <div style={{ display: "flex", gap: 6, justifyContent: "flex-end" }}>
                <button
                  type="button"
                  style={btn}
                  disabled={readOnly}
                  onClick={() => void runBehavior(b.id, b.trigger)}
                >
                  Run
                </button>
                <button
                  type="button"
                  style={btnDanger}
                  disabled={readOnly}
                  onClick={() => removeBehavior(b.id)}
                >
                  Remove
                </button>
              </div>
            </div>
          </div>
        ))}
        <div style={{ display: "flex", justifyContent: "flex-end" }}>
          <button
            type="button"
            style={btnPrimary}
            disabled={readOnly}
            onClick={addBehavior}
            data-testid="behavior-add"
          >
            + Add Behavior
          </button>
        </div>
      </div>
    </FieldLabel>
  );
}

// ── Field factory ───────────────────────────────────────────────────────────

/**
 * Creates a Puck custom field that lists + edits behaviors attached to
 * the currently-selected component. The target is resolved from
 * `kernel.atoms.selectedId` — Puck's inspector only renders this field
 * for the selected item, and the layout panel mirrors that selection
 * into the kernel, so the kernel selection always points at the Puck
 * item whose inspector is open. The stored field value is ignored.
 */
export function behaviorField(
  kernel: StudioKernel,
  opts: { label?: string } = {},
): Field<string> {
  return {
    type: "custom",
    ...(opts.label !== undefined ? { label: opts.label } : {}),
    render: ({ readOnly }): ReactElement => {
      return (
        <BehaviorFieldForSelection
          kernel={kernel}
          readOnly={readOnly ?? false}
          label={opts.label}
        />
      );
    },
  };
}

interface BehaviorFieldForSelectionProps {
  kernel: StudioKernel;
  readOnly: boolean;
  label: string | undefined;
}

function BehaviorFieldForSelection({
  kernel,
  readOnly,
  label,
}: BehaviorFieldForSelectionProps): ReactElement {
  const selectedId = useSyncExternalStore(
    (cb) => kernel.atoms.subscribe(cb),
    () => kernel.atoms.getState().selectedId,
  );
  if (!selectedId) {
    return (
      <FieldLabel label={label ?? "Behaviors"} el="div" readOnly={readOnly}>
        <div style={{ fontSize: 11, color: "#94a3b8", fontStyle: "italic" }}>
          Select a component to attach behaviors.
        </div>
      </FieldLabel>
    );
  }
  return (
    <BehaviorFieldInner
      kernel={kernel}
      targetObjectId={selectedId as ObjectId}
      readOnly={readOnly}
      label={label}
    />
  );
}
