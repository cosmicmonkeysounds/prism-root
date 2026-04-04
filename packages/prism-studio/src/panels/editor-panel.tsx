/**
 * Editor panel — CodeMirror 6 editing a LoroText with real-time sync.
 * The hidden buffer is the Loro CRDT, not the CodeMirror instance.
 */

import { useMemo } from "react";
import type { LoroDoc } from "loro-crdt";
import { useCodemirror } from "@prism/core/layer2/codemirror/use-codemirror";
import { prismJSLang } from "@prism/core/layer2/codemirror/editor-setup";

export type EditorPanelProps = {
  doc: LoroDoc;
};

export function EditorPanel({ doc }: EditorPanelProps) {
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

  return (
    <div style={{ height: "100%", display: "flex", flexDirection: "column" }}>
      <div
        style={{
          padding: "8px 12px",
          fontSize: 12,
          color: "#666",
          borderBottom: "1px solid #eee",
          background: "#fafafa",
        }}
      >
        editor_content (LoroText) — CodeMirror 6
      </div>
      <div
        ref={containerRef}
        style={{ flex: 1, overflow: "auto" }}
      />
    </div>
  );
}
