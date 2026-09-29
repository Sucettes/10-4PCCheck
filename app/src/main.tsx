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

const root = document.getElementById("root");
if (!root) throw new Error("Élément #root introuvable dans index.html");

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
