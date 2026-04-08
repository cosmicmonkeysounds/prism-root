/**
 * Form Builder panel (Tier 8B).
 *
 * Operates on the currently-selected container (section, page, card) and
 * lets the author rapidly assemble a form by clicking "Add" for each input
 * type. Each click creates a real form-input block (`text-input`,
 * `textarea-input`, `select-input`, `checkbox-input`, `number-input`,
 * `date-input`) underneath the container so existing renderers and exporters
 * work without change.
 *
 * The panel is intentionally thin — the heavy lifting lives in the
 * individual input entities and `form-input-renderers.tsx`.
 */

import { useCallback, useMemo } from "react";
import type { GraphObject } from "@prism/core/object-model";
import { useKernel, useSelection, useObject } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
/** Input types the form builder can insert. Each maps 1:1 to an entity. */
export const FORM_INPUT_TYPES: Array<{
  type: string;
  label: string;
  icon: string;
  defaults: Record<string, unknown>;
}> = [
  { type: "text-input", label: "Text", icon: "\u270E", defaults: { label: "Name" } },
  { type: "textarea-input", label: "Textarea", icon: "\u00B6", defaults: { label: "Message", rows: 4 } },
  { type: "select-input", label: "Select", icon: "\u25BE", defaults: { label: "Choose", options: "one,two,three" } },
  { type: "checkbox-input", label: "Checkbox", icon: "\u2611", defaults: { label: "Accept" } },
  { type: "number-input", label: "Number", icon: "#", defaults: { label: "Amount" } },
  { type: "date-input", label: "Date", icon: "\uD83D\uDCC6", defaults: { label: "Date" } },
];

/** Container entity types the builder is allowed to append into. */
const CONTAINER_TYPES = new Set(["section", "page", "card", "columns", "spatial-canvas"]);

