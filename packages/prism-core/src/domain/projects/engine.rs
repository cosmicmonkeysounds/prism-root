//! Projects engine — risk scoring, scope tracking, velocity, health.
//!
//! Port of `@helm/projects` TypeScript module. CPM critical path
//! already exists in `domain::graph_analysis`.

use super::types::*;

pub fn score_risk(risk: &Risk) -> RiskScore {
    let score = risk.impact.min(5) * risk.probability.min(5);
    let severity = match score {
        0..=4 => RiskSeverity::Low,
        5..=9 => RiskSeverity::Medium,
        10..=16 => RiskSeverity::High,
        _ => RiskSeverity::Critical,
    };
    RiskScore {
        risk_id: risk.id.clone(),
        score,
        severity,
    }
}

pub fn score_risks(risks: &[Risk]) -> Vec<RiskScore> {
    risks.iter().map(score_risk).collect()
}

pub fn scope_snapshot(original_points: i32, changes: &[ScopeChange]) -> ScopeSnapshot {
    let net_change: i32 = changes
        .iter()
        .map(|c| c.points_added - c.points_removed)
        .sum();
    let current_points = original_points + net_change;
    let creep_percentage = if original_points > 0 {
        (net_change as f64 / original_points as f64) * 100.0
    } else {
        0.0
    };

    ScopeSnapshot {
        original_points,
        current_points,
        change_count: changes.len() as u32,
        net_change,
        creep_percentage,
    }
}

pub fn compute_velocity(sprints: &[SprintData]) -> VelocityStats {
    if sprints.is_empty() {
        return VelocityStats {
            average_velocity: 0.0,
            min_velocity: 0,
            max_velocity: 0,
            trend: VelocityTrend::Stable,
            sprint_count: 0,
        };
    }

    let velocities: Vec<i32> = sprints.iter().map(|s| s.completed_points).collect();
    let sum: i32 = velocities.iter().sum();
    let average_velocity = sum as f64 / velocities.len() as f64;
    let min_velocity = *velocities.iter().min().unwrap();
    let max_velocity = *velocities.iter().max().unwrap();

    let trend = if velocities.len() >= 3 {
        let last_half = &velocities[velocities.len() / 2..];
        let first_half = &velocities[..velocities.len() / 2];
        let first_avg: f64 =
            first_half.iter().sum::<i32>() as f64 / first_half.len().max(1) as f64;
        let last_avg: f64 =
            last_half.iter().sum::<i32>() as f64 / last_half.len().max(1) as f64;
        let diff = last_avg - first_avg;
        if diff > 1.0 {
            VelocityTrend::Improving
        } else if diff < -1.0 {
            VelocityTrend::Declining
        } else {
            VelocityTrend::Stable
        }
    } else {
        VelocityTrend::Stable
    };

    VelocityStats {
        average_velocity,
        min_velocity,
        max_velocity,
        trend,
        sprint_count: sprints.len() as u32,
    }
}

pub fn project_health(
    schedule_ratio: f64,
    scope_snapshot: &ScopeSnapshot,
    risk_scores: &[RiskScore],
) -> ProjectHealthScore {
    let schedule_health = schedule_ratio.clamp(0.0, 1.0);

    let scope_health = if scope_snapshot.original_points == 0 {
        1.0
    } else {
        let creep = scope_snapshot.creep_percentage.abs();
        (1.0 - creep / 100.0).clamp(0.0, 1.0)
    };

    let risk_health = if risk_scores.is_empty() {
        1.0
    } else {
        let max_possible = risk_scores.len() as f64 * 25.0;
        let actual: f64 = risk_scores.iter().map(|r| r.score as f64).sum();
        (1.0 - actual / max_possible).clamp(0.0, 1.0)
    };

    let overall = (schedule_health * 0.4 + scope_health * 0.3 + risk_health * 0.3).clamp(0.0, 1.0);

    ProjectHealthScore {
        overall,
        schedule_health,
        scope_health,
        risk_health,
    }
}

pub fn generate_burndown(total_points: i32, sprints: &[SprintData]) -> Vec<BurndownPoint> {
    if sprints.is_empty() {
        return Vec::new();
    }

    let sprint_count = sprints.len() as f64;
    let ideal_per_sprint = total_points as f64 / sprint_count;
    let mut remaining = total_points;
    let mut points = Vec::new();

    for (i, sprint) in sprints.iter().enumerate() {
        remaining -= sprint.completed_points;
        points.push(BurndownPoint {
            date: sprint.end_date.clone(),
            remaining,
            ideal: total_points as f64 - ideal_per_sprint * (i + 1) as f64,
        });
    }

    points
}

