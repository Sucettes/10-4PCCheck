import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// Tauri sert le dossier dist/ en production et le serveur Vite en développement.
export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 5173, strictPort: true },
  build: { target: "es2021", outDir: "dist", emptyOutDir: true },
});
