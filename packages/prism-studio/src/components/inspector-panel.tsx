/**
 * Inspector Panel — schema-driven property editor for the selected object.
 *
 * Reads EntityDef fields from ObjectRegistry and renders appropriate
 * inputs. Saves changes back through the kernel (with undo support).
 */

import { useState, useCallback } from "react";
import type { EntityFieldDef } from "@prism/core/object-model";
import { useKernel, useSelection, useObject, useExpression } from "../kernel/index.js";

// ── Field Renderer ──────────────────────────────────────────────────────────

function FieldInput({
  field,
  value,
  onChange,
}: {
  field: EntityFieldDef;
  value: unknown;
  onChange: (value: unknown) => void;
}) {
  const commonStyle = {
    width: "100%",
    padding: "4px 6px",
    fontSize: 12,
    background: "#1e1e1e",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    outline: "none",
    boxSizing: "border-box" as const,
  };

  switch (field.type) {
    case "bool":
      return (
        <label style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 12, color: "#ccc" }}>
          <input
            type="checkbox"
            checked={!!value}
            onChange={(e) => onChange(e.target.checked)}
          />
          {field.label ?? field.id}
        </label>
      );

    case "enum":
      return (
        <select
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          style={commonStyle}
        >
          <option value="">—</option>
          {field.enumOptions?.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      );

    case "int":
    case "float":
      return (
        <input
          type="number"
          value={(value as number) ?? ""}
          step={field.type === "float" ? 0.1 : 1}
          onChange={(e) => {
            const v = field.type === "int"
              ? parseInt(e.target.value, 10)
              : parseFloat(e.target.value);
            onChange(Number.isNaN(v) ? null : v);
          }}
          style={commonStyle}
        />
      );

    case "color":
      return (
        <div style={{ display: "flex", gap: 6, alignItems: "center" }}>
          <input
            type="color"
            value={(value as string) ?? "#000000"}
            onChange={(e) => onChange(e.target.value)}
            style={{ width: 28, height: 28, padding: 0, border: "1px solid #444", borderRadius: 3, cursor: "pointer" }}
          />
          <input
            type="text"
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
            placeholder="#000000"
            style={{ ...commonStyle, flex: 1 }}
          />
        </div>
      );

    case "text":
      if (field.ui?.multiline) {
        return (
          <textarea
            value={(value as string) ?? ""}
            onChange={(e) => onChange(e.target.value)}
            rows={4}
            style={{ ...commonStyle, resize: "vertical", fontFamily: "inherit" }}
          />
        );
      }
      return (
        <input
          type="text"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.ui?.placeholder}
          style={commonStyle}
        />
      );

    case "url":
      return (
        <input
          type="url"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.ui?.placeholder ?? "https://..."}
          style={commonStyle}
        />
      );

    case "date":
      return (
        <input
          type="date"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          style={commonStyle}
        />
      );

    case "datetime":
      return (
        <input
          type="datetime-local"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          style={commonStyle}
        />
      );

    default:
      // string, object_ref, etc.
      return (
        <input
          type="text"
          value={(value as string) ?? ""}
          onChange={(e) => onChange(e.target.value)}
          placeholder={field.ui?.placeholder}
          style={commonStyle}
        />
      );
  }
}

// ── Inspector Panel ─────────────────────────────────────────────────────────

