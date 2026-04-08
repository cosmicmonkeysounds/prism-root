/**
 * Vault Panel — workspace/vault roster manager.
 *
 * Browse, add, remove, pin, and open vaults from the VaultRoster.
 * Displays vault metadata, timestamps, and collection counts.
 */

import { useState, useCallback, useMemo } from "react";
import { useVaultRoster, useKernel } from "../kernel/index.js";
import type { RosterEntry } from "@prism/core/discovery";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
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
  searchBar: {
    marginBottom: "0.75rem",
  },
} as const;

// ── Vault Card ─────────────────────────────────────────────────────────────

function VaultCard({
  vault,
  onRemove,
  onPin,
  onOpen,
}: {
  vault: RosterEntry;
  onRemove: () => void;
  onPin: () => void;
  onOpen: () => void;
}) {
  return (
    <div style={styles.card} data-testid={`vault-${vault.id}`}>
      <div style={styles.cardHeader as React.CSSProperties}>
        <div>
          <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
            {vault.pinned ? "\u2605 " : ""}{vault.name}
          </span>
          {vault.visibility && (
            <span style={{
              ...styles.badge,
              color: vault.visibility === "public" ? "#22c55e"
                : vault.visibility === "team" ? "#3b82f6"
                : "#888",
            }}>
              {vault.visibility}
            </span>
          )}
          {vault.tags?.map((tag) => (
            <span key={tag} style={{ ...styles.badge, color: "#f59e0b" }}>{tag}</span>
          ))}
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button
            style={styles.btn}
            onClick={onPin}
            data-testid={`pin-vault-${vault.id}`}
          >
            {vault.pinned ? "Unpin" : "Pin"}
          </button>
          <button
            style={styles.btnPrimary}
            onClick={onOpen}
            data-testid={`open-vault-${vault.id}`}
          >
            Open
          </button>
          <button
            style={styles.btnDanger}
            onClick={onRemove}
            data-testid={`remove-vault-${vault.id}`}
          >
            Remove
          </button>
        </div>
      </div>
      <div style={styles.meta}>
        {vault.path}
        {vault.description && ` — ${vault.description}`}
      </div>
      <div style={styles.meta}>
        Added: {new Date(vault.addedAt).toLocaleDateString()}
        {" | "}Last opened: {new Date(vault.lastOpenedAt).toLocaleDateString()}
        {vault.collectionCount !== undefined && ` | ${vault.collectionCount} collection(s)`}
      </div>
    </div>
  );
}

// ── Add Vault Form ─────────────────────────────────────────────────────────

function AddVaultForm({ onAdd }: { onAdd: (name: string, path: string) => void }) {
  const [name, setName] = useState("");
  const [path, setPath] = useState("");

  const handleAdd = useCallback(() => {
    if (!name.trim() || !path.trim()) return;
    onAdd(name.trim(), path.trim());
    setName("");
    setPath("");
  }, [name, path, onAdd]);

  return (
    <div style={styles.card} data-testid="add-vault-form">
      <div style={styles.sectionTitle}>Add Vault</div>
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Vault name..."
          value={name}
          onChange={(e) => setName(e.target.value)}
          data-testid="vault-name-input"
        />
        <input
          style={{ ...styles.input, flex: 2 }}
          placeholder="/path/to/vault..."
          value={path}
          onChange={(e) => setPath(e.target.value)}
          data-testid="vault-path-input"
        />
        <button
          style={styles.btnPrimary}
          onClick={handleAdd}
          data-testid="add-vault-btn"
        >
          Add
        </button>
      </div>
    </div>
  );
}

// ── Main Panel ─────────────────────────────────────────────────────────────

export function VaultPanel() {
  const kernel = useKernel();
  const { vaults, addVault, removeVault, pinVault, touchVault } = useVaultRoster();
  const [search, setSearch] = useState("");

  const filteredVaults = useMemo(() => {
    if (!search.trim()) return vaults;
    const q = search.toLowerCase();
    return vaults.filter(
      (v) =>
        v.name.toLowerCase().includes(q) ||
        v.path.toLowerCase().includes(q) ||
        v.description?.toLowerCase().includes(q),
    );
  }, [vaults, search]);

  const pinnedVaults = useMemo(() => filteredVaults.filter((v) => v.pinned), [filteredVaults]);
  const unpinnedVaults = useMemo(() => filteredVaults.filter((v) => !v.pinned), [filteredVaults]);

  const handleAdd = useCallback(
    (name: string, path: string) => {
      const id = `vault_${Date.now().toString(36)}`;
      addVault({
        id,
        name,
        path,
        lastOpenedAt: new Date().toISOString(),
        pinned: false,
      });
      kernel.notifications.add({ title: `Vault added: ${name}`, kind: "success" });
    },
    [addVault, kernel],
  );

  const handleRemove = useCallback(
    (id: string, name: string) => {
      removeVault(id);
      kernel.notifications.add({ title: `Vault removed: ${name}`, kind: "info" });
    },
    [removeVault, kernel],
  );

  const handlePin = useCallback(
    (id: string, currentlyPinned: boolean) => {
      pinVault(id, !currentlyPinned);
    },
    [pinVault],
  );

  const handleOpen = useCallback(
    (id: string, name: string) => {
      touchVault(id);
      kernel.notifications.add({ title: `Opened vault: ${name}`, kind: "info" });
    },
    [touchVault, kernel],
  );

  return (
    <div style={styles.container} data-testid="vault-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Vaults</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {vaults.length} vault(s)
        </span>
      </div>

      <div style={styles.searchBar}>
        <input
          style={styles.input}
          placeholder="Search vaults..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          data-testid="vault-search-input"
        />
      </div>

      <AddVaultForm onAdd={handleAdd} />

      {pinnedVaults.length > 0 && (
        <>
          <div style={styles.sectionTitle}>Pinned ({pinnedVaults.length})</div>
          {pinnedVaults.map((v) => (
            <VaultCard
              key={v.id}
              vault={v}
              onRemove={() => handleRemove(v.id, v.name)}
              onPin={() => handlePin(v.id, v.pinned)}
              onOpen={() => handleOpen(v.id, v.name)}
            />
          ))}
        </>
      )}

      <div style={styles.sectionTitle}>
        {pinnedVaults.length > 0 ? "Other" : "All"} ({unpinnedVaults.length})
      </div>
      {unpinnedVaults.length === 0 && pinnedVaults.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
          No vaults yet. Add one above.
        </div>
      )}
      {unpinnedVaults.map((v) => (
        <VaultCard
          key={v.id}
          vault={v}
          onRemove={() => handleRemove(v.id, v.name)}
          onPin={() => handlePin(v.id, v.pinned)}
          onOpen={() => handleOpen(v.id, v.name)}
        />
      ))}
    </div>
  );
}


// ── Lens registration ──────────────────────────────────────────────────────

export const VAULT_LENS_ID = lensId("vault");

export const vaultLensManifest: LensManifest = {

  id: VAULT_LENS_ID,
  name: "Vaults",
  icon: "\uD83D\uDD12",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-vaults", name: "Switch to Vaults", shortcut: ["w"], section: "Navigation" }],
  },
};

export const vaultLensBundle: LensBundle = defineLensBundle(
  vaultLensManifest,
  VaultPanel,
);
