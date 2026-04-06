/**
 * Editor panel — CodeMirror 6 editing a LoroText with real-time sync.
 * The hidden buffer is the Loro CRDT, not the CodeMirror instance.
 *
 * When a text-block or heading is selected, edits its content field.
 * Otherwise shows the shared scratch document.
 */

import { useEffect, useMemo, useRef } from "react";
import { useCodemirror, prismJSLang } from "@prism/core/codemirror";
import type { ObjectId } from "@prism/core/object-model";
import { useKernel, useSelection, useObject } from "../kernel/index.js";

/** Map editable object types to their text data field key. */
const EDITABLE_FIELD: Record<string, string> = {
  "text-block": "content",
  heading: "text",
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

  const { containerRef } = useCodemirror({
    doc,
    text,
    extensions,
  });

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
      <div
        ref={containerRef}
        style={{ flex: 1, overflow: "auto" }}
      />
    </div>
  );
}
