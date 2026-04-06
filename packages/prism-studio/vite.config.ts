import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { resolve } from "path";
import { readFileSync } from "fs";

// Build per-export aliases from @prism/core package.json exports.
// Each subpath export (e.g. "./object-model") gets its own alias
// (e.g. "@prism/core/object-model" → resolved absolute path to the .ts source).
function buildCoreAliases(): Array<{ find: string; replacement: string }> {
  const corePkg = JSON.parse(
    readFileSync(resolve(__dirname, "../prism-core/package.json"), "utf-8"),
  );
  const exports = corePkg.exports as Record<string, string>;
  return Object.entries(exports).map(([subpath, target]) => ({
    find: `@prism/core${subpath.slice(1)}`,
    replacement: resolve(__dirname, "../prism-core", target),
  }));
}

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
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
