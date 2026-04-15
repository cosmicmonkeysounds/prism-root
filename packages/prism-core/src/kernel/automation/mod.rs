//! `kernel::automation` — trigger / condition / action engine.
//!
//! Port of `kernel/automation/*.ts` at 8426588 per ADR-002 §Part C. The
//! Rust engine is synchronous and timer-agnostic: hosts drive cron via
//! [`AutomationEngine::tick_cron`] and provide a [`DelaySleeper`] for
//! `delay` actions. `AutomationStore` and `ActionHandler` are traits so
//! the host layer owns persistence and the actual effects.

pub mod condition;
pub mod engine;
pub mod types;

pub use condition::{
    compare, context_to_json, evaluate_condition, get_path, interpolate, matches_object_trigger,
};
pub use engine::{
    parse_cron_to_interval_ms, AutomationEngine, AutomationEngineOptions, AutomationFilter,
    AutomationStore, DelaySleeper, NoopSleeper,
};
pub use types::{
    ActionHandler, ActionResult, ActionStatus, Automation, AutomationAction, AutomationCondition,
    AutomationContext, AutomationRun, AutomationTrigger, FieldOperator, ObjectEvent,
    ObjectEventType, ObjectTriggerFilter, RunStatus, TagMode,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Map as JsonMap, Value as JsonValue};
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    // ── Test store ──────────────────────────────────────────────────────────

    #[derive(Default)]
    struct TestStore {
        inner: Mutex<TestStoreState>,
    }

    #[derive(Default)]
    struct TestStoreState {
        automations: Vec<Automation>,
        runs: Vec<AutomationRun>,
    }

    impl TestStore {
        fn with(automations: Vec<Automation>) -> Arc<Self> {
            Arc::new(Self {
                inner: Mutex::new(TestStoreState {
                    automations,
                    runs: Vec::new(),
                }),
            })
        }

        fn runs(&self) -> Vec<AutomationRun> {
            self.inner.lock().unwrap().runs.clone()
        }
    }

    impl AutomationStore for TestStore {
        fn list(&self, filter: &AutomationFilter) -> Vec<Automation> {
            let state = self.inner.lock().unwrap();
            state
                .automations
                .iter()
                .filter(|a| {
                    if let Some(enabled) = filter.enabled {
                        if a.enabled != enabled {
                            return false;
                        }
                    }
                    if let Some(tt) = &filter.trigger_type {
                        if a.trigger.type_tag() != tt {
                            return false;
                        }
                    }
                    true
                })
                .cloned()
                .collect()
        }

        fn get(&self, id: &str) -> Option<Automation> {
            self.inner
                .lock()
                .unwrap()
                .automations
                .iter()
                .find(|a| a.id == id)
                .cloned()
        }

        fn save(&self, automation: Automation) {
            let mut state = self.inner.lock().unwrap();
            if let Some(idx) = state.automations.iter().position(|a| a.id == automation.id) {
                state.automations[idx] = automation;
            } else {
                state.automations.push(automation);
            }
        }

        fn save_run(&self, run: AutomationRun) {
            self.inner.lock().unwrap().runs.push(run);
        }
    }

    // ── Helpers ─────────────────────────────────────────────────────────────

    fn base_automation() -> Automation {
        Automation {
            id: "auto-1".into(),
            name: "Test".into(),
            description: None,
            enabled: true,
            trigger: AutomationTrigger::Manual,
            conditions: vec![],
            actions: vec![],
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
            last_run_at: None,
            run_count: 0,
        }
    }

    fn options_pinned() -> AutomationEngineOptions {
        AutomationEngineOptions {
            now: Some(Box::new(|| "2026-01-15T10:00:00Z".into())),
            id: Some(Box::new(|| "run-1".into())),
            on_run_complete: None,
            sleeper: None,
        }
    }

    struct LoggingHandler {
        log: Arc<Mutex<Vec<String>>>,
        fail_after: Option<usize>,
    }

    impl LoggingHandler {
        fn new(log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                log,
                fail_after: None,
            }
        }
        fn failing_on(log: Arc<Mutex<Vec<String>>>, after: usize) -> Self {
            Self {
                log,
                fail_after: Some(after),
            }
        }
    }

    impl ActionHandler for LoggingHandler {
        fn handle(
            &self,
            action: &AutomationAction,
            _ctx: &AutomationContext,
        ) -> Result<(), String> {
            let mut log = self.log.lock().unwrap();
            let count = log.len();
            log.push(action.type_tag().into());
            if let Some(after) = self.fail_after {
                if count >= after {
                    return Err("boom".into());
                }
            }
            Ok(())
        }
    }

    // ── Condition evaluation ────────────────────────────────────────────────

    #[test]
    fn compare_eq_covers_null_and_numbers() {
        assert!(compare(Some(&json!(5)), FieldOperator::Eq, &json!(5)));
        assert!(!compare(Some(&json!(5)), FieldOperator::Eq, &json!(6)));
        assert!(compare(None, FieldOperator::Eq, &JsonValue::Null));
        assert!(compare(None, FieldOperator::Neq, &json!(1)));
    }

    #[test]
    fn compare_numeric_ops() {
        assert!(compare(Some(&json!(10)), FieldOperator::Gt, &json!(5)));
        assert!(compare(Some(&json!(10)), FieldOperator::Gte, &json!(10)));
        assert!(compare(Some(&json!(3)), FieldOperator::Lt, &json!(10)));
        assert!(compare(Some(&json!(3)), FieldOperator::Lte, &json!(3)));
    }

    #[test]
    fn compare_string_ops() {
        assert!(compare(
            Some(&json!("foobar")),
            FieldOperator::Contains,
            &json!("oob")
        ));
        assert!(compare(
            Some(&json!("foobar")),
            FieldOperator::StartsWith,
            &json!("foo")
        ));
        assert!(compare(
            Some(&json!("foobar")),
            FieldOperator::EndsWith,
            &json!("bar")
        ));
        assert!(compare(
            Some(&json!("hello")),
            FieldOperator::Matches,
            &json!("^h.*o$")
        ));
    }

    #[test]
    fn get_path_walks_nested() {
        let v = json!({ "object": { "data": { "priority": "high" } } });
        assert_eq!(get_path(&v, "object.data.priority"), Some(&json!("high")));
        assert_eq!(get_path(&v, "object.missing"), None);
    }

    #[test]
    fn evaluate_condition_field_and_logical() {
        let ctx = AutomationContext {
            automation_id: "a".into(),
            triggered_at: "t".into(),
            trigger_type: "manual".into(),
            object: Some(
                json!({ "type": "task", "tags": ["urgent"], "status": "open" })
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            previous_object: None,
            extra: None,
        };
        let field = AutomationCondition::Field {
            path: "object.status".into(),
            operator: FieldOperator::Eq,
            value: json!("open"),
        };
        assert!(evaluate_condition(&field, &ctx));

        let not = AutomationCondition::Not {
            condition: Box::new(field.clone()),
        };
        assert!(!evaluate_condition(&not, &ctx));

        let and = AutomationCondition::And {
            conditions: vec![
                field.clone(),
                AutomationCondition::Type {
                    object_type: "task".into(),
                },
            ],
        };
        assert!(evaluate_condition(&and, &ctx));

        let tags = AutomationCondition::Tags {
            tags: vec!["urgent".into()],
            mode: TagMode::All,
        };
        assert!(evaluate_condition(&tags, &ctx));
    }

    #[test]
    fn interpolate_replaces_dot_paths() {
        let ctx = AutomationContext {
            automation_id: "a".into(),
            triggered_at: "t".into(),
            trigger_type: "manual".into(),
            object: None,
            previous_object: None,
            extra: Some(json!({ "source": "email" }).as_object().unwrap().clone()),
        };
        let tpl = json!({ "name": "Follow-up: {{extra.source}}", "count": 2 });
        let out = interpolate(&tpl, &ctx);
        assert_eq!(out["name"], json!("Follow-up: email"));
        assert_eq!(out["count"], json!(2));
    }

    #[test]
    fn matches_object_trigger_filters() {
        let filter = ObjectTriggerFilter {
            object_types: Some(vec!["task".into()]),
            tags: Some(vec!["urgent".into()]),
            field_match: None,
        };
        let task = json!({ "type": "task", "tags": ["urgent"] })
            .as_object()
            .unwrap()
            .clone();
        assert!(matches_object_trigger(&filter, &task));
        let goal = json!({ "type": "goal", "tags": ["urgent"] })
            .as_object()
            .unwrap()
            .clone();
        assert!(!matches_object_trigger(&filter, &goal));
    }

    // ── Engine lifecycle ────────────────────────────────────────────────────

    #[test]
    fn run_manual_automation() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let auto = Automation {
            actions: vec![AutomationAction::CreateObject {
                object_type: "task".into(),
                template: json!({ "name": "auto" }).as_object().unwrap().clone(),
                parent_from_trigger: None,
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let mut handlers: HashMap<String, Arc<dyn ActionHandler>> = HashMap::new();
        handlers.insert(
            "object:create".into(),
            Arc::new(LoggingHandler::new(log.clone())),
        );
        let engine = AutomationEngine::new(store.clone(), handlers, options_pinned());

        let run = engine.run("auto-1", None).unwrap();
        assert_eq!(run.status, RunStatus::Success);
        assert!(run.condition_passed);
        assert_eq!(run.action_results.len(), 1);
        assert_eq!(run.action_results[0].status, ActionStatus::Success);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            &["object:create".to_string()]
        );
        assert_eq!(store.runs().len(), 1);
    }

    #[test]
    fn skips_when_conditions_fail() {
        let auto = Automation {
            conditions: vec![AutomationCondition::Field {
                path: "extra.priority".into(),
                operator: FieldOperator::Eq,
                value: json!("high"),
            }],
            actions: vec![AutomationAction::CreateObject {
                object_type: "task".into(),
                template: JsonMap::new(),
                parent_from_trigger: None,
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store, HashMap::new(), options_pinned());

        let run = engine
            .run(
                "auto-1",
                Some(json!({ "priority": "low" }).as_object().unwrap().clone()),
            )
            .unwrap();
        assert_eq!(run.status, RunStatus::Skipped);
        assert!(!run.condition_passed);
        assert!(run.action_results.is_empty());
    }

    #[test]
    fn handles_object_event_with_filters() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let auto = Automation {
            trigger: AutomationTrigger::ObjectCreated {
                object_types: Some(vec!["task".into()]),
                tags: Some(vec!["urgent".into()]),
                field_match: None,
            },
            actions: vec![AutomationAction::UpdateObject {
                target: "trigger".into(),
                patch: json!({ "status": "flagged" }).as_object().unwrap().clone(),
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let mut handlers: HashMap<String, Arc<dyn ActionHandler>> = HashMap::new();
        handlers.insert(
            "object:update".into(),
            Arc::new(LoggingHandler::new(log.clone())),
        );
        let engine = AutomationEngine::new(store, handlers, options_pinned());

        let runs = engine.handle_object_event(&ObjectEvent {
            event: ObjectEventType::Created,
            object: json!({ "type": "task", "tags": ["urgent"] })
                .as_object()
                .unwrap()
                .clone(),
            previous: None,
        });
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, RunStatus::Success);
        assert_eq!(
            log.lock().unwrap().as_slice(),
            &["object:update".to_string()]
        );

        let no_match = engine.handle_object_event(&ObjectEvent {
            event: ObjectEventType::Created,
            object: json!({ "type": "goal", "tags": ["urgent"] })
                .as_object()
                .unwrap()
                .clone(),
            previous: None,
        });
        assert!(no_match.is_empty());
    }

    #[test]
    fn partial_status_on_mid_sequence_failure() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let auto = Automation {
            actions: vec![
                AutomationAction::CreateObject {
                    object_type: "a".into(),
                    template: JsonMap::new(),
                    parent_from_trigger: None,
                },
                AutomationAction::CreateObject {
                    object_type: "b".into(),
                    template: JsonMap::new(),
                    parent_from_trigger: None,
                },
            ],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let mut handlers: HashMap<String, Arc<dyn ActionHandler>> = HashMap::new();
        handlers.insert(
            "object:create".into(),
            Arc::new(LoggingHandler::failing_on(log, 1)),
        );
        let engine = AutomationEngine::new(store, handlers, options_pinned());

        let run = engine.run("auto-1", None).unwrap();
        assert_eq!(run.status, RunStatus::Partial);
        assert_eq!(run.action_results.len(), 2);
        assert_eq!(run.action_results[0].status, ActionStatus::Success);
        assert_eq!(run.action_results[1].status, ActionStatus::Failed);
        assert!(run.action_results[1]
            .error
            .as_deref()
            .unwrap()
            .contains("boom"));
    }

    #[test]
    fn failed_status_when_first_action_fails() {
        let log = Arc::new(Mutex::new(Vec::new()));
        let auto = Automation {
            actions: vec![AutomationAction::CreateObject {
                object_type: "a".into(),
                template: JsonMap::new(),
                parent_from_trigger: None,
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let mut handlers: HashMap<String, Arc<dyn ActionHandler>> = HashMap::new();
        handlers.insert(
            "object:create".into(),
            Arc::new(LoggingHandler::failing_on(log, 0)),
        );
        let engine = AutomationEngine::new(store, handlers, options_pinned());

        let run = engine.run("auto-1", None).unwrap();
        assert_eq!(run.status, RunStatus::Failed);
    }

    #[test]
    fn skips_actions_with_no_handler() {
        let auto = Automation {
            actions: vec![AutomationAction::Notification {
                target: "someone".into(),
                title: "Hi".into(),
                body: "Test".into(),
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store, HashMap::new(), options_pinned());

        let run = engine.run("auto-1", None).unwrap();
        assert_eq!(run.status, RunStatus::Success);
        assert_eq!(run.action_results[0].status, ActionStatus::Skipped);
        assert!(run.action_results[0]
            .error
            .as_deref()
            .unwrap()
            .contains("No handler"));
    }

    #[test]
    fn interpolates_templates_from_context() {
        let captured: Arc<Mutex<Vec<JsonMap<String, JsonValue>>>> =
            Arc::new(Mutex::new(Vec::new()));

        struct Capture {
            out: Arc<Mutex<Vec<JsonMap<String, JsonValue>>>>,
        }
        impl ActionHandler for Capture {
            fn handle(
                &self,
                action: &AutomationAction,
                _ctx: &AutomationContext,
            ) -> Result<(), String> {
                if let AutomationAction::CreateObject { template, .. } = action {
                    self.out.lock().unwrap().push(template.clone());
                }
                Ok(())
            }
        }

        let auto = Automation {
            actions: vec![AutomationAction::CreateObject {
                object_type: "task".into(),
                template: json!({ "name": "Follow-up: {{extra.source}}" })
                    .as_object()
                    .unwrap()
                    .clone(),
                parent_from_trigger: None,
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let mut handlers: HashMap<String, Arc<dyn ActionHandler>> = HashMap::new();
        handlers.insert(
            "object:create".into(),
            Arc::new(Capture {
                out: captured.clone(),
            }),
        );
        let engine = AutomationEngine::new(store, handlers, options_pinned());

        engine
            .run(
                "auto-1",
                Some(json!({ "source": "email" }).as_object().unwrap().clone()),
            )
            .unwrap();
        let captured = captured.lock().unwrap();
        assert_eq!(captured[0]["name"], json!("Follow-up: email"));
    }

    #[test]
    fn run_count_increments_on_success() {
        let store = TestStore::with(vec![base_automation()]);
        let engine = AutomationEngine::new(store.clone(), HashMap::new(), options_pinned());
        engine.run("auto-1", None).unwrap();
        let updated = store.get("auto-1").unwrap();
        assert_eq!(updated.run_count, 1);
        assert!(updated.last_run_at.is_some());
    }

    #[test]
    fn run_count_unchanged_on_skip() {
        let auto = Automation {
            conditions: vec![AutomationCondition::Field {
                path: "extra.x".into(),
                operator: FieldOperator::Eq,
                value: json!("y"),
            }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store.clone(), HashMap::new(), options_pinned());
        engine
            .run(
                "auto-1",
                Some(json!({ "x": "n" }).as_object().unwrap().clone()),
            )
            .unwrap();
        assert_eq!(store.get("auto-1").unwrap().run_count, 0);
    }

    #[test]
    fn run_errors_on_missing_automation() {
        let store = TestStore::with(vec![]);
        let engine = AutomationEngine::new(store, HashMap::new(), options_pinned());
        let err = engine.run("missing", None).unwrap_err();
        assert!(err.contains("not found"));
    }

    #[test]
    fn run_complete_callback_fires() {
        let received: Arc<Mutex<Vec<RunStatus>>> = Arc::new(Mutex::new(Vec::new()));
        let received_cb = received.clone();
        let opts = AutomationEngineOptions {
            now: Some(Box::new(|| "now".into())),
            id: Some(Box::new(|| "id".into())),
            on_run_complete: Some(Box::new(move |run| {
                received_cb.lock().unwrap().push(run.status);
            })),
            sleeper: None,
        };
        let store = TestStore::with(vec![base_automation()]);
        let engine = AutomationEngine::new(store, HashMap::new(), opts);
        engine.run("auto-1", None).unwrap();
        assert_eq!(received.lock().unwrap().as_slice(), &[RunStatus::Success]);
    }

    // ── Cron ────────────────────────────────────────────────────────────────

    #[test]
    fn parse_cron_intervals() {
        assert_eq!(parse_cron_to_interval_ms("* * * * *"), Some(60_000));
        assert_eq!(parse_cron_to_interval_ms("*/5 * * * *"), Some(300_000));
        assert_eq!(parse_cron_to_interval_ms("0 * * * *"), Some(3_600_000));
        assert_eq!(parse_cron_to_interval_ms("0 0 * * *"), Some(86_400_000));
        assert_eq!(parse_cron_to_interval_ms("garbage"), None);
    }

    #[test]
    fn start_stop_toggles_running() {
        let auto = Automation {
            trigger: AutomationTrigger::Cron {
                cron: "* * * * *".into(),
                timezone: None,
            },
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store, HashMap::new(), options_pinned());
        assert!(!engine.running());
        engine.start();
        assert!(engine.running());
        engine.stop();
        assert!(!engine.running());
    }

    #[test]
    fn tick_cron_fires_due_schedules() {
        let auto = Automation {
            trigger: AutomationTrigger::Cron {
                cron: "* * * * *".into(),
                timezone: None,
            },
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store.clone(), HashMap::new(), options_pinned());
        engine.start();
        let runs = engine.tick_cron(30_000);
        assert!(runs.is_empty());
        let runs = engine.tick_cron(31_000);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, RunStatus::Success);
        assert!(!store.runs().is_empty());
    }

    #[test]
    fn tick_cron_skips_disabled_automations() {
        let auto = Automation {
            enabled: false,
            trigger: AutomationTrigger::Cron {
                cron: "* * * * *".into(),
                timezone: None,
            },
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let engine = AutomationEngine::new(store, HashMap::new(), options_pinned());
        engine.start();
        let runs = engine.tick_cron(120_000);
        assert!(runs.is_empty());
    }

    #[test]
    fn delay_action_runs_through_sleeper() {
        struct CountingSleeper(Arc<Mutex<Vec<f64>>>);
        impl DelaySleeper for CountingSleeper {
            fn sleep(&self, seconds: f64) {
                self.0.lock().unwrap().push(seconds);
            }
        }
        let log = Arc::new(Mutex::new(Vec::new()));
        let auto = Automation {
            actions: vec![AutomationAction::Delay { seconds: 0.5 }],
            ..base_automation()
        };
        let store = TestStore::with(vec![auto]);
        let opts = AutomationEngineOptions {
            now: Some(Box::new(|| "now".into())),
            id: Some(Box::new(|| "id".into())),
            on_run_complete: None,
            sleeper: Some(Arc::new(CountingSleeper(log.clone()))),
        };
        let engine = AutomationEngine::new(store, HashMap::new(), opts);
        let run = engine.run("auto-1", None).unwrap();
        assert_eq!(run.status, RunStatus::Success);
        assert_eq!(run.action_results[0].action_type, "delay");
        assert_eq!(log.lock().unwrap().as_slice(), &[0.5]);
    }

    #[test]
    fn serde_roundtrip() {
        let auto = Automation {
            trigger: AutomationTrigger::ObjectCreated {
                object_types: Some(vec!["task".into()]),
                tags: None,
                field_match: None,
            },
            conditions: vec![
                AutomationCondition::Field {
                    path: "object.status".into(),
                    operator: FieldOperator::Eq,
                    value: json!("open"),
                },
                AutomationCondition::And {
                    conditions: vec![AutomationCondition::Tags {
                        tags: vec!["urgent".into()],
                        mode: TagMode::All,
                    }],
                },
            ],
            actions: vec![AutomationAction::Notification {
                target: "trigger-owner".into(),
                title: "Flag".into(),
                body: "{{object.name}} needs review".into(),
            }],
            ..base_automation()
        };
        let json_str = serde_json::to_string(&auto).unwrap();
        let back: Automation = serde_json::from_str(&json_str).unwrap();
        assert_eq!(back.id, auto.id);
        assert_eq!(back.conditions.len(), 2);
        assert_eq!(back.actions.len(), 1);
    }
}
