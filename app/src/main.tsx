import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource/ibm-plex-sans/400.css";
import "@fontsource/ibm-plex-sans/500.css";
import "@fontsource/ibm-plex-sans/600.css";
import "@fontsource/ibm-plex-mono/400.css";
import "@fontsource/space-grotesk/600.css";
import "@fontsource/space-grotesk/700.css";
import "./styles.css";
import App from "./App";
import { demoMode } from "./demo/mode";

const root = document.getElementById("root");
if (!root) throw new Error("Élément #root introuvable dans index.html");

// Hors de Tauri (navigateur), l'interface passe en mode démo : données fictives, voir demo/mock.ts.
// Import dynamique : le module et ses données ne sont jamais chargés dans l'application réelle.
const start = demoMode ? import("./demo/mock").then((m) => m.installDemo()) : Promise.resolve();

void start.then(() =>
  createRoot(root).render(
    <StrictMode>
      <App />
    </StrictMode>,
  ),
);
