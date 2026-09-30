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

type State = { s: "idle" } | { s: "saving" } | { s: "done"; saved: SavedReport } | { s: "error"; message: string };

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
    return (
      <div className="report-done" role="status">
        <span className="status status-good">Rapport enregistré</span>
        <button type="button" className="btn" onClick={() => void openPath(state.saved.pdf)}>
          Ouvrir le PDF
        </button>
        <button type="button" className="btn" onClick={() => void openPath(state.saved.html)}>
          HTML
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
