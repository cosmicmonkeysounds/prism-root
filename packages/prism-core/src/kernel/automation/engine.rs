//! `automation::engine` — orchestrates trigger evaluation and action
//! dispatch.
//!
//! Port of `kernel/automation/automation-engine.ts`. The TS engine was
//! async and used `setInterval` for cron; the Rust port keeps the same
//! shape but is synchronous and timer-agnostic:
//!
//! - Hosts call [`AutomationEngine::handle_object_event`] when their
//!   store mutates — the engine executes matching automations inline.
//! - Hosts call [`AutomationEngine::tick_cron`] on their own cadence;
//!   the engine fires any cron automation whose next-due stamp has
//!   elapsed.
//! - `delay` actions go through a pluggable [`DelaySleeper`] so tests
//!   can run zero-wait while hosts can back it with a real sleep.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use chrono::Utc;
use serde_json::{Map as JsonMap, Value as JsonValue};

use super::condition::{evaluate_condition, interpolate, matches_object_trigger};
use super::types::{
    ActionHandler, ActionResult, ActionStatus, Automation, AutomationAction, AutomationContext,
    AutomationRun, AutomationTrigger, ObjectEvent, RunStatus,
};

// ── Store trait ─────────────────────────────────────────────────────────────

/// Storage surface the engine reads and writes against.
pub trait AutomationStore: Send + Sync {
    fn list(&self, filter: &AutomationFilter) -> Vec<Automation>;
    fn get(&self, id: &str) -> Option<Automation>;
    fn save(&self, automation: Automation);
    fn save_run(&self, run: AutomationRun);
}

/// Optional filter for [`AutomationStore::list`].
#[derive(Debug, Clone, Default)]
pub struct AutomationFilter {
    pub enabled: Option<bool>,
    pub trigger_type: Option<String>,
}

// ── Delay sleeper ───────────────────────────────────────────────────────────

/// Pluggable sleeper for `delay` actions.
pub trait DelaySleeper: Send + Sync {
    fn sleep(&self, seconds: f64);
}

/// No-op sleeper. Used by tests and hosts that want delay actions to
/// return immediately. Real timed execution should use a host-side
/// implementation that parks the thread (or spawns onto an async runtime
/// and blocks on it).
#[derive(Debug, Default)]
pub struct NoopSleeper;

impl DelaySleeper for NoopSleeper {
    fn sleep(&self, _seconds: f64) {}
}

// ── Engine options ──────────────────────────────────────────────────────────

/// Callback fired when an automation run completes.
pub type RunCompleteCallback = Box<dyn Fn(&AutomationRun) + Send + Sync>;

/// Tunable overrides for the engine (timekeeping, id allocation, etc.).
#[derive(Default)]
pub struct AutomationEngineOptions {
    /// Override for `now()` — returns an ISO-8601 timestamp. Used by
    /// tests to pin timestamps.
    pub now: Option<Box<dyn Fn() -> String + Send + Sync>>,
    /// Override for the run-id generator.
    pub id: Option<Box<dyn Fn() -> String + Send + Sync>>,
    /// Called once after a run finalises.
    pub on_run_complete: Option<RunCompleteCallback>,
    /// Sleeper used to implement `delay` actions. Defaults to
    /// [`NoopSleeper`], which returns immediately.
    pub sleeper: Option<Arc<dyn DelaySleeper>>,
}

// ── Cron interval parser ────────────────────────────────────────────────────

/// Return the minimum sensible interval in milliseconds for a cron
/// expression. Matches the narrow patterns the TS port supported —
/// anything unrecognised falls back to 60 s.
pub fn parse_cron_to_interval_ms(cron: &str) -> Option<u64> {
    let parts: Vec<&str> = cron.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    let minute = parts[0];
    if cron.trim() == "* * * * *" {
        return Some(60_000);
    }
    if let Some(stripped) = minute.strip_prefix("*/") {
        if let Ok(n) = stripped.parse::<u64>() {
            return Some(n * 60_000);
        }
    }
    if minute == "0" && parts[1] == "*" {
        return Some(3_600_000);
    }
    if minute == "0" && parts[1] == "0" {
        return Some(86_400_000);
    }
    Some(60_000)
}

