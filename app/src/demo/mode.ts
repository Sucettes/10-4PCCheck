/** Vrai hors de Tauri (navigateur) : l'interface tourne sur les données fictives de demo/mock.ts.
 * Module séparé, sans les données : l'application réelle ne les charge jamais. */
export const demoMode = typeof window !== "undefined" && !("__TAURI_INTERNALS__" in window);
