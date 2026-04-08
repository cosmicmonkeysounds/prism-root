/**
 * Inspector Panel — schema-driven property editor for the selected object.
 *
 * Reads EntityDef fields from ObjectRegistry and renders appropriate
 * inputs. Saves changes back through the kernel (with undo support).
 */

import { useState, useCallback, useMemo, useRef, useEffect } from "react";
import type { EntityFieldDef } from "@prism/core/object-model";
import {
  createSyntaxEngine,
  type CompletionItem,
  type SchemaContext,
  type SyntaxEngine,
} from "@prism/core/syntax";
import { useKernel, useSelection, useObject, useExpression } from "../kernel/index.js";

/** Lazily constructed shared syntax engine for expression completions. */
let sharedSyntaxEngine: SyntaxEngine | null = null;
function getSyntaxEngine(): SyntaxEngine {
  if (!sharedSyntaxEngine) sharedSyntaxEngine = createSyntaxEngine();
  return sharedSyntaxEngine;
}

// ── Field Renderer ──────────────────────────────────────────────────────────

function FieldInput({
  field,
  value,
  objectId,
  onChange,
}: {
  field: EntityFieldDef;
  value: unknown;
  objectId: string;
  onChange: (value: unknown) => void;
}) {
  // Computed fields: rollup/lookup + any field with an inline expression
  // are rendered read-only and evaluated live.
  if (field.expression || field.type === "lookup" || field.type === "rollup") {
    return <ComputedFieldDisplay field={field} objectId={objectId} />;
  }

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

/**
 * Read-only renderer for computed fields (formula / lookup / rollup).
 * Calls the kernel's expression evaluator when an `expression` is present,
 * otherwise reports the field type as unresolved. Errors are surfaced inline
 * so authors can fix their formulas without hunting through logs.
 */
function ComputedFieldDisplay({
  field,
  objectId,
}: {
  field: EntityFieldDef;
  objectId: string;
}) {
  const { evaluate } = useExpression();
  const result = useMemo(() => {
    if (field.expression) {
      const { result: val, errors } = evaluate(
        field.expression,
        objectId as Parameters<typeof evaluate>[1],
      );
      if (errors.length > 0) {
        return { value: errors.join("; "), isError: true };
      }
      return { value: String(val), isError: false };
    }
    // lookup / rollup without inline expression — resolved at projection time.
    return { value: `(${field.type} — resolved at view time)`, isError: false };
  }, [field, objectId, evaluate]);

  return (
    <div
      data-testid={`computed-field-${field.id}`}
      style={{
        padding: "4px 6px",
        fontSize: 12,
        fontFamily: "monospace",
        background: result.isError ? "#3b1111" : "#1a2e1a",
        border: `1px solid ${result.isError ? "#5c2020" : "#2d4a2d"}`,
        borderRadius: 3,
        color: result.isError ? "#f87171" : "#4ade80",
      }}
    >
      {result.isError ? "⚠ " : "= "}
      {result.value}
      {field.expression && (
        <div
          style={{
            marginTop: 3,
            color: "#666",
            fontSize: 10,
            fontFamily: "monospace",
          }}
        >
          {field.expression}
        </div>
      )}
    </div>
  );
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

  const handleSaveAsTemplate = useCallback(() => {
    if (!obj) return;
    const name = window.prompt("Template name:", `${obj.name} Template`);
    if (!name) return;
    const id = `user-${obj.type}-${Date.now()}`;
    const template = kernel.templateFromObject(obj.id, {
      id,
      name,
      description: `Saved from ${obj.name}`,
      category: obj.type === "section" ? "section" : "user",
    });
    if (!template) {
      kernel.notifications.add({
        title: "Could not build template",
        kind: "error",
      });
      return;
    }
    kernel.registerTemplate(template);
    kernel.notifications.add({
      title: `Saved template "${name}"`,
      kind: "success",
    });
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
                  objectId={obj.id}
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

        {/* Save as Template */}
        <div style={{ marginBottom: 12 }}>
          <button
            data-testid="save-as-template-btn"
            onClick={handleSaveAsTemplate}
            style={{
              width: "100%",
              padding: "6px 8px",
              fontSize: 11,
              background: "#1a2e1a",
              border: "1px solid #2d4a2d",
              borderRadius: 3,
              color: "#4ade80",
              cursor: "pointer",
            }}
          >
            Save as Template
          </button>
        </div>

        {/* Expression */}
        <ExpressionBar objectId={obj.id} objectType={obj.type} schemaFields={fields} />

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

function ExpressionBar({
  objectId,
  objectType,
  schemaFields,
}: {
  objectId: string;
  objectType: string;
  schemaFields: EntityFieldDef[];
}) {
  const { evaluate } = useExpression();
  const [formula, setFormula] = useState("");
  const [cursor, setCursor] = useState(0);
  const [result, setResult] = useState<{ value: string; isError: boolean } | null>(null);
  const [showCompletions, setShowCompletions] = useState(false);
  const [selectedCompletion, setSelectedCompletion] = useState(0);
  const inputRef = useRef<HTMLInputElement | null>(null);

  const schemaContext = useMemo<SchemaContext>(
    () => ({ objectType, fields: schemaFields }),
    [objectType, schemaFields],
  );

  const completions = useMemo<CompletionItem[]>(() => {
    if (!showCompletions) return [];
    try {
      return getSyntaxEngine().complete(formula, cursor, schemaContext).slice(0, 8);
    } catch {
      return [];
    }
  }, [formula, cursor, schemaContext, showCompletions]);

  const diagnostics = useMemo(() => {
    if (!formula.trim()) return [];
    try {
      return getSyntaxEngine().diagnose(formula, schemaContext);
    } catch {
      return [];
    }
  }, [formula, schemaContext]);

  useEffect(() => {
    if (selectedCompletion >= completions.length) setSelectedCompletion(0);
  }, [completions.length, selectedCompletion]);

  const applyCompletion = useCallback(
    (item: CompletionItem) => {
      const insertText = item.insertText ?? item.label;
      const before = formula.slice(0, cursor).replace(/[A-Za-z_][A-Za-z0-9_]*$/, "");
      const after = formula.slice(cursor);
      const next = before + insertText + after;
      setFormula(next);
      setShowCompletions(false);
      setSelectedCompletion(0);
      requestAnimationFrame(() => {
        inputRef.current?.focus();
        const pos = before.length + insertText.length;
        inputRef.current?.setSelectionRange(pos, pos);
        setCursor(pos);
      });
    },
    [formula, cursor],
  );

  const handleEvaluate = useCallback(() => {
    if (!formula.trim()) return;
    const { result: val, errors } = evaluate(formula, objectId as Parameters<typeof evaluate>[1]);
    if (errors.length > 0) {
      setResult({ value: errors.join("; "), isError: true });
    } else {
      setResult({ value: String(val), isError: false });
    }
  }, [formula, evaluate, objectId]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLInputElement>) => {
      if (showCompletions && completions.length > 0) {
        if (e.key === "ArrowDown") {
          e.preventDefault();
          setSelectedCompletion((i) => (i + 1) % completions.length);
          return;
        }
        if (e.key === "ArrowUp") {
          e.preventDefault();
          setSelectedCompletion((i) => (i - 1 + completions.length) % completions.length);
          return;
        }
        if (e.key === "Tab" || (e.key === "Enter" && showCompletions)) {
          e.preventDefault();
          const pick = completions[selectedCompletion];
          if (pick) applyCompletion(pick);
          return;
        }
        if (e.key === "Escape") {
          e.preventDefault();
          setShowCompletions(false);
          return;
        }
      }
      if (e.key === "Enter") handleEvaluate();
      if (e.key === " " && e.ctrlKey) {
        e.preventDefault();
        setShowCompletions(true);
      }
    },
    [showCompletions, completions, selectedCompletion, applyCompletion, handleEvaluate],
  );

  const handleChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setFormula(e.target.value);
    setCursor(e.target.selectionStart ?? e.target.value.length);
    setShowCompletions(true);
  }, []);

  return (
    <div style={{ marginBottom: 12, position: "relative" }} data-testid="expression-bar">
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
          ref={inputRef}
          type="text"
          value={formula}
          onChange={handleChange}
          onKeyDown={handleKeyDown}
          onFocus={() => setShowCompletions(true)}
          onBlur={() => {
            // Delay so click on completion item fires first
            setTimeout(() => setShowCompletions(false), 120);
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

      {showCompletions && completions.length > 0 && (
        <ul
          data-testid="expression-completions"
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: "100%",
            zIndex: 10,
            margin: 0,
            padding: 0,
            listStyle: "none",
            background: "#1e1e1e",
            border: "1px solid #444",
            borderRadius: 3,
            maxHeight: 180,
            overflowY: "auto",
            fontSize: 12,
            fontFamily: "monospace",
          }}
        >
          {completions.map((item, i) => (
            <li
              key={`${item.kind}:${item.label}`}
              data-testid={`completion-${item.label}`}
              onMouseDown={(e) => {
                e.preventDefault();
                applyCompletion(item);
              }}
              style={{
                padding: "4px 8px",
                cursor: "pointer",
                display: "flex",
                gap: 8,
                alignItems: "center",
                background: i === selectedCompletion ? "#2a2a2a" : "transparent",
                color: "#ccc",
              }}
            >
              <span style={{ color: kindColor(item.kind), width: 60, fontSize: 10 }}>
                {item.kind}
              </span>
              <span style={{ flex: 1 }}>{item.label}</span>
              {item.detail && (
                <span style={{ color: "#666", fontSize: 10 }}>{item.detail}</span>
              )}
            </li>
          ))}
        </ul>
      )}

      {diagnostics.length > 0 && !result && (
        <div
          data-testid="expression-diagnostics"
          style={{
            marginTop: 4,
            padding: "4px 6px",
            fontSize: 11,
            fontFamily: "monospace",
            background: "#2a1f0a",
            border: "1px solid #5c4420",
            borderRadius: 3,
            color: "#fbbf24",
          }}
        >
          {diagnostics.map((d, i) => (
            <div key={i}>
              {d.severity === "error" ? "⚠ " : "ℹ "}
              {d.message}
            </div>
          ))}
        </div>
      )}

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

function kindColor(kind: CompletionItem["kind"]): string {
  switch (kind) {
    case "field":
      return "#4ade80";
    case "function":
      return "#60a5fa";
    case "keyword":
      return "#f472b6";
    case "operator":
      return "#fbbf24";
    case "type":
      return "#a78bfa";
    case "value":
      return "#fb923c";
    default:
      return "#888";
  }
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
