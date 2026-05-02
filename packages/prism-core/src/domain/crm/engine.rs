//! CRM engine — deal pipeline, contract lifecycle, client
//! profitability, and bridge functions.
//!
//! Port of `@helm/crm` logic. Pure functions over the types defined
//! in [`super::types`].

use std::collections::HashMap;

use super::types::{
    ClientRevenue, Contract, ContractStage, Deal, DealStage, KickoffTask, PipelineSummary,
    ProjectSeed, StageSummary,
};

// ── Pipeline Summary ─────────────────────────────────────────────

/// Aggregate deals by stage into a pipeline summary.
pub fn pipeline_summary(deals: &[Deal]) -> PipelineSummary {
    let mut stage_map: HashMap<DealStage, (u32, i64)> = HashMap::new();
    let mut total_value: i64 = 0;
    let mut weighted_value: i64 = 0;

    for deal in deals {
        let entry = stage_map.entry(deal.stage).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += deal.value;
        total_value += deal.value;
        weighted_value += (deal.value as f64 * deal.probability) as i64;
    }

    // Emit stages in pipeline order.
    let stage_order = [
        DealStage::Lead,
        DealStage::Qualified,
        DealStage::Proposal,
        DealStage::Negotiation,
        DealStage::ClosedWon,
        DealStage::ClosedLost,
    ];

    let by_stage: Vec<StageSummary> = stage_order
        .iter()
        .filter_map(|stage| {
            stage_map.get(stage).map(|(count, value)| StageSummary {
                stage: *stage,
                count: *count,
                value: *value,
            })
        })
        .collect();

    PipelineSummary {
        total_deals: deals.len() as u32,
        total_value,
        weighted_value,
        by_stage,
    }
}

// ── Stage Advancement ────────────────────────────────────────────

/// Returns the next valid deal stage, or `None` if the deal is
/// already in a terminal stage (ClosedWon or ClosedLost).
pub fn advance_deal_stage(deal: &Deal) -> Option<DealStage> {
    match deal.stage {
        DealStage::Lead => Some(DealStage::Qualified),
        DealStage::Qualified => Some(DealStage::Proposal),
        DealStage::Proposal => Some(DealStage::Negotiation),
        DealStage::Negotiation => Some(DealStage::ClosedWon),
        DealStage::ClosedWon | DealStage::ClosedLost => None,
    }
}

// ── Win Rate ─────────────────────────────────────────────────────

/// Compute win rate as ClosedWon / (ClosedWon + ClosedLost).
/// Returns 0.0 if there are no closed deals.
pub fn win_rate(deals: &[Deal]) -> f64 {
    let won = deals
        .iter()
        .filter(|d| d.stage == DealStage::ClosedWon)
        .count();
    let lost = deals
        .iter()
        .filter(|d| d.stage == DealStage::ClosedLost)
        .count();
    let total = won + lost;
    if total == 0 {
        return 0.0;
    }
    won as f64 / total as f64
}

// ── Client Profitability ─────────────────────────────────────────

/// Compute per-client profitability from won deals and a cost map.
///
/// Only `ClosedWon` deals contribute revenue. `costs` is a slice of
/// `(client_id, cost_amount)` pairs.
pub fn client_profitability(deals: &[Deal], costs: &[(String, i64)]) -> Vec<ClientRevenue> {
    let mut revenue_map: HashMap<String, (i64, u32)> = HashMap::new();
    for deal in deals {
        if deal.stage == DealStage::ClosedWon {
            let entry = revenue_map.entry(deal.client_id.clone()).or_insert((0, 0));
            entry.0 += deal.value;
            entry.1 += 1;
        }
    }

    let mut cost_map: HashMap<String, i64> = HashMap::new();
    for (client_id, cost) in costs {
        *cost_map.entry(client_id.clone()).or_insert(0) += cost;
    }

    // Merge all client IDs from both maps.
    let mut all_clients: Vec<String> = revenue_map.keys().cloned().collect();
    for cid in cost_map.keys() {
        if !all_clients.contains(cid) {
            all_clients.push(cid.clone());
        }
    }
    all_clients.sort();

    all_clients
        .into_iter()
        .map(|client_id| {
            let (total_revenue, deal_count) = revenue_map
                .get(&client_id)
                .copied()
                .unwrap_or((0, 0));
            let total_costs = cost_map.get(&client_id).copied().unwrap_or(0);
            ClientRevenue {
                client_id,
                total_revenue,
                total_costs,
                profit: total_revenue - total_costs,
                deal_count,
            }
        })
        .collect()
}

