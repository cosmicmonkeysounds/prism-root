//! `actor::queue` — the `ProcessQueue` reducer + event bus.
//!
//! Port of `kernel/actor/actor.ts` at 8426588. The legacy TS module
//! used async `executeTask` + a `fire-and-forget` `scheduleNext` loop.
//! The Rust port is sync and drives tasks through `process_next` /
//! `process_all` — hosts that want auto-processing call those in a
//! loop on their own thread/timer. See `types.rs` for the rationale.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;

use super::types::{
    default_capability_scope, ActorRuntime, EnqueueParams, ExecutionTarget, ProcessQueueOptions,
    ProcessTask, QueueEvent, QueueEventType, RuntimeResult, TaskStatus,
};

/// Listener callback for [`ProcessQueue::subscribe`].
pub type QueueListener = Box<dyn FnMut(&QueueEvent) + Send>;

/// Ordered in-memory process queue. Synchronous by design — hosts that
/// want continuous dequeueing call [`ProcessQueue::process_next`] or
/// [`ProcessQueue::process_all`] on their own driver.
pub struct ProcessQueue {
    tasks: HashMap<String, ProcessTask>,
    order: Vec<String>,
    runtimes: HashMap<String, Arc<dyn ActorRuntime>>,
    listeners: Vec<(u64, QueueListener)>,
    next_listener_id: u64,
    processing: bool,
    running_count: usize,
    concurrency: usize,
    id_counter: u64,
}

impl ProcessQueue {
    pub fn new(options: ProcessQueueOptions) -> Self {
        let concurrency = options.concurrency.max(1);
        let mut q = Self {
            tasks: HashMap::new(),
            order: Vec::new(),
            runtimes: HashMap::new(),
            listeners: Vec::new(),
            next_listener_id: 0,
            processing: false,
            running_count: 0,
            concurrency,
            id_counter: 0,
        };
        if options.auto_start {
            q.processing = true;
        }
        q
    }

    fn uid(&mut self, prefix: &str) -> String {
        let ts = Utc::now().timestamp_millis();
        let n = self.id_counter;
        self.id_counter += 1;
        format!("{prefix}-{ts:x}-{n:x}")
    }

    fn notify(&mut self, event_type: QueueEventType, task: &ProcessTask) {
        let ev = QueueEvent {
            event_type,
            task_id: task.id.clone(),
            task: task.clone(),
        };
        for (_, listener) in self.listeners.iter_mut() {
            listener(&ev);
        }
    }

    /// Enqueue `params`. Returns the new task.
    pub fn enqueue(&mut self, params: EnqueueParams) -> ProcessTask {
        let id = self.uid("task");
        let scope = match params.scope {
            Some(ov) => ov.apply(default_capability_scope()),
            None => default_capability_scope(),
        };

        let task = ProcessTask {
            id: id.clone(),
            name: params.name,
            runtime: params.runtime,
            target: params.target.unwrap_or(ExecutionTarget::Local),
            payload: params.payload,
            scope,
            priority: params.priority.unwrap_or(10),
            status: TaskStatus::Pending,
            result: None,
            error: None,
            enqueued_at: Utc::now().to_rfc3339(),
            started_at: None,
            completed_at: None,
        };

        self.tasks.insert(id.clone(), task.clone());
        self.order.push(id);
        self.notify(QueueEventType::Enqueued, &task);
        task
    }

    /// Cancel a pending or running task. Returns `false` if the task
    /// doesn't exist or is already terminal.
    pub fn cancel(&mut self, task_id: &str) -> bool {
        let Some(task) = self.tasks.get_mut(task_id) else {
            return false;
        };
        if !matches!(task.status, TaskStatus::Pending | TaskStatus::Running) {
            return false;
        }
        task.status = TaskStatus::Cancelled;
        task.completed_at = Some(Utc::now().to_rfc3339());
        let snapshot = task.clone();
        self.notify(QueueEventType::Cancelled, &snapshot);
        true
    }

    /// Retrieve a task by id.
    pub fn get(&self, task_id: &str) -> Option<&ProcessTask> {
        self.tasks.get(task_id)
    }

    /// List tasks, optionally filtered by status. Order is insertion-
    /// stable.
    pub fn list(&self, status: Option<TaskStatus>) -> Vec<&ProcessTask> {
        self.order
            .iter()
            .filter_map(|id| self.tasks.get(id))
            .filter(|t| status.is_none_or(|s| t.status == s))
            .collect()
    }

    /// Number of pending tasks.
    pub fn pending_count(&self) -> usize {
        self.tasks
            .values()
            .filter(|t| t.status == TaskStatus::Pending)
            .count()
    }

    /// Number of currently running tasks.
    pub fn running_count(&self) -> usize {
        self.running_count
    }

    /// Whether auto-processing is armed (see [`start`](Self::start)).
    pub fn processing(&self) -> bool {
        self.processing
    }

    /// Configured concurrency hint. Synchronous hosts typically ignore
    /// this and drive `process_next` one task at a time, but forwarded
    /// for parity with the legacy TS API.
    pub fn concurrency(&self) -> usize {
        self.concurrency
    }

