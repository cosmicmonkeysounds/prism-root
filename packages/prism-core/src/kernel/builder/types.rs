//! `kernel::builder::types` — pure data shapes for the self-replicating
//! builder pipeline.
//!
//! Port of `kernel/builder/types.ts` at 8426588. An [`AppProfile`] pins
//! a slice of Studio (plugins, lenses, theme, kbar commands) to a
//! focused app; a [`BuildPlan`] converts a profile + target into a
//! deterministic, serializable list of steps the Prism Daemon can run.
//! Nothing here touches the filesystem — the side-effectful
//! [`BuilderManager`](super::manager::BuilderManager) owns execution.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::collections::BTreeMap;

// ── Built-in profile ids ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuiltInProfileId {
    Studio,
    Flux,
    Lattice,
    Cadence,
    Grip,
    Relay,
}

impl BuiltInProfileId {
    pub fn as_str(self) -> &'static str {
        match self {
            BuiltInProfileId::Studio => "studio",
            BuiltInProfileId::Flux => "flux",
            BuiltInProfileId::Lattice => "lattice",
            BuiltInProfileId::Cadence => "cadence",
            BuiltInProfileId::Grip => "grip",
            BuiltInProfileId::Relay => "relay",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "studio" => Some(BuiltInProfileId::Studio),
            "flux" => Some(BuiltInProfileId::Flux),
            "lattice" => Some(BuiltInProfileId::Lattice),
            "cadence" => Some(BuiltInProfileId::Cadence),
            "grip" => Some(BuiltInProfileId::Grip),
            "relay" => Some(BuiltInProfileId::Relay),
            _ => None,
        }
    }
}

// ── Build targets ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildTarget {
    Web,
    Tauri,
    CapacitorIos,
    CapacitorAndroid,
    RelayNode,
    RelayDocker,
}

pub const ALL_BUILD_TARGETS: &[BuildTarget] = &[
    BuildTarget::Web,
    BuildTarget::Tauri,
    BuildTarget::CapacitorIos,
    BuildTarget::CapacitorAndroid,
    BuildTarget::RelayNode,
    BuildTarget::RelayDocker,
];

// ── App profile ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppThemeConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary: Option<String>,
    #[serde(rename = "brandIcon", default, skip_serializing_if = "Option::is_none")]
    pub brand_icon: Option<String>,
    #[serde(
        rename = "displayName",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterShellChild {
    #[serde(rename = "type")]
    pub type_name: String,
    pub slot: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<BTreeMap<String, JsonValue>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PageTemplateKind {
    Blank,
    Landing,
    Blog,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterRouteTemplate {
    pub path: String,
    pub label: String,
    #[serde(rename = "pageTemplate")]
    pub page_template: PageTemplateKind,
    #[serde(rename = "isHome", default, skip_serializing_if = "Option::is_none")]
    pub is_home: Option<bool>,
    #[serde(rename = "showInNav", default, skip_serializing_if = "Option::is_none")]
    pub show_in_nav: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterShellSlot {
    #[serde(default)]
    pub data: BTreeMap<String, JsonValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<StarterShellChild>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StarterAppTemplate {
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(rename = "appShell")]
    pub app_shell: StarterShellSlot,
    #[serde(rename = "defaultPageShell")]
    pub default_page_shell: StarterShellSlot,
    pub routes: Vec<StarterRouteTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppProfile {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugins: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lenses: Option<Vec<String>>,
    #[serde(
        rename = "defaultLens",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub default_lens: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<AppThemeConfig>,
    #[serde(
        rename = "kbarCommands",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kbar_commands: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<String>,
    #[serde(
        rename = "allowGlassFlip",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub allow_glass_flip: Option<bool>,
    #[serde(
        rename = "relayModules",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub relay_modules: Option<Vec<String>>,
    #[serde(
        rename = "starterApp",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub starter_app: Option<StarterAppTemplate>,
}

// ── Artifacts ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    Directory,
    File,
    DockerImage,
    Installer,
    MobilePackage,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactDescriptor {
    pub kind: ArtifactKind,
    pub path: String,
    pub description: String,
    #[serde(rename = "mimeType", default, skip_serializing_if = "Option::is_none")]
    pub mime_type: Option<String>,
}

// ── Build steps ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum BuildStep {
    EmitFile {
        path: String,
        contents: String,
        description: String,
    },
    RunCommand {
        command: String,
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cwd: Option<String>,
        description: String,
    },
    InvokeIpc {
        name: String,
        payload: BTreeMap<String, JsonValue>,
        description: String,
    },
}

impl BuildStep {
    pub fn description(&self) -> &str {
        match self {
            BuildStep::EmitFile { description, .. } => description,
            BuildStep::RunCommand { description, .. } => description,
            BuildStep::InvokeIpc { description, .. } => description,
        }
    }
}

// ── Build plan ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildPlan {
    #[serde(rename = "profileId")]
    pub profile_id: String,
    #[serde(rename = "profileName")]
    pub profile_name: String,
    pub target: BuildTarget,
    pub steps: Vec<BuildStep>,
    pub artifacts: Vec<ArtifactDescriptor>,
    pub env: BTreeMap<String, String>,
    #[serde(rename = "workingDir")]
    pub working_dir: String,
    #[serde(rename = "dryRun")]
    pub dry_run: bool,
}

// ── Execution result ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildStepStatus {
    Pending,
    Running,
    Success,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildStepResult {
    pub step: BuildStep,
    pub status: BuildStepStatus,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(
        rename = "finishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub finished_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(
        rename = "errorMessage",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildRun {
    pub id: String,
    pub plan: BuildPlan,
    #[serde(rename = "startedAt")]
    pub started_at: u64,
    #[serde(
        rename = "finishedAt",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub finished_at: Option<u64>,
    pub status: BuildStepStatus,
    pub steps: Vec<BuildStepResult>,
    #[serde(rename = "producedArtifacts")]
    pub produced_artifacts: Vec<ArtifactDescriptor>,
}
