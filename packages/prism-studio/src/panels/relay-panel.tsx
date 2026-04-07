/**
 * Relay Panel — manage connections to Prism Relay servers.
 *
 * Studio is a client-only SPA. This panel lets users:
 *   - Add/remove relay endpoints
 *   - Connect/disconnect via WebSocket
 *   - Publish collections as Sovereign Portals
 *   - View portal status and links
 *   - Sync collections to relays
 *
 * Relay servers are started/stopped via CLI, not from Studio.
 */

import { useState, useCallback } from "react";
import { useKernel, useRelay } from "../kernel/index.js";
import type { RelayEntry, DeployedPortal } from "../kernel/index.js";
import type { PortalLevel } from "@prism/core/relay";

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
  },
  card: {
    background: "#252526",
    border: "1px solid #333",
    borderRadius: "0.375rem",
    padding: "0.75rem",
    marginBottom: "0.5rem",
  },
  row: {
    display: "flex",
    alignItems: "center",
    gap: "0.5rem",
  },
  input: {
    background: "#333",
    border: "1px solid #444",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.5rem",
    color: "#e5e5e5",
    fontSize: "0.875rem",
    flex: 1,
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
  button: {
    background: "#0e639c",
    border: "none",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.75rem",
    color: "#fff",
    fontSize: "0.8125rem",
    cursor: "pointer",
    whiteSpace: "nowrap" as const,
  },
  buttonDanger: {
    background: "#c53030",
    border: "none",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.75rem",
    color: "#fff",
    fontSize: "0.8125rem",
    cursor: "pointer",
  },
  buttonOutline: {
    background: "transparent",
    border: "1px solid #555",
    borderRadius: "0.25rem",
    padding: "0.375rem 0.75rem",
    color: "#ccc",
    fontSize: "0.8125rem",
    cursor: "pointer",
  },
  statusDot: (status: string) => ({
    width: "0.5rem",
    height: "0.5rem",
    borderRadius: "50%",
    background:
      status === "connected" ? "#48bb78" :
      status === "connecting" ? "#ecc94b" :
      status === "error" ? "#fc8181" :
      "#666",
    flexShrink: 0,
  }),
  label: {
    fontSize: "0.75rem",
    color: "#888",
  },
  link: {
    color: "#60a5fa",
    fontSize: "0.8125rem",
    textDecoration: "none",
  },
  error: {
    color: "#fc8181",
    fontSize: "0.8125rem",
    marginTop: "0.25rem",
  },
  empty: {
    color: "#666",
    fontSize: "0.875rem",
    fontStyle: "italic",
    padding: "1rem 0",
  },
  badge: (level: number) => ({
    display: "inline-block",
    fontSize: "0.6875rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "9999px",
    background:
      level === 1 ? "#2d3748" :
      level === 2 ? "#2c5282" :
      level === 3 ? "#553c9a" :
      "#744210",
    color: "#e5e5e5",
    marginLeft: "0.375rem",
  }),
} as const;

// ── Add Relay Form ────────────────────────────────────────────────────────

function AddRelayForm({ onAdd }: { onAdd: (name: string, url: string) => void }) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");

  const handleSubmit = useCallback(() => {
    if (!name.trim() || !url.trim()) return;
    onAdd(name.trim(), url.trim());
    setName("");
    setUrl("");
  }, [name, url, onAdd]);

  return (
    <div style={styles.card}>
      <div style={{ ...styles.row, marginBottom: "0.5rem" }}>
        <input
          style={{ ...styles.input, flex: "0 0 8rem" }}
          placeholder="Name"
          value={name}
          onChange={(e) => setName(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
        />
        <input
          style={styles.input}
          placeholder="https://relay.example.com"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && handleSubmit()}
        />
        <button style={styles.button} onClick={handleSubmit}>
          Add Relay
        </button>
      </div>
      <div style={styles.label}>
        Start relays via CLI: prism-relay --mode server --port 4444
      </div>
    </div>
  );
}

