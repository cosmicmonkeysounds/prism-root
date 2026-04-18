//! `network::relay::module_system` — composable relay module architecture.
//!
//! Port of the relay builder pattern from the legacy TS relay. Each
//! module is a struct implementing `RelayModule`; modules compose via
//! `RelayBuilder::use_module()` and share capabilities through a
//! `RelayContext`. Dependency validation happens at build time.

use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

pub mod capabilities {
    pub const MAILBOX: &str = "relay:mailbox";
    pub const ROUTER: &str = "relay:router";
    pub const TIMESTAMPER: &str = "relay:timestamper";
    pub const PINGER: &str = "relay:pinger";
    pub const TOKENS: &str = "relay:tokens";
    pub const WEBHOOKS: &str = "relay:webhooks";
    pub const PORTALS: &str = "relay:portals";
    pub const COLLECTIONS: &str = "relay:collections";
    pub const HASHCASH: &str = "relay:hashcash";
    pub const TRUST: &str = "relay:trust";
    pub const ESCROW: &str = "relay:escrow";
    pub const FEDERATION: &str = "relay:federation";
    pub const ACME: &str = "relay:acme";
    pub const TEMPLATES: &str = "relay:templates";
    pub const SIGNALING: &str = "relay:signaling";
    pub const VAULT_HOST: &str = "relay:vault-host";
    pub const PASSWORD_AUTH: &str = "relay:password-auth";
    pub const OAUTH: &str = "relay:oauth";
}

/// Server-side relay configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayServerConfig {
    pub relay_did: String,
    #[serde(default = "default_ttl")]
    pub default_ttl_ms: u64,
    #[serde(default = "default_max_envelope")]
    pub max_envelope_size_bytes: usize,
    #[serde(default = "default_eviction_interval")]
    pub eviction_interval_ms: u64,
}

fn default_ttl() -> u64 {
    7 * 24 * 60 * 60 * 1000
}
fn default_max_envelope() -> usize {
    1_048_576
}
fn default_eviction_interval() -> u64 {
    60_000
}

impl Default for RelayServerConfig {
    fn default() -> Self {
        Self {
            relay_did: String::new(),
            default_ttl_ms: default_ttl(),
            max_envelope_size_bytes: default_max_envelope(),
            eviction_interval_ms: default_eviction_interval(),
        }
    }
}

/// Shared context available to all modules during install and at runtime.
pub struct RelayContext {
    pub config: RelayServerConfig,
    capabilities: RwLock<HashMap<String, Arc<dyn Any + Send + Sync>>>,
}

impl RelayContext {
    pub fn new(config: RelayServerConfig) -> Self {
        Self {
            config,
            capabilities: RwLock::new(HashMap::new()),
        }
    }

    pub fn set_capability<T: Any + Send + Sync>(&self, name: &str, value: T) {
        self.capabilities
            .write()
            .unwrap()
            .insert(name.to_string(), Arc::new(value));
    }

    pub fn get_capability<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        self.capabilities
            .read()
            .unwrap()
            .get(name)
            .and_then(|v| Arc::clone(v).downcast::<T>().ok())
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.capabilities.read().unwrap().contains_key(name)
    }

    pub fn relay_did(&self) -> &str {
        &self.config.relay_did
    }
}

/// A pluggable relay module.
pub trait RelayModule: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn dependencies(&self) -> &[&str] {
        &[]
    }
    fn install(&self, ctx: &RelayContext);
}

/// Errors from building a relay instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayBuildError {
    DuplicateModule(String),
    MissingDependency { module: String, dependency: String },
}

impl std::fmt::Display for RelayBuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateModule(name) => write!(f, "duplicate module: {name}"),
            Self::MissingDependency { module, dependency } => {
                write!(
                    f,
                    "module '{module}' requires '{dependency}' which is not installed"
                )
            }
        }
    }
}

impl std::error::Error for RelayBuildError {}

/// Builder for composing relay modules.
pub struct RelayBuilder {
    config: RelayServerConfig,
    modules: Vec<Box<dyn RelayModule>>,
}

impl RelayBuilder {
    pub fn new(config: RelayServerConfig) -> Self {
        Self {
            config,
            modules: Vec::new(),
        }
    }

    pub fn use_module(mut self, module: impl RelayModule + 'static) -> Self {
        self.modules.push(Box::new(module));
        self
    }

    pub fn configure(mut self, config: RelayServerConfig) -> Self {
        self.config = config;
        self
    }

