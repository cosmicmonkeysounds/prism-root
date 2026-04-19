//! `kernel::builder::manager` — side-effectful build pipeline owner.
//!
//! Port of `kernel/builder/builder-manager.ts` at 8426588. The
//! BuilderManager holds the registered `AppProfile` set, tracks the
//! current pinned profile, plans and runs builds via an injected
//! [`BuildExecutor`], and fans per-run state changes to subscribers.
//!
//! Architectural deviation: the TS manager is async because it was
//! meant to dispatch through IPC. In the all-Rust stack the
//! daemon interface is an in-process trait, so `run_plan` is
//! synchronous. Async hosts wrap a `TokioBuildExecutor` around their
//! daemon handle and block in the executor itself.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use super::build_plan::{create_build_plan, CreateBuildPlanOptions};
use super::profiles::list_builtin_profiles;
use super::types::{
    AppProfile, ArtifactDescriptor, BuildPlan, BuildRun, BuildStep, BuildStepResult,
    BuildStepStatus, BuildTarget, BuiltInProfileId, ALL_BUILD_TARGETS,
};

// ── Execution types ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct BuildExecutionContext {
    pub working_dir: String,
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorMode {
    Daemon,
    DryRun,
}

pub trait BuildExecutor: Send + Sync {
    fn mode(&self) -> ExecutorMode;
    fn execute_step(&self, step: &BuildStep, context: &BuildExecutionContext) -> BuildStepResult;
}

// ── Dry-run executor ───────────────────────────────────────────────────────

pub struct DryRunExecutor;

impl BuildExecutor for DryRunExecutor {
    fn mode(&self) -> ExecutorMode {
        ExecutorMode::DryRun
    }

    fn execute_step(&self, step: &BuildStep, _context: &BuildExecutionContext) -> BuildStepResult {
        let started_at = now_ms();
        match step {
            BuildStep::EmitFile { contents, .. } => BuildStepResult {
                step: step.clone(),
                status: BuildStepStatus::Success,
                started_at,
                finished_at: Some(now_ms()),
                stdout: Some(contents.clone()),
                stderr: None,
                error_message: None,
            },
            _ => BuildStepResult {
                step: step.clone(),
                status: BuildStepStatus::Skipped,
                started_at,
                finished_at: Some(now_ms()),
                stdout: Some(format!("[dry-run] Skipped: {}", step.description())),
                stderr: None,
                error_message: None,
            },
        }
    }
}

pub fn create_dry_run_executor() -> Arc<dyn BuildExecutor> {
    Arc::new(DryRunExecutor)
}

// ── Daemon-backed executor (callback-driven) ───────────────────────────────

/// Executor that dispatches each step through a caller-supplied
/// `invoke` callback. Hosts that talk to the Prism Daemon wrap
/// their IPC layer in this closure; tests can supply a mock.
pub struct CallbackExecutor<F>
where
    F: Fn(&BuildStep, &BuildExecutionContext) -> Result<CallbackExecutorOutput, String>
        + Send
        + Sync,
{
    invoke: F,
}

#[derive(Debug, Clone, Default)]
pub struct CallbackExecutorOutput {
    pub stdout: Option<String>,
    pub stderr: Option<String>,
}

impl<F> CallbackExecutor<F>
where
    F: Fn(&BuildStep, &BuildExecutionContext) -> Result<CallbackExecutorOutput, String>
        + Send
        + Sync,
{
    pub fn new(invoke: F) -> Self {
        Self { invoke }
    }
}

impl<F> BuildExecutor for CallbackExecutor<F>
where
    F: Fn(&BuildStep, &BuildExecutionContext) -> Result<CallbackExecutorOutput, String>
        + Send
        + Sync
        + 'static,
{
    fn mode(&self) -> ExecutorMode {
        ExecutorMode::Daemon
    }

    fn execute_step(&self, step: &BuildStep, context: &BuildExecutionContext) -> BuildStepResult {
        let started_at = now_ms();
        match (self.invoke)(step, context) {
            Ok(out) => BuildStepResult {
                step: step.clone(),
                status: BuildStepStatus::Success,
                started_at,
                finished_at: Some(now_ms()),
                stdout: out.stdout,
                stderr: out.stderr,
                error_message: None,
            },
            Err(err) => BuildStepResult {
                step: step.clone(),
                status: BuildStepStatus::Failed,
                started_at,
                finished_at: Some(now_ms()),
                stdout: None,
                stderr: None,
                error_message: Some(err),
            },
        }
    }
}