    /// Return the id of the highest-priority pending task, if any.
    fn find_next_pending_id(&self) -> Option<String> {
        let mut best: Option<(&String, i32)> = None;
        for id in self.order.iter() {
            let Some(task) = self.tasks.get(id) else {
                continue;
            };
            if task.status != TaskStatus::Pending {
                continue;
            }
            match best {
                None => best = Some((id, task.priority)),
                Some((_, p)) if task.priority < p => best = Some((id, task.priority)),
                _ => {}
            }
        }
        best.map(|(id, _)| id.clone())
    }

    /// Execute the highest-priority pending task. Returns the task after
    /// it has transitioned to terminal status, or `None` if there were
    /// none pending.
    pub fn process_next(&mut self) -> Option<ProcessTask> {
        let task_id = self.find_next_pending_id()?;
        self.execute_task(&task_id);
        self.tasks.get(&task_id).cloned()
    }

    /// Drain the queue up to `concurrency` tasks. Returns how many were
    /// processed. Convenience for simple hosts that want to fill all
    /// available slots synchronously.
    pub fn process_all(&mut self) -> usize {
        let mut count = 0;
        while self.process_next().is_some() {
            count += 1;
        }
        count
    }

    fn execute_task(&mut self, task_id: &str) {
        let runtime = match self.tasks.get(task_id) {
            Some(t) => self.runtimes.get(&t.runtime).cloned(),
            None => return,
        };

        let Some(runtime) = runtime else {
            if let Some(task) = self.tasks.get_mut(task_id) {
                task.status = TaskStatus::Failed;
                task.error = Some(format!("Runtime \"{}\" not registered", task.runtime));
                task.completed_at = Some(Utc::now().to_rfc3339());
                let snapshot = task.clone();
                self.notify(QueueEventType::Failed, &snapshot);
            }
            return;
        };

        // Mark running
        let (payload, scope) = {
            let task = self.tasks.get_mut(task_id).expect("exists");
            task.status = TaskStatus::Running;
            task.started_at = Some(Utc::now().to_rfc3339());
            (task.payload.clone(), task.scope.clone())
        };
        self.running_count += 1;
        let started_snapshot = self.tasks.get(task_id).cloned().expect("exists");
        self.notify(QueueEventType::Started, &started_snapshot);

        let result: RuntimeResult = runtime.execute(&payload, &scope);
        self.running_count -= 1;

        let task = self.tasks.get_mut(task_id).expect("exists");
        if task.status == TaskStatus::Cancelled {
            return;
        }

        if result.success {
            task.status = TaskStatus::Completed;
            task.result = result.value;
        } else {
            task.status = TaskStatus::Failed;
            task.error = Some(result.error.unwrap_or_else(|| "Unknown error".into()));
        }
        task.completed_at = Some(Utc::now().to_rfc3339());
        let kind = if task.status == TaskStatus::Completed {
            QueueEventType::Completed
        } else {
            QueueEventType::Failed
        };
        let snapshot = task.clone();
        self.notify(kind, &snapshot);
    }

    /// Arm continuous processing. Hosts pair this with their own driver
    /// loop — this method itself does no work, it only sets the flag.
    pub fn start(&mut self) {
        self.processing = true;
    }

    /// Disarm continuous processing. Running tasks complete; no new
    /// ones start.
    pub fn stop(&mut self) {
        self.processing = false;
    }

    /// Subscribe to queue events. Returns a listener id suitable for
    /// [`unsubscribe`](Self::unsubscribe).
    pub fn subscribe<F>(&mut self, listener: F) -> u64
    where
        F: FnMut(&QueueEvent) + Send + 'static,
    {
        let id = self.next_listener_id;
        self.next_listener_id += 1;
        self.listeners.push((id, Box::new(listener)));
        id
    }

    /// Unsubscribe a listener registered via [`subscribe`](Self::subscribe).
    pub fn unsubscribe(&mut self, listener_id: u64) -> bool {
        let before = self.listeners.len();
        self.listeners.retain(|(id, _)| *id != listener_id);
        self.listeners.len() != before
    }

    /// Register a runtime keyed on `runtime.name()`.
    pub fn register_runtime(&mut self, runtime: Arc<dyn ActorRuntime>) {
        self.runtimes.insert(runtime.name().to_string(), runtime);
    }

    /// Retrieve a runtime by name.
    pub fn get_runtime(&self, name: &str) -> Option<&Arc<dyn ActorRuntime>> {
        self.runtimes.get(name)
    }

    /// Names of every registered runtime.
    pub fn runtime_names(&self) -> Vec<String> {
        self.runtimes.keys().cloned().collect()
    }

    /// Drop every completed / failed / cancelled task. Returns the count
    /// removed.
    pub fn prune(&mut self) -> usize {
        let terminal: Vec<String> = self
            .tasks
            .iter()
            .filter_map(|(id, t)| {
                if matches!(
                    t.status,
                    TaskStatus::Completed | TaskStatus::Failed | TaskStatus::Cancelled
                ) {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        let removed = terminal.len();
        for id in &terminal {
            self.tasks.remove(id);
        }
        self.order.retain(|id| !terminal.contains(id));
        removed
    }

    /// Stop processing, drop every task, drop every listener.
    pub fn dispose(&mut self) {
        self.processing = false;
        self.tasks.clear();
        self.order.clear();
        self.listeners.clear();
        self.running_count = 0;
    }
}

impl Default for ProcessQueue {
    fn default() -> Self {
        Self::new(ProcessQueueOptions::default())
    }
}
