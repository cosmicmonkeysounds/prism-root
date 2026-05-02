//! Pure data types for the ledger engine.
//!
//! Port of `@core/ledger` types at commit 8426588. Defines
//! accounts, entries, transactions, balances, and query filters
//! for double-entry bookkeeping.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ── Direction ─────────────────────────────────────────────────────

/// Whether an entry debits or credits an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LedgerDirection {
    Debit,
    Credit,
}

// ── Account Classification ────────────────────────────────────────

/// The fundamental accounting class of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AccountClass {
    Asset,
    Liability,
    Equity,
    Revenue,
    Expense,
    Counter,
}

// ── Dimension ─────────────────────────────────────────────────────

/// The measurement dimension of an account (monetary, unit, custom).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DimensionKind {
    Monetary,
    Unit,
    Custom,
}

/// Describes what an account measures and in what unit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountDimension {
    pub kind: DimensionKind,
    pub currency: Option<String>,
    pub unit_label: Option<String>,
    pub custom_label: Option<String>,
}

// ── Account ───────────────────────────────────────────────────────

/// A named account in the chart of accounts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerAccount {
    pub id: String,
    pub type_name: String,
    pub name: String,
    pub description: Option<String>,
    pub account_class: AccountClass,
    pub dimension: AccountDimension,
    pub contra_account_id: Option<String>,
    pub object_id: Option<String>,
    pub closed: bool,
    pub created_at: String,
    pub updated_at: String,
    pub data: HashMap<String, Value>,
}

// ── Entry ─────────────────────────────────────────────────────────

/// A single debit or credit posted to an account.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerEntry {
    pub id: String,
    pub transaction_id: String,
    pub account_id: String,
    pub direction: LedgerDirection,
    pub quantity: f64,
    pub unit_cost: Option<f64>,
    pub cost_currency: Option<String>,
    pub posted_at: String,
    pub effective_at: String,
    pub description: Option<String>,
    pub reference: Option<String>,
    pub offset_account_id: Option<String>,
    pub reversal_of: Option<String>,
    pub reversed: bool,
    pub source_object_id: Option<String>,
    pub source_object_type: Option<String>,
    pub data: HashMap<String, Value>,
}

// ── Transaction ───────────────────────────────────────────────────

/// Groups one or more entries into a single atomic posting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LedgerTransaction {
    pub id: String,
    pub type_name: String,
    pub description: Option<String>,
    pub reference: Option<String>,
    pub initiated_by: Option<String>,
    pub posted_at: String,
    pub effective_at: String,
    pub source_object_id: Option<String>,
    pub source_object_type: Option<String>,
    pub data: HashMap<String, Value>,
}

// ── Balance ───────────────────────────────────────────────────────

/// Running totals for a single account.
#[derive(Debug, Clone, Default)]
pub struct AccountBalance {
    pub account_id: String,
    pub balance: f64,
    pub total_debits: f64,
    pub total_credits: f64,
    pub entry_count: usize,
    pub latest_entry_at: Option<String>,
}

// ── Query ─────────────────────────────────────────────────────────

/// Filter criteria for querying entries.
#[derive(Debug, Clone, Default)]
pub struct LedgerQuery {
    pub account_id: Option<String>,
    pub account_ids: Option<Vec<String>>,
    pub transaction_id: Option<String>,
    pub direction: Option<LedgerDirection>,
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub source_object_id: Option<String>,
    pub active_only: Option<bool>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn direction_serde_round_trip() {
        let d = LedgerDirection::Debit;
        let json = serde_json::to_string(&d).unwrap();
        assert_eq!(json, "\"debit\"");
        let back: LedgerDirection = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn account_class_serde_round_trip() {
        let c = AccountClass::Revenue;
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"revenue\"");
        let back: AccountClass = serde_json::from_str(&json).unwrap();
        assert_eq!(back, c);
    }

    #[test]
    fn dimension_kind_serde_round_trip() {
        let k = DimensionKind::Monetary;
        let json = serde_json::to_string(&k).unwrap();
        assert_eq!(json, "\"monetary\"");
        let back: DimensionKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, k);
    }

    #[test]
    fn ledger_account_serde() {
        let account = LedgerAccount {
            id: "a1".into(),
            type_name: "cash".into(),
            name: "Cash".into(),
            description: Some("Main cash account".into()),
            account_class: AccountClass::Asset,
            dimension: AccountDimension {
                kind: DimensionKind::Monetary,
                currency: Some("USD".into()),
                unit_label: None,
                custom_label: None,
            },
            contra_account_id: None,
            object_id: None,
            closed: false,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            data: HashMap::new(),
        };
        let json = serde_json::to_value(&account).unwrap();
        assert_eq!(json["name"], "Cash");
        assert_eq!(json["account_class"], "asset");
    }

    #[test]
    fn ledger_entry_serde() {
        let entry = LedgerEntry {
            id: "e1".into(),
            transaction_id: "t1".into(),
            account_id: "a1".into(),
            direction: LedgerDirection::Debit,
            quantity: 100.0,
            unit_cost: None,
            cost_currency: None,
            posted_at: "2026-01-01T00:00:00Z".into(),
            effective_at: "2026-01-01T00:00:00Z".into(),
            description: None,
            reference: None,
            offset_account_id: None,
            reversal_of: None,
            reversed: false,
            source_object_id: None,
            source_object_type: None,
            data: HashMap::new(),
        };
        let json = serde_json::to_value(&entry).unwrap();
        assert_eq!(json["direction"], "debit");
        assert_eq!(json["quantity"], 100.0);
    }

    #[test]
    fn account_balance_default() {
        let bal = AccountBalance::default();
        assert_eq!(bal.balance, 0.0);
        assert_eq!(bal.entry_count, 0);
        assert!(bal.latest_entry_at.is_none());
    }

    #[test]
    fn ledger_query_default() {
        let q = LedgerQuery::default();
        assert!(q.account_id.is_none());
        assert!(q.direction.is_none());
        assert!(q.active_only.is_none());
    }
}