// ── Engine ──────────────────────────────────────────────────────────────────

/// Orchestration core. Thread-safe via internal locks; one engine per
/// store is the expected shape.
pub struct AutomationEngine {
    store: Arc<dyn AutomationStore>,
    handlers: HashMap<String, Arc<dyn ActionHandler>>,
    options: AutomationEngineOptions,
    running: Mutex<bool>,
    cron_schedules: Mutex<HashMap<String, CronSchedule>>,
}

struct CronSchedule {
    interval_ms: u64,
    /// Monotonic ms since engine start; next tick >= `due_ms` fires.
    due_ms: u64,
}

impl AutomationEngine {
    pub fn new(
        store: Arc<dyn AutomationStore>,
        handlers: HashMap<String, Arc<dyn ActionHandler>>,
        options: AutomationEngineOptions,
    ) -> Self {
        Self {
            store,
            handlers,
            options,
            running: Mutex::new(false),
            cron_schedules: Mutex::new(HashMap::new()),
        }
    }

    pub fn running(&self) -> bool {
        *self.running.lock().unwrap()
    }

    /// Activate cron schedules for every enabled cron automation.
    pub fn start(&self) {
        let mut running = self.running.lock().unwrap();
        if *running {
            return;
        }
        *running = true;
        drop(running);

        let mut schedules = self.cron_schedules.lock().unwrap();
        schedules.clear();
        let automations = self.store.list(&AutomationFilter {
            enabled: Some(true),
            trigger_type: None,
        });
        for a in automations {
            if let AutomationTrigger::Cron { cron, .. } = &a.trigger {
                if let Some(interval) = parse_cron_to_interval_ms(cron) {
                    schedules.insert(
                        a.id.clone(),
                        CronSchedule {
                            interval_ms: interval,
                            due_ms: interval,
                        },
                    );
                }
            }
        }
    }

    /// Clear all cron schedules.
    pub fn stop(&self) {
        let mut running = self.running.lock().unwrap();
        *running = false;
        self.cron_schedules.lock().unwrap().clear();
    }

    /// Re-scan the store for a single automation id; update its cron
    /// schedule if it's an enabled cron automation, drop it otherwise.
    pub fn refresh_automation(&self, id: &str) {
        let mut schedules = self.cron_schedules.lock().unwrap();
        schedules.remove(id);
        let Some(automation) = self.store.get(id) else {
            return;
        };
        if !automation.enabled {
            return;
        }
        if let AutomationTrigger::Cron { cron, .. } = &automation.trigger {
            if let Some(interval) = parse_cron_to_interval_ms(cron) {
                schedules.insert(
                    id.to_string(),
                    CronSchedule {
                        interval_ms: interval,
                        due_ms: interval,
                    },
                );
            }
        }
    }

    /// Advance the cron clock by `elapsed_ms`. Every schedule whose
    /// `due_ms` falls within the window fires once and rearms.
    pub fn tick_cron(&self, elapsed_ms: u64) -> Vec<AutomationRun> {
        if !self.running() {
            return Vec::new();
        }
        // Take a snapshot so we can release the lock before executing.
        let mut schedules = self.cron_schedules.lock().unwrap();
        let mut due_ids: Vec<String> = Vec::new();
        let mut drop_ids: Vec<String> = Vec::new();
        for (id, sched) in schedules.iter_mut() {
            let next = sched.due_ms.saturating_sub(elapsed_ms);
            if next == 0 || sched.due_ms <= elapsed_ms {
                // Check that the automation is still enabled.
                match self.store.get(id) {
                    Some(a) if a.enabled && matches!(a.trigger, AutomationTrigger::Cron { .. }) => {
                        due_ids.push(id.clone());
                        sched.due_ms = sched.interval_ms;
                    }
                    _ => drop_ids.push(id.clone()),
                }
            } else {
                sched.due_ms = next;
            }
        }
        for id in drop_ids {
            schedules.remove(&id);
        }
        drop(schedules);

        let mut runs = Vec::new();
        for id in due_ids {
            if let Some(automation) = self.store.get(&id) {
                let ctx = AutomationContext {
                    automation_id: automation.id.clone(),
                    triggered_at: self.now(),
                    trigger_type: "cron".into(),
                    object: None,
                    previous_object: None,
                    extra: None,
                };
                runs.push(self.execute(&automation, ctx));
            }
        }
        runs
    }

