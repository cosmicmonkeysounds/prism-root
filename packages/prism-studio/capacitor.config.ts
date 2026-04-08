/**
 * Capacitor configuration for Prism Studio mobile builds.
 *
 * The same Vite SPA that runs in the browser and the Tauri desktop shell
 * is injected into iOS/Android native wrappers via Capacitor. No forks:
 * the built output of `pnpm --filter @prism/studio build` lives under
 * `dist/` and Capacitor syncs it into each platform's native project.
 *
 * One-time scaffolding (not checked in — generates `ios/` and `android/`
 * directories with Xcode / Gradle projects):
 *
 *   cd packages/prism-studio
 *   pnpm cap add ios
 *   pnpm cap add android
 *
 * After scaffolding, the BuilderManager's `capacitor-ios` / `capacitor-
 * android` targets run `pnpm cap sync <platform>` followed by
 * `pnpm cap build <platform>` via the daemon's `run_build_step` command.
 */

import type { CapacitorConfig } from "@capacitor/cli";

const config: CapacitorConfig = {
  appId: "com.prism.studio",
  appName: "Prism Studio",
  webDir: "dist",
  // Capacitor serves the built Vite bundle. No cleartext over HTTP —
  // Prism's daemon bridge goes through Capacitor plugin IPC, not fetch.
  server: {
    androidScheme: "https",
  },
  ios: {
    contentInset: "automatic",
  },
  android: {
    allowMixedContent: false,
  },
};

export default config;
