import { test, expect, type Page } from "@playwright/test";

// ──────────────────────────────────────────────────────────────────────
// Real-browser E2E for the prism-daemon emscripten build.
//
// These tests compile the daemon to wasm32-unknown-emscripten (via
// scripts/build-wasm.sh), load the resulting .js/.wasm pair into
// Chromium, and drive the C ABI exported from src/wasm.rs through the
// JS shim in e2e/fixtures/harness.html.
//
// They exist because the src/wasm.rs unit tests only exercise the C
// ABI on the host (where mlua is running against native C Lua). That
// catches ownership bugs but proves nothing about whether loro, mlua,
// and the serde stack actually run correctly inside a browser
// WebAssembly instance. These tests close that gap: every passing test
// here is a live demonstration that Rust → emcc → WASM → Chromium
// works end-to-end for the real commands Studio consumes.
//
// Every test runs once per profile. The profile is selected by the
// DAEMON_WASM_PROFILE env var (dev|prod) which e2e/playwright.config.ts
// forwards to e2e/server.mjs. The suite therefore runs twice in CI:
// once against the fat-but-asserting dev build, once against the
// LTO'd prod build — a regression in either shows up as a test fail.
// ──────────────────────────────────────────────────────────────────────

const PROFILE = process.env["DAEMON_WASM_PROFILE"] ?? "dev";

type InvokeEnvelope =
    | { ok: true; result: unknown }
    | { ok: false; error: string };

/**
 * Navigate to the harness and wait for the emscripten module to finish
 * initializing. All subsequent `page.evaluate` calls can assume
 * `window.__prismDaemon` is defined.
 *
 * We also pipe browser console + page errors into Playwright's stdout
 * so any Rust panic / emscripten abort surfaces in the test output
 * instead of dying silently inside the iframe.
 */
async function loadHarness(page: Page): Promise<void> {
    page.on("console", (msg) => {
        // eslint-disable-next-line no-console
        console.log(`[browser:${msg.type()}] ${msg.text()}`);
    });
    page.on("pageerror", (err) => {
        // eslint-disable-next-line no-console
        console.error(`[browser:pageerror] ${err.message}`);
    });

    await page.goto("/harness.html");
    await page.waitForFunction(
        () => (window as unknown as { __prismDaemon?: unknown }).__prismDaemon,
        undefined,
        { timeout: 20_000 },
    );
}