export function FormBuilderPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selected = useObject(selectedId);

  const container = useMemo<GraphObject | null>(() => {
    if (!selected) return null;
    // Walk up until we hit a container type.
    let cursor: GraphObject | null = selected;
    while (cursor && !CONTAINER_TYPES.has(cursor.type)) {
      cursor = cursor.parentId ? kernel.store.getObject(cursor.parentId) ?? null : null;
    }
    return cursor;
  }, [selected, kernel]);

  const children = useMemo<GraphObject[]>(() => {
    if (!container) return [];
    return kernel.store
      .listObjects({ parentId: container.id })
      .filter((o) => !o.deletedAt)
      .sort((a, b) => a.position - b.position);
  }, [container, kernel]);

  const addInput = useCallback(
    (spec: (typeof FORM_INPUT_TYPES)[number]) => {
      if (!container) return;
      const position = kernel.store.listObjects({ parentId: container.id }).length;
      const created = kernel.createObject({
        type: spec.type,
        name: String(spec.defaults.label ?? spec.label),
        parentId: container.id,
        position,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: { ...spec.defaults },
      });
      kernel.select(created.id);
      kernel.notifications.add({
        title: `Added ${spec.label} to "${container.name}"`,
        kind: "success",
      });
    },
    [container, kernel],
  );

  const addSubmitButton = useCallback(() => {
    if (!container) return;
    const position = kernel.store.listObjects({ parentId: container.id }).length;
    kernel.createObject({
      type: "button",
      name: "Submit",
      parentId: container.id,
      position,
      status: null,
      tags: [],
      date: null,
      endDate: null,
      description: "",
      color: null,
      image: null,
      pinned: false,
      data: { label: "Submit", variant: "primary", url: "#" },
    });
    kernel.notifications.add({
      title: "Added submit button",
      kind: "success",
    });
  }, [container, kernel]);

  const move = useCallback(
    (child: GraphObject, direction: -1 | 1) => {
      if (!container) return;
      const sorted = kernel.store
        .listObjects({ parentId: container.id })
        .filter((o) => !o.deletedAt)
        .sort((a, b) => a.position - b.position);
      const idx = sorted.findIndex((o) => o.id === child.id);
      const target = idx + direction;
      if (target < 0 || target >= sorted.length) return;
      const other = sorted[target] as GraphObject;
      kernel.updateObject(child.id, { position: other.position });
      kernel.updateObject(other.id, { position: child.position });
    },
    [container, kernel],
  );

  return (
    <div
      data-testid="form-builder-panel"
      style={{
        height: "100%",
        overflow: "auto",
        padding: 16,
        background: "#1e1e1e",
        color: "#ccc",
        fontSize: 12,
      }}
    >
      <h2 style={{ fontSize: 14, margin: 0, marginBottom: 12, color: "#e5e5e5" }}>
        Form Builder
      </h2>

      {!container && (
        <div style={{ color: "#888", fontSize: 12 }}>
          Select a page, section, or card to start building a form.
        </div>
      )}

      {container && (
        <>
          <div style={{ color: "#888", fontSize: 11, marginBottom: 8 }}>
            Building inside <strong style={{ color: "#e5e5e5" }}>{container.name}</strong>{" "}
            <span style={{ color: "#555" }}>({container.type})</span>
          </div>

          <div style={{ marginBottom: 12 }}>
            <div
              style={{
                fontSize: 10,
                textTransform: "uppercase",
                color: "#666",
                marginBottom: 6,
              }}
            >
              Add Field
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
              {FORM_INPUT_TYPES.map((spec) => (
                <button
                  key={spec.type}
                  data-testid={`add-${spec.type}`}
                  onClick={() => addInput(spec)}
                  style={{
                    padding: "4px 8px",
                    fontSize: 11,
                    background: "#333",
                    border: "1px solid #444",
                    borderRadius: 3,
                    color: "#ccc",
                    cursor: "pointer",
                    display: "flex",
                    gap: 4,
                    alignItems: "center",
                  }}
                >
                  <span>{spec.icon}</span>
                  {spec.label}
                </button>
              ))}
              <button
                data-testid="add-submit-button"
                onClick={addSubmitButton}
                style={{
                  padding: "4px 8px",
                  fontSize: 11,
                  background: "#0e639c",
                  border: "1px solid #1177bb",
                  borderRadius: 3,
                  color: "#fff",
                  cursor: "pointer",
                }}
              >
                + Submit Button
              </button>
            </div>
          </div>

          <div style={{ marginBottom: 12 }}>
            <div
              style={{
                fontSize: 10,
                textTransform: "uppercase",
                color: "#666",
                marginBottom: 6,
              }}
            >
              Fields ({children.length})
            </div>
            {children.length === 0 ? (
              <div style={{ color: "#555", fontSize: 11, fontStyle: "italic" }}>
                No children yet — click a button above to add the first field.
              </div>
            ) : (
              <ul style={{ margin: 0, padding: 0, listStyle: "none" }}>
                {children.map((child) => (
                  <li
                    key={child.id}
                    data-testid={`form-child-${child.id}`}
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      padding: "4px 6px",
                      marginBottom: 2,
                      background: child.id === selectedId ? "#2a2a2a" : "#252526",
                      border: "1px solid #333",
                      borderRadius: 3,
                    }}
                  >
                    <span
                      onClick={() => kernel.select(child.id)}
                      style={{ flex: 1, cursor: "pointer" }}
                    >
                      <strong>{child.name}</strong>{" "}
                      <span style={{ color: "#666", fontSize: 10 }}>{child.type}</span>
                    </span>
                    <button
                      data-testid={`move-up-${child.id}`}
                      onClick={() => move(child, -1)}
                      style={moveBtnStyle}
                    >
                      ↑
                    </button>
                    <button
                      data-testid={`move-down-${child.id}`}
                      onClick={() => move(child, 1)}
                      style={moveBtnStyle}
                    >
                      ↓
                    </button>
                    <button
                      data-testid={`delete-${child.id}`}
                      onClick={() => kernel.deleteObject(child.id)}
                      style={{ ...moveBtnStyle, color: "#f87171" }}
                    >
                      ✕
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </>
      )}
    </div>
  );
}

const moveBtnStyle: React.CSSProperties = {
  padding: "2px 6px",
  fontSize: 10,
  background: "#333",
  border: "1px solid #444",
  borderRadius: 2,
  color: "#ccc",
  cursor: "pointer",
};


// ── Lens registration ──────────────────────────────────────────────────────

export const FORM_BUILDER_LENS_ID = lensId("form-builder");

export const formBuilderLensManifest: LensManifest = {

  id: FORM_BUILDER_LENS_ID,
  name: "Form Builder",
  icon: "\u{1F4DD}",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [
      { id: "switch-form-builder", name: "Switch to Form Builder", shortcut: ["shift+g"], section: "Navigation" },
    ],
  },
};

export const formBuilderLensBundle: LensBundle = defineLensBundle(
  formBuilderLensManifest,
  FormBuilderPanel,
);
