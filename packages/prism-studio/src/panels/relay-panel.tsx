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

import { useState, useCallback, useEffect } from "react";
import { useKernel, useRelay } from "../kernel/index.js";
import type { StudioKernel, RelayEntry, RelayManager, DeployedPortal } from "../kernel/index.js";
import type { PortalLevel } from "@prism/core/relay";

import { lensId } from "@prism/core/lens";
import type { LensManifest } from "@prism/core/lens";
import { defineLensBundle, type LensBundle } from "../lenses/bundle.js";
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

function AddRelayForm({ onAdd, manager }: { onAdd: (name: string, url: string) => void; manager: RelayManager }) {
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [discovered, setDiscovered] = useState<{ did: string; modules: string[]; mode: string } | null>(null);

  const handleSubmit = useCallback(() => {
    if (!name.trim() || !url.trim()) return;
    onAdd(name.trim(), url.trim());
    setName("");
    setUrl("");
    setDiscovered(null);
  }, [name, url, onAdd]);

  const handleProbe = useCallback(async () => {
    if (!url.trim()) return;
    const info = await manager.discoverRelay(url.trim());
    if (info) {
      if (!name.trim()) setName(info.mode ?? "Relay");
      setDiscovered(info);
    } else {
      setDiscovered(null);
    }
  }, [url, name, manager]);

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
          onBlur={handleProbe}
        />
        <button style={styles.button} onClick={handleSubmit}>
          Add Relay
        </button>
      </div>
      {discovered && (
        <div style={{ ...styles.label, color: "#48bb78", marginBottom: "0.375rem" }}>
          Discovered: {discovered.mode} relay | Modules: {discovered.modules.join(", ") || "none"} | DID: {discovered.did.slice(0, 24)}...
        </div>
      )}
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
  onSelect,
  isSelected,
}: {
  entry: RelayEntry;
  onConnect: (id: string) => void;
  onDisconnect: (id: string) => void;
  onRemove: (id: string) => void;
  onPublish: (relayId: string) => void;
  onViewPortals: (relayId: string) => void;
  onSelect: (relayId: string) => void;
  isSelected: boolean;
}) {
  return (
    <div style={{ ...styles.card, ...(isSelected ? { border: "1px solid #0e639c" } : {}) }}>
      <div style={{ ...styles.row, justifyContent: "space-between", marginBottom: "0.375rem" }}>
        <div style={{ ...styles.row, cursor: "pointer" }} onClick={() => onSelect(entry.id)}>
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

// ── Section Toggle ───────────────────────────────────────────────────────

function SectionToggle({ title, name, expanded, onToggle, children }: {
  title: string;
  name: string;
  expanded: string | null;
  onToggle: (name: string) => void;
  children: React.ReactNode;
}) {
  const isExpanded = expanded === name;
  return (
    <div style={{ marginBottom: "0.5rem" }}>
      <div
        style={{ ...styles.card, cursor: "pointer", display: "flex", justifyContent: "space-between", alignItems: "center" }}
        onClick={() => onToggle(name)}
      >
        <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 600 }}>{title}</span>
        <span style={{ color: "#888", fontSize: "0.75rem" }}>{isExpanded ? "\u25BC" : "\u25B6"}</span>
      </div>
      {isExpanded && children}
    </div>
  );
}

// ── Relay Health Section ─────────────────────────────────────────────────

function RelayHealthSection({ relayId, manager }: { relayId: string; manager: RelayManager }) {
  const [health, setHealth] = useState<Record<string, unknown> | null>(null);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const data = await manager.fetchHealth(relayId);
      setHealth(data);
    } catch { setHealth(null); }
    finally { setLoading(false); }
  }, [relayId, manager]);

  useEffect(() => { refresh(); }, [refresh]);

  if (!health) return <div style={styles.empty}>Loading health data...</div>;

  const uptime = typeof health["uptime"] === "number"
    ? `${Math.floor(health["uptime"] as number / 3600)}h ${Math.floor((health["uptime"] as number % 3600) / 60)}m`
    : "unknown";

  return (
    <div style={styles.card}>
      <div style={{ ...styles.row, justifyContent: "space-between" }}>
        <div>
          <div style={styles.label}>Uptime: <strong style={{ color: "#e5e5e5" }}>{uptime}</strong></div>
          <div style={styles.label}>Connections: <strong style={{ color: "#e5e5e5" }}>{String(health["connections"] ?? 0)}</strong></div>
          <div style={styles.label}>Mode: <strong style={{ color: "#e5e5e5" }}>{String(health["mode"] ?? "unknown")}</strong></div>
        </div>
        <button style={styles.buttonOutline} onClick={refresh} disabled={loading}>
          {loading ? "Refreshing..." : "Refresh"}
        </button>
      </div>
    </div>
  );
}

// ── Collections Section ──────────────────────────────────────────────────

