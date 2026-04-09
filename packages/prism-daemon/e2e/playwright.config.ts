import { defineConfig } from "@playwright/test";

// Playwright config for the prism-daemon WASM E2E suite.
//
// The suite drives the real emscripten build (compiled by
// scripts/build-wasm.sh) inside Chromium via a tiny static server
// (e2e/server.mjs). Profile is selected with DAEMON_WASM_PROFILE:
//
//   DAEMON_WASM_PROFILE=dev  pnpm exec playwright test   # debug build
//   DAEMON_WASM_PROFILE=prod pnpm exec playwright test   # release build
//
// Both profiles run the *same* spec file against their own dist-wasm
// subdirectory, so any divergence between debug and release builds
// (LTO drops, assertion-only codepaths, size regressions) surfaces as
// a test failure rather than a silent prod bug.

const PROFILE = process.env["DAEMON_WASM_PROFILE"] ?? "dev";
const PORT = Number(process.env["DAEMON_WASM_PORT"] ?? 4321);

export default defineConfig({
    testDir: ".",
    testMatch: "*.spec.ts",
    timeout: 30_000,
    expect: { timeout: 10_000 },
    retries: 0,
    reporter: [["list"]],
    use: {
        baseURL: `http://localhost:${PORT}`,
        headless: true,
        // The harness page logs emscripten stdout/stderr into the DOM,
        // but we also mirror it to the Playwright console for CI.
        trace: "retain-on-failure",
    },
    webServer: {
        // Start our minimal file server with the right profile picked up
        // from the env. reuseExistingServer is on for local iteration so
        // you can re-run `pnpm test:e2e:dev` without bouncing the port.
        //
        // The command is resolved relative to the config file's directory
        // (this file's parent, `e2e/`), so we reference `./server.mjs`
        // directly — no `e2e/` prefix. Historically we had `e2e/server.mjs`
        // here, which made playwright look for `e2e/e2e/server.mjs`.
        command: `DAEMON_WASM_PROFILE=${PROFILE} DAEMON_WASM_PORT=${PORT} node ./server.mjs`,
        url: `http://localhost:${PORT}/harness.html`,
        reuseExistingServer: !process.env["CI"],
        timeout: 10_000,
        stdout: "pipe",
        stderr: "pipe",
    },
});
