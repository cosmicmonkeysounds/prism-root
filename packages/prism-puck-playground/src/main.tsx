import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@measured/puck/puck.css";
import "leaflet/dist/leaflet.css";
import { PlaygroundApp } from "./playground-app.js";

const container = document.getElementById("root");
if (!container) throw new Error("#root not found");

createRoot(container).render(
  <StrictMode>
    <PlaygroundApp />
  </StrictMode>,
);