pub fn create_callback_executor<F>(invoke: F) -> Arc<dyn BuildExecutor>
where
    F: Fn(&BuildStep, &BuildExecutionContext) -> Result<CallbackExecutorOutput, String>
        + Send
        + Sync
        + 'static,
{
    Arc::new(CallbackExecutor::new(invoke))
}

// ── Manager options + state ────────────────────────────────────────────────

#[derive(Default)]
pub struct BuilderManagerOptions {
    pub executor: Option<Arc<dyn BuildExecutor>>,
    pub profiles: Vec<AppProfile>,
}

pub type BuilderListener = Box<dyn Fn() + Send + Sync>;
pub type BuilderUnsubscribe = Box<dyn FnOnce() + Send + Sync>;

struct ManagerState {
    profiles: HashMap<String, AppProfile>,
    runs: HashMap<String, BuildRun>,
    run_order: Vec<String>,
    active_profile_id: Option<String>,
    listeners: HashMap<u64, BuilderListener>,
    next_listener_id: u64,
}

pub struct BuilderManager {
    state: Arc<Mutex<ManagerState>>,
    executor: Arc<dyn BuildExecutor>,
    run_counter: Arc<AtomicU64>,
}

impl BuilderManager {
    pub fn new(options: BuilderManagerOptions) -> Self {
        let executor = options.executor.unwrap_or_else(create_dry_run_executor);
        let mut profiles: HashMap<String, AppProfile> = HashMap::new();
        for profile in list_builtin_profiles() {
            profiles.insert(profile.id.clone(), profile);
        }
        for profile in options.profiles {
            profiles.insert(profile.id.clone(), profile);
        }
        Self {
            state: Arc::new(Mutex::new(ManagerState {
                profiles,
                runs: HashMap::new(),
                run_order: Vec::new(),
                active_profile_id: None,
                listeners: HashMap::new(),
                next_listener_id: 0,
            })),
            executor,
            run_counter: Arc::new(AtomicU64::new(0)),
        }
    }

    // ── Profiles ───────────────────────────────────────────────────────────

    pub fn list_profiles(&self) -> Vec<AppProfile> {
        let state = self.state.lock().unwrap();
        state.profiles.values().cloned().collect()
    }

    pub fn get_profile(&self, id: &str) -> Option<AppProfile> {
        self.state.lock().unwrap().profiles.get(id).cloned()
    }

    pub fn register_profile(&self, profile: AppProfile) {
        {
            let mut state = self.state.lock().unwrap();
            state.profiles.insert(profile.id.clone(), profile);
        }
        self.notify();
    }

    pub fn remove_profile(&self, id: &str) -> bool {
        let builtin: HashSet<&str> = [
            BuiltInProfileId::Studio.as_str(),
            BuiltInProfileId::Flux.as_str(),
            BuiltInProfileId::Lattice.as_str(),
            BuiltInProfileId::Cadence.as_str(),
            BuiltInProfileId::Grip.as_str(),
            BuiltInProfileId::Relay.as_str(),
        ]
        .into_iter()
        .collect();
        if builtin.contains(id) {
            return false;
        }
        let removed = {
            let mut state = self.state.lock().unwrap();
            let existed = state.profiles.remove(id).is_some();
            if existed && state.active_profile_id.as_deref() == Some(id) {
                state.active_profile_id = None;
            }
            existed
        };
        if removed {
            self.notify();
        }
        removed
    }

    pub fn get_active_profile(&self) -> Option<AppProfile> {
        let state = self.state.lock().unwrap();
        state
            .active_profile_id
            .as_ref()
            .and_then(|id| state.profiles.get(id).cloned())
    }

    pub fn set_active_profile(&self, id: Option<&str>) -> Result<(), String> {
        {
            let mut state = self.state.lock().unwrap();
            match id {
                None => state.active_profile_id = None,
                Some(id) => {
                    if !state.profiles.contains_key(id) {
                        return Err(format!("Unknown profile: {id}"));
                    }
                    state.active_profile_id = Some(id.into());
                }
            }
        }
        self.notify();
        Ok(())
    }

    // ── Build planning ─────────────────────────────────────────────────────