export function InspectorPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const obj = useObject(selectedId);

  const handleShellFieldChange = useCallback(
    (fieldName: string, value: unknown) => {
      if (!obj) return;
      kernel.updateObject(obj.id, { [fieldName]: value } as Partial<typeof obj>);
    },
    [kernel, obj],
  );

  const handleDataFieldChange = useCallback(
    (fieldId: string, value: unknown) => {
      if (!obj) return;
      kernel.updateObject(obj.id, {
        data: { ...obj.data, [fieldId]: value },
      });
    },
    [kernel, obj],
  );

  const handleDelete = useCallback(() => {
    if (!obj) return;
    kernel.deleteObject(obj.id);
    kernel.select(null);
    kernel.notifications.add({
      title: `Deleted "${obj.name}"`,
      kind: "info",
    });
  }, [kernel, obj]);

  const handleCopy = useCallback(() => {
    if (!obj) return;
    kernel.clipboardCopy([obj.id]);
    kernel.notifications.add({ title: `Copied "${obj.name}"`, kind: "info" });
  }, [kernel, obj]);

  const handleCut = useCallback(() => {
    if (!obj) return;
    kernel.clipboardCut([obj.id]);
    kernel.notifications.add({ title: `Cut "${obj.name}"`, kind: "info" });
  }, [kernel, obj]);

  const handlePaste = useCallback(() => {
    if (!obj || !kernel.clipboardHasContent) return;
    const result = kernel.clipboardPaste(obj.id);
    if (result) {
      kernel.notifications.add({
        title: `Pasted ${result.created.length} object(s)`,
        kind: "success",
      });
    }
  }, [kernel, obj]);

  const handleAddChild = useCallback(
    (childType: string) => {
      if (!obj) return;
      const def = kernel.registry.get(childType);
      const siblings = kernel.store.listObjects({ parentId: obj.id } as Parameters<typeof kernel.store.listObjects>[0]);
      const child = kernel.createObject({
        type: childType,
        name: `New ${def?.label ?? childType}`,
        parentId: obj.id,
        position: siblings.length,
        status: null,
        tags: [],
        date: null,
        endDate: null,
        description: "",
        color: null,
        image: null,
        pinned: false,
        data: {},
      });
      kernel.select(child.id);
      kernel.notifications.add({
        title: `Created ${def?.label ?? childType}`,
        kind: "success",
      });
    },
    [kernel, obj],
  );

  if (!obj) {
    return (
      <div style={{ padding: 16, color: "#555", fontSize: 12, textAlign: "center" }}>
        Select an object to inspect its properties.
      </div>
    );
  }

  const def = kernel.registry.get(obj.type);
  const fields = kernel.registry.getEntityFields(obj.type);

  // Group fields by their ui.group
  const grouped = new Map<string, EntityFieldDef[]>();
  for (const f of fields) {
    if (f.ui?.hidden) continue;
    const group = f.ui?.group ?? "Properties";
    const list = grouped.get(group) ?? [];
    list.push(f);
    grouped.set(group, list);
  }

  // Determine valid child types for "Add Child" menu
  const validChildTypes = kernel.registry
    .allDefs()
    .filter((d) => kernel.registry.canBeChildOf(d.type, obj.type))
    .map((d) => d);

  return (
    <div
      data-testid="inspector-content"
      style={{ display: "flex", flexDirection: "column", height: "100%", overflow: "hidden" }}
    >
      {/* Header */}
      <div
        style={{
          padding: "8px 12px",
          borderBottom: "1px solid #333",
          fontSize: 11,
          fontWeight: 600,
          textTransform: "uppercase",
          color: "#888",
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
        }}
      >
        <span>Inspector</span>
        <span style={{ color: def?.color ?? "#888", fontWeight: 400, textTransform: "none" }}>
          {def?.icon} {def?.label ?? obj.type}
        </span>
      </div>

      <div style={{ flex: 1, overflow: "auto", padding: "8px 12px" }}>
        {/* Shell fields */}
        <div style={{ marginBottom: 12 }}>
          <FieldRow label="Name">
            <input
              type="text"
              value={obj.name}
              onChange={(e) => handleShellFieldChange("name", e.target.value)}
              style={{
                width: "100%",
                padding: "4px 6px",
                fontSize: 13,
                fontWeight: 500,
                background: "#1e1e1e",
                border: "1px solid #444",
                borderRadius: 3,
                color: "#fff",
                outline: "none",
              }}
            />
          </FieldRow>

          {obj.description !== undefined && (
            <FieldRow label="Description">
              <textarea
                value={obj.description}
                onChange={(e) => handleShellFieldChange("description", e.target.value)}
                rows={2}
                style={{
                  width: "100%",
                  padding: "4px 6px",
                  fontSize: 12,
                  background: "#1e1e1e",
                  border: "1px solid #444",
                  borderRadius: 3,
                  color: "#ccc",
                  outline: "none",
                  resize: "vertical",
                  fontFamily: "inherit",
                }}
              />
            </FieldRow>
          )}

          <FieldRow label="Status">
            <input
              type="text"
              value={obj.status ?? ""}
              onChange={(e) => handleShellFieldChange("status", e.target.value || null)}
              placeholder="—"
              style={{
                width: "100%",
                padding: "4px 6px",
                fontSize: 12,
                background: "#1e1e1e",
                border: "1px solid #444",
                borderRadius: 3,
                color: "#ccc",
                outline: "none",
              }}
            />
          </FieldRow>
        </div>

        {/* Entity-specific fields by group */}
        {[...grouped.entries()].map(([group, groupFields]) => (
          <div key={group} style={{ marginBottom: 12 }}>
            <div
              style={{
                fontSize: 10,
                fontWeight: 600,
                textTransform: "uppercase",
                color: "#666",
                marginBottom: 6,
                letterSpacing: 0.5,
              }}
            >
              {group}
            </div>
            {groupFields.map((field) => (
              <FieldRow key={field.id} label={field.label ?? field.id} required={field.required ?? false}>
                <FieldInput
                  field={field}
                  value={obj.data[field.id]}
                  onChange={(v) => handleDataFieldChange(field.id, v)}
                />
              </FieldRow>
            ))}
          </div>
        ))}

        {/* Metadata */}
        <div style={{ marginBottom: 12 }}>
          <div
            style={{
              fontSize: 10,
              fontWeight: 600,
              textTransform: "uppercase",
              color: "#666",
              marginBottom: 6,
              letterSpacing: 0.5,
            }}
          >
            Metadata
          </div>
          <div style={{ fontSize: 10, color: "#555", lineHeight: 1.8 }}>
            <div>ID: {obj.id}</div>
            <div>Type: {obj.type}</div>
            <div>Created: {new Date(obj.createdAt).toLocaleString()}</div>
            <div>Updated: {new Date(obj.updatedAt).toLocaleString()}</div>
            {obj.parentId && <div>Parent: {obj.parentId}</div>}
          </div>
        </div>

        {/* Add Child */}
        {validChildTypes.length > 0 && (
          <div style={{ marginBottom: 12 }}>
            <div
              style={{
                fontSize: 10,
                fontWeight: 600,
                textTransform: "uppercase",
                color: "#666",
                marginBottom: 6,
                letterSpacing: 0.5,
              }}
            >
              Add Child
            </div>
            <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
              {validChildTypes.map((childDef) => (
                <button
                  key={childDef.type}
                  onClick={() => handleAddChild(childDef.type)}
                  style={{
                    padding: "3px 8px",
                    fontSize: 11,
                    background: "#333",
                    border: "1px solid #444",
                    borderRadius: 3,
                    color: childDef.color ?? "#ccc",
                    cursor: "pointer",
                  }}
                >
                  {childDef.icon} {childDef.label}
                </button>
              ))}
            </div>
          </div>
        )}

        {/* Clipboard */}
        <div style={{ marginBottom: 12 }}>
          <div
            style={{
              fontSize: 10,
              fontWeight: 600,
              textTransform: "uppercase",
              color: "#666",
              marginBottom: 6,
              letterSpacing: 0.5,
            }}
          >
            Clipboard
          </div>
          <div style={{ display: "flex", gap: 4 }}>
            <button
              data-testid="copy-btn"
              onClick={handleCopy}
              style={clipboardBtnStyle}
            >
              Copy
            </button>
            <button
              data-testid="cut-btn"
              onClick={handleCut}
              style={clipboardBtnStyle}
            >
              Cut
            </button>
            <button
              data-testid="paste-btn"
              onClick={handlePaste}
              disabled={!kernel.clipboardHasContent}
              style={{
                ...clipboardBtnStyle,
                opacity: kernel.clipboardHasContent ? 1 : 0.4,
                cursor: kernel.clipboardHasContent ? "pointer" : "default",
              }}
            >
              Paste
            </button>
          </div>
        </div>

        {/* Expression */}
        <ExpressionBar objectId={obj.id} />

        {/* Delete */}
        <button
          data-testid="delete-object-btn"
          onClick={handleDelete}
          style={{
            width: "100%",
            padding: "6px 8px",
            fontSize: 11,
            background: "#3b1111",
            border: "1px solid #5c2020",
            borderRadius: 3,
            color: "#f87171",
            cursor: "pointer",
            marginTop: 8,
          }}
        >
          Delete {def?.label ?? obj.type}
        </button>
      </div>
    </div>
  );
}

