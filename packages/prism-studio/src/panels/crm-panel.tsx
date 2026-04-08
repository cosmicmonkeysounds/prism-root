/**
 * CRM Panel — Contacts, Organizations, Deal Pipeline.
 *
 * Uses existing Flux Contact/Organization types with CRM-specific views.
 *
 * Lens #30 (Shift+C)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { FLUX_TYPES, CONTACT_TYPES } from "@prism/core/layer1";

const s: Record<string, CSSProperties> = {
  root: { padding: 16, height: "100%", overflow: "auto", fontFamily: "system-ui", fontSize: 13, color: "#ccc", background: "#1a1a1a" },
  header: { display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12 },
  title: { fontSize: 18, fontWeight: 600, color: "#e5e5e5" },
  tabs: { display: "flex", gap: 4, marginBottom: 12 },
  tab: { padding: "6px 12px", border: "1px solid #444", borderRadius: 4, background: "#252526", cursor: "pointer", color: "#ccc" },
  tabActive: { padding: "6px 12px", border: "1px solid #4a9eff", borderRadius: 4, background: "#1e3a5f", cursor: "pointer", color: "#fff" },
  card: { background: "#252526", border: "1px solid #333", borderRadius: 6, padding: 12, marginBottom: 8 },
  cardTitle: { fontWeight: 600, color: "#e5e5e5", marginBottom: 4 },
  badge: { display: "inline-block", padding: "2px 8px", borderRadius: 10, fontSize: 11, background: "#333", marginLeft: 6 },
  btn: { padding: "6px 12px", border: "1px solid #555", borderRadius: 4, background: "#333", color: "#ccc", cursor: "pointer" },
  btnPrimary: { padding: "6px 12px", border: "none", borderRadius: 4, background: "#4a9eff", color: "#fff", cursor: "pointer" },
  field: { display: "flex", gap: 8, alignItems: "center", marginBottom: 4 },
  label: { color: "#888", minWidth: 80 },
  empty: { color: "#666", fontStyle: "italic", textAlign: "center" as const, padding: 32 },
  pipelineRow: { display: "flex", gap: 8, marginBottom: 8 },
  pipelineCol: { flex: 1, background: "#252526", border: "1px solid #333", borderRadius: 6, padding: 8, minHeight: 100 },
  pipelineHeader: { fontSize: 11, fontWeight: 600, color: "#888", textTransform: "uppercase" as const, marginBottom: 8 },
};

type CrmTab = "contacts" | "organizations" | "pipeline";

const DEAL_STAGES = ["prospect", "qualified", "proposal", "negotiation", "closed_won", "closed_lost"];

export function CrmPanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<CrmTab>("contacts");

  const contacts = objects.filter((o: GraphObject) => o.type === FLUX_TYPES.CONTACT);
  const orgs = objects.filter((o: GraphObject) => o.type === FLUX_TYPES.ORGANIZATION);
  const deals = contacts.filter((c: GraphObject) => c.data.dealStage);

  const createContact = useCallback(() => {
    kernel.createObject({ type: FLUX_TYPES.CONTACT, name: "New Contact", parentId: null, position: contacts.length, status: null, tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data: { contactType: "person" } });
  }, [kernel, contacts.length]);

  const createOrg = useCallback(() => {
    kernel.createObject({ type: FLUX_TYPES.ORGANIZATION, name: "New Organization", parentId: null, position: orgs.length, status: null, tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data: {} });
  }, [kernel, orgs.length]);

  const deleteObject = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderContactType = (ct: unknown) => {
    const found = CONTACT_TYPES.find((t) => t.value === ct);
    return <span style={s.badge}>{found?.label ?? String(ct ?? "person")}</span>;
  };

  return (
    <div style={s.root} data-testid="crm-panel">
      <div style={s.header}><span style={s.title}>CRM</span></div>
      <div style={s.tabs}>
        <button style={tab === "contacts" ? s.tabActive : s.tab} onClick={() => setTab("contacts")} data-testid="crm-tab-contacts">Contacts ({contacts.length})</button>
        <button style={tab === "organizations" ? s.tabActive : s.tab} onClick={() => setTab("organizations")} data-testid="crm-tab-orgs">Organizations ({orgs.length})</button>
        <button style={tab === "pipeline" ? s.tabActive : s.tab} onClick={() => setTab("pipeline")} data-testid="crm-tab-pipeline">Pipeline ({deals.length})</button>
      </div>

      {tab === "contacts" && (
        <>
          <button style={s.btnPrimary} onClick={createContact} data-testid="crm-new-contact">+ New Contact</button>
          {contacts.length === 0 && <div style={s.empty}>No contacts</div>}
          {contacts.map((c: GraphObject) => (
            <div key={c.id} style={s.card} data-testid={`crm-contact-${c.id}`}>
              <div style={s.cardTitle}>{c.name}{renderContactType(c.data.contactType)}</div>
              {!!c.data.email && <div style={s.field}><span style={s.label}>Email:</span> {String(c.data.email)}</div>}
              {!!c.data.phone && <div style={s.field}><span style={s.label}>Phone:</span> {String(c.data.phone)}</div>}
              {!!c.data.role && <div style={s.field}><span style={s.label}>Role:</span> {String(c.data.role)}</div>}
              <button style={s.btn} onClick={() => deleteObject(c.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "organizations" && (
        <>
          <button style={s.btnPrimary} onClick={createOrg} data-testid="crm-new-org">+ New Organization</button>
          {orgs.length === 0 && <div style={s.empty}>No organizations</div>}
          {orgs.map((o: GraphObject) => (
            <div key={o.id} style={s.card} data-testid={`crm-org-${o.id}`}>
              <div style={s.cardTitle}>{o.name}</div>
              {!!o.data.industry && <div style={s.field}><span style={s.label}>Industry:</span> {String(o.data.industry)}</div>}
              {!!o.data.website && <div style={s.field}><span style={s.label}>Website:</span> {String(o.data.website)}</div>}
              <button style={s.btn} onClick={() => deleteObject(o.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "pipeline" && (
        <div style={s.pipelineRow} data-testid="crm-pipeline">
          {DEAL_STAGES.map((stage) => {
            const stageDeals = deals.filter((d: GraphObject) => d.data.dealStage === stage);
            return (
              <div key={stage} style={s.pipelineCol}>
                <div style={s.pipelineHeader}>{stage.replace("_", " ")} ({stageDeals.length})</div>
                {stageDeals.map((d: GraphObject) => (
                  <div key={d.id} style={{ ...s.card, padding: 8 }}>{d.name}</div>
                ))}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