    /// Route an object lifecycle event through every matching
    /// automation. Returns the resulting runs (possibly empty).
    pub fn handle_object_event(&self, event: &ObjectEvent) -> Vec<AutomationRun> {
        let trigger_type = event.event.as_str().to_string();
        let automations = self.store.list(&AutomationFilter {
            enabled: Some(true),
            trigger_type: Some(trigger_type.clone()),
        });
        let mut runs = Vec::new();
        for a in automations {
            let Some(evt) = a.trigger.object_event() else {
                continue;
            };
            if evt != event.event {
                continue;
            }
            let Some(filter) = a.trigger.as_object_filter() else {
                continue;
            };
            if !matches_object_trigger(&filter, &event.object) {
                continue;
            }
            let ctx = AutomationContext {
                automation_id: a.id.clone(),
                triggered_at: self.now(),
                trigger_type: trigger_type.clone(),
                object: Some(event.object.clone()),
                previous_object: event.previous.clone(),
                extra: None,
            };
            runs.push(self.execute(&a, ctx));
        }
        runs
    }

    /// Run an automation manually. Errors if the id is unknown.
    pub fn run(
        &self,
        automation_id: &str,
        extra: Option<JsonMap<String, JsonValue>>,
    ) -> Result<AutomationRun, String> {
        let automation = self
            .store
            .get(automation_id)
            .ok_or_else(|| format!("Automation '{automation_id}' not found"))?;
        let ctx = AutomationContext {
            automation_id: automation_id.to_string(),
            triggered_at: self.now(),
            trigger_type: "manual".into(),
            object: None,
            previous_object: None,
            extra,
        };
        Ok(self.execute(&automation, ctx))
    }

    // ── Execution ───────────────────────────────────────────────────────────

    fn execute(&self, automation: &Automation, ctx: AutomationContext) -> AutomationRun {
        let run_id = self.random_id();

        let condition_passed = automation
            .conditions
            .iter()
            .all(|c| evaluate_condition(c, &ctx));

        let mut run = AutomationRun {
            id: run_id,
            automation_id: automation.id.clone(),
            status: RunStatus::Skipped,
            triggered_at: ctx.triggered_at.clone(),
            completed_at: None,
            condition_passed,
            action_results: Vec::new(),
            error: None,
        };

        if !condition_passed {
            self.finalise(&mut run, automation);
            return run;
        }

        let mut results: Vec<ActionResult> = Vec::new();
        let mut failed = false;

        for (idx, action) in automation.actions.iter().enumerate() {
            let action_type = action.type_tag().to_string();
            let start = std::time::Instant::now();

            if let AutomationAction::Delay { seconds } = action {
                self.sleeper().sleep(*seconds);
                results.push(ActionResult {
                    action_index: idx,
                    action_type,
                    status: ActionStatus::Success,
                    error: None,
                    elapsed_ms: Some(start.elapsed().as_millis() as u64),
                });
                continue;
            }

            let Some(handler) = self.handlers.get(action.type_tag()) else {
                results.push(ActionResult {
                    action_index: idx,
                    action_type,
                    status: ActionStatus::Skipped,
                    error: Some(format!(
                        "No handler registered for action type '{}'",
                        action.type_tag()
                    )),
                    elapsed_ms: None,
                });
                continue;
            };

            let interpolated = interpolate_action(action, &ctx);
            match handler.handle(&interpolated, &ctx) {
                Ok(()) => results.push(ActionResult {
                    action_index: idx,
                    action_type,
                    status: ActionStatus::Success,
                    error: None,
                    elapsed_ms: Some(start.elapsed().as_millis() as u64),
                }),
                Err(err) => {
                    results.push(ActionResult {
                        action_index: idx,
                        action_type,
                        status: ActionStatus::Failed,
                        error: Some(err),
                        elapsed_ms: Some(start.elapsed().as_millis() as u64),
                    });
                    failed = true;
                    break;
                }
            }
        }

        run.status = if failed {
            if results.iter().any(|r| r.status == ActionStatus::Success) {
                RunStatus::Partial
            } else {
                RunStatus::Failed
            }
        } else {
            RunStatus::Success
        };
        run.action_results = results;
        self.finalise(&mut run, automation);
        run
    }

