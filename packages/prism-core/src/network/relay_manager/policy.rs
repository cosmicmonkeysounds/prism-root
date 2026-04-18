//! `network::relay_manager::policy` — relay selection policies.
//!
//! Defines how the manager selects which relay to use for a given
//! operation. Policies can consider health, priority, latency, and
//! tags.

use super::types::ManagedRelay;
use crate::network::relay::RelayId;

/// Policy type for relay selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum RelayPolicyKind {
    /// Use the first available relay (by priority)
    #[default]
    FirstAvailable,
    /// Round-robin across available relays
    RoundRobin,
    /// Select the relay with lowest latency
    LowestLatency,
    /// Random selection among available relays
    Random,
    /// Always use a specific relay
    Fixed,
}

/// Relay selection policy.
#[derive(Debug, Clone)]
pub struct RelayPolicy {
    pub kind: RelayPolicyKind,
    /// For Fixed policy: the relay to use
    pub fixed_relay: Option<RelayId>,
    /// Required tags (all must match)
    pub required_tags: Vec<String>,
    /// Excluded tags (none may match)
    pub excluded_tags: Vec<String>,
    /// Maximum acceptable latency in ms (0 = unlimited)
    pub max_latency_ms: u64,
    /// Maximum consecutive failures before excluding (0 = unlimited)
    pub max_failures: u32,
}

impl Default for RelayPolicy {
    fn default() -> Self {
        Self {
            kind: RelayPolicyKind::FirstAvailable,
            fixed_relay: None,
            required_tags: Vec::new(),
            excluded_tags: Vec::new(),
            max_latency_ms: 0,
            max_failures: 5,
        }
    }
}

impl RelayPolicy {
    pub fn first_available() -> Self {
        Self::default()
    }

    pub fn round_robin() -> Self {
        Self {
            kind: RelayPolicyKind::RoundRobin,
            ..Default::default()
        }
    }

    pub fn lowest_latency() -> Self {
        Self {
            kind: RelayPolicyKind::LowestLatency,
            ..Default::default()
        }
    }

    pub fn fixed(relay_id: RelayId) -> Self {
        Self {
            kind: RelayPolicyKind::Fixed,
            fixed_relay: Some(relay_id),
            ..Default::default()
        }
    }

    pub fn with_required_tag(mut self, tag: impl Into<String>) -> Self {
        self.required_tags.push(tag.into());
        self
    }

    pub fn with_excluded_tag(mut self, tag: impl Into<String>) -> Self {
        self.excluded_tags.push(tag.into());
        self
    }

    pub fn with_max_latency(mut self, ms: u64) -> Self {
        self.max_latency_ms = ms;
        self
    }

    pub fn with_max_failures(mut self, count: u32) -> Self {
        self.max_failures = count;
        self
    }

    /// Check if a relay matches this policy's filters.
    pub fn matches(&self, relay: &ManagedRelay) -> bool {
        // Must be available
        if !relay.status.is_available() {
            return false;
        }

        // Check required tags
        for tag in &self.required_tags {
            if !relay.tags.contains(tag) {
                return false;
            }
        }

        // Check excluded tags
        for tag in &self.excluded_tags {
            if relay.tags.contains(tag) {
                return false;
            }
        }

        // Check latency
        if self.max_latency_ms > 0 {
            if let Some(avg) = relay.health.avg_ping_ms {
                if avg > self.max_latency_ms {
                    return false;
                }
            }
        }

        // Check failure count
        if self.max_failures > 0 && relay.health.consecutive_failures >= self.max_failures {
            return false;
        }

        true
    }
}

/// Selector that applies a policy to choose a relay.
pub struct RelaySelector {
    policy: RelayPolicy,
    round_robin_index: usize,
}

impl RelaySelector {
    pub fn new(policy: RelayPolicy) -> Self {
        Self {
            policy,
            round_robin_index: 0,
        }
    }