    pub fn build(self) -> Result<RelayInstance, RelayBuildError> {
        let installed: Vec<&str> = self.modules.iter().map(|m| m.name()).collect();

        // Check for duplicates
        let mut seen = std::collections::HashSet::new();
        for name in &installed {
            if !seen.insert(*name) {
                return Err(RelayBuildError::DuplicateModule(name.to_string()));
            }
        }

        // Validate dependencies
        for module in &self.modules {
            for dep in module.dependencies() {
                if !installed.contains(dep) {
                    return Err(RelayBuildError::MissingDependency {
                        module: module.name().to_string(),
                        dependency: dep.to_string(),
                    });
                }
            }
        }

        let ctx = Arc::new(RelayContext::new(self.config));

        // Install in order
        for module in &self.modules {
            module.install(&ctx);
        }

        let module_names: Vec<String> = self.modules.iter().map(|m| m.name().to_string()).collect();

        Ok(RelayInstance {
            context: ctx,
            module_names,
        })
    }
}

/// A built relay instance with installed modules and capabilities.
pub struct RelayInstance {
    context: Arc<RelayContext>,
    module_names: Vec<String>,
}

impl RelayInstance {
    pub fn relay_did(&self) -> &str {
        self.context.relay_did()
    }

    pub fn modules(&self) -> &[String] {
        &self.module_names
    }

    pub fn context(&self) -> &Arc<RelayContext> {
        &self.context
    }

    pub fn get_capability<T: Any + Send + Sync>(&self, name: &str) -> Option<Arc<T>> {
        self.context.get_capability(name)
    }

    pub fn has_capability(&self, name: &str) -> bool {
        self.context.has_capability(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestModule {
        name: &'static str,
        deps: Vec<&'static str>,
    }

    impl RelayModule for TestModule {
        fn name(&self) -> &str {
            self.name
        }
        fn description(&self) -> &str {
            "test"
        }
        fn dependencies(&self) -> &[&str] {
            &self.deps
        }
        fn install(&self, ctx: &RelayContext) {
            ctx.set_capability(self.name, self.name.to_string());
        }
    }

    #[test]
    fn build_empty_relay() {
        let relay = RelayBuilder::new(RelayServerConfig::default())
            .build()
            .unwrap();
        assert!(relay.modules().is_empty());
    }

    #[test]
    fn build_with_modules() {
        let relay = RelayBuilder::new(RelayServerConfig::default())
            .use_module(TestModule {
                name: "alpha",
                deps: vec![],
            })
            .use_module(TestModule {
                name: "beta",
                deps: vec![],
            })
            .build()
            .unwrap();
        assert_eq!(relay.modules().len(), 2);
        assert!(relay.has_capability("alpha"));
        assert!(relay.has_capability("beta"));
    }

    #[test]
    fn duplicate_module_rejected() {
        let result = RelayBuilder::new(RelayServerConfig::default())
            .use_module(TestModule {
                name: "alpha",
                deps: vec![],
            })
            .use_module(TestModule {
                name: "alpha",
                deps: vec![],
            })
            .build();
        assert!(matches!(result, Err(RelayBuildError::DuplicateModule(_))));
    }

    #[test]
    fn missing_dependency_rejected() {
        let result = RelayBuilder::new(RelayServerConfig::default())
            .use_module(TestModule {
                name: "beta",
                deps: vec!["alpha"],
            })
            .build();
        assert!(matches!(
            result,
            Err(RelayBuildError::MissingDependency { .. })
        ));
    }

    #[test]
    fn dependency_satisfied() {
        let relay = RelayBuilder::new(RelayServerConfig::default())
            .use_module(TestModule {
                name: "alpha",
                deps: vec![],
            })
            .use_module(TestModule {
                name: "beta",
                deps: vec!["alpha"],
            })
            .build()
            .unwrap();
        assert_eq!(relay.modules().len(), 2);
    }

    #[test]
    fn capability_roundtrip() {
        let ctx = RelayContext::new(RelayServerConfig::default());
        ctx.set_capability("test", 42u32);
        let val = ctx.get_capability::<u32>("test").unwrap();
        assert_eq!(*val, 42);
        assert!(ctx.get_capability::<String>("test").is_none());
        assert!(ctx.get_capability::<u32>("missing").is_none());
    }

    #[test]
    fn config_defaults() {
        let cfg = RelayServerConfig::default();
        assert_eq!(cfg.default_ttl_ms, 7 * 24 * 60 * 60 * 1000);
        assert_eq!(cfg.max_envelope_size_bytes, 1_048_576);
        assert_eq!(cfg.eviction_interval_ms, 60_000);
    }
}
