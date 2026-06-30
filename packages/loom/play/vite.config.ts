import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// The Loom event server (API + SSE) to proxy to during `pnpm dev`.
// Override with LOOM_SERVER, e.g. http://192.168.x.x:7000.
const target = process.env.LOOM_SERVER ?? "http://localhost:7000";

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5174,
    host: true, // expose on the LAN so phones can hit the dev server too
    proxy: {
      "/api": { target, changeOrigin: true },
      "/console": { target, changeOrigin: true },
      // SSE — keep the connection streaming (no buffering).
      "/events": { target, changeOrigin: true },
    },
  },
});
