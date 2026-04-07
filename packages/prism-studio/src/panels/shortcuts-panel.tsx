/**
 * Shortcuts Panel — keyboard binding viewer and manager.
 *
 * Shows all keyboard bindings from the InputRouter's global scope,
 * allows adding/removing bindings, and displays recent input events.
 */

import { useState, useCallback } from "react";
import { useInputRouter, useKernel } from "../kernel/index.js";

// ── Styles ──────────────────────────────────────────────────────────────────

const styles = {
  container: {
    padding: "1rem",
    height: "100%",
    overflow: "auto",
    fontFamily: "system-ui, -apple-system, sans-serif",
    color: "#ccc",
    background: "#1e1e1e",
  },
  header: {
    fontSize: "1.25rem",
    fontWeight: 600,
    marginBottom: "1rem",
    color: "#e5e5e5",
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#333",
    color: "#888",
    marginLeft: "0.375rem",
  },
  btn: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#333",
    border: "1px solid #444",
    borderRadius: 3,
    color: "#ccc",
    cursor: "pointer",
  },
  btnPrimary: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#0e639c",
    border: "1px solid #1177bb",
    borderRadius: 3,
    color: "#fff",
    cursor: "pointer",
  },
  btnDanger: {
    padding: "4px 10px",
    fontSize: 11,
    background: "#3b1111",
    border: "1px solid #5c2020",
    borderRadius: 3,
    color: "#f87171",
    cursor: "pointer",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    outline: "none",
    boxSizing: "border-box" as const,
  },
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  tabRow: {
    display: "flex",
    gap: 4,
    marginBottom: "0.75rem",
    borderBottom: "1px solid #333",
    paddingBottom: "0.375rem",
  },
  bindingRow: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    padding: "0.375rem 0.5rem",
    borderBottom: "1px solid #2a2a2a",
    fontSize: "0.8125rem",
  },
  kbd: {
    display: "inline-block",
    padding: "2px 6px",
    background: "#333",
    border: "1px solid #555",
    borderRadius: "3px",
    fontSize: "0.75rem",
    fontFamily: "monospace",
    color: "#e5e5e5",
  },
  eventRow: {
    padding: "0.25rem 0.5rem",
    borderBottom: "1px solid #2a2a2a",
    fontSize: "0.75rem",
    fontFamily: "monospace",
    color: "#888",
  },
} as const;

// ── Binding Row ────────────────────────────────────────────────────────────

function BindingRow({
  shortcut,
  action,
  onRemove,
}: {
  shortcut: string;
  action: string;
  onRemove: () => void;
}) {
  return (
    <div style={styles.bindingRow as React.CSSProperties} data-testid={`binding-${shortcut.replace(/\+/g, "-")}`}>
      <div>
        <span style={styles.kbd}>{shortcut}</span>
        <span style={{ marginLeft: 8, color: "#ccc" }}>{action}</span>
      </div>
      <button
        style={styles.btnDanger}
        onClick={onRemove}
        data-testid={`remove-binding-${shortcut.replace(/\+/g, "-")}`}
      >
        Remove
      </button>
    </div>
  );
}

// ── Add Binding Form ───────────────────────────────────────────────────────

function AddBindingForm({ onAdd }: { onAdd: (shortcut: string, action: string) => void }) {
  const [shortcut, setShortcut] = useState("");
  const [action, setAction] = useState("");

  const handleAdd = useCallback(() => {
    if (!shortcut.trim() || !action.trim()) return;
    onAdd(shortcut.trim(), action.trim());
    setShortcut("");
    setAction("");
  }, [shortcut, action, onAdd]);

  return (
    <div style={styles.card} data-testid="add-binding-form">
      <div style={styles.sectionTitle}>Add Binding</div>
      <div style={{ display: "flex", gap: 6 }}>
        <input
          style={{ ...styles.input, width: "40%" }}
          placeholder="cmd+shift+x"
          value={shortcut}
          onChange={(e) => setShortcut(e.target.value)}
          data-testid="binding-shortcut-input"
        />
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="action-name"
          value={action}
          onChange={(e) => setAction(e.target.value)}
          data-testid="binding-action-input"
        />
        <button
          style={styles.btnPrimary}
          onClick={handleAdd}
          data-testid="add-binding-btn"
        >
          Bind
        </button>
      </div>
    </div>
  );
}

