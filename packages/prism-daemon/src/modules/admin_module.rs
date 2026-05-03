//! Admin module — exposes `daemon.admin` returning a normalised admin
//! snapshot matching the `AdminSnapshot` shape from `@prism/admin-kit`.
//!
//! This gives every transport adapter (HTTP, stdio, IPC, WASM) a single
//! command that returns everything a dashboard needs: health, uptime,
//! metrics, services, and activity.
//!
//! `daemon.admin` is registered at [`Permission::User`] — it's strictly
//! read-only introspection and has to be reachable from published
//! end-user builds (Flux / Lattice / Musica) so their dashboards can
//! render a health badge without switching to a dev-tier kernel.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::{CommandError, CommandRegistry};
use prism_luau_derive::daemon_command;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;

/// Shared state for the admin module — tracks uptime, the module IDs
/// captured at install, and a back-reference to the registry so the
/// snapshot handler can introspect command counts.
struct AdminState {
    started_at: Instant,
    module_ids: Vec<String>,
    registry: Arc<CommandRegistry>,
}

#[derive(Debug, Default, Deserialize)]
struct AdminArgs {}

#[derive(Debug, Serialize)]
pub struct AdminSnapshot {
    pub health: HealthSnapshot,
    #[serde(rename = "uptimeSeconds")]
    pub uptime_seconds: u64,
    pub metrics: Vec<Metric>,
    pub services: Vec<ServiceEntry>,
    pub activity: Vec<JsonValue>,
}

#[derive(Debug, Serialize)]
pub struct HealthSnapshot {
    pub level: &'static str,
    pub label: &'static str,
    pub detail: String,
}

#[derive(Debug, Serialize)]
pub struct Metric {
    pub id: &'static str,
    pub label: &'static str,
    pub value: u64,
}

#[derive(Debug, Serialize)]
pub struct ServiceEntry {
    pub id: String,
    pub name: String,
    pub health: &'static str,
    pub status: &'static str,
}

pub struct AdminModule;

impl DaemonModule for AdminModule {
    fn id(&self) -> &str {
        "admin"
    }

    fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
        // Capture module IDs at install time. Admin is typically installed
        // last (via with_defaults), so this snapshot covers every module
        // that came before. The handler also keeps a registry handle so
        // it can introspect live command counts.
        let registry: Arc<CommandRegistry> = builder.registry().clone();
        let state = Arc::new(AdminState {
            started_at: Instant::now(),
            module_ids: builder.module_ids.clone(),
            registry: registry.clone(),
        });

        register_admin_snapshot(&registry, state)?;
        Ok(())
    }
}

#[daemon_command(id = "daemon.admin", permission = User)]
fn admin_snapshot(
    state: &AdminState,
    _args: AdminArgs,
) -> Result<AdminSnapshot, std::convert::Infallible> {
    let uptime_seconds = state.started_at.elapsed().as_secs();

    let services: Vec<ServiceEntry> = state
        .module_ids
        .iter()
        .map(|id| ServiceEntry {
            id: id.clone(),
            name: id.clone(),
            health: "ok",
            status: "loaded",
        })
        .collect();

    let commands = state.registry.list();
    let command_count = commands.len();

    let mut module_set = std::collections::HashSet::new();
    for cmd in &commands {
        if let Some(dot) = cmd.find('.') {
            module_set.insert(cmd[..dot].to_string());
        }
    }

    Ok(AdminSnapshot {
        health: HealthSnapshot {
            level: "ok",
            label: "Healthy",
            detail: format!(
                "{} modules, {} commands",
                state.module_ids.len(),
                command_count
            ),
        },
        uptime_seconds,
        metrics: vec![
            Metric {
                id: "modules",
                label: "Modules",
                value: state.module_ids.len() as u64,
            },
            Metric {
                id: "commands",
                label: "Commands",
                value: command_count as u64,
            },
            Metric {
                id: "namespaces",
                label: "Namespaces",
                value: module_set.len() as u64,
            },
        ],
        services,
        activity: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;
    use serde_json::json;

    #[test]
    fn admin_module_registers_command() {
        let kernel = DaemonBuilder::new()
            .with_module(AdminModule)
            .build()
            .unwrap();
        assert!(kernel.capabilities().contains(&"daemon.admin".to_string()));
    }

    #[test]
    fn admin_command_returns_valid_snapshot() {
        let kernel = DaemonBuilder::new()
            .with_module(AdminModule)
            .build()
            .unwrap();

        let result = kernel.invoke("daemon.admin", json!({})).unwrap();

        assert_eq!(result["health"]["level"], "ok");
        assert_eq!(result["health"]["label"], "Healthy");
        assert!(result["uptimeSeconds"].is_u64());
        assert!(result["metrics"].is_array());
        assert!(result["services"].is_array());
        assert!(result["activity"].is_array());

        // Admin captures modules installed *before* it, so when it's the
        // only module, the services list is empty. The commands metric
        // still reflects daemon.admin since it reads from the live registry.
        let metrics = result["metrics"].as_array().unwrap();
        let cmd_metric = metrics.iter().find(|m| m["id"] == "commands").unwrap();
        assert!(cmd_metric["value"].as_u64().unwrap() >= 1);
    }

    #[test]
    fn admin_with_other_modules() {
        struct DummyModule;
        impl DaemonModule for DummyModule {
            fn id(&self) -> &str {
                "dummy"
            }
            fn install(&self, builder: &mut DaemonBuilder) -> Result<(), CommandError> {
                builder
                    .registry()
                    .register("dummy.ping", |_| Ok(json!("pong")))?;
                Ok(())
            }
        }

        let kernel = DaemonBuilder::new()
            .with_module(DummyModule)
            .with_module(AdminModule)
            .build()
            .unwrap();

        let result = kernel.invoke("daemon.admin", json!({})).unwrap();

        // Dummy was installed before admin, so it shows in services
        let services = result["services"].as_array().unwrap();
        assert!(services.iter().any(|s| s["id"] == "dummy"));

        // Should count both commands (dummy.ping + daemon.admin)
        let metrics = result["metrics"].as_array().unwrap();
        let cmd_metric = metrics.iter().find(|m| m["id"] == "commands").unwrap();
        assert!(cmd_metric["value"].as_u64().unwrap() >= 2);
    }
}