    /// Select the best relay according to the policy.
    pub fn select<'a>(&mut self, relays: &'a [ManagedRelay]) -> Option<&'a ManagedRelay> {
        // Filter to matching relays
        let candidates: Vec<_> = relays.iter().filter(|r| self.policy.matches(r)).collect();

        if candidates.is_empty() {
            return None;
        }

        match self.policy.kind {
            RelayPolicyKind::Fixed => {
                let fixed_id = self.policy.fixed_relay.as_ref()?;
                candidates.iter().find(|r| &r.id == fixed_id).copied()
            }

            RelayPolicyKind::FirstAvailable => {
                // Sort by priority, return first
                let mut sorted = candidates.clone();
                sorted.sort_by_key(|r| r.priority);
                sorted.first().copied()
            }

            RelayPolicyKind::RoundRobin => {
                self.round_robin_index = (self.round_robin_index + 1) % candidates.len();
                candidates.get(self.round_robin_index).copied()
            }

            RelayPolicyKind::LowestLatency => candidates
                .iter()
                .min_by_key(|r| r.health.avg_ping_ms.unwrap_or(u64::MAX))
                .copied(),

            RelayPolicyKind::Random => {
                // Simple pseudo-random: use current index mod len
                let idx = (self.round_robin_index + 7) % candidates.len();
                self.round_robin_index = idx;
                candidates.get(idx).copied()
            }
        }
    }

    /// Reset the selector state (e.g., round-robin index).
    pub fn reset(&mut self) {
        self.round_robin_index = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::relay::{RelayConfig, RelayId};
    use crate::network::relay_manager::RelayStatus;

    fn make_relay(id: &str, status: RelayStatus, priority: u32) -> ManagedRelay {
        let mut r = ManagedRelay::new(
            RelayId::new(id),
            RelayConfig::websocket("ws://example.com/ws"),
        );
        r.status = status;
        r.priority = priority;
        r
    }

    #[test]
    fn policy_matches_available_only() {
        let policy = RelayPolicy::first_available();

        let active = make_relay("a", RelayStatus::Active, 1);
        let idle = make_relay("b", RelayStatus::Idle, 1);

        assert!(policy.matches(&active));
        assert!(!policy.matches(&idle));
    }

    #[test]
    fn policy_matches_required_tags() {
        let policy = RelayPolicy::first_available().with_required_tag("sync");

        let mut with_tag = make_relay("a", RelayStatus::Active, 1);
        with_tag.tags.push("sync".into());

        let without_tag = make_relay("b", RelayStatus::Active, 1);

        assert!(policy.matches(&with_tag));
        assert!(!policy.matches(&without_tag));
    }

    #[test]
    fn policy_matches_excluded_tags() {
        let policy = RelayPolicy::first_available().with_excluded_tag("deprecated");

        let normal = make_relay("a", RelayStatus::Active, 1);

        let mut deprecated = make_relay("b", RelayStatus::Active, 1);
        deprecated.tags.push("deprecated".into());

        assert!(policy.matches(&normal));
        assert!(!policy.matches(&deprecated));
    }

    #[test]
    fn policy_matches_max_failures() {
        let policy = RelayPolicy::first_available().with_max_failures(3);

        let mut healthy = make_relay("a", RelayStatus::Active, 1);
        healthy.health.consecutive_failures = 2;

        let mut failing = make_relay("b", RelayStatus::Active, 1);
        failing.health.consecutive_failures = 3;

        assert!(policy.matches(&healthy));
        assert!(!policy.matches(&failing));
    }

    #[test]
    fn selector_first_available_by_priority() {
        let relays = vec![
            make_relay("low", RelayStatus::Active, 100),
            make_relay("high", RelayStatus::Active, 10),
            make_relay("mid", RelayStatus::Active, 50),
        ];

        let mut selector = RelaySelector::new(RelayPolicy::first_available());
        let selected = selector.select(&relays).unwrap();
        assert_eq!(selected.id.as_str(), "high");
    }

    #[test]
    fn selector_round_robin_cycles() {
        let relays = vec![
            make_relay("a", RelayStatus::Active, 1),
            make_relay("b", RelayStatus::Active, 1),
            make_relay("c", RelayStatus::Active, 1),
        ];

        let mut selector = RelaySelector::new(RelayPolicy::round_robin());

        let first = selector.select(&relays).unwrap().id.clone();
        let second = selector.select(&relays).unwrap().id.clone();
        let third = selector.select(&relays).unwrap().id.clone();
        let fourth = selector.select(&relays).unwrap().id.clone();

        // Should cycle through all three, then repeat
        assert_ne!(first, second);
        assert_ne!(second, third);
        assert_eq!(first, fourth);
    }

    #[test]
    fn selector_lowest_latency() {
        let mut relays = vec![
            make_relay("slow", RelayStatus::Active, 1),
            make_relay("fast", RelayStatus::Active, 1),
            make_relay("mid", RelayStatus::Active, 1),
        ];
        relays[0].health.avg_ping_ms = Some(200);
        relays[1].health.avg_ping_ms = Some(50);
        relays[2].health.avg_ping_ms = Some(100);

        let mut selector = RelaySelector::new(RelayPolicy::lowest_latency());
        let selected = selector.select(&relays).unwrap();
        assert_eq!(selected.id.as_str(), "fast");
    }

    #[test]
    fn selector_fixed_relay() {
        let relays = vec![
            make_relay("a", RelayStatus::Active, 1),
            make_relay("b", RelayStatus::Active, 1),
        ];

        let mut selector = RelaySelector::new(RelayPolicy::fixed(RelayId::new("b")));
        let selected = selector.select(&relays).unwrap();
        assert_eq!(selected.id.as_str(), "b");
    }

    #[test]
    fn selector_returns_none_when_no_candidates() {
        let relays = vec![make_relay("a", RelayStatus::Idle, 1)];

        let mut selector = RelaySelector::new(RelayPolicy::first_available());
        assert!(selector.select(&relays).is_none());
    }
}
