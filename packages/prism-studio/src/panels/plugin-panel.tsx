/**
 * Plugin Panel — view and manage registered plugins and their contributions.
 *
 * Shows all plugins from PluginRegistry with their contributed views,
 * commands, keybindings, and other extension points.
 */

import { useState, useCallback } from "react";
import { usePlugins, useKernel } from "../kernel/index.js";
import type { PrismPlugin } from "@prism/core/plugin";
import { pluginId } from "@prism/core/plugin";

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
  cardHeader: {
    display: "flex",
    justifyContent: "space-between",
    alignItems: "center",
    marginBottom: "0.375rem",
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
    width: "100%",
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
  contrib: {
    fontSize: "0.6875rem",
    color: "#888",
    padding: "0.25rem 0",
    borderBottom: "1px solid #2a2a2a",
  },
} as const;

// ── Contribution Summary ───────────────────────────────────────────────────

function ContributionSummary({ plugin }: { plugin: PrismPlugin }) {
  const c = plugin.contributes;
  if (!c) return <div style={styles.meta}>No contributions</div>;

  const items: string[] = [];
  if (c.views?.length) items.push(`${c.views.length} view(s)`);
  if (c.commands?.length) items.push(`${c.commands.length} command(s)`);
  if (c.keybindings?.length) items.push(`${c.keybindings.length} keybinding(s)`);
  if (c.contextMenus?.length) items.push(`${c.contextMenus.length} menu(s)`);
  if (c.settings?.length) items.push(`${c.settings.length} setting(s)`);
  if (c.toolbar?.length) items.push(`${c.toolbar.length} toolbar(s)`);
  if (c.statusBar?.length) items.push(`${c.statusBar.length} status bar(s)`);

  return (
    <div data-testid={`plugin-contributions-${plugin.id}`}>
      {items.length === 0 ? (
        <div style={styles.meta}>No contributions</div>
      ) : (
        items.map((item) => (
          <div key={item} style={styles.contrib}>{item}</div>
        ))
      )}
    </div>
  );
}

// ── Plugin Card ────────────────────────────────────────────────────────────

function PluginCard({
  plugin,
  onRemove,
}: {
  plugin: PrismPlugin;
  onRemove: () => void;
}) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div style={styles.card} data-testid={`plugin-${plugin.id}`}>
      <div style={styles.cardHeader as React.CSSProperties}>
        <div>
          <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
            {plugin.icon ? `${plugin.icon as string} ` : ""}{plugin.name}
          </span>
          <span style={styles.badge}>{plugin.id}</span>
          {plugin.requires && plugin.requires.length > 0 && (
            <span style={{ ...styles.badge, color: "#f59e0b" }}>
              deps: {plugin.requires.length}
            </span>
          )}
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            style={styles.btn}
            onClick={() => setExpanded(!expanded)}
            data-testid={`expand-plugin-${plugin.id}`}
          >
            {expanded ? "Collapse" : "Details"}
          </button>
          <button
            style={styles.btnDanger}
            onClick={onRemove}
            data-testid={`remove-plugin-${plugin.id}`}
          >
            Remove
          </button>
        </div>
      </div>

      {expanded && <ContributionSummary plugin={plugin} />}
    </div>
  );
}

// ── Register Plugin Form ───────────────────────────────────────────────────

