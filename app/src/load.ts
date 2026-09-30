import { isCommandError } from "./api";
import { commandErrorMessage } from "./format";

/** Donnée chargée depuis le moteur : en cours, reçue, ou en erreur (message déjà traduit). */
export type Load<T> = { state: "loading" } | { state: "ok"; value: T } | { state: "error"; message: string };

export function errorMessage(e: unknown): string {
  if (isCommandError(e)) return commandErrorMessage(e);
  return e instanceof Error ? e.message : String(e);
}