pub fn generate_burnup(sprints: &[SprintData]) -> Vec<BurnupPoint> {
    let mut completed = 0;
    let scope: i32 = sprints.iter().map(|s| s.planned_points).sum();
    let mut points = Vec::new();

    for sprint in sprints {
        completed += sprint.completed_points;
        points.push(BurnupPoint {
            date: sprint.end_date.clone(),
            completed,
            total_scope: scope,
        });
    }

    points
}

// ── Widget contributions ─────────────────────────────────────────

pub fn widget_contributions() -> Vec<crate::widget::WidgetContribution> {
    use crate::widget::{
        FieldSpec, LayoutDirection, NumericBounds, SignalSpec, TemplateNode, ToolbarAction,
        WidgetCategory, WidgetContribution, WidgetSize, WidgetTemplate,
    };
    use serde_json::json;

    vec![
        WidgetContribution {
            id: "project-health".into(),
            label: "Project Health".into(),
            description: "Composite health gauge from schedule, scope, and risk".into(),
            icon: Some("activity".into()),
            category: WidgetCategory::Display,
            signals: vec![SignalSpec::new("axis-selected", "A health axis was selected")
                .with_payload(vec![FieldSpec::text("axis", "Axis")])],
            default_size: WidgetSize::new(2, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Horizontal,
                    gap: Some(16),
                    padding: Some(12),
                    children: vec![TemplateNode::DataBinding {
                        field: "overall".into(),
                        component_id: "text".into(),
                        prop_key: "body".into(),
                    }],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "risk-matrix".into(),
            label: "Risk Matrix".into(),
            description: "Impact x probability grid for risk register".into(),
            icon: Some("alert-triangle".into()),
            category: WidgetCategory::Display,
            signals: vec![SignalSpec::new("risk-selected", "A risk was selected")
                .with_payload(vec![FieldSpec::text("risk_id", "Risk ID")])],
            toolbar_actions: vec![ToolbarAction::signal("add-risk", "Add Risk", "plus")],
            default_size: WidgetSize::new(2, 2),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(4),
                    padding: Some(8),
                    children: vec![
                        TemplateNode::Component {
                            component_id: "heading".into(),
                            props: json!({"body": "Risk Matrix", "level": 3}),
                        },
                        TemplateNode::Repeater {
                            source: "risks".into(),
                            item_template: Box::new(TemplateNode::DataBinding {
                                field: "title".into(),
                                component_id: "text".into(),
                                prop_key: "body".into(),
                            }),
                            empty_label: Some("No risks".into()),
                        },
                    ],
                },
            },
            ..Default::default()
        },
        WidgetContribution {
            id: "velocity-chart".into(),
            label: "Velocity Chart".into(),
            description: "Sprint velocity trend over time".into(),
            icon: Some("trending-up".into()),
            category: WidgetCategory::Display,
            config_fields: vec![FieldSpec::number(
                "sprint_count",
                "Sprints to Show",
                NumericBounds::min_max(1.0, 20.0),
            )
            .with_default(json!(6))],
            signals: vec![SignalSpec::new("sprint-selected", "A sprint was selected")
                .with_payload(vec![FieldSpec::text("sprint_id", "Sprint ID")])],
            default_size: WidgetSize::new(2, 1),
            template: WidgetTemplate {
                root: TemplateNode::Container {
                    direction: LayoutDirection::Vertical,
                    gap: Some(8),
                    padding: Some(12),
                    children: vec![TemplateNode::DataBinding {
                        field: "average_velocity".into(),
                        component_id: "text".into(),
                        prop_key: "body".into(),
                    }],
                },
            },
            ..Default::default()
        },
    ]
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_risk(id: &str, impact: u8, probability: u8) -> Risk {
        Risk {
            id: id.into(),
            title: format!("Risk {id}"),
            description: String::new(),
            impact,
            probability,
            mitigation: None,
            status: RiskStatus::Open,
        }
    }

    fn make_sprint(id: &str, planned: i32, completed: i32) -> SprintData {
        SprintData {
            sprint_id: id.into(),
            planned_points: planned,
            completed_points: completed,
            start_date: "2026-01-01".into(),
            end_date: "2026-01-14".into(),
        }
    }

    #[test]
    fn risk_score_low() {
        let score = score_risk(&make_risk("r1", 1, 2));
        assert_eq!(score.score, 2);
        assert_eq!(score.severity, RiskSeverity::Low);
    }

    #[test]
    fn risk_score_medium() {
        let score = score_risk(&make_risk("r1", 3, 3));
        assert_eq!(score.score, 9);
        assert_eq!(score.severity, RiskSeverity::Medium);
    }

    #[test]
    fn risk_score_high() {
        let score = score_risk(&make_risk("r1", 4, 3));
        assert_eq!(score.score, 12);
        assert_eq!(score.severity, RiskSeverity::High);
    }

    #[test]
    fn risk_score_critical() {
        let score = score_risk(&make_risk("r1", 5, 5));
        assert_eq!(score.score, 25);
        assert_eq!(score.severity, RiskSeverity::Critical);
    }

    #[test]
    fn scope_no_changes() {
        let snap = scope_snapshot(100, &[]);
        assert_eq!(snap.current_points, 100);
        assert_eq!(snap.net_change, 0);
        assert!((snap.creep_percentage).abs() < f64::EPSILON);
    }

    #[test]
    fn scope_with_creep() {
        let changes = vec![
            ScopeChange {
                id: "c1".into(),
                description: "Added".into(),
                points_added: 20,
                points_removed: 0,
                date: "2026-02-01".into(),
            },
            ScopeChange {
                id: "c2".into(),
                description: "Removed".into(),
                points_added: 0,
                points_removed: 5,
                date: "2026-02-15".into(),
            },
        ];
        let snap = scope_snapshot(100, &changes);
        assert_eq!(snap.current_points, 115);
        assert_eq!(snap.net_change, 15);
        assert!((snap.creep_percentage - 15.0).abs() < f64::EPSILON);
    }

    #[test]
    fn velocity_empty() {
        let stats = compute_velocity(&[]);
        assert_eq!(stats.sprint_count, 0);
        assert_eq!(stats.trend, VelocityTrend::Stable);
    }

    #[test]
    fn velocity_improving() {
        let sprints = vec![
            make_sprint("s1", 20, 10),
            make_sprint("s2", 20, 12),
            make_sprint("s3", 20, 15),
            make_sprint("s4", 20, 18),
        ];
        let stats = compute_velocity(&sprints);
        assert_eq!(stats.trend, VelocityTrend::Improving);
    }

    #[test]
    fn velocity_declining() {
        let sprints = vec![
            make_sprint("s1", 20, 20),
            make_sprint("s2", 20, 18),
            make_sprint("s3", 20, 12),
            make_sprint("s4", 20, 8),
        ];
        let stats = compute_velocity(&sprints);
        assert_eq!(stats.trend, VelocityTrend::Declining);
    }

    #[test]
    fn health_perfect() {
        let snap = scope_snapshot(100, &[]);
        let health = project_health(1.0, &snap, &[]);
        assert!((health.overall - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn health_with_issues() {
        let changes = vec![ScopeChange {
            id: "c1".into(),
            description: "Added".into(),
            points_added: 50,
            points_removed: 0,
            date: "2026-01-01".into(),
        }];
        let snap = scope_snapshot(100, &changes);
        let risks = vec![RiskScore {
            risk_id: "r1".into(),
            score: 15,
            severity: RiskSeverity::High,
        }];
        let health = project_health(0.8, &snap, &risks);
        assert!(health.overall < 1.0);
        assert!((health.scope_health - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn burndown_basic() {
        let sprints = vec![make_sprint("s1", 15, 10), make_sprint("s2", 15, 12)];
        let points = generate_burndown(30, &sprints);
        assert_eq!(points.len(), 2);
        assert_eq!(points[0].remaining, 20);
        assert_eq!(points[1].remaining, 8);
    }

    #[test]
    fn burnup_basic() {
        let sprints = vec![make_sprint("s1", 15, 10), make_sprint("s2", 15, 12)];
        let points = generate_burnup(&sprints);
        assert_eq!(points[0].completed, 10);
        assert_eq!(points[1].completed, 22);
        assert_eq!(points[0].total_scope, 30);
    }

    #[test]
    fn burndown_empty() {
        assert!(generate_burndown(30, &[]).is_empty());
    }

    #[test]
    fn widget_contributions_count() {
        let widgets = widget_contributions();
        assert_eq!(widgets.len(), 3);
        assert_eq!(widgets[0].id, "project-health");
        assert_eq!(widgets[1].id, "risk-matrix");
        assert_eq!(widgets[2].id, "velocity-chart");
    }
}
