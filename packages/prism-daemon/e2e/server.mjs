// Minimal static file server for the Playwright E2E harness.
//
// Serves dist-wasm/{dev,prod}/ depending on DAEMON_WASM_PROFILE. No
// dependencies — uses only Node's built-in http/fs/path modules so
// nothing extra needs to land in the monorepo's lockfile.
//
// Why not `serve` / `http-server` / a Vite dev server?
//   1. emscripten's .wasm needs the right MIME type
//      (`application/wasm`). `serve` gets it right today but we'd
//      rather own it than depend on it.
//   2. The browser console sometimes wants COOP/COEP headers to treat
//      the wasm memory as a SharedArrayBuffer. We set them here so both
//      the dev and prod harness behave identically whether or not the
//      particular wasm uses them.
//   3. It's 60 lines. The cost of a dependency is higher than this.

import { createServer } from "node:http";
import { readFile, stat } from "node:fs/promises";
import { dirname, extname, join, normalize, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));

const PROFILE = process.env.DAEMON_WASM_PROFILE ?? "dev";
const PORT = Number(process.env.DAEMON_WASM_PORT ?? 4321);
const ROOT = resolve(__dirname, "..", "dist-wasm", PROFILE);

const MIME = {
    ".html": "text/html; charset=utf-8",
    ".js": "application/javascript; charset=utf-8",
    ".mjs": "application/javascript; charset=utf-8",
    ".wasm": "application/wasm",
    ".json": "application/json; charset=utf-8",
    ".map": "application/json; charset=utf-8",
    ".css": "text/css; charset=utf-8",
    ".svg": "image/svg+xml",
    ".ico": "image/x-icon",
};

const server = createServer(async (req, res) => {
    try {
        // Default route → harness.html. Everything else is a file under
        // dist-wasm/<profile>/.
        let urlPath = decodeURIComponent((req.url ?? "/").split("?")[0]);
        if (urlPath === "/" || urlPath === "") {
            urlPath = "/harness.html";
        }

        // Reject path traversal attempts — we stay inside ROOT.
        const safePath = normalize(join(ROOT, urlPath));
        if (!safePath.startsWith(ROOT)) {
            res.writeHead(403, { "Content-Type": "text/plain" });
            res.end("forbidden");
            return;
        }

        const stats = await stat(safePath).catch(() => null);
        if (!stats || !stats.isFile()) {
            res.writeHead(404, { "Content-Type": "text/plain" });
            res.end(`not found: ${urlPath}`);
            return;
        }

        const body = await readFile(safePath);
        const type = MIME[extname(safePath)] ?? "application/octet-stream";

        // COOP/COEP so the harness can opt into SharedArrayBuffer +
        // crossOriginIsolated if the wasm ever wants to spin up a
        // worker. Harmless for today's single-threaded build.
        res.writeHead(200, {
            "Content-Type": type,
            "Content-Length": body.length,
            "Cache-Control": "no-store",
            "Cross-Origin-Opener-Policy": "same-origin",
            "Cross-Origin-Embedder-Policy": "require-corp",
            "Cross-Origin-Resource-Policy": "same-origin",
        });
        res.end(body);
    } catch (err) {
        res.writeHead(500, { "Content-Type": "text/plain" });
        res.end(`server error: ${err.message}`);
    }
});

server.listen(PORT, () => {
    // eslint-disable-next-line no-console
    console.log(
        `[prism-daemon e2e] serving ${ROOT} on http://localhost:${PORT} (profile=${PROFILE})`,
    );
});
