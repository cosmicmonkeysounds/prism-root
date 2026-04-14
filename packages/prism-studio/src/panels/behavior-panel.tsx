/**
 * Behavior Panel — author Luau scripts that fire on component events.
 *
 * Lists every `behavior` object whose `data.targetObjectId` matches the
 * current selection, plus an "Add behavior" dropdown. Each row has:
 *   • trigger picker (enum field from the entity def)
 *   • enable/disable toggle
 *   • textarea Luau editor (kept lightweight — richer CodeMirror binding
 *     is a follow-up)
 *   • delete button
 *
 * All pure CRUD lives in `./behavior-data.ts`.
 *
 * Authoring only — executing the scripts at preview time is explicitly out
 * of scope per `docs/dev/current-plan.md`.
 */

import { useCallback, useEffect, useMemo, useState, useSyncExternalStore } from "react";
import { useKernel, useSelection } from "../kernel/index.js";
import { validateLuau } from "@prism/core/luau";
import type { ObjectId } from "@prism/core/object-model";
import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
import {
  listBehaviorsFor,
  mergeBehaviorEdit,
  newBehaviorDraft,
  summariseBehavior,
  type BehaviorRow,
  type BehaviorTrigger,
} from "./behavior-data.js";

const TRIGGERS: ReadonlyArray<{ value: BehaviorTrigger; label: string }> = [
  { value: "onClick", label: "On Click" },
  { value: "onMount", label: "On Mount" },
  { value: "onChange", label: "On Change" },
  { value: "onRouteEnter", label: "On Route Enter" },
  { value: "onRouteLeave", label: "On Route Leave" },
];

export function BehaviorPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const storeVersion = useSyncExternalStore(
    (cb) => kernel.store.onChange(() => cb()),
    () => kernel.store.allObjects().length,
  );

  const rows = useMemo<BehaviorRow[]>(() => {
    void storeVersion;
    if (!selectedId) return [];
    return listBehaviorsFor(
      selectedId as unknown as string,
      kernel.store.allObjects(),
    );
  }, [kernel, selectedId, storeVersion]);

  const [validationErrors, setValidationErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    let cancelled = false;
    (async () => {
      const next: Record<string, string> = {};
      for (const row of rows) {
        if (!row.source.trim()) continue;
        try {
          const diag = await validateLuau(row.source);
          if (diag.length > 0) {
            next[row.id] = diag[0]?.message ?? "Invalid Luau";
          }
        } catch (err) {
          next[row.id] = (err as Error).message;
        }
      }
      if (!cancelled) setValidationErrors(next);
    })();
    return () => {
      cancelled = true;
    };
  }, [rows]);

  const add = useCallback(
    (trigger: BehaviorTrigger) => {
      if (!selectedId) return;
      // Parent behavior objects under the nearest enclosing `app`, or the
      // target itself when there's no app ancestor. Walking the parent chain
      // finds the right owner without assuming project structure.
      const all = kernel.store.allObjects();
      const byId = new Map<string, (typeof all)[number]>();
      for (const o of all) byId.set(o.id as unknown as string, o);
      let cursor = byId.get(selectedId as unknown as string);
      let appParent: string | null = null;
      while (cursor) {
        if (cursor.type === "app") {
          appParent = cursor.id as unknown as string;
          break;
        }
        if (!cursor.parentId) break;
        cursor = byId.get(cursor.parentId as unknown as string);
      }
      const parentId = appParent ?? (selectedId as unknown as string);
      const draft = newBehaviorDraft(
        selectedId as unknown as string,
        parentId,
        trigger,
      );
      kernel.createObject({
        type: draft.type,
        name: draft.name,
        parentId: draft.parentId as unknown as ObjectId | null,
        position: draft.position,
        data: draft.data as unknown as Record<string, unknown>,
      });
    },
    [kernel, selectedId],
  );

  const patch = useCallback(
    (row: BehaviorRow, edit: Parameters<typeof mergeBehaviorEdit>[1]) => {
      const existing = kernel.store.getObject(row.id as unknown as ObjectId);
      if (!existing) return;
      kernel.updateObject(existing.id, mergeBehaviorEdit(existing, edit));
    },
    [kernel],
  );

  const remove = useCallback(
    (row: BehaviorRow) => {
      kernel.deleteObject(row.id as unknown as ObjectId);
    },
    [kernel],
  );

  if (!selectedId) {
    return (
      <div
        data-testid="behavior-panel"
        style={panelStyle}
      >
        <em style={{ color: "#888" }}>
          Select a component or route to attach behaviors.
        </em>
      </div>
    );
  }

  return (
    <div data-testid="behavior-panel" style={panelStyle}>
      <h2 style={headingStyle}>Behaviors</h2>
      <div style={{ color: "#888", fontSize: 11, marginBottom: 12 }}>
        Luau scripts that fire when the selected block receives an event.
        Authoring only — execution is not yet wired into preview.
      </div>
      <div style={{ display: "flex", gap: 6, marginBottom: 12, flexWrap: "wrap" }}>
        {TRIGGERS.map((t) => (
          <button
            key={t.value}
            data-testid={`add-behavior-${t.value}`}
            onClick={() => add(t.value)}
            style={buttonStyle}
          >
            + {t.label}
          </button>
        ))}
      </div>

      {rows.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic" }}>No behaviors attached.</div>
      )}

      <ul style={{ listStyle: "none", padding: 0, margin: 0 }}>
        {rows.map((row) => (
          <li
            key={row.id}
            data-testid={`behavior-row-${row.id}`}
            style={rowStyle(row.enabled)}
          >
            <div style={{ display: "flex", gap: 6, alignItems: "center", marginBottom: 6 }}>
              <select
                value={row.trigger}
                onChange={(e) =>
                  patch(row, { trigger: e.target.value as BehaviorTrigger })
                }
                style={selectStyle}
                data-testid={`behavior-trigger-${row.id}`}
              >
                {TRIGGERS.map((t) => (
                  <option key={t.value} value={t.value}>
                    {t.label}
                  </option>
                ))}
              </select>
              <label style={{ fontSize: 11, color: "#ccc", flex: 1 }}>
                <input
                  type="checkbox"
                  checked={row.enabled}
                  onChange={(e) => patch(row, { enabled: e.target.checked })}
                  data-testid={`behavior-enabled-${row.id}`}
                />{" "}
                enabled
              </label>
              <button
                onClick={() => remove(row)}
                style={{ ...buttonStyle, color: "#f87171" }}
                data-testid={`behavior-delete-${row.id}`}
              >
                delete
              </button>
            </div>
            <div style={{ color: "#888", fontSize: 11, marginBottom: 4 }}>
              {summariseBehavior(row)}
            </div>
            <textarea
              value={row.source}
              onChange={(e) => patch(row, { source: e.target.value })}
              placeholder='ui.navigate("/about")'
              rows={4}
              data-testid={`behavior-source-${row.id}`}
              style={textareaStyle}
            />
            {validationErrors[row.id] ? (
              <div
                data-testid={`behavior-error-${row.id}`}
                style={{ color: "#f87171", fontSize: 11, marginTop: 4 }}
              >
                {validationErrors[row.id]}
              </div>
            ) : null}
          </li>
        ))}
      </ul>
    </div>
  );
}

