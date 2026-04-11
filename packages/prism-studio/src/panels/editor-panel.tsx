/**
 * Editor panel — CodeMirror 6 editing a LoroText with real-time sync.
 * The hidden buffer is the Loro CRDT, not the CodeMirror instance.
 *
 * When a text-block or heading is selected, edits its content field.
 * Otherwise shows the shared scratch document.
 */

import { useEffect, useMemo, useRef, useCallback } from "react";
import { useCodemirror, prismJSLang } from "@prism/core/codemirror";
import type { ObjectId } from "@prism/core/object-model";
import { useKernel, useSelection, useObject } from "../kernel/index.js";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
/** Non-null EditorView handle exposed by `useCodemirror`. */
type CmView = NonNullable<ReturnType<typeof useCodemirror>["view"]>;

export type MarkdownAction =
  | { wrap: { before: string; after: string } }
  | { linePrefix: string }
  | { link: true };

/**
 * Pure edit builder for markdown toolbar actions. Given the current source,
 * selection, and a markdown action, returns the replacement range + text and
 * where the new caret/selection should land. Exported for unit testing so we
 * don't need a live EditorView to verify toolbar behaviour.
 */
export function computeMarkdownEdit(
  doc: string,
  from: number,
  to: number,
  action: MarkdownAction,
): { from: number; to: number; insert: string; anchor: number; head: number } {
  const text = doc.slice(from, to);

  if ("wrap" in action) {
    const { before, after } = action.wrap;
    const insert = `${before}${text}${after}`;
    return {
      from,
      to,
      insert,
      anchor: from + before.length,
      head: from + before.length + text.length,
    };
  }

  if ("linePrefix" in action) {
    const prefix = action.linePrefix;
    // Expand to full-line boundaries
    const lineStart = doc.lastIndexOf("\n", from - 1) + 1;
    const nextNl = doc.indexOf("\n", to);
    const lineEnd = nextNl === -1 ? doc.length : nextNl;
    const region = doc.slice(lineStart, lineEnd);
    const insert = region
      .split("\n")
      .map((line) => prefix + line)
      .join("\n");
    return {
      from: lineStart,
      to: lineEnd,
      insert,
      anchor: lineStart,
      head: lineStart + insert.length,
    };
  }

  // link
  const label = text || "link text";
  const insert = `[${label}](https://)`;
  return {
    from,
    to,
    insert,
    anchor: from + 1,
    head: from + 1 + label.length,
  };
}

/** Apply a markdown transformation to the current CodeMirror selection. */
export function applyMarkdown(view: CmView, action: MarkdownAction): void {
  const state = view.state;
  const { from, to } = state.selection.main;
  const edit = computeMarkdownEdit(state.doc.toString(), from, to, action);
  view.dispatch({
    changes: { from: edit.from, to: edit.to, insert: edit.insert },
    selection: { anchor: edit.anchor, head: edit.head },
  });
  view.focus();
}

/** Map editable object types to their text data field key. */
const EDITABLE_FIELD: Record<string, string> = {
  "text-block": "content",
  heading: "text",
  "code-block": "source",
  "luau-block": "source",
};

