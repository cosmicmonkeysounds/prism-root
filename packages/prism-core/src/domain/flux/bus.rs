//! Flux event bus — cross-module event routing.
//!
//! Defines domain events (deal won, contract signed, task completed,
//! etc.) and rules that map events to follow-up actions. The bus is
//! pure data — hosts wire it to their `AutomationEngine` or process
//! queue for actual dispatch.

use serde::{Deserialize, Serialize};

// ── Domain Events ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FluxEvent {
    DealWon {
        deal_id: String,
        client_id: String,
        value: i64,
    },
    DealLost {
        deal_id: String,
        client_id: String,
    },
    ContractSigned {
        contract_id: String,
        deal_id: Option<String>,
        client_id: String,
    },
    ContractExpired {
        contract_id: String,
        client_id: String,
    },
    TaskCompleted {
        task_id: String,
        project_id: Option<String>,
    },
    MilestoneCompleted {
        milestone_id: String,
        goal_id: String,
    },
    InvoiceOverdue {
        invoice_id: String,
        client_id: String,
        amount: i64,
    },
    HabitCompleted {
        habit_id: String,
        date: String,
    },
    ReminderDue {
        reminder_id: String,
    },
    GoalCompleted {
        goal_id: String,
    },
}

// ── Follow-up Actions ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum FluxAction {
    CreateProject {
        title: String,
        client_id: String,
        budget: i64,
        source_deal_id: String,
    },
    CreateInvoice {
        client_id: String,
        amount: i64,
        source_deal_id: String,
    },
    CreateKickoffTasks {
        contract_id: String,
        client_id: String,
    },
    UpdateProjectProgress {
        project_id: String,
    },
    UpdateGoalProgress {
        goal_id: String,
    },
    SendReminder {
        reminder_id: String,
    },
    CreateReminder {
        title: String,
        object_id: String,
        object_type: String,
        due_date: String,
    },
    LogActivity {
        object_id: String,
        verb: String,
        description: String,
    },
    SendNotification {
        title: String,
        body: String,
        object_id: Option<String>,
    },
}

// ── Routing Rules ────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusRule {
    pub id: String,
    pub label: String,
    pub enabled: bool,
    pub event_type: String,
    pub actions: Vec<FluxAction>,
}

pub fn default_rules() -> Vec<BusRule> {
    vec![
        BusRule {
            id: "deal-won-project".into(),
            label: "Deal Won → Create Project + Invoice".into(),
            enabled: true,
            event_type: "deal-won".into(),
            actions: vec![
                FluxAction::LogActivity {
                    object_id: "{{deal_id}}".into(),
                    verb: "won".into(),
                    description: "Deal closed — creating project".into(),
                },
            ],
        },
        BusRule {
            id: "contract-signed-kickoff".into(),
            label: "Contract Signed → Kickoff Tasks".into(),
            enabled: true,
            event_type: "contract-signed".into(),
            actions: vec![
                FluxAction::LogActivity {
                    object_id: "{{contract_id}}".into(),
                    verb: "signed".into(),
                    description: "Contract signed — creating kickoff tasks".into(),
                },
            ],
        },
        BusRule {
            id: "task-completed-progress".into(),
            label: "Task Completed → Update Project Progress".into(),
            enabled: true,
            event_type: "task-completed".into(),
            actions: vec![],
        },
        BusRule {
            id: "milestone-completed-goal".into(),
            label: "Milestone Completed → Update Goal Progress".into(),
            enabled: true,
            event_type: "milestone-completed".into(),
            actions: vec![],
        },
        BusRule {
            id: "invoice-overdue-reminder".into(),
            label: "Invoice Overdue → Send Reminder".into(),
            enabled: true,
            event_type: "invoice-overdue".into(),
            actions: vec![
                FluxAction::SendNotification {
                    title: "Invoice Overdue".into(),
                    body: "An invoice is past due".into(),
                    object_id: Some("{{invoice_id}}".into()),
                },
            ],
        },
        BusRule {
            id: "reminder-due-notify".into(),
            label: "Reminder Due → Send Notification".into(),
            enabled: true,
            event_type: "reminder-due".into(),
            actions: vec![
                FluxAction::SendReminder {
                    reminder_id: "{{reminder_id}}".into(),
                },
            ],
        },
    ]
}

/// Resolve which actions should fire for a given event.
pub fn resolve_actions(event: &FluxEvent, rules: &[BusRule]) -> Vec<FluxAction> {
    let event_type = event_type_str(event);
    rules
        .iter()
        .filter(|r| r.enabled && r.event_type == event_type)
        .flat_map(|r| r.actions.clone())
        .collect()
}