// ── Relay Card ────────────────────────────────────────────────────────────

function RelayCard({
  entry,
  onConnect,
  onDisconnect,
  onRemove,
  onPublish,
  onViewPortals,
}: {
  entry: RelayEntry;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onRemove: (id: string) => void;
  onPublish: (relayId: string) => void;
  onViewPortals: (relayId: string) => void;
}) {
  return (
    <div style={styles.card}>
      <div style={{ ...styles.row, justifyContent: "space-between", marginBottom: "0.375rem" }}>
        <div style={styles.row}>
          <span style={styles.statusDot(entry.status)} />
          <strong style={{ color: "#e5e5e5" }}>{entry.name}</strong>
          <span style={styles.label}>{entry.status}</span>
        </div>
        <div style={styles.row}>
          {entry.status === "disconnected" || entry.status === "error" ? (
            <button style={styles.button} onClick={() => onConnect(entry.id)}>
              Connect
            </button>
          ) : entry.status === "connected" ? (
            <button style={styles.buttonOutline} onClick={() => onDisconnect(entry.id)}>
              Disconnect
            </button>
          ) : null}
          <button style={styles.buttonDanger} onClick={() => onRemove(entry.id)}>
            Remove
          </button>
        </div>
      </div>
      <div style={styles.label}>{entry.url}</div>
      {entry.relayDid && (
        <div style={styles.label}>DID: {entry.relayDid}</div>
      )}
      {entry.modules.length > 0 && (
        <div style={{ ...styles.label, marginTop: "0.25rem" }}>
          Modules: {entry.modules.join(", ")}
        </div>
      )}
      {entry.error && (
        <div style={styles.error}>{entry.error}</div>
      )}
      {entry.status === "connected" && (
        <div style={{ ...styles.row, marginTop: "0.5rem", gap: "0.375rem" }}>
          <button style={styles.button} onClick={() => onPublish(entry.id)}>
            Publish Portal
          </button>
          <button style={styles.buttonOutline} onClick={() => onViewPortals(entry.id)}>
            View Portals
          </button>
        </div>
      )}
    </div>
  );
}

// ── Publish Portal Dialog ─────────────────────────────────────────────────

