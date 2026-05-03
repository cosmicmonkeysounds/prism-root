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
use crate::registry::CommandError;
use crate::typed_command::CommandRegistryExt;
use serde::Serialize;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use std::time::Instant;

/// Shared state for the admin module — tracks uptime.
struct AdminState {
    started_at: Instant,
}

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
        let state = Arc::new(AdminState {
            started_at: Instant::now(),
        });

        // Capture the module IDs at install time. Since admin is typically
        // installed last (via with_defaults or explicitly), this snapshot
        // includes all modules installed before it. The full list is also
        // available from kernel.installed_modules() at runtime — but we
        // need a copy here because the command handler closure only captures
        // the registry, not the kernel.
        let module_ids: Vec<String> = builder.module_ids.clone();

        let registry = builder.registry().clone();
        let registry_inner = registry.clone();
        let admin_state = state.clone();
        let mods = module_ids;

        registry.register_typed_user(
            "daemon.admin",
            move |_args: JsonValue| -> Result<AdminSnapshot, std::convert::Infallible> {
                let uptime_seconds = admin_state.started_at.elapsed().as_secs();

                let services: Vec<ServiceEntry> = mods
                    .iter()
                    .map(|id| ServiceEntry {
                        id: id.clone(),
                        name: id.clone(),
                        health: "ok",
                        status: "loaded",
                    })
                    .collect();

                let commands = registry_inner.list();
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
                        detail: format!("{} modules, {} commands", mods.len(), command_count),
                    },
                    uptime_seconds,
                    metrics: vec![
                        Metric {
                            id: "modules",
                            label: "Modules",
                            value: mods.len() as u64,
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
            },
        )?;

        Ok(())
    }
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
