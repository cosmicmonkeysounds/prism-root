/**
 * Trust Panel — sovereign immune system dashboard.
 *
 * Peer reputation graph, content flags, schema validation,
 * sandbox policy viewer, Shamir secret sharing, escrow deposits.
 */

import { useState, useCallback, useMemo } from "react";
import { useTrust, useKernel } from "../kernel/index.js";
import type { PeerReputation, TrustLevel } from "@prism/core/trust";

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
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
  },
  tabs: {
    display: "flex",
    gap: 0,
    marginBottom: "0.75rem",
    borderBottom: "1px solid #333",
  },
  tab: {
    padding: "6px 12px",
    fontSize: "0.75rem",
    background: "transparent",
    border: "none",
    borderBottom: "2px solid transparent",
    color: "#888",
    cursor: "pointer",
  },
  tabActive: {
    padding: "6px 12px",
    fontSize: "0.75rem",
    background: "transparent",
    border: "none",
    borderBottom: "2px solid #4fc1ff",
    color: "#e5e5e5",
    cursor: "pointer",
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
  mono: {
    fontFamily: "monospace",
    fontSize: "0.6875rem",
    color: "#4fc1ff",
    wordBreak: "break-all" as const,
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
} as const;

// ── Trust level colors ──────────────────────────────────────────────────────

const TRUST_COLORS: Record<TrustLevel, string> = {
  "unknown": "#888",
  "untrusted": "#f87171",
  "neutral": "#f59e0b",
  "trusted": "#22c55e",
  "highly-trusted": "#4fc1ff",
};

function TrustBadge({ level }: { level: TrustLevel }) {
  return (
    <span style={{
      display: "inline-block",
      fontSize: "0.625rem",
      padding: "0.125rem 0.375rem",
      borderRadius: "0.25rem",
      background: "#252526",
      color: TRUST_COLORS[level],
      border: `1px solid ${TRUST_COLORS[level]}44`,
    }}>
      {level}
    </span>
  );
}

// ── Peer Card ───────────────────────────────────────────────────────────────

function PeerCard({
  peer,
  onTrust,
  onDistrust,
  onBan,
  onUnban,
}: {
  peer: PeerReputation;
  onTrust: () => void;
  onDistrust: () => void;
  onBan: () => void;
  onUnban: () => void;
}) {
  return (
    <div
      style={{
        ...styles.card,
        borderColor: peer.banned ? "#5c2020" : "#333",
      }}
      data-testid={`peer-${peer.peerId}`}
    >
      <div style={styles.cardHeader as React.CSSProperties}>
        <div>
          <span style={{ color: "#e5e5e5", fontSize: "0.875rem", fontWeight: 500 }}>
            {peer.peerId}
          </span>
          <span style={{ marginLeft: 8 }}>
            <TrustBadge level={peer.trustLevel} />
          </span>
          {peer.banned && (
            <span style={{
              display: "inline-block",
              fontSize: "0.625rem",
              padding: "0.125rem 0.375rem",
              borderRadius: "0.25rem",
              background: "#3b1111",
              color: "#f87171",
              marginLeft: "0.375rem",
            }}>
              Banned
            </span>
          )}
        </div>
        <div style={{ display: "flex", gap: 4 }}>
          <button style={styles.btn} onClick={onTrust} data-testid={`trust-${peer.peerId}`}>+</button>
          <button style={styles.btn} onClick={onDistrust} data-testid={`distrust-${peer.peerId}`}>-</button>
          {peer.banned ? (
            <button style={styles.btn} onClick={onUnban} data-testid={`unban-${peer.peerId}`}>Unban</button>
          ) : (
            <button style={styles.btnDanger} onClick={onBan} data-testid={`ban-${peer.peerId}`}>Ban</button>
          )}
        </div>
      </div>
      <div style={styles.meta}>
        Score: {peer.score} | +{peer.positiveInteractions} / -{peer.negativeInteractions}
        {peer.banReason && ` | Reason: ${peer.banReason}`}
        {" | "}Last seen: {new Date(peer.lastSeenAt).toLocaleString()}
      </div>
    </div>
  );
}

// ── Peers Tab ───────────────────────────────────────────────────────────────

function PeersTab() {
  const { peers, trustPeer, distrustPeer, banPeer, unbanPeer } = useTrust();
  const [newPeerId, setNewPeerId] = useState("");

  const handleAddPeer = useCallback(() => {
    if (!newPeerId.trim()) return;
    trustPeer(newPeerId.trim());
    setNewPeerId("");
  }, [newPeerId, trustPeer]);

  const handleBan = useCallback(
    (peerId: string) => {
      banPeer(peerId, "Banned from Trust panel");
    },
    [banPeer],
  );

  return (
    <div>
      <div style={{ display: "flex", gap: 6, marginBottom: "0.75rem" }}>
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Peer ID or DID..."
          value={newPeerId}
          onChange={(e) => setNewPeerId(e.target.value)}
          data-testid="add-peer-input"
        />
        <button style={styles.btnPrimary} onClick={handleAddPeer} data-testid="add-peer-btn">
          Add Peer
        </button>
      </div>

      {peers.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
          No peers tracked yet.
        </div>
      )}
      {peers.map((peer) => (
        <PeerCard
          key={peer.peerId}
          peer={peer}
          onTrust={() => trustPeer(peer.peerId)}
          onDistrust={() => distrustPeer(peer.peerId)}
          onBan={() => handleBan(peer.peerId)}
          onUnban={() => unbanPeer(peer.peerId)}
        />
      ))}
    </div>
  );
}

