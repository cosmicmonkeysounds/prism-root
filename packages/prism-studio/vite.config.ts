import { defineConfig, type Plugin } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { resolve } from "path";
import {
  readFileSync,
  existsSync,
  statSync,
  mkdirSync,
  copyFileSync,
} from "fs";

// The emscripten-built prism-daemon wasm lives outside this package, in
// `packages/prism-daemon/dist-wasm/{dev,prod}/`. The browser bootstrap
// in `src/wasm-bootstrap.ts` fetches `/daemon/prism_daemon_wasm.{js,wasm}`
// — this plugin wires those two requests to the daemon package in dev
// and copies the files into `dist/daemon/` on build so the same paths
// work in production. Profile selection mirrors Vite's command: `dev`
// serves the debug build, `build` copies the release build.
function prismDaemonWasm(): Plugin {
  const daemonRoot = resolve(__dirname, "../prism-daemon/dist-wasm");
  const files = ["prism_daemon_wasm.js", "prism_daemon_wasm.wasm"];

  function profileDir(profile: "dev" | "prod"): string {
    const dir = resolve(daemonRoot, profile);
    if (!existsSync(dir)) {
      throw new Error(
        `[prism-daemon-wasm] missing ${dir} — run packages/prism-daemon/scripts/build-wasm.sh`,
      );
    }
    return dir;
  }

  return {
    name: "prism-daemon-wasm",
    configureServer(server) {
      const dir = profileDir("dev");
      // Mount as middleware so the dev server returns the .js/.wasm with
      // correct MIME types without needing a public/ copy on disk.
      server.middlewares.use("/daemon", (req, res, next) => {
        if (!req.url) return next();
        const name = req.url.split("?")[0]!.replace(/^\//, "");
        if (!files.includes(name)) return next();
        const full = resolve(dir, name);
        if (!existsSync(full)) return next();
        const body = readFileSync(full);
        res.setHeader(
          "Content-Type",
          name.endsWith(".wasm") ? "application/wasm" : "application/javascript",
        );
        res.setHeader("Content-Length", String(statSync(full).size));
        res.end(body);
      });
    },
    writeBundle(opts) {
      // Copy the release profile into dist/daemon/ so the built SPA
      // serves the same URLs from the static bundle.
      const dir = profileDir("prod");
      const outDir = resolve(opts.dir ?? "dist", "daemon");
      mkdirSync(outDir, { recursive: true });
      for (const f of files) {
        copyFileSync(resolve(dir, f), resolve(outDir, f));
      }
    },
  };
}

// Build per-export aliases from @prism/core package.json exports.
// Each subpath export (e.g. "./object-model") gets its own alias
// (e.g. "@prism/core/object-model" → resolved absolute path to the .ts source).
function buildCoreAliases(): Array<{ find: string; replacement: string }> {
  const corePkg = JSON.parse(
    readFileSync(resolve(__dirname, "../prism-core/package.json"), "utf-8"),
  );
  const exports = corePkg.exports as Record<string, string>;
  // Longest prefix first. Vite alias matching is prefix-based, so if
  // `@prism/core` (from the "." export) is checked before
  // `@prism/core/shell`, an import of `@prism/core/shell` gets rewritten
  // to `src/index.ts/shell` — not a real path. Sorting descending by
  // `find` length makes the more specific subpath win.
  return Object.entries(exports)
    .map(([subpath, target]) => ({
      find: `@prism/core${subpath.slice(1)}`,
      replacement: resolve(__dirname, "../prism-core", target),
    }))
    .sort((a, b) => b.find.length - a.find.length);
}

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait(), prismDaemonWasm()],
  resolve: {
    alias: [
      ...buildCoreAliases(),
      { find: "@prism/shared", replacement: resolve(__dirname, "../shared/src") },
      // Use bundled elkjs (no web-worker dependency) for browser
      { find: "elkjs", replacement: "elkjs/lib/elk.bundled.js" },
    ],
  },
  // Tauri expects a fixed port for dev
  server: {
    port: 1420,
    strictPort: true,
  },
  // loro-crdt uses top-level await, needs esnext for dep optimization too
  optimizeDeps: {
    esbuildOptions: {
      target: "esnext",
    },
  },
  // Tauri uses a custom protocol for production builds
  build: {
    target: "esnext",
    outDir: "dist",
  },
});
