/**
 * BuildPlan factory — converts an AppProfile + BuildTarget into a
 * deterministic list of BuildSteps and expected ArtifactDescriptors.
 *
 * All planning is pure TypeScript — no filesystem, no spawn(). The
 * BuilderManager in Studio executes the plan by dispatching each step
 * to the Prism Daemon via Tauri IPC, or emits the plan as JSON in
 * dry-run mode.
 */

import type {
  AppProfile,
  BuildPlan,
  BuildStep,
  BuildTarget,
  ArtifactDescriptor,
} from "./types.js";
import { serializeAppProfile } from "./profiles.js";

export interface CreateBuildPlanOptions {
  profile: AppProfile;
  target: BuildTarget;
  /** Optional override for the working directory (relative to monorepo). */
  workingDir?: string;
  /** Dry-run mode — emit files but skip run-command/invoke-ipc execution. Default: true. */
  dryRun?: boolean;
  /** Additional env vars to pass to daemon commands. */
  env?: Record<string, string>;
}

const DEFAULT_WORKING_DIR = "packages/prism-studio";

function profileEmitStep(profile: AppProfile): BuildStep {
  return {
    kind: "emit-file",
    path: `.prism/profiles/${profile.id}.prism-app.json`,
    contents: serializeAppProfile(profile),
    description: `Emit pinned App Profile for ${profile.name}`,
  };
}

function webTargetSteps(profile: AppProfile): {
  steps: BuildStep[];
  artifacts: ArtifactDescriptor[];
} {
  const steps: BuildStep[] = [
    profileEmitStep(profile),
    {
      kind: "run-command",
      command: "pnpm",
      args: ["--filter", "@prism/studio", "build"],
      description: "Vite production build",
    },
  ];
  const artifacts: ArtifactDescriptor[] = [
    {
      kind: "directory",
      path: "packages/prism-studio/dist",
      description: `Static web build for ${profile.name}`,
    },
  ];
  return { steps, artifacts };
}

function tauriTargetSteps(profile: AppProfile): {
  steps: BuildStep[];
  artifacts: ArtifactDescriptor[];
} {
  const steps: BuildStep[] = [
    profileEmitStep(profile),
    {
      kind: "run-command",
      command: "pnpm",
      args: ["--filter", "@prism/studio", "build"],
      description: "Vite production build (Tauri frontend)",
    },
    {
      kind: "run-command",
      command: "pnpm",
      args: ["--filter", "@prism/studio", "tauri", "build"],
      description: "Tauri 2.0 desktop bundle",
    },
  ];
  const artifacts: ArtifactDescriptor[] = [
    {
      kind: "installer",
      path: `packages/prism-studio/src-tauri/target/release/bundle/dmg/${profile.name}.dmg`,
      description: "macOS disk image",
      mimeType: "application/x-apple-diskimage",
    },
    {
      kind: "installer",
      path: `packages/prism-studio/src-tauri/target/release/bundle/msi/${profile.name}.msi`,
      description: "Windows installer",
      mimeType: "application/x-msi",
    },
    {
      kind: "installer",
      path: `packages/prism-studio/src-tauri/target/release/bundle/appimage/${profile.name}.AppImage`,
      description: "Linux AppImage",
    },
  ];
  return { steps, artifacts };
}

function capacitorTargetSteps(
  profile: AppProfile,
  platform: "ios" | "android",
): { steps: BuildStep[]; artifacts: ArtifactDescriptor[] } {
  const steps: BuildStep[] = [
    profileEmitStep(profile),
    {
      kind: "run-command",
      command: "pnpm",
      args: ["--filter", "@prism/studio", "build"],
      description: "Vite production build (Capacitor web assets)",
    },
    {
      kind: "run-command",
      command: "pnpm",
      args: ["cap", "sync", platform],
      description: `Capacitor sync (${platform})`,
    },
    {
      kind: "run-command",
      command: "pnpm",
      args: ["cap", "build", platform],
      description: `Capacitor build (${platform})`,
    },
  ];
  const artifacts: ArtifactDescriptor[] =
    platform === "ios"
      ? [
          {
            kind: "mobile-package",
            path: `packages/prism-studio/ios/App/build/${profile.name}.ipa`,
            description: "iOS application archive",
          },
        ]
      : [
          {
            kind: "mobile-package",
            path: `packages/prism-studio/android/app/build/outputs/apk/release/${profile.id}-release.apk`,
            description: "Android APK",
          },
          {
            kind: "mobile-package",
            path: `packages/prism-studio/android/app/build/outputs/bundle/release/${profile.id}-release.aab`,
            description: "Android App Bundle",
          },
        ];
  return { steps, artifacts };
}