// ── Validation Tab ──────────────────────────────────────────────────────────

function ValidationTab() {
  const { validateImport } = useTrust();
  const [jsonInput, setJsonInput] = useState("");
  const [result, setResult] = useState<{ valid: boolean; issues: Array<{ path: string; message: string; severity: string; rule: string }> } | null>(null);

  const handleValidate = useCallback(() => {
    if (!jsonInput.trim()) return;
    try {
      const data = JSON.parse(jsonInput);
      setResult(validateImport(data));
    } catch {
      setResult({ valid: false, issues: [{ path: "$", message: "Invalid JSON", severity: "error", rule: "parse" }] });
    }
  }, [jsonInput, validateImport]);

  return (
    <div>
      <div style={styles.sectionTitle}>Schema Validation</div>
      <textarea
        style={{
          background: "#333",
          border: "1px solid #444",
          borderRadius: "0.25rem",
          padding: "0.375rem 0.5rem",
          color: "#e5e5e5",
          fontSize: "0.75rem",
          fontFamily: "monospace",
          width: "100%",
          outline: "none",
          boxSizing: "border-box" as const,
          resize: "vertical" as const,
          height: 100,
          marginBottom: 6,
        }}
        placeholder='Paste JSON to validate...\ne.g. {"data": {"fields": [...]}}'
        value={jsonInput}
        onChange={(e) => setJsonInput(e.target.value)}
        data-testid="validate-json-input"
      />
      <button style={styles.btnPrimary} onClick={handleValidate} data-testid="validate-btn">
        Validate
      </button>

      {result && (
        <div style={{ ...styles.card, marginTop: "0.75rem", borderColor: result.valid ? "#1a4731" : "#5c2020" }} data-testid="validation-result">
          <div style={{
            fontWeight: 600,
            color: result.valid ? "#22c55e" : "#f87171",
            marginBottom: "0.375rem",
          }}>
            {result.valid ? "Valid — safe to import" : `Invalid — ${result.issues.length} issue(s)`}
          </div>
          {result.issues.map((issue, i) => (
            <div key={i} style={styles.meta}>
              <span style={{ color: issue.severity === "error" ? "#f87171" : "#f59e0b" }}>
                [{issue.severity}]
              </span>{" "}
              <span style={styles.mono}>{issue.path}</span> — {issue.message} ({issue.rule})
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Content Flags Tab ───────────────────────────────────────────────────────

function ContentFlagsTab() {
  const { flaggedContent, flagContent } = useTrust();
  const [hash, setHash] = useState("");
  const [category, setCategory] = useState("spam");

  const handleFlag = useCallback(() => {
    if (!hash.trim()) return;
    flagContent(hash.trim(), category);
    setHash("");
  }, [hash, category, flagContent]);

  return (
    <div>
      <div style={styles.sectionTitle}>Flag Content</div>
      <div style={{ display: "flex", gap: 6, marginBottom: "0.75rem" }}>
        <input
          style={{ ...styles.input, flex: 2 }}
          placeholder="Content hash..."
          value={hash}
          onChange={(e) => setHash(e.target.value)}
          data-testid="flag-hash-input"
        />
        <select
          style={{ ...styles.input, flex: 1 }}
          value={category}
          onChange={(e) => setCategory(e.target.value)}
          data-testid="flag-category-select"
        >
          <option value="spam">Spam</option>
          <option value="malware">Malware</option>
          <option value="phishing">Phishing</option>
          <option value="toxic">Toxic</option>
        </select>
        <button style={styles.btnDanger} onClick={handleFlag} data-testid="flag-content-btn">
          Flag
        </button>
      </div>

      <div style={styles.sectionTitle}>Flagged Content ({flaggedContent.length})</div>
      {flaggedContent.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
          No content flagged.
        </div>
      )}
      {flaggedContent.map((item) => (
        <div key={item.hash} style={styles.card} data-testid={`flagged-${item.hash.slice(0, 8)}`}>
          <div style={styles.mono}>{item.hash}</div>
          <div style={styles.meta}>
            Category: <span style={{ color: "#f87171" }}>{item.category}</span>
            {" | "}Reported by: {item.reportedBy}
            {" | "}{new Date(item.reportedAt).toLocaleString()}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Escrow Tab ──────────────────────────────────────────────────────────────

function EscrowTab() {
  const kernel = useKernel();
  const { depositEscrow, listEscrowDeposits } = useTrust();
  const [payload, setPayload] = useState("");
  const deposits = listEscrowDeposits();

  const handleDeposit = useCallback(() => {
    if (!payload.trim()) return;
    const deposit = depositEscrow(payload.trim());
    if (deposit) {
      kernel.notifications.add({ title: `Escrow deposited: ${deposit.id}`, kind: "success" });
      setPayload("");
    } else {
      kernel.notifications.add({ title: "Generate an identity first", kind: "warning" });
    }
  }, [payload, depositEscrow, kernel]);

  return (
    <div>
      <div style={styles.sectionTitle}>Deposit Encrypted Key Material</div>
      <div style={{ display: "flex", gap: 6, marginBottom: "0.75rem" }}>
        <input
          style={{ ...styles.input, flex: 1 }}
          placeholder="Encrypted payload..."
          value={payload}
          onChange={(e) => setPayload(e.target.value)}
          data-testid="escrow-payload-input"
        />
        <button style={styles.btnPrimary} onClick={handleDeposit} data-testid="escrow-deposit-btn">
          Deposit
        </button>
      </div>

      <div style={styles.sectionTitle}>Deposits ({deposits.length})</div>
      {deposits.length === 0 && (
        <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
          No escrow deposits.
        </div>
      )}
      {deposits.map((dep) => (
        <div key={dep.id} style={styles.card} data-testid={`deposit-${dep.id}`}>
          <div style={{ display: "flex", justifyContent: "space-between" }}>
            <span style={styles.mono}>{dep.id}</span>
            <span style={{
              fontSize: "0.625rem",
              padding: "0.125rem 0.375rem",
              borderRadius: "0.25rem",
              background: dep.claimed ? "#1a4731" : "#333",
              color: dep.claimed ? "#22c55e" : "#888",
            }}>
              {dep.claimed ? "Claimed" : "Pending"}
            </span>
          </div>
          <div style={styles.meta}>
            Deposited: {new Date(dep.depositedAt).toLocaleString()}
            {dep.expiresAt && ` | Expires: ${new Date(dep.expiresAt).toLocaleString()}`}
          </div>
        </div>
      ))}
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────

type TrustTab = "peers" | "validation" | "flags" | "escrow";

export function TrustPanel() {
  const { peers, flaggedContent } = useTrust();
  const [activeTab, setActiveTab] = useState<TrustTab>("peers");

  const bannedCount = useMemo(() => peers.filter((p) => p.banned).length, [peers]);

  return (
    <div style={styles.container} data-testid="trust-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Trust &amp; Safety</span>
        <span style={{ fontSize: "0.75rem", color: "#666" }}>
          {peers.length} peer(s) | {bannedCount} banned | {flaggedContent.length} flagged
        </span>
      </div>

      <div style={styles.tabs as React.CSSProperties}>
        {(["peers", "validation", "flags", "escrow"] as TrustTab[]).map((tab) => (
          <button
            key={tab}
            style={activeTab === tab ? styles.tabActive : styles.tab}
            onClick={() => setActiveTab(tab)}
            data-testid={`trust-tab-${tab}`}
          >
            {tab === "peers" ? `Peers (${peers.length})`
              : tab === "validation" ? "Validation"
              : tab === "flags" ? `Flags (${flaggedContent.length})`
              : "Escrow"}
          </button>
        ))}
      </div>

      {activeTab === "peers" && <PeersTab />}
      {activeTab === "validation" && <ValidationTab />}
      {activeTab === "flags" && <ContentFlagsTab />}
      {activeTab === "escrow" && <EscrowTab />}
    </div>
  );
}
