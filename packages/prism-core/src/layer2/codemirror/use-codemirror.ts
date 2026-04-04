/**
 * React hook for mounting a CodeMirror 6 editor synced to a LoroText.
 * The hook manages the EditorView lifecycle and Loro subscription.
 */

import { useEffect, useRef, useState } from "react";
import { EditorView } from "@codemirror/view";
import { EditorState, type Extension } from "@codemirror/state";
import type { LoroDoc, LoroText } from "loro-crdt";
import { loroSync } from "./loro-sync.js";
import { prismEditorSetup } from "./editor-setup.js";

export type UseCodemirrorOptions = {
  /** The LoroDoc containing the text. */
  doc: LoroDoc;
  /** The LoroText node to edit. */
  text: LoroText;
  /** Additional CodeMirror extensions (language, theme, etc). */
  extensions?: Extension[];
  /** Whether the editor is read-only. */
  readOnly?: boolean;
};

/**
 * Mount a CodeMirror 6 editor synced to a LoroText CRDT node.
 *
 * Returns a ref to attach to a container div and the EditorView instance.
 *
 * ```tsx
 * function MyEditor() {
 *   const { containerRef } = useCodemirror({ doc, text });
 *   return <div ref={containerRef} />;
 * }
 * ```
 */
export function useCodemirror(options: UseCodemirrorOptions) {
  const { doc, text, extensions = [], readOnly = false } = options;
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const [view, setView] = useState<EditorView | null>(null);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const state = EditorState.create({
      doc: text.toString(),
      extensions: [
        prismEditorSetup(),
        loroSync({ doc, text }),
        EditorView.editable.of(!readOnly),
        ...extensions,
      ],
    });

    const editorView = new EditorView({ state, parent: container });
    viewRef.current = editorView;
    setView(editorView);

    return () => {
      editorView.destroy();
      viewRef.current = null;
      setView(null);
    };
  }, [doc, text, readOnly]); // Intentionally not including extensions to avoid re-mount

  return { containerRef, view };
}
