/**
 * Undo Status Bar — shows undo/redo buttons with labels in the header.
 */

import { useUndo } from "../kernel/index.js";

export function UndoStatusBar() {
  const { canUndo, canRedo, undoLabel, redoLabel, undo, redo } = useUndo();

  const btnStyle = (enabled: boolean) => ({
    background: "none",
    border: "none",
    color: enabled ? "#ccc" : "#444",
    cursor: enabled ? "pointer" : "default",
    fontSize: 13,
    padding: "0 4px",
    lineHeight: "32px",
  });

  return (
    <div style={{ display: "flex", alignItems: "center", gap: 2 }}>
      <button
        data-testid="undo-btn"
        onClick={undo}
        disabled={!canUndo}
        title={undoLabel ? `Undo: ${undoLabel}` : "Nothing to undo"}
        style={btnStyle(canUndo)}
      >
        {"\u21B6"}
      </button>
      <button
        data-testid="redo-btn"
        onClick={redo}
        disabled={!canRedo}
        title={redoLabel ? `Redo: ${redoLabel}` : "Nothing to redo"}
        style={btnStyle(canRedo)}
      >
        {"\u21B7"}
      </button>
    </div>
  );
}