// ── Bridge: Deal → Project Seed ──────────────────────────────────

/// Generate a project seed from a won deal.
pub fn deal_to_project_seed(deal: &Deal) -> ProjectSeed {
    ProjectSeed {
        title: format!("Project: {}", deal.title),
        client_id: deal.client_id.clone(),
        budget: deal.value,
        source_deal_id: deal.id.clone(),
    }
}

// ── Bridge: Contract → Kickoff Tasks ─────────────────────────────

/// Generate kickoff tasks from a signed/active contract.
pub fn contract_to_kickoff_tasks(contract: &Contract) -> Vec<KickoffTask> {
    vec![
        KickoffTask {
            title: "Schedule kickoff meeting".into(),
            description: format!(
                "Schedule the project kickoff meeting for contract {}.",
                contract.id
            ),
            source_contract_id: contract.id.clone(),
        },
        KickoffTask {
            title: "Assign project team".into(),
            description: format!(
                "Assemble and assign the project team for contract {}.",
                contract.id
            ),
            source_contract_id: contract.id.clone(),
        },
        KickoffTask {
            title: "Set up project workspace".into(),
            description: format!(
                "Create the project workspace and configure access for contract {}.",
                contract.id
            ),
            source_contract_id: contract.id.clone(),
        },
    ]
}

// ── Contract Stage Transitions ───────────────────────────────────

/// Check whether a contract stage transition is valid.
pub fn contract_stage_valid_transition(from: &ContractStage, to: &ContractStage) -> bool {
    matches!(
        (from, to),
        (ContractStage::Draft, ContractStage::Sent)
            | (ContractStage::Draft, ContractStage::Cancelled)
            | (ContractStage::Sent, ContractStage::Signed)
            | (ContractStage::Sent, ContractStage::Cancelled)
            | (ContractStage::Signed, ContractStage::Active)
            | (ContractStage::Active, ContractStage::Expired)
            | (ContractStage::Active, ContractStage::Cancelled)
    )
}

/// Returns the next natural contract stage, or `None` if the
/// contract is in a terminal stage (Expired or Cancelled).
pub fn advance_contract_stage(contract: &Contract) -> Option<ContractStage> {
    match contract.stage {
        ContractStage::Draft => Some(ContractStage::Sent),
        ContractStage::Sent => Some(ContractStage::Signed),
        ContractStage::Signed => Some(ContractStage::Active),
        ContractStage::Active => Some(ContractStage::Expired),
        ContractStage::Expired | ContractStage::Cancelled => None,
    }
}