function RegisterPluginForm({ onRegister }: { onRegister: (plugin: PrismPlugin) => void }) {
  const [name, setName] = useState("");
  const [id, setId] = useState("");

  const handleCreate = useCallback(() => {
    if (!name.trim() || !id.trim()) return;

    const plugin: PrismPlugin = {
      id: pluginId(id.trim()),
      name: name.trim(),
      contributes: {
        commands: [{ id: `${id.trim()}.hello`, label: `${name.trim()}: Hello`, category: "Plugins", action: `${id.trim()}.hello` }],
      },
    };

    onRegister(plugin);
    setName("");
    setId("");
  }, [name, id, onRegister]);

  return (
    <div style={styles.card} data-testid="register-plugin-form">
      <div style={styles.sectionTitle}>Register Plugin</div>
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Plugin ID..."
          value={id}
          onChange={(e) => setId(e.target.value)}
          data-testid="plugin-id-input"
        />
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Plugin name..."
          value={name}
          onChange={(e) => setName(e.target.value)}
          data-testid="plugin-name-input"
        />
        <button
          style={styles.btnPrimary}
          onClick={handleCreate}
          data-testid="register-plugin-btn"
        >
          Register
        </button>
      </div>
    </div>
  );
}

// ── Main Panel ─────────────────────────────────────────────────────────────

export function PluginPanel() {
  const kernel = useKernel();
  const { plugins, register, unregister } = usePlugins();
  const [tab, setTab] = useState<"installed" | "contributions">("installed");

  const handleRegister = useCallback(
    (plugin: PrismPlugin) => {
      register(plugin);
      kernel.notifications.add({
        title: `Plugin registered: ${plugin.name}`,
        kind: "success",
      });
    },
    [register, kernel],
  );

  const handleRemove = useCallback(
    (id: string, name: string) => {
      unregister(id);
      kernel.notifications.add({
        title: `Plugin removed: ${name}`,
        kind: "info",
      });
    },
    [unregister, kernel],
  );

  // Gather all contributions across plugins
  const allCommands = kernel.plugins.commands.all();
  const allViews = kernel.plugins.views.all();

  return (
    <div style={styles.container} data-testid="plugin-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Plugins</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {plugins.length} registered
        </span>
      </div>

      <div style={styles.tabRow as React.CSSProperties}>
        <button
          style={{ ...styles.btn, background: tab === "installed" ? "#094771" : "#333" }}
          onClick={() => setTab("installed")}
          data-testid="plugin-tab-installed"
        >
          Installed ({plugins.length})
        </button>
        <button
          style={{ ...styles.btn, background: tab === "contributions" ? "#094771" : "#333" }}
          onClick={() => setTab("contributions")}
          data-testid="plugin-tab-contributions"
        >
          Contributions
        </button>
      </div>

      {tab === "installed" && (
        <>
          <RegisterPluginForm onRegister={handleRegister} />
          {plugins.length === 0 && (
            <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
              No plugins registered. Add one above.
            </div>
          )}
          {plugins.map((p) => (
            <PluginCard
              key={p.id}
              plugin={p}
              onRemove={() => handleRemove(p.id, p.name)}
            />
          ))}
        </>
      )}

      {tab === "contributions" && (
        <div data-testid="plugin-contributions-list">
          <div style={styles.sectionTitle}>Commands ({allCommands.length})</div>
          {allCommands.length === 0 ? (
            <div style={{ ...styles.meta, padding: "0.375rem 0" }}>No commands contributed.</div>
          ) : (
            allCommands.map((cmd) => (
              <div key={cmd.id} style={styles.card}>
                <span style={{ color: "#e5e5e5", fontSize: "0.8125rem" }}>{cmd.label}</span>
                <span style={styles.badge}>{cmd.id}</span>
                {cmd.shortcut && (
                  <span style={{ ...styles.badge, color: "#4a9eff" }}>
                    {cmd.shortcut}
                  </span>
                )}
              </div>
            ))
          )}

          <div style={styles.sectionTitle}>Views ({allViews.length})</div>
          {allViews.length === 0 ? (
            <div style={{ ...styles.meta, padding: "0.375rem 0" }}>No views contributed.</div>
          ) : (
            allViews.map((view) => (
              <div key={view.id} style={styles.card}>
                <span style={{ color: "#e5e5e5", fontSize: "0.8125rem" }}>{view.label ?? view.id}</span>
                <span style={styles.badge}>{view.zone}</span>
              </div>
            ))
          )}
        </div>
      )}
    </div>
  );
}