    fn finalise(&self, run: &mut AutomationRun, automation: &Automation) {
        run.completed_at = Some(self.now());
        self.store.save_run(run.clone());

        if run.status == RunStatus::Success {
            let updated = Automation {
                last_run_at: run.completed_at.clone(),
                run_count: automation.run_count + 1,
                updated_at: run.completed_at.clone().unwrap_or_default(),
                ..automation.clone()
            };
            self.store.save(updated);
        }

        if let Some(cb) = &self.options.on_run_complete {
            cb(run);
        }
    }

    fn now(&self) -> String {
        match &self.options.now {
            Some(f) => f(),
            None => Utc::now().to_rfc3339(),
        }
    }

    fn random_id(&self) -> String {
        match &self.options.id {
            Some(f) => f(),
            None => {
                use rand::RngCore;
                let mut bytes = [0u8; 8];
                rand::thread_rng().fill_bytes(&mut bytes);
                hex::encode(bytes)
            }
        }
    }

    fn sleeper(&self) -> Arc<dyn DelaySleeper> {
        self.options
            .sleeper
            .clone()
            .unwrap_or_else(|| Arc::new(NoopSleeper))
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn interpolated_map(
    map: &JsonMap<String, JsonValue>,
    ctx: &AutomationContext,
) -> JsonMap<String, JsonValue> {
    let v = interpolate(&JsonValue::Object(map.clone()), ctx);
    match v {
        JsonValue::Object(m) => m,
        _ => JsonMap::new(),
    }
}

fn interpolated_string(src: &str, ctx: &AutomationContext) -> String {
    match interpolate(&JsonValue::String(src.into()), ctx) {
        JsonValue::String(s) => s,
        other => other.to_string(),
    }
}

fn interpolate_action(action: &AutomationAction, ctx: &AutomationContext) -> AutomationAction {
    match action {
        AutomationAction::CreateObject {
            object_type,
            template,
            parent_from_trigger,
        } => AutomationAction::CreateObject {
            object_type: object_type.clone(),
            template: interpolated_map(template, ctx),
            parent_from_trigger: *parent_from_trigger,
        },
        AutomationAction::UpdateObject { target, patch } => AutomationAction::UpdateObject {
            target: interpolated_string(target, ctx),
            patch: interpolated_map(patch, ctx),
        },
        AutomationAction::DeleteObject { target } => AutomationAction::DeleteObject {
            target: interpolated_string(target, ctx),
        },
        AutomationAction::Notification {
            target,
            title,
            body,
        } => AutomationAction::Notification {
            target: interpolated_string(target, ctx),
            title: interpolated_string(title, ctx),
            body: interpolated_string(body, ctx),
        },
        AutomationAction::Delay { seconds } => AutomationAction::Delay { seconds: *seconds },
        AutomationAction::RunAutomation { automation_id } => AutomationAction::RunAutomation {
            automation_id: automation_id.clone(),
        },
        AutomationAction::Email {
            to,
            subject,
            body,
            template_id,
        } => AutomationAction::Email {
            to: interpolated_string(to, ctx),
            subject: interpolated_string(subject, ctx),
            body: interpolated_string(body, ctx),
            template_id: template_id.clone(),
        },
    }
}
