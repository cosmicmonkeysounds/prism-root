/**
 * Editor panel — CodeMirror 6 editing a LoroText with real-time sync.
 * The hidden buffer is the Loro CRDT, not the CodeMirror instance.
 *
 * When a text-block or heading is selected, edits its content field.
 * Otherwise shows the shared scratch document.
 */

import { useMemo } from "react";
import { useCodemirror, prismJSLang } from "@prism/core/codemirror";
import { useKernel, useSelection, useObject } from "../kernel/index.js";

export function EditorPanel() {
  const kernel = useKernel();
  const { selectedId } = useSelection();
  const selectedObj = useObject(selectedId);

  // Use the kernel's CollectionStore Loro doc for the shared text buffer
  const doc = kernel.store.doc;

  const text = useMemo(() => {
    const t = doc.getText("editor_content");
    if (t.length === 0) {
      t.insert(
        0,
        '-- Prism Studio Editor\n-- This text is synced to the Loro CRDT.\n-- Edit here and see changes in the CRDT Inspector.\n\nlocal greeting = "Hello from Prism!"\nprint(greeting)\n',
      );
      doc.commit();
    }
    return t;
  }, [doc]);

  const extensions = useMemo(() => [prismJSLang()], []);

  const { containerRef } = useCodemirror({
    doc,
    text,
    extensions,
  });

  // Determine what we're editing
  const editLabel = selectedObj
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
        {selectedObj && (
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
