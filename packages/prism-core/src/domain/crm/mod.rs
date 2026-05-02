//! `domain::crm` — deal pipeline, contract lifecycle, and client
//! profitability engine.
//!
//! Port of `@helm/crm`. Deal pipeline stages, contract lifecycle
//! (draft → sent → signed → active → expired), e-signature tracking,
//! deal→project bridge, and client profitability calculations.

pub mod engine;
pub mod types;

pub use engine::{
    advance_contract_stage, advance_deal_stage, client_profitability,
    contract_stage_valid_transition, contract_to_kickoff_tasks, deal_to_project_seed,
    pipeline_summary, widget_contributions, win_rate,
};
pub use types::{
    ClientRevenue, Contract, ContractStage, Deal, DealStage, KickoffTask, PipelineSummary,
    ProjectSeed, StageSummary,
};
