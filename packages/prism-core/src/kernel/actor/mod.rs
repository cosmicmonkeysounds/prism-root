//! `kernel::actor` — process queue + pluggable actor runtime trait.
//!
//! Port of `kernel/actor/{actor,actor-types}.ts` at 8426588. Unlike the
//! legacy TS implementation — which was async, used `Promise`s and a
//! fire-and-forget `scheduleNext` loop — the Rust port is synchronous:
//! `process_next` / `process_all` drive one task at a time and return
//! after the runtime's `execute` call returns. Hosts that want
//! continuous processing (Studio, the daemon) run the driver on their
//! own thread or timer.
//!
//! The sibling [`kernel::intelligence`](super::intelligence) module
//! owns the AI provider registry, split out of the legacy
//! `kernel/actor/ai*.ts` files per ADR-002 §Part C.

pub mod queue;
pub mod runtimes;
pub mod types;

pub use queue::{ProcessQueue, QueueListener};
pub use runtimes::TestRuntime;
pub use types::{
    default_capability_scope, ActorRuntime, CapabilityScope, CapabilityScopeOverride,
    EnqueueParams, ExecutionTarget, ProcessQueueOptions, ProcessTask, QueueEvent, QueueEventType,
    RuntimeResult, TaskStatus,
};

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, Mutex};

    fn math_runtime() -> Arc<dyn ActorRuntime> {
        Arc::new(TestRuntime::new("math", |payload| {
            let op = payload.get("op").and_then(|v| v.as_str()).unwrap_or("");
            let a = payload.get("a").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let b = payload.get("b").and_then(|v| v.as_f64()).unwrap_or(0.0);
            match op {
                "add" => Ok(json!(a + b)),
                "mul" => Ok(json!(a * b)),
                other => Err(format!("Unknown op: {other}")),
            }
        }))
    }

    fn echo_runtime() -> Arc<dyn ActorRuntime> {
        Arc::new(TestRuntime::new("echo", |payload| {
            Ok(payload.get("v").cloned().unwrap_or(json!(null)))
        }))
    }

    #[test]
    fn enqueues_a_task_with_defaults() {
        let mut q = ProcessQueue::default();
        q.register_runtime(math_runtime());
        let task = q.enqueue(EnqueueParams::new(
            "add",
            "math",
            json!({"op": "add", "a": 1, "b": 2}),
        ));
        assert_eq!(task.name, "add");
        assert_eq!(task.status, TaskStatus::Pending);
        assert_eq!(task.target, ExecutionTarget::Local);
        assert_eq!(task.priority, 10);
        assert!(task.scope.crdt_read);
        assert_eq!(q.pending_count(), 1);
    }

    #[test]
    fn processes_a_task_and_returns_result() {
        let mut q = ProcessQueue::default();
        q.register_runtime(math_runtime());
        q.enqueue(EnqueueParams::new(
            "add",
            "math",
            json!({"op": "add", "a": 3, "b": 4}),
        ));
        let t = q.process_next().expect("task processed");
        assert_eq!(t.status, TaskStatus::Completed);
        assert_eq!(t.result, Some(json!(7.0)));
        assert!(t.started_at.is_some());
        assert!(t.completed_at.is_some());
    }

    #[test]
    fn handles_runtime_errors_gracefully() {
        let mut q = ProcessQueue::default();
        q.register_runtime(math_runtime());
        q.enqueue(EnqueueParams::new(
            "bad",
            "math",
            json!({"op": "div", "a": 1, "b": 0}),
        ));
        let t = q.process_next().expect("task processed");
        assert_eq!(t.status, TaskStatus::Failed);
        assert!(t.error.unwrap().contains("Unknown op: div"));
    }

    #[test]
    fn fails_when_runtime_is_not_registered() {
        let mut q = ProcessQueue::default();
        q.enqueue(EnqueueParams::new("orphan", "nonexistent", json!({})));
        let t = q.process_next().expect("task processed");
        assert_eq!(t.status, TaskStatus::Failed);
        assert!(t.error.unwrap().contains("not registered"));
    }

    #[test]
    fn returns_none_when_queue_is_empty() {
        let mut q = ProcessQueue::default();
        assert!(q.process_next().is_none());
    }

    #[test]
    fn processes_by_priority() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());

        let mut p = EnqueueParams::new("low", "echo", json!({"v": "low"}));
        p.priority = Some(20);
        q.enqueue(p);

        let mut p = EnqueueParams::new("high", "echo", json!({"v": "high"}));
        p.priority = Some(1);
        q.enqueue(p);

        let mut p = EnqueueParams::new("mid", "echo", json!({"v": "mid"}));
        p.priority = Some(10);
        q.enqueue(p);

        let t1 = q.process_next().unwrap();
        assert_eq!(t1.result, Some(json!("high")));
        let t2 = q.process_next().unwrap();
        assert_eq!(t2.result, Some(json!("mid")));
        let t3 = q.process_next().unwrap();
        assert_eq!(t3.result, Some(json!("low")));
    }

    #[test]
    fn cancels_a_pending_task() {
        let mut q = ProcessQueue::default();
        q.register_runtime(math_runtime());
        let task = q.enqueue(EnqueueParams::new("cancel-me", "math", json!({})));
        assert!(q.cancel(&task.id));
        assert_eq!(q.get(&task.id).unwrap().status, TaskStatus::Cancelled);
        assert_eq!(q.pending_count(), 0);
    }

    #[test]
    fn subscribes_to_queue_events() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());
        let log: Arc<Mutex<Vec<QueueEventType>>> = Arc::new(Mutex::new(Vec::new()));
        let log_cl = log.clone();
        q.subscribe(move |ev| log_cl.lock().unwrap().push(ev.event_type));
        q.enqueue(EnqueueParams::new("e", "echo", json!({"v": "x"})));
        q.process_next();
        let observed = log.lock().unwrap().clone();
        assert_eq!(
            observed,
            vec![
                QueueEventType::Enqueued,
                QueueEventType::Started,
                QueueEventType::Completed
            ]
        );
    }

    #[test]
    fn list_filters_by_status() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());
        q.enqueue(EnqueueParams::new("a", "echo", json!({"v": 1})));
        q.enqueue(EnqueueParams::new("b", "echo", json!({"v": 2})));
        q.process_next();
        assert_eq!(q.list(Some(TaskStatus::Pending)).len(), 1);
        assert_eq!(q.list(Some(TaskStatus::Completed)).len(), 1);
        assert_eq!(q.list(None).len(), 2);
    }

    #[test]
    fn prune_drops_terminal_tasks() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());
        q.enqueue(EnqueueParams::new("a", "echo", json!({"v": 1})));
        q.enqueue(EnqueueParams::new("b", "echo", json!({"v": 2})));
        q.process_all();
        assert_eq!(q.prune(), 2);
        assert_eq!(q.list(None).len(), 0);
    }

    #[test]
    fn registers_and_lists_runtimes() {
        let mut q = ProcessQueue::default();
        q.register_runtime(math_runtime());
        q.register_runtime(echo_runtime());
        let mut names = q.runtime_names();
        names.sort();
        assert_eq!(names, vec!["echo".to_string(), "math".to_string()]);
        assert!(q.get_runtime("math").is_some());
        assert!(q.get_runtime("missing").is_none());
    }

    #[test]
    fn scope_override_merges_defaults() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());
        let mut params = EnqueueParams::new("e", "echo", json!({"v": 1}));
        params.scope = Some(CapabilityScopeOverride {
            network: Some(true),
            max_duration_ms: Some(60_000),
            ..Default::default()
        });
        let task = q.enqueue(params);
        assert!(task.scope.network);
        assert_eq!(task.scope.max_duration_ms, 60_000);
        // Defaults preserved
        assert!(task.scope.crdt_read);
        assert!(!task.scope.crdt_write);
    }

    #[test]
    fn dispose_clears_everything() {
        let mut q = ProcessQueue::default();
        q.register_runtime(echo_runtime());
        q.enqueue(EnqueueParams::new("a", "echo", json!({"v": 1})));
        q.dispose();
        assert_eq!(q.list(None).len(), 0);
        assert!(!q.processing());
    }

    #[test]
    fn task_serialises_round_trip() {
        let task = ProcessTask {
            id: "t1".into(),
            name: "n".into(),
            runtime: "r".into(),
            target: ExecutionTarget::Local,
            payload: json!(42),
            scope: default_capability_scope(),
            priority: 5,
            status: TaskStatus::Pending,
            result: None,
            error: None,
            enqueued_at: "2026-04-15T00:00:00Z".into(),
            started_at: None,
            completed_at: None,
        };
        let bytes = serde_json::to_vec(&task).unwrap();
        let back: ProcessTask = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(back.id, "t1");
        assert_eq!(back.priority, 5);
    }
}
