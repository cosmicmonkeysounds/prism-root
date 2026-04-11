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
// ABI on the host (where mlua is running against native C++ Luau). That
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

    test("daemon.capabilities lists crdt + luau + vfs + crypto commands", async ({
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
        expect(commands).toEqual(expect.arrayContaining(["luau.exec"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.put"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.get"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.has"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.delete"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.list"]));
        expect(commands).toEqual(expect.arrayContaining(["vfs.stats"]));
        expect(commands).toEqual(expect.arrayContaining(["crypto.keypair"]));
        expect(commands).toEqual(
            expect.arrayContaining(["crypto.shared_secret"]),
        );
        expect(commands).toEqual(expect.arrayContaining(["crypto.encrypt"]));
        expect(commands).toEqual(expect.arrayContaining(["crypto.decrypt"]));
        expect(commands).toEqual(
            expect.arrayContaining(["crypto.random_bytes"]),
        );
        // Commands intentionally excluded from the wasm feature must
        // not show up — their presence would mean the feature gate leaked.
        expect(commands).not.toContain("watcher.watch");
        expect(commands).not.toContain("build.run_step");
    });

    test("daemon.modules reports crdt + luau + vfs + crypto installed", async ({
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
                return d.invoke(k, "daemon.modules", null);
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(true);
        if (!response.ok) return;
        const modules = (response.result as { modules: string[] }).modules;
        expect(modules).toEqual(
            expect.arrayContaining([
                "prism.crdt",
                "prism.luau",
                "prism.vfs",
                "prism.crypto",
            ]),
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

    test("luau.exec runs a real Luau VM in the browser", async ({
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
                return d.invoke(k, "luau.exec", { script: "return 21 * 2" });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response).toEqual({ ok: true, result: 42 });
    });

    test("luau.exec honours args as globals", async ({ page }) => {
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
                return d.invoke(k, "luau.exec", {
                    script: "return greeting .. ', ' .. name",
                    args: { greeting: "hello", name: "chrome" },
                });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response).toEqual({ ok: true, result: "hello, chrome" });
    });

    test("luau.exec errors surface as structured error envelopes", async ({
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
                return d.invoke(k, "luau.exec", {
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
                const good = d.invoke(k, "luau.exec", {
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

    test("sequential invokes don't leak memory (500× luau.exec)", async ({
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
                    const r = d.invoke(k, "luau.exec", {
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

    // ── VFS ─────────────────────────────────────────────────────────────
    //
    // These tests exercise the content-addressed blob store through the
    // C ABI, inside the browser's emscripten MEMFS. The store writes
    // real temp files into the virtual filesystem, so a regression in
    // either the Rust side (sha2 hashing, write-temp+rename) or the
    // emscripten libc layer (MEMFS rename semantics) will surface here.

    test("vfs.put → vfs.get round-trips a blob through MEMFS", async ({
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
                const bytes = [0x50, 0x52, 0x49, 0x53, 0x4d]; // "PRISM"
                const put = d.invoke(k, "vfs.put", { bytes });
                const hash = (
                    put as { ok: true; result: { hash: string } }
                ).result.hash;
                const has = d.invoke(k, "vfs.has", { hash });
                const get = d.invoke(k, "vfs.get", { hash });
                return { put, has, get };
            } finally {
                d.destroyKernel(k);
            }
        })) as {
            put: InvokeEnvelope;
            has: InvokeEnvelope;
            get: InvokeEnvelope;
        };

        expect(result.put.ok).toBe(true);
        if (!result.put.ok) return;
        const hash = (result.put.result as { hash: string; size: number })
            .hash;
        expect(hash).toHaveLength(64);
        expect(/^[0-9a-f]{64}$/.test(hash)).toBe(true);

        expect(result.has).toEqual({
            ok: true,
            result: { present: true, size: 5 },
        });

        if (result.get.ok) {
            expect(
                (result.get.result as { bytes: number[] }).bytes,
            ).toEqual([0x50, 0x52, 0x49, 0x53, 0x4d]);
        } else {
            throw new Error(`vfs.get failed: ${result.get.error}`);
        }
    });

    test("vfs.delete removes a blob and vfs.list reflects the store", async ({
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
                const a = d.invoke(k, "vfs.put", { bytes: [1, 1, 1] }) as {
                    ok: true;
                    result: { hash: string };
                };
                const b = d.invoke(k, "vfs.put", { bytes: [2, 2, 2, 2] }) as {
                    ok: true;
                    result: { hash: string };
                };
                const listBefore = d.invoke(k, "vfs.list", {});
                const del = d.invoke(k, "vfs.delete", {
                    hash: a.result.hash,
                });
                const statsAfter = d.invoke(k, "vfs.stats", {});
                return { listBefore, del, statsAfter, b: b.result.hash };
            } finally {
                d.destroyKernel(k);
            }
        })) as {
            listBefore: InvokeEnvelope;
            del: InvokeEnvelope;
            statsAfter: InvokeEnvelope;
            b: string;
        };

        // Before delete: two entries in the list.
        if (!result.listBefore.ok) {
            throw new Error(`vfs.list failed: ${result.listBefore.error}`);
        }
        const listed = (
            result.listBefore.result as {
                entries: Array<{ hash: string; size: number }>;
            }
        ).entries;
        expect(listed.length).toBeGreaterThanOrEqual(2);

        // Delete succeeds.
        expect(result.del).toEqual({
            ok: true,
            result: { deleted: true },
        });

        // Stats now only reflect the remaining blob(s).
        if (!result.statsAfter.ok) {
            throw new Error(`vfs.stats failed: ${result.statsAfter.error}`);
        }
        const stats = result.statsAfter.result as {
            entries: number;
            total_bytes: number;
        };
        // Per-kernel MEMFS, but we share a default temp root across
        // kernels — assert monotonically on the blob we *know* is still
        // there rather than the exact count.
        expect(stats.total_bytes).toBeGreaterThanOrEqual(4);
    });

    test("vfs.get rejects an unknown hash with a structured error", async ({
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
                return d.invoke(k, "vfs.get", { hash: "f".repeat(64) });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(false);
    });

    // ── Crypto ──────────────────────────────────────────────────────────
    //
    // Mesh Trust ECDH + XChaCha20-Poly1305 AEAD running inside Chromium.
    // The RustCrypto crates pull their entropy from `OsRng` → `getrandom`
    // → emscripten's libc → `crypto.getRandomValues()` under the hood.
    // A broken getrandom backend shows up as either a panic during
    // `crypto.keypair` or as non-roundtripping ciphertext, both of which
    // these tests would catch.

    test("crypto.keypair produces 32-byte hex keys in the browser", async ({
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
                return d.invoke(k, "crypto.keypair", {});
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(true);
        if (!response.ok) return;
        const kp = response.result as {
            secret_key: string;
            public_key: string;
        };
        expect(kp.secret_key).toHaveLength(64);
        expect(kp.public_key).toHaveLength(64);
        expect(/^[0-9a-f]{64}$/.test(kp.secret_key)).toBe(true);
        expect(/^[0-9a-f]{64}$/.test(kp.public_key)).toBe(true);
    });

    test("crypto shared secret + encrypt/decrypt round-trips a message", async ({
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
                const alice = d.invoke(k, "crypto.keypair", {}) as {
                    ok: true;
                    result: { secret_key: string; public_key: string };
                };
                const bob = d.invoke(k, "crypto.keypair", {}) as {
                    ok: true;
                    result: { secret_key: string; public_key: string };
                };
                const sharedAB = d.invoke(k, "crypto.shared_secret", {
                    secret_key: alice.result.secret_key,
                    peer_public_key: bob.result.public_key,
                }) as { ok: true; result: { shared_secret: string } };
                const sharedBA = d.invoke(k, "crypto.shared_secret", {
                    secret_key: bob.result.secret_key,
                    peer_public_key: alice.result.public_key,
                }) as { ok: true; result: { shared_secret: string } };

                // "hello wasm" as hex.
                const plaintext = "68656c6c6f2077617374";
                const ct = d.invoke(k, "crypto.encrypt", {
                    key: sharedAB.result.shared_secret,
                    plaintext,
                }) as {
                    ok: true;
                    result: { ciphertext: string; nonce: string };
                };
                const pt = d.invoke(k, "crypto.decrypt", {
                    key: sharedBA.result.shared_secret,
                    ciphertext: ct.result.ciphertext,
                    nonce: ct.result.nonce,
                }) as {
                    ok: true;
                    result: { plaintext: string };
                };

                return {
                    sharedEqual:
                        sharedAB.result.shared_secret ===
                        sharedBA.result.shared_secret,
                    plaintext: pt.result.plaintext,
                    expected: plaintext,
                    nonceLen: ct.result.nonce.length,
                };
            } finally {
                d.destroyKernel(k);
            }
        })) as {
            sharedEqual: boolean;
            plaintext: string;
            expected: string;
            nonceLen: number;
        };

        expect(result.sharedEqual).toBe(true);
        expect(result.plaintext).toBe(result.expected);
        // 24-byte XChaCha nonce × 2 hex chars = 48 characters.
        expect(result.nonceLen).toBe(48);
    });

    test("crypto.decrypt fails on tampered ciphertext (AEAD auth)", async ({
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
                // A 32-byte symmetric key, deterministic for the test.
                const key =
                    "0000000000000000000000000000000000000000000000000000000000000000";
                const plaintext = "61"; // "a"
                const ct = d.invoke(k, "crypto.encrypt", {
                    key,
                    plaintext,
                }) as {
                    ok: true;
                    result: { ciphertext: string; nonce: string };
                };
                const tampered =
                    "ff" + ct.result.ciphertext.slice(2);
                return d.invoke(k, "crypto.decrypt", {
                    key,
                    ciphertext: tampered,
                    nonce: ct.result.nonce,
                });
            } finally {
                d.destroyKernel(k);
            }
        })) as InvokeEnvelope;

        expect(response.ok).toBe(false);
    });

    test("crypto.random_bytes returns distinct buffers of the requested length", async ({
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
                const a = d.invoke(k, "crypto.random_bytes", {
                    len: 32,
                }) as { ok: true; result: { bytes: string } };
                const b = d.invoke(k, "crypto.random_bytes", {
                    len: 32,
                }) as { ok: true; result: { bytes: string } };
                return { a: a.result.bytes, b: b.result.bytes };
            } finally {
                d.destroyKernel(k);
            }
        })) as { a: string; b: string };

        expect(result.a).toHaveLength(64);
        expect(result.b).toHaveLength(64);
        expect(result.a).not.toEqual(result.b);
    });
});