function relayNodeTargetSteps(profile: AppProfile): {
  steps: BuildStep[];
  artifacts: ArtifactDescriptor[];
} {
  const modules = profile.relayModules ?? [];
  const relayConfig = {
    mode: "server",
    modules,
    did: null,
    httpPort: 8080,
    wsPort: 8081,
  };
  const steps: BuildStep[] = [
    profileEmitStep(profile),
    {
      kind: "emit-file",
      path: ".prism/relay/relay.config.json",
      contents: `${JSON.stringify(relayConfig, null, 2)}\n`,
      description: "Emit relay.config.json from composed modules",
    },
    {
      kind: "run-command",
      command: "pnpm",
      args: ["--filter", "@prism/relay", "build"],
      description: "Node/TypeScript build for Relay",
    },
  ];
  const artifacts: ArtifactDescriptor[] = [
    {
      kind: "directory",
      path: "packages/prism-relay/dist",
      description: "Compiled Relay Node bundle",
    },
    {
      kind: "file",
      path: ".prism/relay/relay.config.json",
      description: "Composed relay configuration",
    },
  ];
  return { steps, artifacts };
}

function relayDockerTargetSteps(profile: AppProfile): {
  steps: BuildStep[];
  artifacts: ArtifactDescriptor[];
} {
  const { steps: nodeSteps } = relayNodeTargetSteps(profile);
  const steps: BuildStep[] = [
    ...nodeSteps,
    {
      kind: "run-command",
      command: "docker",
      args: [
        "build",
        "-f",
        "packages/prism-relay/Dockerfile",
        "-t",
        `prism-relay:${profile.id}-${profile.version}`,
        ".",
      ],
      description: "Build Relay OCI image",
    },
  ];
  const artifacts: ArtifactDescriptor[] = [
    {
      kind: "docker-image",
      path: `prism-relay:${profile.id}-${profile.version}`,
      description: "Relay Docker image tag",
    },
  ];
  return { steps, artifacts };
}

/**
 * Convert an AppProfile + BuildTarget into a BuildPlan.
 *
 * Pure function — same inputs always produce the same output, making
 * plans cache-friendly and easy to inspect in CI or tests.
 */
export function createBuildPlan(options: CreateBuildPlanOptions): BuildPlan {
  const { profile, target } = options;
  const workingDir = options.workingDir ?? DEFAULT_WORKING_DIR;
  const dryRun = options.dryRun ?? true;
  const env = options.env ?? {};

  let steps: BuildStep[];
  let artifacts: ArtifactDescriptor[];

  switch (target) {
    case "web": {
      ({ steps, artifacts } = webTargetSteps(profile));
      break;
    }
    case "tauri": {
      ({ steps, artifacts } = tauriTargetSteps(profile));
      break;
    }
    case "capacitor-ios": {
      ({ steps, artifacts } = capacitorTargetSteps(profile, "ios"));
      break;
    }
    case "capacitor-android": {
      ({ steps, artifacts } = capacitorTargetSteps(profile, "android"));
      break;
    }
    case "relay-node": {
      ({ steps, artifacts } = relayNodeTargetSteps(profile));
      break;
    }
    case "relay-docker": {
      ({ steps, artifacts } = relayDockerTargetSteps(profile));
      break;
    }
    default: {
      const exhaustive: never = target;
      throw new Error(`Unknown build target: ${String(exhaustive)}`);
    }
  }

  return {
    profileId: profile.id,
    profileName: profile.name,
    target,
    steps,
    artifacts,
    env,
    workingDir,
    dryRun,
  };
}

/** Serialize a BuildPlan to JSON text (canonical form). */
export function serializeBuildPlan(plan: BuildPlan): string {
  return `${JSON.stringify(plan, null, 2)}\n`;
}
