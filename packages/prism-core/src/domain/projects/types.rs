//! Pure data types for the projects engine.
//!
//! Port of `@helm/projects` risk register, scope tracker, velocity,
//! and project health types. CPM critical path lives in
//! `domain::graph_analysis`.

use serde::{Deserialize, Serialize};

// ── Risk Severity ───────────────────────────────────────────────

/// Severity classification derived from impact * probability score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskSeverity {
    Low,
    Medium,
    High,
    Critical,
}

// ── Risk Status ─────────────────────────────────────────────────

/// Lifecycle status of a risk entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RiskStatus {
    Open,
    Mitigated,
    Closed,
    Accepted,
}

// ── Risk ────────────────────────────────────────────────────────

/// A single risk entry in the register.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Risk {
    pub id: String,
    pub title: String,
    pub description: String,
    /// 1-5 scale.
    pub impact: u8,
    /// 1-5 scale.
    pub probability: u8,
    pub mitigation: Option<String>,
    pub status: RiskStatus,
}

// ── Risk Score ──────────────────────────────────────────────────

/// Computed score for a single risk (impact * probability).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskScore {
    pub risk_id: String,
    pub score: u8,
    pub severity: RiskSeverity,
}

// ── Scope Change ────────────────────────────────────────────────

/// A single scope change event (addition or removal of work).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeChange {
    pub id: String,
    pub description: String,
    pub points_added: i32,
    pub points_removed: i32,
    pub date: String,
}

// ── Scope Snapshot ──────────────────────────────────────────────

/// Aggregated view of scope drift from the original baseline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScopeSnapshot {
    pub original_points: i32,
    pub current_points: i32,
    pub change_count: u32,
    pub net_change: i32,
    pub creep_percentage: f64,
}

// ── Sprint Data ─────────────────────────────────────────────────

/// Raw data for a single sprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprintData {
    pub sprint_id: String,
    pub planned_points: i32,
    pub completed_points: i32,
    pub start_date: String,
    pub end_date: String,
}

// ── Velocity Trend ──────────────────────────────────────────────

/// Direction of velocity change across recent sprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VelocityTrend {
    Improving,
    Stable,
    Declining,
}

// ── Velocity Stats ──────────────────────────────────────────────

/// Aggregated velocity statistics across sprints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VelocityStats {
    pub average_velocity: f64,
    pub min_velocity: i32,
    pub max_velocity: i32,
    pub trend: VelocityTrend,
    pub sprint_count: u32,
}

// ── Project Health Score ────────────────────────────────────────

/// Composite project health from schedule, scope, and risk axes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectHealthScore {
    /// 0.0 to 1.0.
    pub overall: f64,
    pub schedule_health: f64,
    pub scope_health: f64,
    pub risk_health: f64,
}

// ── Burndown / Burnup ───────────────────────────────────────────

/// A single point on a burndown chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurndownPoint {
    pub date: String,
    pub remaining: i32,
    pub ideal: f64,
}

/// A single point on a burnup chart.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnupPoint {
    pub date: String,
    pub completed: i32,
    pub total_scope: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn risk_severity_serde_round_trip() {
        let s = RiskSeverity::Critical;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"critical\"");
        let back: RiskSeverity = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn risk_status_serde_round_trip() {
        let s = RiskStatus::Mitigated;
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, "\"mitigated\"");
        let back: RiskStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn velocity_trend_serde_round_trip() {
        let t = VelocityTrend::Improving;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"improving\"");
        let back: VelocityTrend = serde_json::from_str(&json).unwrap();
        assert_eq!(back, t);
    }

    #[test]
    fn risk_serde() {
        let risk = Risk {
            id: "r1".into(),
            title: "Server outage".into(),
            description: "Main server could fail".into(),
            impact: 4,
            probability: 3,
            mitigation: Some("Redundancy".into()),
            status: RiskStatus::Open,
        };
        let json = serde_json::to_value(&risk).unwrap();
        assert_eq!(json["title"], "Server outage");
        assert_eq!(json["status"], "open");
        assert_eq!(json["impact"], 4);
    }

    #[test]
    fn sprint_data_serde() {
        let sprint = SprintData {
            sprint_id: "s1".into(),
            planned_points: 20,
            completed_points: 18,
            start_date: "2026-01-01".into(),
            end_date: "2026-01-14".into(),
        };
        let json = serde_json::to_value(&sprint).unwrap();
        assert_eq!(json["planned_points"], 20);
        assert_eq!(json["completed_points"], 18);
    }
}
