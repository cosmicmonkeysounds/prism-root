#!/usr/bin/env node
/**
 * prism-studio — Node launcher for Studio.
 *
 * Thin wrapper around Vite that translates four high-level subcommands
 * into a `BootConfig` JSON, shoves it into the `VITE_PRISM_BOOT_CONFIG`
 * env var, and spawns the dev server (or a production `vite build`).
 *
 *   prism-studio run     [--profile=flux] [vite args...]   # use mode, user permission
 *   prism-studio build   [--profile=flux] [vite args...]   # build mode, dev permission
 *   prism-studio admin   [--profile=flux] [vite args...]   # admin mode, dev permission
 *   prism-studio dev     [vite args...]                     # legacy: no boot override
 *   prism-studio bundle  [vite args...]                     # production `vite build`
 *
 * The boot-config resolver (`src/boot/load-boot-config.ts`) reads
 * `VITE_PRISM_BOOT_CONFIG` on startup, so this launcher doesn't need to
 * touch Studio source to swap modes — the same bundle can boot into any
 * of the three shell trees depending on how it was launched.
 *
 * Kept intentionally small: no arg parser, no color library, no helpers.
 * Pure helpers live next to the side-effects so the bin is testable in
 * isolation (see `bin/prism-studio.test.mjs`).
 */

import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const HERE = dirname(fileURLToPath(import.meta.url));
const PACKAGE_ROOT = resolve(HERE, "..");

// ── Subcommand → BootConfig table ───────────────────────────────────────
//
// `dev` is intentionally absent so the boot resolver falls through to
// its build-time default (full IDE, dev permission). `bundle` is a
// production `vite build` and also skips boot-config injection — the
// build ceiling comes from VITE_PRISM_BOOT_DEFAULT, not the subcommand.

const SUBCOMMAND_BOOT_CONFIGS = Object.freeze({
  run: Object.freeze({ shellMode: "use", permission: "user" }),
  build: Object.freeze({ shellMode: "build", permission: "dev" }),
  admin: Object.freeze({ shellMode: "admin", permission: "dev" }),
});

const KNOWN_SUBCOMMANDS = Object.freeze([
  "run",
  "build",
  "admin",
  "dev",
  "bundle",
  "help",
  "--help",
  "-h",
]);

/**
 * Merge a subcommand's boot config with any `--profile=…` flag found in
 * argv. Returns `null` when the subcommand has no baseline (dev/bundle),
 * signalling the launcher to leave `VITE_PRISM_BOOT_CONFIG` untouched.
 */
export function buildBootConfigForSubcommand(subcommand, argv) {
  const baseline = SUBCOMMAND_BOOT_CONFIGS[subcommand];
  if (!baseline) return null;
  const out = { ...baseline };
  const profile = extractProfileFlag(argv);
  if (profile) out.profile = profile;
  out.launcher = {
    name: "prism-studio",
    startedAt: new Date().toISOString(),
  };
  return out;
}

/**
 * Scan for `--profile=<value>` or `--profile <value>` and return the
 * first match. Returns `undefined` when no profile flag is present.
 * Does NOT mutate argv — the caller forwards it verbatim to vite so
 * unknown flags reach the underlying process unchanged.
 */
export function extractProfileFlag(argv) {
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (typeof arg !== "string") continue;
    if (arg.startsWith("--profile=")) {
      const value = arg.slice("--profile=".length);
      return value.length > 0 ? value : undefined;
    }
    if (arg === "--profile") {
      const next = argv[i + 1];
      if (typeof next === "string" && next.length > 0) return next;
      return undefined;
    }
  }
  return undefined;
}

/**
 * Assemble the child env. Returns a new object — does not mutate the
 * caller's env. The boot config is stringified into `VITE_PRISM_BOOT_CONFIG`
 * so Studio's boot resolver can pick it up in the browser.
 */
export function buildChildEnv(baseEnv, bootConfig) {
  const out = { ...baseEnv };
  if (bootConfig) {
    out.VITE_PRISM_BOOT_CONFIG = JSON.stringify(bootConfig);
  }
  return out;
}

/**
 * Map the incoming subcommand to the vite CLI verb. `bundle` → `build`
 * (production bundle); every other known subcommand spawns the dev
 * server (no verb, defaults to `dev`).
 */
export function viteArgsForSubcommand(subcommand, forwardedArgs) {
  if (subcommand === "bundle") return ["build", ...forwardedArgs];
  return forwardedArgs;
}

/** Strip the `--profile[=value]` flag — vite doesn't understand it. */
export function stripProfileFlag(argv) {
  const out = [];
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (typeof arg !== "string") continue;
    if (arg.startsWith("--profile=")) continue;
    if (arg === "--profile") {
      i += 1;
      continue;
    }
    out.push(arg);
  }
  return out;
}

export function printHelp(write) {
  const w = write ?? ((line) => process.stdout.write(`${line}\n`));
  w("prism-studio — launcher for Prism Studio");
  w("");
  w("Usage:");
  w("  prism-studio run     [--profile=<id>] [vite args...]");
  w("  prism-studio build   [--profile=<id>] [vite args...]");
  w("  prism-studio admin   [--profile=<id>] [vite args...]");
  w("  prism-studio dev     [vite args...]");
  w("  prism-studio bundle  [vite args...]");
  w("");
  w("Subcommands:");
  w("  run     launch Vite in 'use' shell mode (published app view, user tier)");
  w("  build   launch Vite in 'build' shell mode (authoring palette, dev tier)");
  w("  admin   launch Vite in 'admin' shell mode (full IDE, dev tier)");
  w("  dev     launch Vite with no boot override (legacy default)");
  w("  bundle  run `vite build` for a production bundle");
  w("");
  w("Flags:");
  w("  --profile=<id>   restrict to one focused-app profile");
}

function resolveViteBin() {
  const require = createRequire(import.meta.url);
  // `vite/bin/vite.js` is an ES module but node runs it directly.
  return require.resolve("vite/bin/vite.js", { paths: [PACKAGE_ROOT] });
}

async function main(argv) {
  const [rawSubcommand = "help", ...rest] = argv;
  const subcommand = rawSubcommand.toLowerCase();

  if (subcommand === "help" || subcommand === "--help" || subcommand === "-h") {
    printHelp();
    return 0;
  }
  if (!KNOWN_SUBCOMMANDS.includes(subcommand)) {
    process.stderr.write(`prism-studio: unknown subcommand '${rawSubcommand}'\n`);
    printHelp((line) => process.stderr.write(`${line}\n`));
    return 2;
  }

  const bootConfig = buildBootConfigForSubcommand(subcommand, rest);
  const childEnv = buildChildEnv(process.env, bootConfig);
  const forwarded = stripProfileFlag(rest);
  const viteArgs = viteArgsForSubcommand(subcommand, forwarded);

  const viteBin = resolveViteBin();
  return await new Promise((resolvePromise) => {
    const child = spawn(process.execPath, [viteBin, ...viteArgs], {
      cwd: PACKAGE_ROOT,
      env: childEnv,
      stdio: "inherit",
    });
    child.on("exit", (code, signal) => {
      if (signal) {
        resolvePromise(128);
        return;
      }
      resolvePromise(code ?? 0);
    });
    child.on("error", (err) => {
      process.stderr.write(`prism-studio: failed to spawn vite: ${err.message}\n`);
      resolvePromise(1);
    });
  });
}

const entryPath = fileURLToPath(import.meta.url);
if (process.argv[1] === entryPath) {
  const exitCode = await main(process.argv.slice(2));
  process.exit(exitCode);
}