function CollectionsSection({ relayId, manager, kernel }: { relayId: string; manager: RelayManager; kernel: StudioKernel }) {
  const [collections, setCollections] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try { setCollections(await manager.listCollections(relayId)); }
    catch { setCollections([]); }
    finally { setLoading(false); }
  }, [relayId, manager]);

  useEffect(() => { refresh(); }, [refresh]);

  const handleDelete = useCallback(async (id: string) => {
    await manager.deleteCollection(relayId, id);
    setCollections(prev => prev.filter(c => c !== id));
    kernel.notifications.add({ title: `Deleted collection "${id}"`, kind: "success" });
  }, [relayId, manager, kernel]);

  return (
    <div style={styles.card}>
      {loading ? <div style={styles.empty}>Loading...</div> :
       collections.length === 0 ? <div style={styles.empty}>No collections hosted.</div> :
       collections.map(id => (
         <div key={id} style={{ ...styles.row, justifyContent: "space-between", padding: "0.25rem 0" }}>
           <span style={{ color: "#e5e5e5", fontSize: "0.875rem" }}>{id}</span>
           <button style={styles.buttonDanger} onClick={() => handleDelete(id)}>Delete</button>
         </div>
       ))}
    </div>
  );
}

// ── Federation Section ───────────────────────────────────────────────────

function FederationSection({ relayId, manager, kernel }: { relayId: string; manager: RelayManager; kernel: StudioKernel }) {
  const [peers, setPeers] = useState<Array<{ relayDid: string; url: string }>>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try { setPeers(await manager.listPeers(relayId)); }
    catch { setPeers([]); }
    finally { setLoading(false); }
  }, [relayId, manager]);

  useEffect(() => { refresh(); }, [refresh]);

  const handleBan = useCallback(async (did: string) => {
    await manager.banPeer(relayId, did);
    kernel.notifications.add({ title: `Banned peer ${did.slice(0, 20)}...`, kind: "success" });
  }, [relayId, manager, kernel]);

  const handleUnban = useCallback(async (did: string) => {
    await manager.unbanPeer(relayId, did);
    kernel.notifications.add({ title: `Unbanned peer ${did.slice(0, 20)}...`, kind: "success" });
  }, [relayId, manager, kernel]);

  return (
    <div style={styles.card}>
      {loading ? <div style={styles.empty}>Loading...</div> :
       peers.length === 0 ? <div style={styles.empty}>No federation peers.</div> :
       peers.map(p => (
         <div key={p.relayDid} style={{ ...styles.card, background: "#1e1e1e" }}>
           <div style={{ color: "#e5e5e5", fontSize: "0.8125rem", fontFamily: "monospace" }}>{p.relayDid}</div>
           <div style={styles.label}>{p.url}</div>
           <div style={{ ...styles.row, marginTop: "0.375rem", gap: "0.25rem" }}>
             <button style={styles.buttonDanger} onClick={() => handleBan(p.relayDid)}>Ban</button>
             <button style={styles.buttonOutline} onClick={() => handleUnban(p.relayDid)}>Unban</button>
           </div>
         </div>
       ))}
    </div>
  );
}

// ── Webhooks Section ─────────────────────────────────────────────────────

function WebhooksSection({ relayId, manager, kernel }: { relayId: string; manager: RelayManager; kernel: StudioKernel }) {
  const [webhooks, setWebhooks] = useState<Array<{ id: string; url: string; events: string[]; active: boolean }>>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try { setWebhooks(await manager.listWebhooks(relayId)); }
    catch { setWebhooks([]); }
    finally { setLoading(false); }
  }, [relayId, manager]);

  useEffect(() => { refresh(); }, [refresh]);

  const handleDelete = useCallback(async (id: string) => {
    await manager.deleteWebhook(relayId, id);
    setWebhooks(prev => prev.filter(w => w.id !== id));
    kernel.notifications.add({ title: `Deleted webhook "${id}"`, kind: "success" });
  }, [relayId, manager, kernel]);

  return (
    <div style={styles.card}>
      {loading ? <div style={styles.empty}>Loading...</div> :
       webhooks.length === 0 ? <div style={styles.empty}>No webhooks configured.</div> :
       webhooks.map(w => (
         <div key={w.id} style={{ ...styles.card, background: "#1e1e1e" }}>
           <div style={{ ...styles.row, justifyContent: "space-between" }}>
             <div>
               <div style={{ color: "#e5e5e5", fontSize: "0.8125rem", fontFamily: "monospace" }}>{w.url}</div>
               <div style={styles.label}>Events: {w.events.join(", ")}</div>
               <div style={styles.label}>Status: {w.active ? "Active" : "Inactive"}</div>
             </div>
             <button style={styles.buttonDanger} onClick={() => handleDelete(w.id)}>Delete</button>
           </div>
         </div>
       ))}
    </div>
  );
}

// ── Certificates Section ─────────────────────────────────────────────────

