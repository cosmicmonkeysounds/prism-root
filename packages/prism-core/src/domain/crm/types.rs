//! Pure data types for the CRM engine.
//!
//! Port of `@helm/crm` type definitions. Deal pipeline stages,
//! contract lifecycle, client profitability, and bridge types for
//! deal-to-project and contract-to-kickoff conversions.

use serde::{Deserialize, Serialize};

// ── Deal Stage ───────────────────────────────────────────────────

/// Pipeline stage for a deal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DealStage {
    Lead,
    Qualified,
    Proposal,
    Negotiation,
    ClosedWon,
    ClosedLost,
}

// ── Deal ─────────────────────────────────────────────────────────

/// A deal in the sales pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Deal {
    pub id: String,
    pub title: String,
    pub client_id: String,
    pub stage: DealStage,
    /// Deal value in cents.
    pub value: i64,
    /// Win probability, 0.0 to 1.0.
    pub probability: f64,
    pub expected_close: Option<String>,
    pub created_at: String,
    pub closed_at: Option<String>,
}

// ── Contract Stage ───────────────────────────────────────────────

/// Lifecycle stage for a contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStage {
    Draft,
    Sent,
    Signed,
    Active,
    Expired,
    Cancelled,
}

// ── Contract ─────────────────────────────────────────────────────

/// A contract tied to a client and optionally a deal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Contract {
    pub id: String,
    pub deal_id: Option<String>,
    pub client_id: String,
    pub stage: ContractStage,
    /// Contract value in cents.
    pub value: i64,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub signed_at: Option<String>,
}

// ── Client Revenue ───────────────────────────────────────────────

/// Revenue and cost breakdown for a single client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientRevenue {
    pub client_id: String,
    /// Total revenue in cents.
    pub total_revenue: i64,
    /// Total costs in cents.
    pub total_costs: i64,
    /// Profit (revenue - costs) in cents.
    pub profit: i64,
    pub deal_count: u32,
}

// ── Pipeline Summary ─────────────────────────────────────────────

/// Aggregate pipeline metrics across all deals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipelineSummary {
    pub total_deals: u32,
    pub total_value: i64,
    /// Sum of value * probability across all deals.
    pub weighted_value: i64,
    pub by_stage: Vec<StageSummary>,
}

// ── Stage Summary ────────────────────────────────────────────────

/// Per-stage aggregate within a pipeline summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageSummary {
    pub stage: DealStage,
    pub count: u32,
    pub value: i64,
}

// ── Project Seed ─────────────────────────────────────────────────

/// Seed data for creating a project from a won deal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSeed {
    pub title: String,
    pub client_id: String,
    /// Budget in cents, seeded from the deal value.
    pub budget: i64,
    pub source_deal_id: String,
}

// ── Kickoff Task ─────────────────────────────────────────────────

/// A task generated from a signed contract for project kickoff.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KickoffTask {
    pub title: String,
    pub description: String,
    pub source_contract_id: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deal_stage_serde_round_trip() {
        let s = DealStage::ClosedWon;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"closed_won\"");
        let back: DealStage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn contract_stage_serde_round_trip() {
        let s = ContractStage::Signed;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"signed\"");
        let back: ContractStage = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn deal_serde() {
        let deal = Deal {
            id: "d1".into(),
            title: "Acme Corp".into(),
            client_id: "c1".into(),
            stage: DealStage::Proposal,
            value: 50_000,
            probability: 0.6,
            expected_close: Some("2026-06-01".into()),
            created_at: "2026-01-01".into(),
            closed_at: None,
        };
        let json = serde_json::to_value(&deal).unwrap();
        assert_eq!(json["title"], "Acme Corp");
        assert_eq!(json["stage"], "proposal");
    }

    #[test]
    fn contract_serde() {
        let contract = Contract {
            id: "ct1".into(),
            deal_id: Some("d1".into()),
            client_id: "c1".into(),
            stage: ContractStage::Active,
            value: 10_000_000,
            start_date: Some("2026-01-15".into()),
            end_date: Some("2027-01-15".into()),
            signed_at: Some("2026-01-10".into()),
        };
        let json = serde_json::to_value(&contract).unwrap();
        assert_eq!(json["stage"], "active");
        assert_eq!(json["value"], 10_000_000);
    }
}
