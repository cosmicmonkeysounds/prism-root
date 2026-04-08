/**
 * Finance Panel — Loans, Grants, Budgets.
 *
 * Lens #29 (Shift+F)
 */

import { useState, useCallback, type CSSProperties } from "react";
import { useKernel, useObjects } from "../kernel/kernel-context.js";
import type { GraphObject, ObjectId } from "@prism/core/object-model";
import { FINANCE_TYPES, LOAN_STATUSES, GRANT_STATUSES, BUDGET_STATUSES } from "@prism/core/layer1";

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
  label: { color: "#888", minWidth: 100 },
  empty: { color: "#666", fontStyle: "italic", textAlign: "center" as const, padding: 32 },
};

type FinanceTab = "loans" | "grants" | "budgets";

export function FinancePanel() {
  const kernel = useKernel();
  const objects = useObjects();
  const [tab, setTab] = useState<FinanceTab>("loans");

  const loans = objects.filter((o: GraphObject) => o.type === FINANCE_TYPES.LOAN);
  const grants = objects.filter((o: GraphObject) => o.type === FINANCE_TYPES.GRANT);
  const budgets = objects.filter((o: GraphObject) => o.type === FINANCE_TYPES.BUDGET);

  const createLoan = useCallback(() => {
    kernel.createObject({ type: FINANCE_TYPES.LOAN, name: "New Loan", parentId: null, position: loans.length, status: "application", tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data: { principal: 0, currency: "USD" } });
  }, [kernel, loans.length]);

  const createGrant = useCallback(() => {
    kernel.createObject({ type: FINANCE_TYPES.GRANT, name: "New Grant", parentId: null, position: grants.length, status: "researching", tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data: { currency: "USD" } });
  }, [kernel, grants.length]);

  const createBudget = useCallback(() => {
    kernel.createObject({ type: FINANCE_TYPES.BUDGET, name: "New Budget", parentId: null, position: budgets.length, status: "draft", tags: [], date: null, endDate: null, description: "", color: null, image: null, pinned: false, data: { period: "monthly", plannedAmount: 0, currency: "USD", startDate: new Date().toISOString().slice(0, 10) } });
  }, [kernel, budgets.length]);

  const deleteObject = useCallback((id: ObjectId) => { kernel.deleteObject(id); }, [kernel]);

  const renderBadge = (status: string | null, statuses: ReadonlyArray<{ value: string; label: string }>) => {
    const found = statuses.find((st) => st.value === status);
    return <span style={s.badge}>{found?.label ?? status ?? "—"}</span>;
  };

  return (
    <div style={s.root} data-testid="finance-panel">
      <div style={s.header}><span style={s.title}>Finance</span></div>
      <div style={s.tabs}>
        <button style={tab === "loans" ? s.tabActive : s.tab} onClick={() => setTab("loans")} data-testid="finance-tab-loans">Loans ({loans.length})</button>
        <button style={tab === "grants" ? s.tabActive : s.tab} onClick={() => setTab("grants")} data-testid="finance-tab-grants">Grants ({grants.length})</button>
        <button style={tab === "budgets" ? s.tabActive : s.tab} onClick={() => setTab("budgets")} data-testid="finance-tab-budgets">Budgets ({budgets.length})</button>
      </div>

      {tab === "loans" && (
        <>
          <button style={s.btnPrimary} onClick={createLoan} data-testid="finance-new-loan">+ New Loan</button>
          {loans.length === 0 && <div style={s.empty}>No loans</div>}
          {loans.map((l: GraphObject) => (
            <div key={l.id} style={s.card} data-testid={`finance-loan-${l.id}`}>
              <div style={s.cardTitle}>{l.name}{renderBadge(l.status, LOAN_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Principal:</span> {String(l.data.principal ?? 0)} {String(l.data.currency ?? "USD")}</div>
              <div style={s.field}><span style={s.label}>Interest:</span> {String(l.data.interestRate ?? "—")}%</div>
              <button style={s.btn} onClick={() => deleteObject(l.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "grants" && (
        <>
          <button style={s.btnPrimary} onClick={createGrant} data-testid="finance-new-grant">+ New Grant</button>
          {grants.length === 0 && <div style={s.empty}>No grants</div>}
          {grants.map((g: GraphObject) => (
            <div key={g.id} style={s.card} data-testid={`finance-grant-${g.id}`}>
              <div style={s.cardTitle}>{g.name}{renderBadge(g.status, GRANT_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Amount:</span> {String(g.data.amount ?? "—")} {String(g.data.currency ?? "USD")}</div>
              <button style={s.btn} onClick={() => deleteObject(g.id)}>Delete</button>
            </div>
          ))}
        </>
      )}

      {tab === "budgets" && (
        <>
          <button style={s.btnPrimary} onClick={createBudget} data-testid="finance-new-budget">+ New Budget</button>
          {budgets.length === 0 && <div style={s.empty}>No budgets</div>}
          {budgets.map((b: GraphObject) => (
            <div key={b.id} style={s.card} data-testid={`finance-budget-${b.id}`}>
              <div style={s.cardTitle}>{b.name}{renderBadge(b.status, BUDGET_STATUSES)}</div>
              <div style={s.field}><span style={s.label}>Planned:</span> {String(b.data.plannedAmount ?? 0)} {String(b.data.currency ?? "USD")}</div>
              <div style={s.field}><span style={s.label}>Spent:</span> {String(b.data.actualAmount ?? 0)}</div>
              <button style={s.btn} onClick={() => deleteObject(b.id)}>Delete</button>
            </div>
          ))}
        </>
      )}
    </div>
  );
}