    pub fn targets(&self) -> &'static [BuildTarget] {
        ALL_BUILD_TARGETS
    }

    pub fn plan_build(
        &self,
        profile_id: &str,
        target: BuildTarget,
        dry_run: bool,
    ) -> Result<BuildPlan, String> {
        let profile = self
            .get_profile(profile_id)
            .ok_or_else(|| format!("Unknown profile: {profile_id}"))?;
        Ok(create_build_plan(CreateBuildPlanOptions {
            profile: &profile,
            target,
            working_dir: None,
            dry_run: Some(dry_run),
            env: None,
        }))
    }

    pub fn plan_builds(
        &self,
        profile_id: &str,
        targets: &[BuildTarget],
        dry_run: bool,
    ) -> Result<Vec<BuildPlan>, String> {
        targets
            .iter()
            .map(|t| self.plan_build(profile_id, *t, dry_run))
            .collect()
    }

    // ── Execution ──────────────────────────────────────────────────────────

    pub fn run_plan(&self, plan: BuildPlan) -> BuildRun {
        let run_id = self.gen_run_id();
        let mut run = BuildRun {
            id: run_id.clone(),
            plan: plan.clone(),
            started_at: now_ms(),
            finished_at: None,
            status: BuildStepStatus::Running,
            steps: Vec::new(),
            produced_artifacts: Vec::new(),
        };
        {
            let mut state = self.state.lock().unwrap();
            state.runs.insert(run_id.clone(), run.clone());
            state.run_order.push(run_id.clone());
        }
        self.notify();

        let context = BuildExecutionContext {
            working_dir: plan.working_dir.clone(),
            env: plan.env.clone(),
        };

        for step in &plan.steps {
            let result = self.executor.execute_step(step, &context);
            let failed = result.status == BuildStepStatus::Failed;
            run.steps.push(result);
            self.replace_run(&run_id, &run);
            self.notify();
            if failed {
                run.status = BuildStepStatus::Failed;
                run.finished_at = Some(now_ms());
                self.replace_run(&run_id, &run);
                self.notify();
                return run;
            }
        }

        // In dry-run, always claim to have produced the declared artifacts
        // since the whole point is to preview what WOULD be built. In
        // daemon mode, hand back the declared artifacts unless every step
        // was skipped (mirrors the TS semantics).
        let all_skipped = run
            .steps
            .iter()
            .all(|s| s.status == BuildStepStatus::Skipped);
        let produced: Vec<ArtifactDescriptor> =
            if self.executor.mode() == ExecutorMode::DryRun || !all_skipped {
                plan.artifacts.clone()
            } else {
                Vec::new()
            };

        run.produced_artifacts = produced;
        run.status = if run
            .steps
            .iter()
            .any(|s| s.status == BuildStepStatus::Failed)
        {
            BuildStepStatus::Failed
        } else {
            BuildStepStatus::Success
        };
        run.finished_at = Some(now_ms());
        self.replace_run(&run_id, &run);
        self.notify();
        run
    }

    pub fn list_runs(&self) -> Vec<BuildRun> {
        let state = self.state.lock().unwrap();
        let mut runs: Vec<BuildRun> = state
            .run_order
            .iter()
            .filter_map(|id| state.runs.get(id).cloned())
            .collect();
        runs.sort_by(|a, b| b.started_at.cmp(&a.started_at));
        runs
    }

    pub fn get_run(&self, id: &str) -> Option<BuildRun> {
        self.state.lock().unwrap().runs.get(id).cloned()
    }

    pub fn clear_runs(&self) {
        {
            let mut state = self.state.lock().unwrap();
            state.runs.clear();
            state.run_order.clear();
        }
        self.notify();
    }

    // ── Subscriptions ──────────────────────────────────────────────────────

    pub fn subscribe<F>(&self, listener: F) -> BuilderUnsubscribe
    where
        F: Fn() + Send + Sync + 'static,
    {
        let id = {
            let mut state = self.state.lock().unwrap();
            let id = state.next_listener_id;
            state.next_listener_id += 1;
            state.listeners.insert(id, Box::new(listener));
            id
        };
        let state = Arc::clone(&self.state);
        Box::new(move || {
            let mut s = state.lock().unwrap();
            s.listeners.remove(&id);
        })
    }

    pub fn dispose(&self) {
        let mut state = self.state.lock().unwrap();
        state.listeners.clear();
        state.runs.clear();
        state.run_order.clear();
        state.profiles.clear();
        state.active_profile_id = None;
    }

    // ── Internals ──────────────────────────────────────────────────────────

    fn notify(&self) {
        let state = self.state.lock().unwrap();
        for listener in state.listeners.values() {
            listener();
        }
    }

    fn replace_run(&self, id: &str, run: &BuildRun) {
        let mut state = self.state.lock().unwrap();
        state.runs.insert(id.into(), run.clone());
    }

    fn gen_run_id(&self) -> String {
        let millis = now_ms();
        let counter = self.run_counter.fetch_add(1, Ordering::Relaxed);
        format!("build_{millis:x}_{counter:x}")
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
