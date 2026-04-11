import { test, expect } from "@playwright/test";

// Verifies that the Studio Vite dev server serves the daemon WASM
// assets and that `wasm-bootstrap.ts` successfully boots them into a
// usable `window.__prismDaemon` bridge. This is the browser-host leg
// of the universal Studio pipeline — desktop uses Tauri IPC, mobile
// uses the Capacitor plugin, and the browser uses exactly this path.
//
// The test navigates to a stub HTML page that only imports the
// bootstrap (not the full Studio app), because prism-core has
// pre-existing build errors that shouldn't gate daemon verification.
test.describe("WASM daemon bootstrap", () => {
  test("serves the daemon asset under /daemon/", async ({ request }) => {
    const js = await request.get("/daemon/prism_daemon_wasm.js");
    expect(js.status()).toBe(200);
    expect(js.headers()["content-type"]).toContain("javascript");
    const wasm = await request.get("/daemon/prism_daemon_wasm.wasm");
    expect(wasm.status()).toBe(200);
    expect(wasm.headers()["content-type"]).toBe("application/wasm");
  });

  test("bootWasmDaemon exposes window.__prismDaemon and round-trips invoke", async ({
    page,
  }) => {
    // Navigate to the root so the dev server is warm, then execute the
    // bootstrap module inline. We re-import it by absolute path — Vite
    // handles the module graph automatically. The full Studio app
    // pulls in unrelated pre-existing TS errors, so we bypass it and
    // just exercise the wasm-bootstrap module directly.
    await page.goto("/");
    await page.waitForLoadState("domcontentloaded");

    const result = await page.evaluate(async () => {
      const mod = (await import(
        "/src/wasm-bootstrap.ts"
      )) as typeof import("../src/wasm-bootstrap.js");
      await mod.bootWasmDaemon();
      const bridge = (
        window as unknown as {
          __prismDaemon?: {
            createKernel: () => number;
            destroyKernel: (k: number) => void;
            invoke: (
              k: number,
              name: string,
              payload: unknown,
            ) => { ok: boolean; result?: unknown; error?: string };
          };
        }
      ).__prismDaemon;
      if (!bridge) return { stage: "no-bridge" as const };
      const k = bridge.createKernel();
      try {
        const caps = bridge.invoke(k, "daemon.capabilities", null);
        const luau = bridge.invoke(k, "luau.exec", { script: "return 7*6" });
        return { stage: "ok" as const, caps, luau };
      } finally {
        bridge.destroyKernel(k);
      }
    });

    expect(result.stage).toBe("ok");
    if (result.stage !== "ok") throw new Error("unreachable");
    expect(result.caps.ok).toBe(true);
    expect(result.luau.ok).toBe(true);
  });
});