const panelStyle: React.CSSProperties = {
  height: "100%",
  overflow: "auto",
  padding: 16,
  background: "#1e1e1e",
  color: "#ccc",
  fontSize: 12,
};

const headingStyle: React.CSSProperties = {
  fontSize: 14,
  margin: 0,
  marginBottom: 12,
  color: "#e5e5e5",
};

const buttonStyle: React.CSSProperties = {
  padding: "3px 8px",
  fontSize: 11,
  background: "#333",
  border: "1px solid #444",
  borderRadius: 2,
  color: "#ccc",
  cursor: "pointer",
};

const selectStyle: React.CSSProperties = {
  padding: "3px 6px",
  fontSize: 11,
  background: "#252526",
  border: "1px solid #444",
  borderRadius: 2,
  color: "#ccc",
};

const textareaStyle: React.CSSProperties = {
  width: "100%",
  padding: "6px 8px",
  fontSize: 11,
  background: "#111",
  border: "1px solid #333",
  borderRadius: 2,
  color: "#e5e5e5",
  fontFamily: "monospace",
  resize: "vertical",
};

function rowStyle(enabled: boolean): React.CSSProperties {
  return {
    padding: 8,
    marginBottom: 8,
    background: "#252526",
    border: "1px solid #333",
    borderRadius: 3,
    opacity: enabled ? 1 : 0.5,
  };
}

// ── Lens registration ──────────────────────────────────────────────────────

export const BEHAVIOR_LENS_ID = lensId("behavior");

export const behaviorLensManifest: LensManifest = {
  id: BEHAVIOR_LENS_ID,
  name: "Behaviors",
  icon: "\u26A1",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      {
        id: "switch-behaviors",
        name: "Switch to Behaviors",
        shortcut: ["shift+h"],
        section: "Navigation",
      },
    ],
  },
};

export const behaviorLensBundle: LensBundle = defineLensBundle(
  behaviorLensManifest,
  BehaviorPanel,
);
