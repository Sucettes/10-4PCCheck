import { isCommandError } from "./api";
import { commandErrorMessage } from "./format";

/** Donnée chargée depuis le moteur : en cours, reçue, ou en erreur (message déjà traduit). */
export type Load<T> = { state: "loading" } | { state: "ok"; value: T } | { state: "error"; message: string };

export function errorMessage(e: unknown): string {
  if (isCommandError(e)) return commandErrorMessage(e);
  if (e instanceof Error) return e.message;
  if (typeof e === "string") return e;
  // Erreur structurée non prévue ({ code, detail }) : lisible plutôt que « [object Object] ».
  try {
    return JSON.stringify(e);
  } catch {
    return String(e);
  }
}
