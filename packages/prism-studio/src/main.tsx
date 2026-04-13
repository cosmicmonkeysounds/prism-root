import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "./tailwind.css";
import "leaflet/dist/leaflet.css";
import { App } from "./App.js";
import { bootWasmDaemon } from "./wasm-bootstrap.js";

const root = document.getElementById("root");
if (!root) throw new Error("Root element not found");

// Kick off the browser-host daemon bootstrap in parallel with React
// mount. On Tauri / Capacitor hosts this is a no-op (the function
// early-returns), so the same `main.tsx` ships to every runtime. The
// universal ipc-bridge does its own transport detection on first
// `invoke()`, which will reliably see `window.__prismDaemon` by then
// because the WASM assets are served from the same origin and load
// faster than any user-driven action.
void bootWasmDaemon();

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