// ── Widget Contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        DataQuery, FieldSpec, LayoutDirection, NumericBounds, QuerySort, SelectOption, SignalSpec,
        TemplateNode, ToolbarAction, WidgetCategory, WidgetContribution, WidgetSize,
        WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "deal-pipeline".into(),
            label: "Deal Pipeline".into(),
            description: "Pipeline funnel view of active deals".into(),
            category: WidgetCategory::Finance,
            config_fields: vec![
                FieldSpec::select(
                    "stage_filter",
                    "Stage Filter",
                    vec![
                        SelectOption::new("all", "All Stages"),
                        SelectOption::new("lead", "Lead"),
                        SelectOption::new("qualified", "Qualified"),
                        SelectOption::new("proposal", "Proposal"),
                        SelectOption::new("negotiation", "Negotiation"),
                        SelectOption::new("closed_won", "Closed Won"),
                        SelectOption::new("closed_lost", "Closed Lost"),
                    ],
                ),
                FieldSpec::boolean("show_value", "Show Deal Value"),
            ],
            signals: vec![
                SignalSpec::new("deal-selected", "A deal was selected")
                    .with_payload(vec![FieldSpec::text("deal_id", "Deal ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
                ToolbarAction::signal("new-deal", "New Deal", "add"),
            ],
            default_size: WidgetSize::new(3, 2),
            data_query: Some(DataQuery {
                object_type: Some("deal".into()),
                sort: vec![QuerySort {
                    field: "stage".into(),
                    descending: false,
                }],
                ..Default::default()
            }),
            data_key: Some("deals".into()),
            data_fields: vec![
                FieldSpec::text("title", "Title"),
                FieldSpec::text("stage", "Stage"),
                FieldSpec::number("value", "Value", NumericBounds::unbounded()),
                FieldSpec::number("probability", "Probability", NumericBounds::unbounded()),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Deal Pipeline", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "deals".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "deal"}),
                            }),
                            empty_label: Some("No deals".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "client-profitability".into(),
            label: "Client Profitability".into(),
            description: "Revenue and cost breakdown by client".into(),
            category: WidgetCategory::Finance,
            config_fields: vec![
                FieldSpec::text("currency", "Currency").with_default(json!("USD")),
            ],
            signals: vec![
                SignalSpec::new("client-selected", "A client was selected")
                    .with_payload(vec![FieldSpec::text("client_id", "Client ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
                ToolbarAction::signal("export", "Export", "export"),
            ],
            default_size: WidgetSize::new(3, 2),
            data_query: Some(DataQuery {
                object_type: Some("client_revenue".into()),
                sort: vec![QuerySort {
                    field: "profit".into(),
                    descending: true,
                }],
                ..Default::default()
            }),
            data_key: Some("clients".into()),
            data_fields: vec![
                FieldSpec::text("client_id", "Client"),
                FieldSpec::number("total_revenue", "Revenue", NumericBounds::unbounded()),
                FieldSpec::number("total_costs", "Costs", NumericBounds::unbounded()),
                FieldSpec::number("profit", "Profit", NumericBounds::unbounded()),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Client Profitability", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "clients".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "client"}),
                            }),
                            empty_label: Some("No client data".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "contract-status".into(),
            label: "Contract Status".into(),
            description: "Contract lifecycle tracker".into(),
            category: WidgetCategory::Display,
            config_fields: vec![
                FieldSpec::select(
                    "stage_filter",
                    "Stage Filter",
                    vec![
                        SelectOption::new("all", "All Stages"),
                        SelectOption::new("draft", "Draft"),
                        SelectOption::new("sent", "Sent"),
                        SelectOption::new("signed", "Signed"),
                        SelectOption::new("active", "Active"),
                        SelectOption::new("expired", "Expired"),
                        SelectOption::new("cancelled", "Cancelled"),
                    ],
                ),
            ],
            signals: vec![
                SignalSpec::new("contract-selected", "A contract was selected")
                    .with_payload(vec![FieldSpec::text("contract_id", "Contract ID")]),
            ],
            toolbar_actions: vec![
                ToolbarAction::signal("refresh", "Refresh", "refresh"),
                ToolbarAction::signal("new-contract", "New Contract", "add"),
            ],
            default_size: WidgetSize::new(2, 2),
            data_query: Some(DataQuery {
                object_type: Some("contract".into()),
                sort: vec![QuerySort {
                    field: "stage".into(),
                    descending: false,
                }],
                ..Default::default()
            }),
            data_key: Some("contracts".into()),
            data_fields: vec![
                FieldSpec::text("id", "Contract ID"),
                FieldSpec::text("client_id", "Client"),
                FieldSpec::text("stage", "Stage"),
                FieldSpec::number("value", "Value", NumericBounds::unbounded()),
            ],
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Contracts", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "contracts".into(),
                            item_template: Box::new(TemplateNode::Component {
                                component_id: "text".into(),
                                props: json!({"body": "contract"}),
                            }),
                            empty_label: Some("No contracts".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Test Helpers ─────────────────────────────────────────────────

#[cfg(test)]
fn make_deal(id: &str, stage: DealStage, value: i64, probability: f64, client_id: &str) -> Deal {
    Deal {
        id: id.into(),
        title: format!("Deal {id}"),
        client_id: client_id.into(),
        stage,
        value,
        probability,
        expected_close: None,
        created_at: "2026-01-01".into(),
        closed_at: if matches!(stage, DealStage::ClosedWon | DealStage::ClosedLost) {
            Some("2026-03-01".into())
        } else {
            None
        },
    }
}

#[cfg(test)]
fn make_contract(id: &str, stage: ContractStage, client_id: &str, value: i64) -> Contract {
    Contract {
        id: id.into(),
        deal_id: None,
        client_id: client_id.into(),
        stage,
        value,
        start_date: None,
        end_date: None,
        signed_at: if matches!(
            stage,
            ContractStage::Signed | ContractStage::Active | ContractStage::Expired
        ) {
            Some("2026-02-01".into())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── pipeline_summary ─────────────────────────────────────────

    #[test]
    fn pipeline_summary_basic() {
        let deals = vec![
            make_deal("d1", DealStage::Lead, 10_000, 0.1, "c1"),
            make_deal("d2", DealStage::Proposal, 50_000, 0.5, "c2"),
            make_deal("d3", DealStage::ClosedWon, 30_000, 1.0, "c1"),
        ];
        let summary = pipeline_summary(&deals);
        assert_eq!(summary.total_deals, 3);
        assert_eq!(summary.total_value, 90_000);
        // weighted = 10000*0.1 + 50000*0.5 + 30000*1.0 = 1000 + 25000 + 30000 = 56000
        assert_eq!(summary.weighted_value, 56_000);
        assert_eq!(summary.by_stage.len(), 3);
    }

    #[test]
    fn pipeline_summary_empty() {
        let summary = pipeline_summary(&[]);
        assert_eq!(summary.total_deals, 0);
        assert_eq!(summary.total_value, 0);
        assert_eq!(summary.weighted_value, 0);
        assert!(summary.by_stage.is_empty());
    }

    #[test]
    fn pipeline_summary_stage_order() {
        let deals = vec![
            make_deal("d1", DealStage::ClosedWon, 10_000, 1.0, "c1"),
            make_deal("d2", DealStage::Lead, 5_000, 0.1, "c2"),
            make_deal("d3", DealStage::Proposal, 20_000, 0.5, "c3"),
        ];
        let summary = pipeline_summary(&deals);
        // Stages should appear in pipeline order: Lead, Proposal, ClosedWon
        assert_eq!(summary.by_stage[0].stage, DealStage::Lead);
        assert_eq!(summary.by_stage[1].stage, DealStage::Proposal);
        assert_eq!(summary.by_stage[2].stage, DealStage::ClosedWon);
    }

    // ── advance_deal_stage ───────────────────────────────────────

    #[test]
    fn advance_deal_stage_progression() {
        let deal_lead = make_deal("d1", DealStage::Lead, 10_000, 0.1, "c1");
        assert_eq!(advance_deal_stage(&deal_lead), Some(DealStage::Qualified));

        let deal_qualified = make_deal("d2", DealStage::Qualified, 10_000, 0.3, "c1");
        assert_eq!(
            advance_deal_stage(&deal_qualified),
            Some(DealStage::Proposal)
        );

        let deal_proposal = make_deal("d3", DealStage::Proposal, 10_000, 0.5, "c1");
        assert_eq!(
            advance_deal_stage(&deal_proposal),
            Some(DealStage::Negotiation)
        );

        let deal_negotiation = make_deal("d4", DealStage::Negotiation, 10_000, 0.8, "c1");
        assert_eq!(
            advance_deal_stage(&deal_negotiation),
            Some(DealStage::ClosedWon)
        );
    }

    #[test]
    fn advance_deal_stage_terminal() {
        let won = make_deal("d1", DealStage::ClosedWon, 10_000, 1.0, "c1");
        assert_eq!(advance_deal_stage(&won), None);

        let lost = make_deal("d2", DealStage::ClosedLost, 10_000, 0.0, "c1");
        assert_eq!(advance_deal_stage(&lost), None);
    }

    // ── win_rate ─────────────────────────────────────────────────

    #[test]
    fn win_rate_basic() {
        let deals = vec![
            make_deal("d1", DealStage::ClosedWon, 10_000, 1.0, "c1"),
            make_deal("d2", DealStage::ClosedWon, 20_000, 1.0, "c2"),
            make_deal("d3", DealStage::ClosedLost, 15_000, 0.0, "c3"),
        ];
        let rate = win_rate(&deals);
        // 2 won / 3 closed = 0.666...
        assert!((rate - 2.0 / 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn win_rate_no_closed_deals() {
        let deals = vec![
            make_deal("d1", DealStage::Lead, 10_000, 0.1, "c1"),
            make_deal("d2", DealStage::Proposal, 20_000, 0.5, "c2"),
        ];
        assert_eq!(win_rate(&deals), 0.0);
    }

    #[test]
    fn win_rate_all_won() {
        let deals = vec![
            make_deal("d1", DealStage::ClosedWon, 10_000, 1.0, "c1"),
            make_deal("d2", DealStage::ClosedWon, 20_000, 1.0, "c2"),
        ];
        assert_eq!(win_rate(&deals), 1.0);
    }

    #[test]
    fn win_rate_empty() {
        assert_eq!(win_rate(&[]), 0.0);
    }

    // ── client_profitability ─────────────────────────────────────

    #[test]
    fn client_profitability_basic() {
        let deals = vec![
            make_deal("d1", DealStage::ClosedWon, 100_000, 1.0, "c1"),
            make_deal("d2", DealStage::ClosedWon, 50_000, 1.0, "c1"),
            make_deal("d3", DealStage::ClosedWon, 80_000, 1.0, "c2"),
            make_deal("d4", DealStage::Lead, 200_000, 0.1, "c1"), // not won, should not count
        ];
        let costs = vec![
            ("c1".to_string(), 60_000_i64),
            ("c2".to_string(), 30_000_i64),
        ];
        let result = client_profitability(&deals, &costs);
        assert_eq!(result.len(), 2);

        let c1 = result.iter().find(|r| r.client_id == "c1").unwrap();
        assert_eq!(c1.total_revenue, 150_000);
        assert_eq!(c1.total_costs, 60_000);
        assert_eq!(c1.profit, 90_000);
        assert_eq!(c1.deal_count, 2);

        let c2 = result.iter().find(|r| r.client_id == "c2").unwrap();
        assert_eq!(c2.total_revenue, 80_000);
        assert_eq!(c2.total_costs, 30_000);
        assert_eq!(c2.profit, 50_000);
        assert_eq!(c2.deal_count, 1);
    }

    #[test]
    fn client_profitability_costs_only() {
        let costs = vec![("c1".to_string(), 10_000_i64)];
        let result = client_profitability(&[], &costs);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].total_revenue, 0);
        assert_eq!(result[0].profit, -10_000);
        assert_eq!(result[0].deal_count, 0);
    }

    // ── deal_to_project_seed ─────────────────────────────────────

    #[test]
    fn deal_to_project_seed_basic() {
        let deal = make_deal("d1", DealStage::ClosedWon, 100_000, 1.0, "c1");
        let seed = deal_to_project_seed(&deal);
        assert_eq!(seed.title, "Project: Deal d1");
        assert_eq!(seed.client_id, "c1");
        assert_eq!(seed.budget, 100_000);
        assert_eq!(seed.source_deal_id, "d1");
    }

    // ── contract_to_kickoff_tasks ────────────────────────────────

    #[test]
    fn contract_to_kickoff_tasks_basic() {
        let contract = make_contract("ct1", ContractStage::Signed, "c1", 50_000);
        let tasks = contract_to_kickoff_tasks(&contract);
        assert_eq!(tasks.len(), 3);
        assert!(tasks[0].title.contains("kickoff"));
        assert!(tasks[1].title.contains("team"));
        assert!(tasks[2].title.contains("workspace"));
        for task in &tasks {
            assert_eq!(task.source_contract_id, "ct1");
        }
    }

    // ── contract_stage_valid_transition ───────────────────────────

    #[test]
    fn contract_stage_valid_transitions() {
        // Valid forward transitions
        assert!(contract_stage_valid_transition(
            &ContractStage::Draft,
            &ContractStage::Sent
        ));
        assert!(contract_stage_valid_transition(
            &ContractStage::Sent,
            &ContractStage::Signed
        ));
        assert!(contract_stage_valid_transition(
            &ContractStage::Signed,
            &ContractStage::Active
        ));
        assert!(contract_stage_valid_transition(
            &ContractStage::Active,
            &ContractStage::Expired
        ));

        // Valid cancellation transitions
        assert!(contract_stage_valid_transition(
            &ContractStage::Draft,
            &ContractStage::Cancelled
        ));
        assert!(contract_stage_valid_transition(
            &ContractStage::Sent,
            &ContractStage::Cancelled
        ));
        assert!(contract_stage_valid_transition(
            &ContractStage::Active,
            &ContractStage::Cancelled
        ));

        // Invalid transitions
        assert!(!contract_stage_valid_transition(
            &ContractStage::Draft,
            &ContractStage::Active
        ));
        assert!(!contract_stage_valid_transition(
            &ContractStage::Expired,
            &ContractStage::Active
        ));
        assert!(!contract_stage_valid_transition(
            &ContractStage::Cancelled,
            &ContractStage::Draft
        ));
        assert!(!contract_stage_valid_transition(
            &ContractStage::Signed,
            &ContractStage::Cancelled
        ));
    }

    // ── advance_contract_stage ───────────────────────────────────

    #[test]
    fn advance_contract_stage_progression() {
        let draft = make_contract("ct1", ContractStage::Draft, "c1", 10_000);
        assert_eq!(advance_contract_stage(&draft), Some(ContractStage::Sent));

        let sent = make_contract("ct2", ContractStage::Sent, "c1", 10_000);
        assert_eq!(advance_contract_stage(&sent), Some(ContractStage::Signed));

        let signed = make_contract("ct3", ContractStage::Signed, "c1", 10_000);
        assert_eq!(
            advance_contract_stage(&signed),
            Some(ContractStage::Active)
        );

        let active = make_contract("ct4", ContractStage::Active, "c1", 10_000);
        assert_eq!(
            advance_contract_stage(&active),
            Some(ContractStage::Expired)
        );
    }

    #[test]
    fn advance_contract_stage_terminal() {
        let expired = make_contract("ct1", ContractStage::Expired, "c1", 10_000);
        assert_eq!(advance_contract_stage(&expired), None);

        let cancelled = make_contract("ct2", ContractStage::Cancelled, "c1", 10_000);
        assert_eq!(advance_contract_stage(&cancelled), None);
    }

    // ── widget_contributions ─────────────────────────────────────

    #[test]
    fn widget_contributions_returns_3_widgets() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 3);
        let ids: Vec<&str> = widgets.iter().map(|w| w.id.as_str()).collect();
        assert!(ids.contains(&"deal-pipeline"));
        assert!(ids.contains(&"client-profitability"));
        assert!(ids.contains(&"contract-status"));
    }
}