// ── Expression Bar ──────────────────────────────────────────────────────────

function ExpressionBar({ objectId }: { objectId: string }) {
  const { evaluate } = useExpression();
  const [formula, setFormula] = useState("");
  const [result, setResult] = useState<{ value: string; isError: boolean } | null>(null);

  const handleEvaluate = useCallback(() => {
    if (!formula.trim()) return;
    const { result: val, errors } = evaluate(formula, objectId as Parameters<typeof evaluate>[1]);
    if (errors.length > 0) {
      setResult({ value: errors.join("; "), isError: true });
    } else {
      setResult({ value: String(val), isError: false });
    }
  }, [formula, evaluate, objectId]);

  return (
    <div style={{ marginBottom: 12 }} data-testid="expression-bar">
      <div
        style={{
          fontSize: 10,
          fontWeight: 600,
          textTransform: "uppercase",
          color: "#666",
          marginBottom: 6,
          letterSpacing: 0.5,
        }}
      >
        Expression
      </div>
      <div style={{ display: "flex", gap: 4 }}>
        <input
          type="text"
          value={formula}
          onChange={(e) => setFormula(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleEvaluate();
          }}
          placeholder="e.g. position + 1"
          data-testid="expression-input"
          style={{
            flex: 1,
            padding: "4px 6px",
            fontSize: 12,
            background: "#1e1e1e",
            border: "1px solid #444",
            borderRadius: 3,
            color: "#ccc",
            outline: "none",
            fontFamily: "monospace",
          }}
        />
        <button
          onClick={handleEvaluate}
          data-testid="expression-eval-btn"
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
          Eval
        </button>
      </div>
      {result && (
        <div
          data-testid="expression-result"
          style={{
            marginTop: 4,
            padding: "4px 6px",
            fontSize: 12,
            fontFamily: "monospace",
            background: result.isError ? "#3b1111" : "#1a2e1a",
            border: `1px solid ${result.isError ? "#5c2020" : "#2d4a2d"}`,
            borderRadius: 3,
            color: result.isError ? "#f87171" : "#4ade80",
          }}
        >
          {result.isError ? "Error: " : "= "}
          {result.value}
        </div>
      )}
    </div>
  );
}

const clipboardBtnStyle = {
  flex: 1,
  padding: "4px 8px",
  fontSize: 11,
  background: "#333",
  border: "1px solid #444",
  borderRadius: 3,
  color: "#ccc",
  cursor: "pointer",
} as const;

// ── Layout Helper ───────────────────────────────────────────────────────────

function FieldRow({
  label,
  required,
  children,
}: {
  label: string;
  required?: boolean;
  children: React.ReactNode;
}) {
  return (
    <div style={{ marginBottom: 8 }}>
      <div
        style={{
          fontSize: 11,
          color: "#999",
          marginBottom: 3,
        }}
      >
        {label}
        {required && <span style={{ color: "#f87171", marginLeft: 2 }}>*</span>}
      </div>
      {children}
    </div>
  );
}