export function EditorPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selectedObj = useObject(selectedId);

  // Use the kernel's CollectionStore Loro doc for all text buffers
  const doc = kernel.store.doc;

  // Determine if the selected object is editable
  const fieldKey =
    selectedObj && !selectedObj.deletedAt
      ? EDITABLE_FIELD[selectedObj.type] ?? null
      : null;

  // When editable, use the object's ID; otherwise fall back to scratch buffer
  const editingObjId: ObjectId | null =
    fieldKey && selectedObj ? selectedObj.id : null;

  // Get or create the LoroText for the current editing target.
  // For objects: keyed as "obj_content_{id}", seeded from object data.
  // For scratch: keyed as "editor_content", seeded with default text.
  const text = useMemo(() => {
    if (!editingObjId) {
      const t = doc.getText("editor_content");
      if (t.length === 0) {
        t.insert(
          0,
          '-- Prism Studio Editor\n-- This text is synced to the Loro CRDT.\n-- Edit here and see changes in the CRDT Inspector.\n\nlocal greeting = "Hello from Prism!"\nprint(greeting)\n',
        );
        doc.commit();
      }
      return t;
    }

    // Object-specific LoroText
    const textKey = `obj_content_${editingObjId}`;
    const t = doc.getText(textKey);

    // Seed from object data if the LoroText is empty but the object has content
    const objContent = fieldKey
      ? ((selectedObj?.data as Record<string, unknown>)?.[fieldKey] as
          | string
          | undefined)
      : undefined;
    if (t.length === 0 && objContent) {
      t.insert(0, objContent);
      doc.commit();
    }

    return t;
  }, [doc, editingObjId, fieldKey, selectedObj]);

  // Debounced sync: when LoroText changes, update the kernel object
  const timerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  const editingObjIdRef = useRef(editingObjId);
  const fieldKeyRef = useRef(fieldKey);
  editingObjIdRef.current = editingObjId;
  fieldKeyRef.current = fieldKey;

  useEffect(() => {
    if (!editingObjId) return;

    const sub = doc.subscribe(() => {
      // Only sync when we're still editing the same object
      const currentObjId = editingObjIdRef.current;
      const currentField = fieldKeyRef.current;
      if (!currentObjId || !currentField) return;

      const textKey = `obj_content_${currentObjId}`;
      const loroText = doc.getText(textKey);
      const content = loroText.toString();

      clearTimeout(timerRef.current);
      timerRef.current = setTimeout(() => {
        const obj = kernel.store.getObject(currentObjId);
        if (!obj) return;
        kernel.updateObject(obj.id, {
          data: { ...obj.data, [currentField]: content },
        });
      }, 500);
    });

    return () => {
      sub();
      clearTimeout(timerRef.current);
    };
  }, [doc, editingObjId, kernel]);

  const extensions = useMemo(() => [prismJSLang()], []);

  const { containerRef, view } = useCodemirror({
    doc,
    text,
    extensions,
  });

  const isMarkdownBlock =
    selectedObj?.type === "text-block" &&
    ((selectedObj.data as Record<string, unknown>)?.format ?? "markdown") === "markdown";

  const runMarkdown = useCallback(
    (kind: Parameters<typeof applyMarkdown>[1]) => {
      if (!view) return;
      applyMarkdown(view, kind);
    },
    [view],
  );

  // Header label
  const editLabel =
    editingObjId && selectedObj
      ? `${selectedObj.type}: ${selectedObj.name}`
      : "editor_content (LoroText)";

  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column" }}>
      <div
        style={{
          padding: "8px 12px",
          fontSize: 12,
          color: "#999",
          borderBottom: "1px solid #333",
          background: "#252526",
          display: "flex",
          justifyContent: "space-between",
        }}
      >
        <span>{editLabel} — CodeMirror 6</span>
        {editingObjId && selectedObj && (
          <span style={{ color: "#007acc", fontSize: 10 }}>
            {selectedObj.id}
          </span>
        )}
      </div>
      {isMarkdownBlock && (
        <div
          data-testid="markdown-toolbar"
          style={{
            display: "flex",
            gap: 4,
            padding: "4px 8px",
            borderBottom: "1px solid #333",
            background: "#1f1f1f",
          }}
        >
          <ToolbarButton
            label="B"
            title="Bold"
            onClick={() => runMarkdown({ wrap: { before: "**", after: "**" } })}
            bold
          />
          <ToolbarButton
            label="I"
            title="Italic"
            onClick={() => runMarkdown({ wrap: { before: "_", after: "_" } })}
            italic
          />
          <ToolbarButton
            label="</>"
            title="Inline code"
            onClick={() => runMarkdown({ wrap: { before: "`", after: "`" } })}
          />
          <div style={{ width: 1, background: "#333", margin: "2px 4px" }} />
          <ToolbarButton label="H1" title="Heading 1" onClick={() => runMarkdown({ linePrefix: "# " })} />
          <ToolbarButton label="H2" title="Heading 2" onClick={() => runMarkdown({ linePrefix: "## " })} />
          <ToolbarButton label="H3" title="Heading 3" onClick={() => runMarkdown({ linePrefix: "### " })} />
          <div style={{ width: 1, background: "#333", margin: "2px 4px" }} />
          <ToolbarButton label="•" title="Bullet list" onClick={() => runMarkdown({ linePrefix: "- " })} />
          <ToolbarButton label="1." title="Numbered list" onClick={() => runMarkdown({ linePrefix: "1. " })} />
          <ToolbarButton label="❝" title="Quote" onClick={() => runMarkdown({ linePrefix: "> " })} />
          <ToolbarButton label="🔗" title="Link" onClick={() => runMarkdown({ link: true })} />
        </div>
      )}
      <div
        ref={containerRef}
        style={{ flex: 1, overflow: "auto" }}
      />
    </div>
  );
}

function ToolbarButton({
  label,
  title,
  onClick,
  bold,
  italic,
}: {
  label: string;
  title: string;
  onClick: () => void;
  bold?: boolean;
  italic?: boolean;
}) {
  return (
    <button
      type="button"
      title={title}
      onClick={onClick}
      data-testid={`md-${title.toLowerCase().replace(/\s+/g, "-")}`}
      style={{
        padding: "3px 8px",
        fontSize: 12,
        minWidth: 26,
        background: "#2a2a2a",
        border: "1px solid #444",
        borderRadius: 3,
        color: "#ccc",
        cursor: "pointer",
        fontWeight: bold ? 700 : 400,
        fontStyle: italic ? "italic" : "normal",
      }}
    >
      {label}
    </button>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const EDITOR_LENS_ID = lensId("editor");

export const editorLensManifest: LensManifest = {

  id: EDITOR_LENS_ID,
  name: "Editor",
  icon: "\u270E",
  category: "editor",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-editor", name: "Switch to Editor", shortcut: ["e"], section: "Navigation" }],
  },
};

export const editorLensBundle: LensBundle = defineLensBundle(
  editorLensManifest,
  EditorPanel,
);
