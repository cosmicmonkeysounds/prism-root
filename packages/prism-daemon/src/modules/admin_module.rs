//! Admin module — exposes `daemon.admin` returning a normalised admin
//! snapshot matching the `AdminSnapshot` shape from `@prism/admin-kit`.
//!
//! This gives every transport adapter (HTTP, stdio, Tauri, WASM) a single
//! command that returns everything a dashboard needs: health, uptime,
//! metrics, services, and activity.

use crate::builder::DaemonBuilder;
use crate::module::DaemonModule;
use crate::registry::CommandError;
use serde_json::{json, Value as JsonValue};
use std::sync::Arc;
use std::time::Instant;

/// Shared state for the admin module — tracks uptime.
struct AdminState {
    started_at: Instant,
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
        // Clone for the closure — the outer `registry` is used for .register()
        let registry_inner = registry.clone();
        let admin_state = state.clone();
        let mods = module_ids.clone();

        registry.register("daemon.admin", move |_payload: JsonValue| {
            let uptime = admin_state.started_at.elapsed();
            let uptime_seconds = uptime.as_secs();

            // Derive the list of services from modules
            let services: Vec<JsonValue> = mods
                .iter()
                .map(|id| {
                    json!({
                        "id": id,
                        "name": id,
                        "health": "ok",
                        "status": "loaded"
                    })
                })
                .collect();

            // Count registered commands
            let commands = registry_inner.list();
            let command_count = commands.len();

            // Group commands by module prefix for metric display
            let mut module_set = std::collections::HashSet::new();
            for cmd in &commands {
                if let Some(dot) = cmd.find('.') {
                    module_set.insert(cmd[..dot].to_string());
                }
            }

            Ok(json!({
                "health": {
                    "level": "ok",
                    "label": "Healthy",
                    "detail": format!("{} modules, {} commands", mods.len(), command_count)
                },
                "uptimeSeconds": uptime_seconds,
                "metrics": [
                    { "id": "modules", "label": "Modules", "value": mods.len() },
                    { "id": "commands", "label": "Commands", "value": command_count },
                    { "id": "namespaces", "label": "Namespaces", "value": module_set.len() },
                ],
                "services": services,
                "activity": []
            }))
        })?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builder::DaemonBuilder;

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
