import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage } from "./load";

/** Chemins des fichiers d'un rapport enregistré (crates/report : `SavedReport`). */
export interface SavedReport {
  json: string;
  html: string;
  pdf: string;
}

export const openPath = (path: string): Promise<void> => invoke<void>("open_report_file", { path });

type State = { s: "idle" } | { s: "saving" } | { s: "done"; saved: SavedReport; openError?: string } | { s: "error"; message: string };

/**
 * Génère et enregistre le rapport de l'écran courant (JSON + HTML + PDF dans rapports/ de la clé),
 * puis propose de l'ouvrir. `command` : commande Tauri qui construit le rapport côté moteur.
 */
export function ReportButton({ command, args = {} }: { command: string; args?: Record<string, unknown> }) {
  const [state, setState] = useState<State>({ s: "idle" });

  const save = () => {
    setState({ s: "saving" });
    invoke<SavedReport>(command, args)
      .then((saved) => setState({ s: "done", saved }))
      .catch((e: unknown) => setState({ s: "error", message: errorMessage(e) }));
  };

  if (state.s === "done") {
    const open = (path: string) =>
      void openPath(path).catch((e: unknown) => setState({ ...state, openError: errorMessage(e) }));
    return (
      <div className="report-done" role="status">
        {state.openError && <span className="text-bad small">{state.openError}</span>}
        <span className="status status-good">Rapport enregistré</span>
        <button type="button" className="btn" onClick={() => open(state.saved.pdf)}>
          Ouvrir le PDF
        </button>
        <button type="button" className="btn" onClick={() => open(state.saved.html)}>
          HTML
        </button>
        {/* Nouveaux résultats depuis (scan de surface, tests) : rapport à jour, dans un nouveau fichier. */}
        <button type="button" className="link-btn" onClick={save} title="Nouveau rapport avec les derniers résultats">
          Régénérer
        </button>
      </div>
    );
  }
  return (
    <div className="report-done">
      {state.s === "error" && <span className="text-bad small">{state.message}</span>}
      <button type="button" className="btn btn-primary" onClick={save} disabled={state.s === "saving"}>
        {state.s === "saving" ? "Génération…" : "Générer le rapport"}
      </button>
    </div>
  );
}