function CertificatesSection({ relayId, manager }: { relayId: string; manager: RelayManager }) {
  const [certs, setCerts] = useState<Array<{ domain: string; expiresAt: string; issuedAt: string }>>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try { setCerts(await manager.listCertificates(relayId)); }
    catch { setCerts([]); }
    finally { setLoading(false); }
  }, [relayId, manager]);

  useEffect(() => { refresh(); }, [refresh]);

  return (
    <div style={styles.card}>
      {loading ? <div style={styles.empty}>Loading...</div> :
       certs.length === 0 ? <div style={styles.empty}>No certificates.</div> :
       certs.map(c => {
         const expires = new Date(c.expiresAt);
         const now = new Date();
         const daysRemaining = Math.ceil((expires.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
         const expiryColor = daysRemaining < 14 ? "#fc8181" : daysRemaining < 30 ? "#ecc94b" : "#48bb78";

         return (
           <div key={c.domain} style={{ ...styles.card, background: "#1e1e1e" }}>
             <div style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 600 }}>{c.domain}</div>
             <div style={styles.label}>Issued: {new Date(c.issuedAt).toLocaleDateString()}</div>
             <div style={styles.label}>
               Expires: {expires.toLocaleDateString()}{" "}
               <strong style={{ color: expiryColor }}>({daysRemaining}d remaining)</strong>
             </div>
           </div>
         );
       })}
    </div>
  );
}

// ── Backup & Restore Section ─────────────────────────────────────────────

function BackupRestoreSection({ relayId, manager, kernel }: { relayId: string; manager: RelayManager; kernel: StudioKernel }) {
  const [loading, setLoading] = useState(false);

  const handleBackup = useCallback(async () => {
    setLoading(true);
    try {
      const data = await manager.backupRelay(relayId);
      const blob = new Blob([JSON.stringify(data, null, 2)], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "relay-backup.json";
      a.click();
      URL.revokeObjectURL(url);
      kernel.notifications.add({ title: "Backup downloaded", kind: "success" });
    } catch (err) {
      kernel.notifications.add({ title: "Backup failed", kind: "error", body: String(err) });
    } finally { setLoading(false); }
  }, [relayId, manager, kernel]);

  return (
    <div style={styles.card}>
      <div style={styles.row}>
        <button style={styles.button} onClick={handleBackup} disabled={loading}>
          {loading ? "Exporting..." : "Export Backup"}
        </button>
      </div>
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
  const [selectedRelayId, setSelectedRelayId] = useState<string | null>(null);
  const [expandedSection, setExpandedSection] = useState<string | null>(null);

  const toggleSection = useCallback((name: string) => {
    setExpandedSection(prev => prev === name ? null : name);
  }, []);

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
        <AddRelayForm onAdd={handleAdd} manager={manager} />
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
                onSelect={(id) => setSelectedRelayId(prev => prev === id ? null : id)}
                isSelected={selectedRelayId === entry.id}
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

      {/* Manage selected relay */}
      {selectedRelayId && (
        <div style={styles.section}>
          <div style={styles.sectionTitle}>Manage Relay</div>

          <SectionToggle title="Health" name="health" expanded={expandedSection} onToggle={toggleSection}>
            <RelayHealthSection relayId={selectedRelayId} manager={manager} />
          </SectionToggle>

          <SectionToggle title="Collections" name="collections" expanded={expandedSection} onToggle={toggleSection}>
            <CollectionsSection relayId={selectedRelayId} manager={manager} kernel={kernel} />
          </SectionToggle>

          <SectionToggle title="Federation Peers" name="federation" expanded={expandedSection} onToggle={toggleSection}>
            <FederationSection relayId={selectedRelayId} manager={manager} kernel={kernel} />
          </SectionToggle>

          <SectionToggle title="Webhooks" name="webhooks" expanded={expandedSection} onToggle={toggleSection}>
            <WebhooksSection relayId={selectedRelayId} manager={manager} kernel={kernel} />
          </SectionToggle>

          <SectionToggle title="Certificates" name="certs" expanded={expandedSection} onToggle={toggleSection}>
            <CertificatesSection relayId={selectedRelayId} manager={manager} />
          </SectionToggle>

          <SectionToggle title="Backup & Restore" name="backup" expanded={expandedSection} onToggle={toggleSection}>
            <BackupRestoreSection relayId={selectedRelayId} manager={manager} kernel={kernel} />
          </SectionToggle>
        </div>
      )}

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


// ── Lens registration ──────────────────────────────────────────────────────

export const RELAY_LENS_ID = lensId("relay");

export const relayLensManifest: LensManifest = {

  id: RELAY_LENS_ID,
  name: "Relay",
  icon: "\u21C6",
  category: "custom",
  contributes: {
    views: [{ slot: "main" }],
    commands: [{ id: "switch-relay", name: "Switch to Relay Manager", shortcut: ["r"], section: "Navigation" }],
  },
};

export const relayLensBundle: LensBundle = defineLensBundle(
  relayLensManifest,
  RelayPanel,
);