test.describe(`prism-daemon wasm (${PROFILE})`, () => {
    test.beforeEach(async ({ page }) => {
        await loadHarness(page);
    });

    test("emscripten module loads and exposes the C ABI", async ({ page }) => {
        const shape = await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        raw: Record<string, unknown>;
                        Module: { _prism_daemon_create?: unknown };
                    };
                }
            ).__prismDaemon;
            return {
                hasRawInvoke: typeof d.raw["invoke"] === "function",
                hasRawCreate: typeof d.raw["create"] === "function",
                hasRawDestroy: typeof d.raw["destroy"] === "function",
                hasRawFree: typeof d.raw["freeString"] === "function",
                hasModuleExport:
                    typeof d.Module._prism_daemon_create === "function",
            };
        });
        expect(shape).toEqual({
            hasRawInvoke: true,
            hasRawCreate: true,
            hasRawDestroy: true,
            hasRawFree: true,
            hasModuleExport: true,
        });
    });

    test("daemon.capabilities lists crdt + lua commands", async ({ page }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "daemon.capabilities", null);
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(true);
        if (!response.ok) return;
        const commands = (response.result as { commands: string[] }).commands;
        expect(commands).toEqual(expect.arrayContaining(["crdt.write"]));
        expect(commands).toEqual(expect.arrayContaining(["crdt.read"]));
        expect(commands).toEqual(expect.arrayContaining(["crdt.export"]));
        expect(commands).toEqual(expect.arrayContaining(["crdt.import"]));
        expect(commands).toEqual(expect.arrayContaining(["lua.exec"]));
        // Commands intentionally excluded from the wasm feature must
        // not show up — their presence would mean the feature gate leaked.
        expect(commands).not.toContain("watcher.watch");
        expect(commands).not.toContain("build.run_step");
    });

    test("daemon.modules reports crdt + lua installed", async ({ page }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "daemon.modules", null);
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(true);
        if (!response.ok) return;
        const modules = (response.result as { modules: string[] }).modules;
        expect(modules).toEqual(
            expect.arrayContaining(["prism.crdt", "prism.lua"]),
        );
        expect(modules).not.toContain("prism.watcher");
        expect(modules).not.toContain("prism.build");
    });

    test("crdt.write → crdt.read round-trips through a real loro doc", async ({
        page,
    }) => {
        const result = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                const write = d.invoke(k, "crdt.write", {
                    docId: "browser-notes",
                    key: "title",
                    value: "Hello from Chrome",
                });
                const read = d.invoke(k, "crdt.read", {
                    docId: "browser-notes",
                    key: "title",
                });
                return { write, read };
            } finally {
                d.destroyKernel(k);
            }
        })) as {
            write: InvokeEnvelope;
            read: InvokeEnvelope;
        };

        expect(result.write.ok).toBe(true);
        expect(result.read.ok).toBe(true);
        if (!result.read.ok) return;

        // DocManager stores values as JSON-encoded strings, so what
        // comes back is `"\"Hello from Chrome\""` — a string literal
        // with embedded quotes. src/modules/crdt_module.rs unit tests
        // assert the same shape.
        expect((result.read.result as { value: string }).value).toBe(
            '"Hello from Chrome"',
        );
    });

    test("crdt.export snapshot rehydrates into a fresh kernel", async ({
        page,
    }) => {
        // Two distinct kernels inside the same wasm instance: one to
        // create and export a doc, another to import and read it back.
        // Proves that crdt state isn't leaking through globals.
        const result = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;

            const producer = d.createKernel();
            let snapshot: number[];
            try {
                d.invoke(producer, "crdt.write", {
                    docId: "x",
                    key: "k",
                    value: "original",
                });
                const exported = d.invoke(producer, "crdt.export", {
                    docId: "x",
                }) as { ok: true; result: { bytes: number[] } };
                snapshot = exported.result.bytes;
            } finally {
                d.destroyKernel(producer);
            }

            const consumer = d.createKernel();
            try {
                d.invoke(consumer, "crdt.import", {
                    docId: "x",
                    snapshot,
                });
                return d.invoke(consumer, "crdt.read", {
                    docId: "x",
                    key: "k",
                });
            } finally {
                d.destroyKernel(consumer);
            }
        })) as InvokeEnvelope;

        expect(result).toMatchObject({
            ok: true,
            result: { value: '"original"' },
        });
    });

    test("lua.exec runs a real Lua 5.4 VM in the browser", async ({
        page,
    }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "lua.exec", { script: "return 21 * 2" });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response).toEqual({ ok: true, result: 42 });
    });

    test("lua.exec honours args as globals", async ({ page }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "lua.exec", {
                    script: "return greeting .. ', ' .. name",
                    args: { greeting: "hello", name: "chrome" },
                });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response).toEqual({ ok: true, result: "hello, chrome" });
    });

    test("lua.exec errors surface as structured error envelopes", async ({
        page,
    }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "lua.exec", {
                    script: "error('boom')",
                });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(false);
        if (response.ok) return;
        expect(response.error.toLowerCase()).toContain("boom");
    });

    test("unknown command returns a structured not-found error", async ({
        page,
    }) => {
        const response = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                return d.invoke(k, "this.does.not.exist", {});
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(false);
        if (response.ok) return;
        expect(response.error.toLowerCase()).toContain("this.does.not.exist");
    });

    test("invalid JSON payload is reported without crashing the kernel", async ({
        page,
    }) => {
        // Drive the raw C ABI directly with intentionally-malformed JSON
        // (the high-level `invoke` helper always sends valid JSON). The
        // kernel must stay usable for subsequent calls afterwards.
        const result = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                        raw: {
                            invoke: (
                                k: number,
                                name: string,
                                payload: string,
                            ) => number;
                            freeString: (p: number) => void;
                            UTF8ToString: (p: number) => string;
                        };
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            try {
                const ptr = d.raw.invoke(k, "crdt.read", "not valid json");
                const bad = JSON.parse(d.raw.UTF8ToString(ptr));
                d.raw.freeString(ptr);

                // Next call after an error must still succeed.
                const good = d.invoke(k, "lua.exec", {
                    script: "return 1",
                });
                return { bad, good };
            } finally {
                d.destroyKernel(k);
            }
        })) as { bad: InvokeEnvelope; good: InvokeEnvelope };

        expect(result.bad.ok).toBe(false);
        if (!result.bad.ok) {
            expect(result.bad.error.toLowerCase()).toMatch(/json/);
        }
        expect(result.good).toEqual({ ok: true, result: 1 });
    });

    test("sequential invokes don't leak memory (500× lua.exec)", async ({
        page,
    }) => {
        // Smoke test for the create/invoke/free/destroy dance under
        // pressure. If `prism_daemon_free_string` is wrong, the heap
        // grows unboundedly and ALLOW_MEMORY_GROWTH eventually aborts.
        // 500 iterations is enough to surface a pathological leak on
        // this hot path without making the suite slow.
        const summary = await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const k = d.createKernel();
            let okCount = 0;
            try {
                for (let i = 0; i < 500; i++) {
                    const r = d.invoke(k, "lua.exec", {
                        script: `return ${i}`,
                    }) as { ok: true; result: number } | { ok: false };
                    if (r.ok && r.result === i) okCount++;
                }
            } finally {
                d.destroyKernel(k);
            }
            return okCount;
        });
        expect(summary).toBe(500);
    });

    test("multiple kernels in one wasm instance are isolated", async ({
        page,
    }) => {
        // Write different values to the same docId in two kernels —
        // each kernel owns its own DocManager, so the reads must not
        // see each other's writes.
        const result = (await page.evaluate(() => {
            const d = (
                window as unknown as {
                    __prismDaemon: {
                        createKernel: () => number;
                        destroyKernel: (k: number) => void;
                        invoke: (
                            k: number,
                            name: string,
                            p?: unknown,
                        ) => unknown;
                    };
                }
            ).__prismDaemon;
            const a = d.createKernel();
            const b = d.createKernel();
            try {
                d.invoke(a, "crdt.write", {
                    docId: "shared",
                    key: "owner",
                    value: "alice",
                });
                d.invoke(b, "crdt.write", {
                    docId: "shared",
                    key: "owner",
                    value: "bob",
                });
                return {
                    a: d.invoke(a, "crdt.read", {
                        docId: "shared",
                        key: "owner",
                    }),
                    b: d.invoke(b, "crdt.read", {
                        docId: "shared",
                        key: "owner",
                    }),
                };
            } finally {
                d.destroyKernel(a);
                d.destroyKernel(b);
            }
        })) as { a: InvokeEnvelope; b: InvokeEnvelope };

        if (result.a.ok) {
            expect((result.a.result as { value: string }).value).toBe(
                '"alice"',
            );
        } else {
            throw new Error(`kernel A failed: ${result.a.error}`);
        }
        if (result.b.ok) {
            expect((result.b.result as { value: string }).value).toBe(
                '"bob"',
            );
        } else {
            throw new Error(`kernel B failed: ${result.b.error}`);
        }
    });
});
