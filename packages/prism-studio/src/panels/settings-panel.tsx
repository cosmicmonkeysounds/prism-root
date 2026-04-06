/**
 * Settings Panel — category-grouped configuration UI.
 *
 * Renders all registered SettingDefinitions from ConfigRegistry,
 * grouped by tag (ui, editor, sync, ai, notifications).
 * Changes are applied to the "user" scope via ConfigModel.set().
 */

import { useState, useCallback } from "react";
import { useKernel, useConfigSettings } from "../kernel/index.js";
import type { SettingDefinition } from "@prism/core/config";

// ── Styles ────────────────────────────────────────────────────────────────

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
  },
  section: {
    marginBottom: "1.5rem",
  },
  sectionTitle: {
    fontSize: "0.875rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.5rem",
    borderBottom: "1px solid #333",
    paddingBottom: "0.375rem",
  },
  settingRow: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    padding: "0.5rem 0",
    borderBottom: "1px solid #2a2a2a",
  },
  settingInfo: {
    flex: 1,
    marginRight: "1rem",
  },
  settingLabel: {
    color: "#e5e5e5",
    fontSize: "0.875rem",
  },
  settingDescription: {
    color: "#666",
    fontSize: "0.75rem",
    marginTop: "0.125rem",
  },
  settingKey: {
    color: "#555",
    fontSize: "0.6875rem",
    fontFamily: "monospace",
    marginTop: "0.125rem",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "8rem",
    outline: "none",
  },
  select: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    outline: "none",
  },
  toggle: {
    position: "relative" as const,
    width: "2.5rem",
    height: "1.25rem",
    borderRadius: "0.625rem",
    cursor: "pointer",
    transition: "background 0.2s",
    flexShrink: 0,
  },
  toggleKnob: {
    position: "absolute" as const,
    top: "0.125rem",
    width: "1rem",
    height: "1rem",
    borderRadius: "50%",
    background: "#fff",
    transition: "left 0.2s",
  },
  searchInput: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.5rem 0.75rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    width: "100%",
    outline: "none",
    marginBottom: "1rem",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.0625rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#333",
    color: "#888",
    marginLeft: "0.5rem",
  },
} as const;

// ── Tag metadata ──────────────────────────────────────────────────────────

const TAG_INFO: Record<string, { label: string; order: number }> = {
  ui: { label: "User Interface", order: 0 },
  editor: { label: "Editor", order: 1 },
  sync: { label: "Sync & Collaboration", order: 2 },
  ai: { label: "AI Features", order: 3 },
  notifications: { label: "Notifications", order: 4 },
};

// ── Toggle Component ──────────────────────────────────────────────────────

function Toggle({
  checked,
  onChange,
  testId,
}: {
  checked: boolean;
  onChange: (v: boolean) => void;
  testId?: string;
}) {
  return (
    <div
      data-testid={testId}
      style={{
        ...styles.toggle,
        background: checked ? "#0e639c" : "#555",
      }}
      onClick={() => onChange(!checked)}
      role="switch"
      aria-checked={checked}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onChange(!checked);
        }
      }}
    >
      <div
        style={{
          ...styles.toggleKnob,
          left: checked ? "1.375rem" : "0.125rem",
        }}
      />
    </div>
  );
}

// ── Setting Row Component ─────────────────────────────────────────────────

function SettingRow({ def }: { def: SettingDefinition }) {
  const kernel = useKernel();
  const currentValue = kernel.config.get(def.key);

  const handleChange = useCallback(
    (value: unknown) => {
      kernel.config.set(def.key, value, "user");
    },
    [kernel, def.key],
  );

  // Don't render secret settings
  if (def.secret) return null;

  return (
    <div style={styles.settingRow} data-testid={`setting-${def.key}`}>
      <div style={styles.settingInfo}>
        <div style={styles.settingLabel}>
          {def.label}
          {def.scopes && def.scopes.length === 1 && (
            <span style={styles.badge}>{def.scopes[0]}</span>
          )}
        </div>
        {def.description && (
          <div style={styles.settingDescription}>{def.description}</div>
        )}
        <div style={styles.settingKey}>{def.key}</div>
      </div>

      {def.type === "boolean" && (
        <Toggle
          checked={currentValue as boolean}
          onChange={(v) => handleChange(v)}
          testId={`toggle-${def.key}`}
        />
      )}

      {def.type === "select" && def.options && (
        <select
          style={styles.select}
          value={String(currentValue ?? def.default)}
          onChange={(e) => handleChange(e.target.value)}
          data-testid={`select-${def.key}`}
        >
          {def.options.map((opt) => (
            <option key={String(opt)} value={String(opt)}>
              {String(opt)}
            </option>
          ))}
        </select>
      )}

      {def.type === "number" && (
        <input
          style={styles.input}
          type="number"
          value={String(currentValue ?? def.default)}
          onChange={(e) => {
            const v = Number(e.target.value);
            if (!isNaN(v)) handleChange(v);
          }}
          data-testid={`number-${def.key}`}
        />
      )}

      {def.type === "string" && !def.secret && (
        <input
          style={styles.input}
          type="text"
          value={String(currentValue ?? def.default ?? "")}
          onChange={(e) => handleChange(e.target.value)}
          data-testid={`text-${def.key}`}
        />
      )}
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────

export function SettingsPanel() {
  const [search, setSearch] = useState("");
  const allSettings = useConfigSettings();

  // Group settings by first tag
  const grouped = new Map<string, SettingDefinition[]>();
  for (const def of allSettings) {
    if (def.secret) continue;
    const tag = def.tags?.[0] ?? "other";

    // Filter by search
    if (search) {
      const q = search.toLowerCase();
      const matches =
        def.label.toLowerCase().includes(q) ||
        def.key.toLowerCase().includes(q) ||
        (def.description ?? "").toLowerCase().includes(q);
      if (!matches) continue;
    }

    const list = grouped.get(tag);
    if (list) {
      list.push(def);
    } else {
      grouped.set(tag, [def]);
    }
  }

  // Sort groups by predefined order
  const sortedTags = [...grouped.keys()].sort((a, b) => {
    const orderA = TAG_INFO[a]?.order ?? 99;
    const orderB = TAG_INFO[b]?.order ?? 99;
    return orderA - orderB;
  });

  return (
    <div style={styles.container} data-testid="settings-panel">
      <div style={styles.header}>Settings</div>

      <input
        style={styles.searchInput}
        placeholder="Search settings..."
        value={search}
        onChange={(e) => setSearch(e.target.value)}
        data-testid="settings-search"
      />

      {sortedTags.length === 0 && (
        <div style={{ color: "#666", fontStyle: "italic" }}>
          No settings match your search.
        </div>
      )}

      {sortedTags.map((tag) => {
        const defs = grouped.get(tag);
        if (!defs || defs.length === 0) return null;
        const info = TAG_INFO[tag] ?? { label: tag };

        return (
          <div key={tag} style={styles.section} data-testid={`settings-group-${tag}`}>
            <div style={styles.sectionTitle}>{info.label}</div>
            {defs.map((def) => (
              <SettingRow key={def.key} def={def} />
            ))}
          </div>
        );
      })}
    </div>
  );
}
