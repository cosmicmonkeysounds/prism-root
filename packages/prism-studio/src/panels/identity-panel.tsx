/**
 * Identity Panel — W3C DID identity management.
 *
 * Generate, display, export/import Ed25519 identities.
 * Sign and verify arbitrary text payloads.
 */

import { useState, useCallback } from "react";
import { useIdentity, useKernel } from "../kernel/index.js";

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
  sectionTitle: {
    fontSize: "0.75rem",
    fontWeight: 600,
    textTransform: "uppercase" as const,
    letterSpacing: "0.05em",
    color: "#888",
    marginBottom: "0.375rem",
    marginTop: "0.75rem",
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
  textarea: {
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
  },
  mono: {
    fontFamily: "monospace",
    fontSize: "0.75rem",
    color: "#4fc1ff",
    wordBreak: "break-all" as const,
  },
  meta: {
    fontSize: "0.6875rem",
    color: "#666",
  },
  badge: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#1a4731",
    color: "#22c55e",
    marginLeft: "0.375rem",
  },
  badgeNone: {
    display: "inline-block",
    fontSize: "0.625rem",
    padding: "0.125rem 0.375rem",
    borderRadius: "0.25rem",
    background: "#333",
    color: "#888",
    marginLeft: "0.375rem",
  },
} as const;

// ── Sign / Verify Section ─────────────────────────────────────────────────

function SignVerifySection() {
  const { identity, sign, verify } = useIdentity();
  const [payload, setPayload] = useState("");
  const [signature, setSignature] = useState("");
  const [verifyResult, setVerifyResult] = useState<boolean | null>(null);

  const handleSign = useCallback(async () => {
    if (!payload.trim()) return;
    const data = new TextEncoder().encode(payload);
    const sig = await sign(data);
    if (sig) {
      setSignature(Array.from(sig).map((b) => b.toString(16).padStart(2, "0")).join(""));
      setVerifyResult(null);
    }
  }, [payload, sign]);

  const handleVerify = useCallback(async () => {
    if (!payload.trim() || !signature.trim()) return;
    const data = new TextEncoder().encode(payload);
    const sigBytes = new Uint8Array(
      (signature.match(/.{1,2}/g) ?? []).map((h) => parseInt(h, 16)),
    );
    const result = await verify(data, sigBytes);
    setVerifyResult(result);
  }, [payload, signature, verify]);

  return (
    <div style={styles.card} data-testid="sign-verify-section">
      <div style={styles.sectionTitle}>Sign &amp; Verify</div>
      <textarea
        style={{ ...styles.textarea, height: 60, marginBottom: 6 }}
        placeholder="Payload to sign..."
        value={payload}
        onChange={(e) => setPayload(e.target.value)}
        data-testid="sign-payload-input"
      />
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <button
          style={styles.btnPrimary}
          onClick={handleSign}
          disabled={!identity}
          data-testid="sign-btn"
        >
          Sign
        </button>
        <button
          style={styles.btn}
          onClick={handleVerify}
          disabled={!identity || !signature}
          data-testid="verify-btn"
        >
          Verify
        </button>
        {verifyResult !== null && (
          <span style={verifyResult ? styles.badge : { ...styles.badgeNone, color: "#f87171", background: "#3b1111" }}>
            {verifyResult ? "Valid" : "Invalid"}
          </span>
        )}
      </div>
      {signature && (
        <div style={{ ...styles.mono, fontSize: "0.625rem", marginTop: 4 }} data-testid="signature-output">
          {signature}
        </div>
      )}
    </div>
  );
}

// ── Export / Import Section ────────────────────────────────────────────────

function ExportImportSection() {
  const kernel = useKernel();
  const { identity, exportId, importId } = useIdentity();
  const [importJson, setImportJson] = useState("");

  const handleExport = useCallback(async () => {
    const exported = await exportId();
    if (exported) {
      setImportJson(JSON.stringify(exported, null, 2));
      kernel.notifications.add({ title: "Identity exported to JSON", kind: "info" });
    }
  }, [exportId, kernel]);

  const handleImport = useCallback(async () => {
    if (!importJson.trim()) return;
    try {
      const parsed = JSON.parse(importJson);
      await importId(parsed);
    } catch {
      kernel.notifications.add({ title: "Invalid identity JSON", kind: "error" });
    }
  }, [importJson, importId, kernel]);

  return (
    <div style={styles.card} data-testid="export-import-section">
      <div style={styles.sectionTitle}>Export / Import</div>
      <div style={{ display: "flex", gap: 6, marginBottom: 6 }}>
        <button
          style={styles.btn}
          onClick={handleExport}
          disabled={!identity}
          data-testid="export-identity-btn"
        >
          Export JSON
        </button>
        <button
          style={styles.btnPrimary}
          onClick={handleImport}
          disabled={!importJson.trim()}
          data-testid="import-identity-btn"
        >
          Import JSON
        </button>
      </div>
      <textarea
        style={{ ...styles.textarea, height: 80 }}
        placeholder="Paste exported identity JSON..."
        value={importJson}
        onChange={(e) => setImportJson(e.target.value)}
        data-testid="import-json-input"
      />
    </div>
  );
}

// ── Main Panel ────────────────────────────────────────────────────────────

export function IdentityPanel() {
  const { identity, generate } = useIdentity();

  const handleGenerate = useCallback(async () => {
    await generate();
  }, [generate]);

  return (
    <div style={styles.container} data-testid="identity-panel">
      <div style={styles.header as React.CSSProperties}>
        <span>Identity</span>
        <span style={identity ? styles.badge : styles.badgeNone}>
          {identity ? "Active" : "None"}
        </span>
      </div>

      {/* Identity card */}
      <div style={styles.card} data-testid="identity-card">
        {identity ? (
          <>
            <div style={styles.sectionTitle}>DID</div>
            <div style={styles.mono} data-testid="identity-did">{identity.did}</div>
            <div style={styles.sectionTitle}>Document</div>
            <div style={styles.meta}>
              Method: {identity.did.split(":")[1]}
              {" | "}Created: {identity.document.created}
              {" | "}Verification methods: {identity.document.verificationMethod.length}
            </div>
            <div style={styles.sectionTitle}>Public Key</div>
            <div style={{ ...styles.mono, fontSize: "0.625rem" }} data-testid="identity-pubkey">
              {identity.document.verificationMethod[0]?.publicKeyMultibase ?? "—"}
            </div>
          </>
        ) : (
          <div style={{ color: "#555", fontStyle: "italic", textAlign: "center", padding: "1rem" }}>
            No identity generated yet.
          </div>
        )}
        <div style={{ marginTop: "0.75rem" }}>
          <button
            style={styles.btnPrimary}
            onClick={handleGenerate}
            data-testid="generate-identity-btn"
          >
            {identity ? "Regenerate Identity" : "Generate Identity"}
          </button>
        </div>
      </div>

      <SignVerifySection />
      <ExportImportSection />
    </div>
  );
}
