import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { viteSingleFile } from "vite-plugin-singlefile";
import { resolve } from "path";
import { readFileSync } from "fs";

// Re-use prism-core's package.json `exports` map so we get one alias per
// subpath (e.g. "@prism/core/object-model" → ../prism-core/src/...). Same
// trick the studio uses, copied here so the playground stands alone.
function buildCoreAliases(): Array<{ find: string; replacement: string }> {
  const corePkg = JSON.parse(
    readFileSync(resolve(__dirname, "../prism-core/package.json"), "utf-8"),
  );
  const exports = corePkg.exports as Record<string, string>;
  return Object.entries(exports)
    .map(([subpath, target]) => ({
      find: `@prism/core${subpath.slice(1)}`,
      replacement: resolve(__dirname, "../prism-core", target),
    }))
    .sort((a, b) => b.find.length - a.find.length);
}

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait(), viteSingleFile()],
  resolve: {
    alias: [
      ...buildCoreAliases(),
      {
        find: /^@prism\/studio\/(.*)$/,
        replacement: resolve(__dirname, "../prism-studio/src/$1"),
      },
      {
        find: "@prism/shared",
        replacement: resolve(__dirname, "../shared/src"),
      },
      // Bundled elkjs (no web-worker dep) — same as studio.
      { find: "elkjs", replacement: "elkjs/lib/elk.bundled.js" },
    ],
  },
  server: {
    port: 4179,
    strictPort: true,
  },
  optimizeDeps: {
    esbuildOptions: {
      target: "esnext",
    },
  },
  build: {
    target: "esnext",
    outDir: "dist",
    // viteSingleFile inlines everything into one HTML file.
    cssCodeSplit: false,
    assetsInlineLimit: 100_000_000,
  },
});
