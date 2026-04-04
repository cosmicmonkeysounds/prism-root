import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import wasm from "vite-plugin-wasm";
import topLevelAwait from "vite-plugin-top-level-await";
import { resolve } from "path";

export default defineConfig({
  plugins: [react(), wasm(), topLevelAwait()],
  resolve: {
    alias: {
      "@prism/core": resolve(__dirname, "../prism-core/src"),
      "@prism/shared": resolve(__dirname, "../shared/src"),
    },
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