/// Map an event to its type string for rule matching.
pub fn event_type_str(event: &FluxEvent) -> &'static str {
    match event {
        FluxEvent::DealWon { .. } => "deal-won",
        FluxEvent::DealLost { .. } => "deal-lost",
        FluxEvent::ContractSigned { .. } => "contract-signed",
        FluxEvent::ContractExpired { .. } => "contract-expired",
        FluxEvent::TaskCompleted { .. } => "task-completed",
        FluxEvent::MilestoneCompleted { .. } => "milestone-completed",
        FluxEvent::InvoiceOverdue { .. } => "invoice-overdue",
        FluxEvent::HabitCompleted { .. } => "habit-completed",
        FluxEvent::ReminderDue { .. } => "reminder-due",
        FluxEvent::GoalCompleted { .. } => "goal-completed",
    }
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_rules_count() {
        let rules = default_rules();
        assert_eq!(rules.len(), 6);
        assert!(rules.iter().all(|r| r.enabled));
    }

    #[test]
    fn resolve_deal_won_actions() {
        let rules = default_rules();
        let event = FluxEvent::DealWon {
            deal_id: "d1".into(),
            client_id: "c1".into(),
            value: 50000,
        };
        let actions = resolve_actions(&event, &rules);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], FluxAction::LogActivity { .. }));
    }

    #[test]
    fn resolve_no_matching_rules() {
        let rules = default_rules();
        let event = FluxEvent::DealLost {
            deal_id: "d1".into(),
            client_id: "c1".into(),
        };
        let actions = resolve_actions(&event, &rules);
        assert!(actions.is_empty());
    }

    #[test]
    fn resolve_disabled_rule_skipped() {
        let mut rules = default_rules();
        for r in &mut rules {
            r.enabled = false;
        }
        let event = FluxEvent::DealWon {
            deal_id: "d1".into(),
            client_id: "c1".into(),
            value: 10000,
        };
        let actions = resolve_actions(&event, &rules);
        assert!(actions.is_empty());
    }

    #[test]
    fn event_type_str_mapping() {
        assert_eq!(
            event_type_str(&FluxEvent::DealWon {
                deal_id: "".into(),
                client_id: "".into(),
                value: 0
            }),
            "deal-won"
        );
        assert_eq!(
            event_type_str(&FluxEvent::ContractSigned {
                contract_id: "".into(),
                deal_id: None,
                client_id: "".into()
            }),
            "contract-signed"
        );
        assert_eq!(
            event_type_str(&FluxEvent::TaskCompleted {
                task_id: "".into(),
                project_id: None
            }),
            "task-completed"
        );
        assert_eq!(
            event_type_str(&FluxEvent::ReminderDue {
                reminder_id: "".into()
            }),
            "reminder-due"
        );
    }

    #[test]
    fn resolve_reminder_due() {
        let rules = default_rules();
        let event = FluxEvent::ReminderDue {
            reminder_id: "rem1".into(),
        };
        let actions = resolve_actions(&event, &rules);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], FluxAction::SendReminder { .. }));
    }

    #[test]
    fn resolve_invoice_overdue() {
        let rules = default_rules();
        let event = FluxEvent::InvoiceOverdue {
            invoice_id: "inv1".into(),
            client_id: "c1".into(),
            amount: 5000,
        };
        let actions = resolve_actions(&event, &rules);
        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], FluxAction::SendNotification { .. }));
    }

    #[test]
    fn flux_event_serde_roundtrip() {
        let event = FluxEvent::DealWon {
            deal_id: "d1".into(),
            client_id: "c1".into(),
            value: 100000,
        };
        let json = serde_json::to_string(&event).unwrap();
        let back: FluxEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(back, event);
    }

    #[test]
    fn flux_action_serde_roundtrip() {
        let action = FluxAction::CreateProject {
            title: "New Project".into(),
            client_id: "c1".into(),
            budget: 50000,
            source_deal_id: "d1".into(),
        };
        let json = serde_json::to_string(&action).unwrap();
        let back: FluxAction = serde_json::from_str(&json).unwrap();
        assert_eq!(json.contains("create-project"), true);
    }

    #[test]
    fn custom_rules() {
        let rules = vec![BusRule {
            id: "custom".into(),
            label: "Custom rule".into(),
            enabled: true,
            event_type: "goal-completed".into(),
            actions: vec![FluxAction::SendNotification {
                title: "Goal done!".into(),
                body: "Congrats".into(),
                object_id: None,
            }],
        }];
        let event = FluxEvent::GoalCompleted {
            goal_id: "g1".into(),
        };
        let actions = resolve_actions(&event, &rules);
        assert_eq!(actions.len(), 1);
    }
}
