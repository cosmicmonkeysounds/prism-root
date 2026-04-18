//! Request metrics tracking.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;

pub struct RequestMetrics {
    pub requests_total: AtomicU64,
    pub counters: RwLock<HashMap<String, u64>>,
    pub start_time: Instant,
}

impl RequestMetrics {
    pub fn new() -> Self {
        Self {
            requests_total: AtomicU64::new(0),
            counters: RwLock::new(HashMap::new()),
            start_time: Instant::now(),
        }
    }

    pub fn record_request(&self, method: &str, route: &str, status: u16) {
        self.requests_total.fetch_add(1, Ordering::Relaxed);
        let key = format!("{method} {route} {status}");
        let mut counters = self.counters.write().unwrap();
        *counters.entry(key).or_insert(0) += 1;
    }

    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    pub fn render_prometheus(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!(
            "relay_requests_total {}\n",
            self.requests_total.load(Ordering::Relaxed)
        ));
        out.push_str(&format!("relay_uptime_seconds {}\n", self.uptime_seconds()));
        let counters = self.counters.read().unwrap();
        for (key, count) in counters.iter() {
            let parts: Vec<&str> = key.splitn(3, ' ').collect();
            if parts.len() == 3 {
                out.push_str(&format!(
                    "relay_requests_total{{method=\"{}\",route=\"{}\",status=\"{}\"}} {}\n",
                    parts[0], parts[1], parts[2], count
                ));
            }
        }
        out
    }
}

impl Default for RequestMetrics {
    fn default() -> Self {
        Self::new()
    }
}