function PublishDialog({
  relayId,
  onPublish,
  onCancel,
}: {
  relayId: string;
  onPublish: (opts: {
    relayId: string;
    name: string;
    level: PortalLevel;
    basePath: string;
  }) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState("");
  const [level, setLevel] = useState<PortalLevel>(1);
  const [basePath, setBasePath] = useState("/");

  const handlePublish = useCallback(() => {
    if (!name.trim()) return;
    onPublish({ relayId, name: name.trim(), level, basePath });
  }, [relayId, name, level, basePath, onPublish]);

  return (
    <div style={{ ...styles.card, border: "1px solid #0e639c" }}>
      <div style={{ ...styles.sectionTitle, color: "#60a5fa" }}>Publish Sovereign Portal</div>
      <div style={{ display: "flex", flexDirection: "column", gap: "0.5rem" }}>
        <div style={styles.row}>
          <span style={{ ...styles.label, width: "5rem" }}>Name</span>
          <input
            style={styles.input}
            placeholder="My Portal"
            value={name}
            onChange={(e) => setName(e.target.value)}
          />
        </div>
        <div style={styles.row}>
          <span style={{ ...styles.label, width: "5rem" }}>Level</span>
          <select
            style={styles.select}
            value={level}
            onChange={(e) => setLevel(Number(e.target.value) as PortalLevel)}
          >
            <option value={1}>Level 1 - Read-Only</option>
            <option value={2}>Level 2 - Live Dashboard</option>
            <option value={3}>Level 3 - Interactive Forms</option>
            <option value={4}>Level 4 - Full App</option>
          </select>
        </div>
        <div style={styles.row}>
          <span style={{ ...styles.label, width: "5rem" }}>Path</span>
          <input
            style={styles.input}
            placeholder="/"
            value={basePath}
            onChange={(e) => setBasePath(e.target.value)}
          />
        </div>
        <div style={{ ...styles.row, justifyContent: "flex-end", marginTop: "0.25rem" }}>
          <button style={styles.buttonOutline} onClick={onCancel}>Cancel</button>
          <button style={styles.button} onClick={handlePublish}>Publish</button>
        </div>
      </div>
    </div>
  );
}

// ── Portal List ───────────────────────────────────────────────────────────

function PortalList({
  portals,
  onUnpublish,
  onClose,
}: {
  portals: DeployedPortal[];
  onUnpublish: (relayId: string, portalId: string) => void;
  onClose: () => void;
}) {
  return (
    <div style={{ ...styles.card, border: "1px solid #553c9a" }}>
      <div style={{ ...styles.row, justifyContent: "space-between", marginBottom: "0.5rem" }}>
        <span style={{ ...styles.sectionTitle, marginBottom: 0 }}>Portals</span>
        <button style={styles.buttonOutline} onClick={onClose}>Close</button>
      </div>
      {portals.length === 0 ? (
        <div style={styles.empty}>No portals deployed on this relay.</div>
      ) : (
        portals.map((p) => (
          <div key={p.manifest.portalId} style={{ ...styles.card, background: "#1e1e1e" }}>
            <div style={{ ...styles.row, justifyContent: "space-between" }}>
              <div>
                <strong style={{ color: "#e5e5e5" }}>{p.manifest.name}</strong>
                <span style={styles.badge(p.manifest.level)}>L{p.manifest.level}</span>
              </div>
              <button
                style={styles.buttonDanger}
                onClick={() => onUnpublish(p.relayId, p.manifest.portalId)}
              >
                Unpublish
              </button>
            </div>
            <div style={styles.label}>
              Path: {p.manifest.basePath} | Collection: {p.manifest.collectionId}
            </div>
            <a
              href={p.viewUrl}
              target="_blank"
              rel="noopener noreferrer"
              style={styles.link}
            >
              {p.viewUrl}
            </a>
          </div>
        ))
      )}
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────

export function RelayPanel() {
  const kernel = useKernel();
  const { relays, manager } = useRelay();

  const [publishingRelayId, setPublishingRelayId] = useState<string | null>(null);
  const [viewingPortalsRelayId, setViewingPortalsRelayId] = useState<string | null>(null);
  const [portals, setPortals] = useState<DeployedPortal[]>([]);
  const [loading, setLoading] = useState(false);

  const handleAdd = useCallback((name: string, url: string) => {
    manager.addRelay(name, url);
    kernel.notifications.add({ title: `Added relay "${name}"`, kind: "info" });
  }, [manager, kernel]);

  const handleConnect = useCallback(async (relayId: string) => {
    // Identity comes from the daemon via IPC — not available in SPA mode.
    // Relay connections are started via CLI; Studio connects as a client.
    const entry = manager.getRelay(relayId);
    kernel.notifications.add({
      title: `Connect "${entry?.name ?? relayId}" via CLI`,
      kind: "info",
      body: `Relay connections require the daemon. Use: prism-relay --mode dev --port 4444`,
    });
  }, [kernel, manager]);

  const handleDisconnect = useCallback((relayId: string) => {
    manager.disconnect(relayId);
    kernel.notifications.add({ title: "Disconnected from relay", kind: "info" });
  }, [manager, kernel]);

  const handleRemove = useCallback((relayId: string) => {
    const entry = manager.getRelay(relayId);
    manager.removeRelay(relayId);
    kernel.notifications.add({ title: `Removed relay "${entry?.name ?? relayId}"`, kind: "info" });
  }, [manager, kernel]);

  const handlePublish = useCallback(async (opts: {
    relayId: string;
    name: string;
    level: PortalLevel;
    basePath: string;
  }) => {
    setLoading(true);
    try {
      const result = await manager.publishPortal({
        relayId: opts.relayId,
        collectionId: "default",
        name: opts.name,
        level: opts.level,
        basePath: opts.basePath,
        isPublic: true,
      });
      kernel.notifications.add({
        title: `Published "${result.manifest.name}"`,
        kind: "success",
        body: result.viewUrl,
      });
      setPublishingRelayId(null);
    } catch (err) {
      kernel.notifications.add({
        title: "Publish failed",
        kind: "error",
        body: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoading(false);
    }
  }, [manager, kernel]);

  const handleViewPortals = useCallback(async (relayId: string) => {
    setLoading(true);
    try {
      const result = await manager.listPortals(relayId);
      setPortals(result);
      setViewingPortalsRelayId(relayId);
    } catch (err) {
      kernel.notifications.add({
        title: "Failed to load portals",
        kind: "error",
        body: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setLoading(false);
    }
  }, [manager, kernel]);

  const handleUnpublish = useCallback(async (relayId: string, portalId: string) => {
    try {
      await manager.unpublishPortal(relayId, portalId);
      setPortals((prev) => prev.filter((p) => p.manifest.portalId !== portalId));
      kernel.notifications.add({ title: "Portal unpublished", kind: "success" });
    } catch (err) {
      kernel.notifications.add({
        title: "Unpublish failed",
        kind: "error",
        body: err instanceof Error ? err.message : String(err),
      });
    }
  }, [manager, kernel]);

  const connectedCount = relays.filter((r) => r.status === "connected").length;

  return (
    <div style={styles.container}>
      <div style={styles.header}>Relay Manager</div>

      {/* Summary */}
      <div style={{ ...styles.label, marginBottom: "1rem" }}>
        {relays.length} relay{relays.length !== 1 ? "s" : ""} configured
        {connectedCount > 0 && ` | ${connectedCount} connected`}
        {loading && " | Loading..."}
      </div>

      {/* Add Relay */}
      <div style={styles.section}>
        <div style={styles.sectionTitle}>Add Relay</div>
        <AddRelayForm onAdd={handleAdd} />
      </div>

      {/* Relay List */}
      <div style={styles.section}>
        <div style={styles.sectionTitle}>Relays</div>
        {relays.length === 0 ? (
          <div style={styles.empty}>
            No relays configured. Add one above, or start a relay via CLI.
          </div>
        ) : (
          relays.map((entry) => (
            <div key={entry.id}>
              <RelayCard
                entry={entry}
                onConnect={handleConnect}
                onDisconnect={handleDisconnect}
                onRemove={handleRemove}
                onPublish={(id) => setPublishingRelayId(id)}
                onViewPortals={handleViewPortals}
              />
              {publishingRelayId === entry.id && (
                <PublishDialog
                  relayId={entry.id}
                  onPublish={handlePublish}
                  onCancel={() => setPublishingRelayId(null)}
                />
              )}
              {viewingPortalsRelayId === entry.id && (
                <PortalList
                  portals={portals}
                  onUnpublish={handleUnpublish}
                  onClose={() => setViewingPortalsRelayId(null)}
                />
              )}
            </div>
          ))
        )}
      </div>

      {/* CLI reference */}
      <div style={styles.section}>
        <div style={styles.sectionTitle}>CLI Reference</div>
        <div style={styles.card}>
          <div style={{ ...styles.label, fontFamily: "monospace", lineHeight: 1.8 }}>
            <div># Start a dev relay locally</div>
            <div style={{ color: "#e5e5e5" }}>prism-relay --mode dev --port 4444</div>
            <div style={{ marginTop: "0.5rem" }}># Start a production relay</div>
            <div style={{ color: "#e5e5e5" }}>prism-relay --mode server --port 443</div>
            <div style={{ marginTop: "0.5rem" }}># Start a P2P federated peer</div>
            <div style={{ color: "#e5e5e5" }}>prism-relay --mode p2p --port 8080</div>
            <div style={{ marginTop: "0.5rem" }}># View relay status</div>
            <div style={{ color: "#e5e5e5" }}>curl http://localhost:4444/api/status</div>
          </div>
        </div>
      </div>
    </div>
  );
}
