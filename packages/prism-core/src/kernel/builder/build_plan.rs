//! `kernel::builder::build_plan` — pure `AppProfile` + `BuildTarget` →
//! `BuildPlan` factory.
//!
//! Port of `kernel/builder/build-plan.ts` at 8426588. Same inputs
//! always produce the same plan — deterministic, cache-friendly, and
//! inspectable in CI.

use std::collections::BTreeMap;

use super::profiles::serialize_app_profile;
use super::types::{
    AppProfile, ArtifactDescriptor, ArtifactKind, BuildPlan, BuildStep, BuildTarget,
};

const DEFAULT_WORKING_DIR: &str = "packages/prism-studio";

#[derive(Debug, Clone)]
pub struct CreateBuildPlanOptions<'a> {
    pub profile: &'a AppProfile,
    pub target: BuildTarget,
    pub working_dir: Option<String>,
    pub dry_run: Option<bool>,
    pub env: Option<BTreeMap<String, String>>,
}

impl<'a> CreateBuildPlanOptions<'a> {
    pub fn new(profile: &'a AppProfile, target: BuildTarget) -> Self {
        Self {
            profile,
            target,
            working_dir: None,
            dry_run: None,
            env: None,
        }
    }
}

fn profile_emit_step(profile: &AppProfile) -> BuildStep {
    BuildStep::EmitFile {
        path: format!(".prism/profiles/{}.prism-app.json", profile.id),
        contents: serialize_app_profile(profile),
        description: format!("Emit pinned App Profile for {}", profile.name),
    }
}

fn run_command(command: &str, args: &[&str], description: &str) -> BuildStep {
    BuildStep::RunCommand {
        command: command.into(),
        args: args.iter().map(|s| (*s).into()).collect(),
        cwd: None,
        description: description.into(),
    }
}

fn web_target(profile: &AppProfile) -> (Vec<BuildStep>, Vec<ArtifactDescriptor>) {
    let steps = vec![
        profile_emit_step(profile),
        run_command(
            "pnpm",
            &["--filter", "@prism/studio", "build"],
            "Vite production build",
        ),
    ];
    let artifacts = vec![ArtifactDescriptor {
        kind: ArtifactKind::Directory,
        path: "packages/prism-studio/dist".into(),
        description: format!("Static web build for {}", profile.name),
        mime_type: None,
    }];
    (steps, artifacts)
}

fn tauri_target(profile: &AppProfile) -> (Vec<BuildStep>, Vec<ArtifactDescriptor>) {
    let steps = vec![
        profile_emit_step(profile),
        run_command(
            "pnpm",
            &["--filter", "@prism/studio", "build"],
            "Vite production build (Tauri frontend)",
        ),
        run_command(
            "pnpm",
            &["--filter", "@prism/studio", "tauri", "build"],
            "Tauri 2.0 desktop bundle",
        ),
    ];
    let artifacts = vec![
        ArtifactDescriptor {
            kind: ArtifactKind::Installer,
            path: format!(
                "packages/prism-studio/src-tauri/target/release/bundle/dmg/{}.dmg",
                profile.name
            ),
            description: "macOS disk image".into(),
            mime_type: Some("application/x-apple-diskimage".into()),
        },
        ArtifactDescriptor {
            kind: ArtifactKind::Installer,
            path: format!(
                "packages/prism-studio/src-tauri/target/release/bundle/msi/{}.msi",
                profile.name
            ),
            description: "Windows installer".into(),
            mime_type: Some("application/x-msi".into()),
        },
        ArtifactDescriptor {
            kind: ArtifactKind::Installer,
            path: format!(
                "packages/prism-studio/src-tauri/target/release/bundle/appimage/{}.AppImage",
                profile.name
            ),
            description: "Linux AppImage".into(),
            mime_type: None,
        },
    ];
    (steps, artifacts)
}