// ── Event Kind Badge ───────────────────────────────────────────────────────

function EventKindBadge({ kind }: { kind: string }) {
  const colors: Record<string, string> = {
    dispatched: "#22c55e",
    pushed: "#3b82f6",
    popped: "#f59e0b",
    unhandled: "#ef4444",
  };
  return (
    <span style={{ ...styles.badge, color: colors[kind] ?? "#888" }}>
      {kind}
    </span>
  );
}

// ── Main Panel ─────────────────────────────────────────────────────────────

export function ShortcutsPanel() {
  const kernel = useKernel();
  const { bindings, bind, unbind, recentEvents } = useInputRouter();
  const [tab, setTab] = useState<"bindings" | "events" | "scopes">("bindings");

  const handleAdd = useCallback(
    (shortcut: string, action: string) => {
      bind(shortcut, action);
      kernel.notifications.add({
        title: `Bound: ${shortcut} -> ${action}`,
        kind: "success",
      });
    },
    [bind, kernel],
  );

  const handleRemove = useCallback(
    (shortcut: string) => {
      unbind(shortcut);
      kernel.notifications.add({
        title: `Unbound: ${shortcut}`,
        kind: "info",
      });
    },
    [unbind, kernel],
  );

  const scopes = kernel.inputRouter.allScopes;

  return (
    <div style={styles.container} data-testid="shortcuts-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Shortcuts</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {bindings.length} binding(s)
        </span>
      </div>

      <div style={styles.tabRow as React.CSSProperties}>
        <button
          style={{ ...styles.btn, background: tab === "bindings" ? "#094771" : "#333" }}
          onClick={() => setTab("bindings")}
          data-testid="shortcuts-tab-bindings"
        >
          Bindings ({bindings.length})
        </button>
        <button
          style={{ ...styles.btn, background: tab === "scopes" ? "#094771" : "#333" }}
          onClick={() => setTab("scopes")}
          data-testid="shortcuts-tab-scopes"
        >
          Scopes ({scopes.length})
        </button>
        <button
          style={{ ...styles.btn, background: tab === "events" ? "#094771" : "#333" }}
          onClick={() => setTab("events")}
          data-testid="shortcuts-tab-events"
        >
          Events ({recentEvents.length})
        </button>
      </div>

      {tab === "bindings" && (
        <>
          <AddBindingForm onAdd={handleAdd} />
          {bindings.length === 0 ? (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No bindings configured.
            </div>
          ) : (
            <div data-testid="bindings-list">
              {bindings.map((b) => (
                <BindingRow
                  key={b.shortcut}
                  shortcut={b.shortcut}
                  action={b.action}
                  onRemove={() => handleRemove(b.shortcut)}
                />
              ))}
            </div>
          )}
        </>
      )}

      {tab === "scopes" && (
        <div data-testid="scopes-list">
          {scopes.length === 0 ? (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No input scopes active.
            </div>
          ) : (
            scopes.map((scope) => (
              <div key={scope.id} style={styles.card} data-testid={`scope-${scope.id}`}>
                <div style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
                  {scope.label}
                  <span style={styles.badge}>{scope.id}</span>
                  {kernel.inputRouter.activeScope?.id === scope.id && (
                    <span style={{ ...styles.badge, color: "#22c55e" }}>active</span>
                  )}
                </div>
                <div style={styles.meta}>
                  {scope.keyboard.allBindings().length} binding(s) |{" "}
                  {scope.handlers.size} handler(s)
                </div>
              </div>
            ))
          )}
        </div>
      )}

      {tab === "events" && (
        <div data-testid="events-list">
          {recentEvents.length === 0 ? (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No input events yet. Press keys to see events.
            </div>
          ) : (
            [...recentEvents].reverse().map((event, i) => (
              <div key={i} style={styles.eventRow}>
                <EventKindBadge kind={event.kind} />
                <span style={{ marginLeft: 8 }}>
                  {"action" in event ? event.action : ""}
                  {"scopeId" in event ? ` [${event.scopeId}]` : ""}
                </span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
