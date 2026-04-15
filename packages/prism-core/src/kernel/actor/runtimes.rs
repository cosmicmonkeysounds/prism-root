//! `actor::runtimes` — in-crate runtimes.
//!
//! The test runtime is the only "real" implementation that ships here —
//! the production Luau runtime lives downstream in `prism-daemon`. Luau
//! and sidecar runtimes in the legacy TS version were thin adapters
//! over injected functions, so there is nothing interesting to keep
//! here for them.

use std::sync::{Arc, Mutex};

use serde_json::Value as JsonValue;

use super::types::{ActorRuntime, CapabilityScope, RuntimeResult};

/// Synchronous test runtime. Delegates to a boxed handler that maps a
/// `JsonValue` payload to a `Result<JsonValue, String>`. Used by the
/// `ProcessQueue` tests and any host that wants a trivial "pretend I
/// ran the script" behaviour during bring-up.
type BoxedHandler = Box<dyn FnMut(&JsonValue) -> Result<JsonValue, String> + Send>;

pub struct TestRuntime {
    name: String,
    handler: Arc<Mutex<BoxedHandler>>,
}

impl TestRuntime {
    pub fn new<F>(name: impl Into<String>, handler: F) -> Self
    where
        F: FnMut(&JsonValue) -> Result<JsonValue, String> + Send + 'static,
    {
        Self {
            name: name.into(),
            handler: Arc::new(Mutex::new(Box::new(handler))),
        }
    }
}

impl ActorRuntime for TestRuntime {
    fn name(&self) -> &str {
        &self.name
    }

    fn is_available(&self) -> bool {
        true
    }

    fn execute(&self, payload: &JsonValue, _scope: &CapabilityScope) -> RuntimeResult {
        let start = std::time::Instant::now();
        let outcome = {
            let mut handler = self.handler.lock().expect("handler not poisoned");
            (handler)(payload)
        };
        let duration_ms = start.elapsed().as_millis() as u64;
        match outcome {
            Ok(v) => RuntimeResult::ok(v, duration_ms),
            Err(e) => RuntimeResult::err(e, duration_ms),
        }
    }
}