fn capacitor_target(
    profile: &AppProfile,
    platform: &str,
) -> (Vec<BuildStep>, Vec<ArtifactDescriptor>) {
    let steps = vec![
        profile_emit_step(profile),
        run_command(
            "pnpm",
            &["--filter", "@prism/studio", "build"],
            "Vite production build (Capacitor web assets)",
        ),
        run_command(
            "pnpm",
            &["cap", "sync", platform],
            &format!("Capacitor sync ({})", platform),
        ),
        run_command(
            "pnpm",
            &["cap", "build", platform],
            &format!("Capacitor build ({})", platform),
        ),
    ];
    let artifacts = if platform == "ios" {
        vec![ArtifactDescriptor {
            kind: ArtifactKind::MobilePackage,
            path: format!("packages/prism-studio/ios/App/build/{}.ipa", profile.name),
            description: "iOS application archive".into(),
            mime_type: None,
        }]
    } else {
        vec![
            ArtifactDescriptor {
                kind: ArtifactKind::MobilePackage,
                path: format!(
                    "packages/prism-studio/android/app/build/outputs/apk/release/{}-release.apk",
                    profile.id
                ),
                description: "Android APK".into(),
                mime_type: None,
            },
            ArtifactDescriptor {
                kind: ArtifactKind::MobilePackage,
                path: format!(
                    "packages/prism-studio/android/app/build/outputs/bundle/release/{}-release.aab",
                    profile.id
                ),
                description: "Android App Bundle".into(),
                mime_type: None,
            },
        ]
    };
    (steps, artifacts)
}

fn relay_node_target(profile: &AppProfile) -> (Vec<BuildStep>, Vec<ArtifactDescriptor>) {
    let modules = profile.relay_modules.clone().unwrap_or_default();
    let relay_config = serde_json::json!({
        "mode": "server",
        "modules": modules,
        "did": null,
        "httpPort": 8080,
        "wsPort": 8081,
    });
    let mut contents = serde_json::to_string_pretty(&relay_config).expect("relay config");
    contents.push('\n');
    let steps = vec![
        profile_emit_step(profile),
        BuildStep::EmitFile {
            path: ".prism/relay/relay.config.json".into(),
            contents,
            description: "Emit relay.config.json from composed modules".into(),
        },
        run_command(
            "pnpm",
            &["--filter", "@prism/relay", "build"],
            "Node/TypeScript build for Relay",
        ),
    ];
    let artifacts = vec![
        ArtifactDescriptor {
            kind: ArtifactKind::Directory,
            path: "packages/prism-relay/dist".into(),
            description: "Compiled Relay Node bundle".into(),
            mime_type: None,
        },
        ArtifactDescriptor {
            kind: ArtifactKind::File,
            path: ".prism/relay/relay.config.json".into(),
            description: "Composed relay configuration".into(),
            mime_type: None,
        },
    ];
    (steps, artifacts)
}

fn relay_docker_target(profile: &AppProfile) -> (Vec<BuildStep>, Vec<ArtifactDescriptor>) {
    let (mut steps, _) = relay_node_target(profile);
    let image_tag = format!("prism-relay:{}-{}", profile.id, profile.version);
    steps.push(BuildStep::RunCommand {
        command: "docker".into(),
        args: vec![
            "build".into(),
            "-f".into(),
            "packages/prism-relay/Dockerfile".into(),
            "-t".into(),
            image_tag.clone(),
            ".".into(),
        ],
        cwd: None,
        description: "Build Relay OCI image".into(),
    });
    let artifacts = vec![ArtifactDescriptor {
        kind: ArtifactKind::DockerImage,
        path: image_tag,
        description: "Relay Docker image tag".into(),
        mime_type: None,
    }];
    (steps, artifacts)
}

pub fn create_build_plan(options: CreateBuildPlanOptions<'_>) -> BuildPlan {
    let profile = options.profile;
    let target = options.target;
    let working_dir = options
        .working_dir
        .unwrap_or_else(|| DEFAULT_WORKING_DIR.into());
    let dry_run = options.dry_run.unwrap_or(true);
    let env = options.env.unwrap_or_default();

    let (steps, artifacts) = match target {
        BuildTarget::Web => web_target(profile),
        BuildTarget::Tauri => tauri_target(profile),
        BuildTarget::CapacitorIos => capacitor_target(profile, "ios"),
        BuildTarget::CapacitorAndroid => capacitor_target(profile, "android"),
        BuildTarget::RelayNode => relay_node_target(profile),
        BuildTarget::RelayDocker => relay_docker_target(profile),
    };

    BuildPlan {
        profile_id: profile.id.clone(),
        profile_name: profile.name.clone(),
        target,
        steps,
        artifacts,
        env,
        working_dir,
        dry_run,
    }
}

pub fn serialize_build_plan(plan: &BuildPlan) -> String {
    let mut out = serde_json::to_string_pretty(plan).expect("build plan serialises cleanly");
    out.push('\n');
    out
}
