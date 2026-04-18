//! `kernel::builder` — self-replicating Studio primitives.
//!
//! Port of `@prism/core/kernel/builder/*` at 8426588 per ADR-002
//! §Part C. The module exposes a pure data model — [`AppProfile`],
//! [`BuildTarget`], [`BuildPlan`], [`BuildStep`] — plus the pure
//! [`create_build_plan`] factory, the built-in profiles for the four
//! ecosystem apps (Flux, Lattice, Cadence, Grip) + universal Studio
//! host + headless Relay profile, a kernel-agnostic
//! [`materialize_starter_app`] helper that turns a `StarterAppTemplate`
//! into a concrete `app → app-shell + routes + pages` tree via a
//! caller-supplied `createObject` closure, and finally the
//! side-effectful [`BuilderManager`] that wraps a [`BuildExecutor`]
//! trait for dry-run / daemon-backed execution.

mod build_plan;
mod manager;
mod profiles;
mod starter_app;
mod types;

pub use build_plan::{create_build_plan, serialize_build_plan, CreateBuildPlanOptions};
pub use manager::{
    create_callback_executor, create_dry_run_executor, BuildExecutionContext, BuildExecutor,
    BuilderListener, BuilderManager, BuilderManagerOptions, BuilderUnsubscribe, CallbackExecutor,
    CallbackExecutorOutput, DryRunExecutor, ExecutorMode,
};
pub use profiles::{
    cadence_profile, flux_profile, get_builtin_profile, grip_profile, lattice_profile,
    list_builtin_profiles, parse_app_profile, relay_profile, serialize_app_profile, studio_profile,
    ProfileParseError,
};
pub use starter_app::{
    materialize_starter_app, MaterializedStarterApp, StarterAppError, StarterCreateObjectFn,
    StarterCreateObjectInput, StarterCreatedObject,
};
pub use types::{
    AppProfile, AppThemeConfig, ArtifactDescriptor, ArtifactKind, BuildPlan, BuildRun, BuildStep,
    BuildStepResult, BuildStepStatus, BuildTarget, BuiltInProfileId, PageTemplateKind,
    StarterAppTemplate, StarterRouteTemplate, StarterShellChild, StarterShellSlot,
    ALL_BUILD_TARGETS,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    // ── Profiles ───────────────────────────────────────────────────────────

    #[test]
    fn builtin_profiles_cover_all_six_ids() {
        let profiles = list_builtin_profiles();
        assert_eq!(profiles.len(), 6);
        let ids: Vec<&str> = profiles.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(
            ids,
            vec!["studio", "flux", "lattice", "cadence", "grip", "relay"]
        );
    }

    #[test]
    fn get_builtin_profile_returns_expected_fields() {
        let flux = get_builtin_profile(BuiltInProfileId::Flux);
        assert_eq!(flux.name, "Flux");
        assert_eq!(flux.default_lens.as_deref(), Some("record-browser"));
        assert!(flux
            .plugins
            .as_ref()
            .unwrap()
            .iter()
            .any(|p| p == "finance"));
        let relay = get_builtin_profile(BuiltInProfileId::Relay);
        assert_eq!(relay.allow_glass_flip, Some(false));
        assert_eq!(relay.plugins.as_ref().unwrap().len(), 0);
        let modules = relay.relay_modules.as_ref().unwrap();
        assert!(modules.iter().any(|m| m == "sovereign-portals"));
    }

    #[test]
    fn serialize_round_trip_preserves_profile() {
        let original = flux_profile();
        let text = serialize_app_profile(&original);
        assert!(text.ends_with('\n'));
        let back = parse_app_profile(&text).unwrap();
        assert_eq!(back.id, original.id);
        assert_eq!(
            back.theme.as_ref().unwrap().primary.as_deref(),
            Some("#6C5CE7")
        );
    }

    #[test]
    fn parse_rejects_non_object() {
        let err = parse_app_profile("[]").unwrap_err();
        assert!(matches!(err, ProfileParseError::NotObject));
    }

    #[test]
    fn parse_rejects_missing_fields() {
        let err = parse_app_profile("{\"id\": \"x\"}").unwrap_err();
        assert!(matches!(err, ProfileParseError::MissingFields));
    }

    // ── Build plan ─────────────────────────────────────────────────────────

    #[test]
    fn web_plan_has_emit_file_then_pnpm_build() {
        let profile = flux_profile();
        let plan = create_build_plan(CreateBuildPlanOptions::new(&profile, BuildTarget::Web));
        assert_eq!(plan.profile_id, "flux");
        assert_eq!(plan.steps.len(), 2);
        assert!(matches!(plan.steps[0], BuildStep::EmitFile { .. }));
        match &plan.steps[1] {
            BuildStep::RunCommand { command, args, .. } => {
                assert_eq!(command, "pnpm");
                assert_eq!(args, &vec!["--filter", "@prism/studio", "build"]);
            }
            _ => panic!("expected run-command"),
        }
        assert_eq!(plan.artifacts.len(), 1);
        assert_eq!(plan.artifacts[0].kind, ArtifactKind::Directory);
        assert!(plan.dry_run);
    }

    #[test]
    fn tauri_plan_emits_three_installer_artifacts() {
        let profile = studio_profile();
        let plan = create_build_plan(CreateBuildPlanOptions::new(&profile, BuildTarget::Tauri));
        assert_eq!(plan.steps.len(), 3);
        assert_eq!(plan.artifacts.len(), 3);
        assert!(plan
            .artifacts
            .iter()
            .all(|a| a.kind == ArtifactKind::Installer));
        assert!(plan.artifacts[0].path.contains("Prism Studio.dmg"));
    }

    #[test]
    fn capacitor_ios_and_android_have_distinct_artifacts() {
        let profile = flux_profile();
        let ios = create_build_plan(CreateBuildPlanOptions::new(
            &profile,
            BuildTarget::CapacitorIos,
        ));
        let android = create_build_plan(CreateBuildPlanOptions::new(
            &profile,
            BuildTarget::CapacitorAndroid,
        ));
        assert_eq!(ios.artifacts.len(), 1);
        assert_eq!(android.artifacts.len(), 2);
        assert!(android.artifacts[0].path.ends_with("flux-release.apk"));
        assert!(android.artifacts[1].path.ends_with("flux-release.aab"));
    }

    #[test]
    fn relay_docker_plan_tags_image_with_profile_and_version() {
        let profile = relay_profile();
        let plan = create_build_plan(CreateBuildPlanOptions::new(
            &profile,
            BuildTarget::RelayDocker,
        ));
        // Emit profile + emit relay.config + pnpm build + docker build
        assert_eq!(plan.steps.len(), 4);
        let last = plan.steps.last().unwrap();
        match last {
            BuildStep::RunCommand { command, args, .. } => {
                assert_eq!(command, "docker");
                assert!(args.iter().any(|a| a == "prism-relay:relay-0.1.0"));
            }
            _ => panic!("expected docker run-command last"),
        }
        assert_eq!(plan.artifacts.len(), 1);
        assert_eq!(plan.artifacts[0].kind, ArtifactKind::DockerImage);
    }

    #[test]
    fn serialize_build_plan_ends_with_newline() {
        let profile = flux_profile();
        let plan = create_build_plan(CreateBuildPlanOptions::new(&profile, BuildTarget::Web));
        let s = serialize_build_plan(&plan);
        assert!(s.ends_with('\n'));
        assert!(s.contains("\"profileId\": \"flux\""));
    }

    // ── Starter-app materialiser ───────────────────────────────────────────

    #[test]
    fn materialize_flux_starter_creates_app_shell_routes_pages() {
        let profile = flux_profile();
        let counter = Cell::new(0u64);
        let mut create = |input: StarterCreateObjectInput| {
            let id = format!("obj-{}-{}", input.type_name, counter.get());
            counter.set(counter.get() + 1);
            StarterCreatedObject { id }
        };
        let result = materialize_starter_app(&profile, &mut create).unwrap();
        assert!(result.app_id.starts_with("obj-app-"));
        assert!(result.app_shell_id.starts_with("obj-app-shell-"));
        assert_eq!(result.route_ids.len(), 4);
        assert_eq!(result.route_to_page_id.len(), 4);
        assert!(result.home_route_id.starts_with("obj-route-"));
    }

    #[test]
    fn materialize_blank_studio_profile_emits_landing_hero_for_home() {
        let profile = studio_profile();
        let collected: std::rc::Rc<std::cell::RefCell<Vec<String>>> =
            std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let c2 = collected.clone();
        let counter = Cell::new(0u64);
        let mut create = move |input: StarterCreateObjectInput| {
            c2.borrow_mut().push(input.type_name.clone());
            let id = format!("obj-{}", counter.get());
            counter.set(counter.get() + 1);
            StarterCreatedObject { id }
        };
        materialize_starter_app(&profile, &mut create).unwrap();
        let types = collected.borrow();
        // Studio has `landing` home + `blog` docs — both should seed
        // a hero/heading + text-block pair into page-shells.
        assert!(types.iter().any(|t| t == "hero"));
        assert!(types.iter().any(|t| t == "text-block"));
        assert!(types.iter().any(|t| t == "heading")); // from blog template
    }

    #[test]
    fn materialize_profile_without_template_fails() {
        let profile = relay_profile(); // has no starterApp
        let counter = Cell::new(0u64);
        let mut create = |_: StarterCreateObjectInput| StarterCreatedObject {
            id: format!("obj-{}", {
                let c = counter.get();
                counter.set(c + 1);
                c
            }),
        };
        let err = materialize_starter_app(&profile, &mut create).unwrap_err();
        assert!(matches!(err, StarterAppError::MissingTemplate(_)));
    }

    // ── BuilderManager ─────────────────────────────────────────────────────

    #[test]
    fn manager_seeds_builtin_profiles() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let profiles = mgr.list_profiles();
        assert_eq!(profiles.len(), 6);
        assert!(mgr.get_profile("flux").is_some());
    }

    #[test]
    fn manager_cannot_remove_builtin_profile() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        assert!(!mgr.remove_profile("flux"));
        assert!(mgr.get_profile("flux").is_some());
    }

    #[test]
    fn manager_register_and_remove_custom_profile() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let mut custom = studio_profile();
        custom.id = "my-app".into();
        custom.name = "My App".into();
        mgr.register_profile(custom);
        assert!(mgr.get_profile("my-app").is_some());
        assert!(mgr.remove_profile("my-app"));
        assert!(mgr.get_profile("my-app").is_none());
    }

    #[test]
    fn manager_set_active_profile_rejects_unknown() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        assert!(mgr.set_active_profile(Some("nope")).is_err());
        assert!(mgr.set_active_profile(Some("flux")).is_ok());
        assert_eq!(mgr.get_active_profile().unwrap().id, "flux");
        assert!(mgr.set_active_profile(None).is_ok());
        assert!(mgr.get_active_profile().is_none());
    }

    #[test]
    fn manager_dry_run_plan_runs_emit_step_and_skips_commands() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let plan = mgr.plan_build("flux", BuildTarget::Web, true).unwrap();
        let run = mgr.run_plan(plan);
        assert_eq!(run.status, BuildStepStatus::Success);
        assert_eq!(run.steps.len(), 2);
        assert_eq!(run.steps[0].status, BuildStepStatus::Success);
        assert_eq!(run.steps[1].status, BuildStepStatus::Skipped);
        // Dry-run still reports declared artifacts.
        assert_eq!(run.produced_artifacts.len(), 1);
    }

    #[test]
    fn manager_callback_executor_reports_failure_and_stops_chain() {
        let exec = create_callback_executor(|step, _ctx| match step {
            BuildStep::EmitFile { .. } => Ok(CallbackExecutorOutput {
                stdout: Some("wrote".into()),
                stderr: None,
            }),
            _ => Err("nope".into()),
        });
        let mgr = BuilderManager::new(BuilderManagerOptions {
            executor: Some(exec),
            profiles: Vec::new(),
        });
        let plan = mgr.plan_build("flux", BuildTarget::Web, false).unwrap();
        let run = mgr.run_plan(plan);
        assert_eq!(run.status, BuildStepStatus::Failed);
        assert_eq!(run.steps.len(), 2);
        assert_eq!(run.steps[1].status, BuildStepStatus::Failed);
        assert_eq!(run.steps[1].error_message.as_deref(), Some("nope"));
    }

    #[test]
    fn manager_subscribe_fires_on_mutations_and_unsubscribes() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let counter = Arc::new(AtomicUsize::new(0));
        let counter_cb = Arc::clone(&counter);
        let unsub = mgr.subscribe(move || {
            counter_cb.fetch_add(1, Ordering::SeqCst);
        });
        mgr.set_active_profile(Some("flux")).unwrap();
        mgr.set_active_profile(None).unwrap();
        unsub();
        mgr.set_active_profile(Some("flux")).unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn manager_plan_builds_creates_one_per_target() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let plans = mgr
            .plan_builds("flux", &[BuildTarget::Web, BuildTarget::Tauri], true)
            .unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(plans[0].target, BuildTarget::Web);
        assert_eq!(plans[1].target, BuildTarget::Tauri);
    }

    #[test]
    fn manager_list_runs_is_sorted_newest_first() {
        let mgr = BuilderManager::new(BuilderManagerOptions::default());
        let p1 = mgr.plan_build("flux", BuildTarget::Web, true).unwrap();
        let p2 = mgr.plan_build("flux", BuildTarget::Tauri, true).unwrap();
        let _r1 = mgr.run_plan(p1);
        let _r2 = mgr.run_plan(p2);
        let runs = mgr.list_runs();
        assert_eq!(runs.len(), 2);
        assert!(runs[0].started_at >= runs[1].started_at);
        mgr.clear_runs();
        assert!(mgr.list_runs().is_empty());
    }
}
