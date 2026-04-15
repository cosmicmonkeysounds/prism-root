//! Luau sandbox capability enforcement. Wraps a [`SandboxPolicy`]
//! and answers capability / URL / path questions against it. Port of
//! the `createLuauSandbox` factory in `trust/trust.ts`.

use std::collections::HashSet;

use regex::Regex;

use super::types::{SandboxCapability, SandboxPolicy, SandboxViolation};

/// Stateful sandbox built from a [`SandboxPolicy`].
#[derive(Debug, Clone)]
pub struct LuauSandbox {
    policy: SandboxPolicy,
    capabilities: HashSet<SandboxCapability>,
    url_regexes: Vec<Regex>,
    path_regexes: Vec<Regex>,
    violations: Vec<SandboxViolation>,
}

impl LuauSandbox {
    pub fn new(policy: SandboxPolicy) -> Self {
        let capabilities: HashSet<_> = policy.capabilities.iter().copied().collect();
        let url_regexes = policy
            .allowed_urls
            .iter()
            .map(|p| glob_to_regex(p))
            .collect();
        let path_regexes = policy
            .allowed_paths
            .iter()
            .map(|p| glob_to_regex(p))
            .collect();
        Self {
            policy,
            capabilities,
            url_regexes,
            path_regexes,
            violations: Vec::new(),
        }
    }

    pub fn policy(&self) -> &SandboxPolicy {
        &self.policy
    }

    pub fn violations(&self) -> &[SandboxViolation] {
        &self.violations
    }

    pub fn has_capability(&self, capability: SandboxCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    pub fn is_url_allowed(&self, url: &str) -> bool {
        if !self.capabilities.contains(&SandboxCapability::NetFetch)
            && !self.capabilities.contains(&SandboxCapability::NetWebsocket)
        {
            return false;
        }
        if self.url_regexes.is_empty() {
            return false;
        }
        self.url_regexes.iter().any(|re| re.is_match(url))
    }

    pub fn is_path_allowed(&self, path: &str) -> bool {
        if !self.capabilities.contains(&SandboxCapability::FsRead)
            && !self.capabilities.contains(&SandboxCapability::FsWrite)
        {
            return false;
        }
        if self.path_regexes.is_empty() {
            return false;
        }
        self.path_regexes.iter().any(|re| re.is_match(path))
    }

    pub fn record_violation(&mut self, violation: SandboxViolation) {
        self.violations.push(violation);
    }
}

/// Convenience factory mirroring the TS `createLuauSandbox`.
pub fn create_luau_sandbox(policy: SandboxPolicy) -> LuauSandbox {
    LuauSandbox::new(policy)
}

/// Translate a glob pattern (`*`, `?`) into an anchored regex, mirroring
/// the TS `globToRegex` helper.
fn glob_to_regex(pattern: &str) -> Regex {
    let mut out = String::with_capacity(pattern.len() + 2);
    out.push('^');
    for ch in pattern.chars() {
        match ch {
            '*' => out.push_str(".*"),
            '?' => out.push('.'),
            '.' | '+' | '^' | '$' | '{' | '}' | '(' | ')' | '|' | '[' | ']' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('$');
    Regex::new(&out).expect("glob regex compiles")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::trust::types::SandboxCapability;

    fn policy_with(overrides: impl FnOnce(&mut SandboxPolicy)) -> SandboxPolicy {
        let mut p = SandboxPolicy {
            plugin_id: "test-plugin".into(),
            capabilities: vec![SandboxCapability::CrdtRead, SandboxCapability::NetFetch],
            max_duration_ms: 5000,
            max_memory_bytes: 0,
            allowed_urls: vec!["https://api.example.com/*".into()],
            allowed_paths: vec![],
        };
        overrides(&mut p);
        p
    }

    #[test]
    fn grants_listed_capabilities() {
        let sandbox = create_luau_sandbox(policy_with(|_| {}));
        assert!(sandbox.has_capability(SandboxCapability::CrdtRead));
        assert!(sandbox.has_capability(SandboxCapability::NetFetch));
    }

    #[test]
    fn denies_unlisted_capabilities() {
        let sandbox = create_luau_sandbox(policy_with(|_| {}));
        assert!(!sandbox.has_capability(SandboxCapability::CrdtWrite));
        assert!(!sandbox.has_capability(SandboxCapability::FsRead));
        assert!(!sandbox.has_capability(SandboxCapability::ProcessSpawn));
    }

    #[test]
    fn allows_urls_matching_glob_patterns() {
        let sandbox = create_luau_sandbox(policy_with(|_| {}));
        assert!(sandbox.is_url_allowed("https://api.example.com/v1/data"));
        assert!(!sandbox.is_url_allowed("https://evil.com/attack"));
    }

    #[test]
    fn denies_all_urls_when_net_capability_missing() {
        let sandbox = create_luau_sandbox(policy_with(|p| {
            p.capabilities = vec![SandboxCapability::CrdtRead];
        }));
        assert!(!sandbox.is_url_allowed("https://api.example.com/v1/data"));
    }

    #[test]
    fn denies_all_urls_when_allowed_urls_empty() {
        let sandbox = create_luau_sandbox(policy_with(|p| {
            p.allowed_urls = vec![];
        }));
        assert!(!sandbox.is_url_allowed("https://api.example.com/v1/data"));
    }

    #[test]
    fn allows_paths_matching_glob_patterns() {
        let sandbox = create_luau_sandbox(policy_with(|p| {
            p.capabilities = vec![SandboxCapability::FsRead];
            p.allowed_paths = vec!["/home/user/docs/*".into()];
        }));
        assert!(sandbox.is_path_allowed("/home/user/docs/file.txt"));
        assert!(!sandbox.is_path_allowed("/etc/passwd"));
    }

    #[test]
    fn denies_all_paths_when_fs_capability_missing() {
        let sandbox = create_luau_sandbox(policy_with(|p| {
            p.capabilities = vec![SandboxCapability::CrdtRead];
            p.allowed_paths = vec!["/home/user/*".into()];
        }));
        assert!(!sandbox.is_path_allowed("/home/user/file.txt"));
    }

    #[test]
    fn records_violations() {
        let mut sandbox = create_luau_sandbox(policy_with(|_| {}));
        assert_eq!(sandbox.violations().len(), 0);
        sandbox.record_violation(SandboxViolation {
            capability: SandboxCapability::CrdtWrite.as_str().to_string(),
            message: "Write not allowed".into(),
            timestamp: "2026-01-01T00:00:00Z".into(),
            plugin_id: "test-plugin".into(),
        });
        assert_eq!(sandbox.violations().len(), 1);
        assert_eq!(sandbox.violations()[0].capability, "crdt:write");
    }

    #[test]
    fn exposes_policy() {
        let policy = policy_with(|_| {});
        let sandbox = create_luau_sandbox(policy);
        assert_eq!(sandbox.policy().plugin_id, "test-plugin");
        assert_eq!(sandbox.policy().max_duration_ms, 5000);
    }
}
