//! `domain::projects` — project health, risk, scope, velocity engine.
//!
//! Port of `@helm/projects` TypeScript module. Risk register with
//! impact x probability scoring, scope tracking with creep alerts,
//! velocity calculation, and composite project health. CPM critical
//! path lives in `domain::graph_analysis`.

pub mod engine;
pub mod types;

pub use engine::{
    compute_velocity, generate_burndown, generate_burnup, project_health, scope_snapshot,
    score_risk, score_risks, widget_contributions,
};
pub use types::{
    BurndownPoint, BurnupPoint, ProjectHealthScore, Risk, RiskScore, RiskSeverity, RiskStatus,
    ScopeChange, ScopeSnapshot, SprintData, VelocityStats, VelocityTrend,
};
