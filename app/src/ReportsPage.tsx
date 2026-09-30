import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { errorMessage, type Load } from "./load";
import type { Level, ReportSummary } from "./moreTypes";
import { openPath } from "./ReportButton";

const KINDS = { machine: "Ordinateur", disk: "Disque", phone: "Téléphone", recovery: "Récupération" } as const;
const VERDICTS: Record<Level, { label: string; cls: string }> = {
  ok: { label: "Bon achat", cls: "pill-good" },
  warn: { label: "À négocier", cls: "pill-warn" },
  info: { label: "À vérifier", cls: "pill-warn" },
  bad: { label: "À éviter", cls: "pill-bad" },
  neutral: { label: "Sans verdict", cls: "pill-neutral" },
};
/** Résultat d'une récupération : les libellés d'achat n'y ont pas de sens (crates/report, `label_for`). */
const RECOVERY_LABELS: Record<Level, string> = {
  ok: "Réussie",
  warn: "Partielle",
  bad: "Sans résultat",
  info: "Interrompue",
  neutral: "Interrompue",
};

const dateFmt = new Intl.DateTimeFormat("fr-CA", { dateStyle: "medium", timeStyle: "short" });

export const listReports = (): Promise<ReportSummary[]> => invoke<ReportSummary[]>("list_reports");

/** Écran « Rapports » : rapports enregistrés dans rapports/ de la clé, du plus récent au plus ancien. */
export function ReportsPage() {
  const [reports, setReports] = useState<Load<ReportSummary[]>>({ state: "loading" });
  useEffect(() => {
    listReports()
      .then((value) => setReports({ state: "ok", value }))
      .catch((e: unknown) => setReports({ state: "error", message: errorMessage(e) }));
  }, []);

  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Dossier rapports de la clé</div>
          <h1>Rapports</h1>
        </div>
        <button type="button" className="btn" onClick={() => void invoke("open_report_file", { path: null })}>
          Ouvrir le dossier
        </button>
      </header>
      {reports.state === "loading" && <p className="muted">Lecture des rapports…</p>}
      {reports.state === "error" && <p className="text-bad">{reports.message}</p>}
      {reports.state === "ok" && reports.value.length === 0 && (
        <section className="panel">
          <p className="muted">
            Aucun rapport pour l'instant. Utilise « Générer le rapport » sur l'écran d'un disque, du téléphone ou de
            l'analyse complète.
          </p>
        </section>
      )}
      {reports.state === "ok" && reports.value.length > 0 && <ReportTable reports={reports.value} />}
    </>
  );
}

export function ReportTable({ reports }: { reports: ReportSummary[] }) {
  return (
    <div className="report-list" role="table" aria-label="Rapports enregistrés">
      {reports.map((r) => {
        const base = VERDICTS[r.verdict_level];
        const v = r.subject_kind === "recovery" ? { ...base, label: RECOVERY_LABELS[r.verdict_level] } : base;
        return (
          <div className="report-row" role="row" key={r.id}>
            <span role="cell" className="report-title">
              {r.title}
              <span className="muted small"> · {KINDS[r.subject_kind]}</span>
            </span>
            <span role="cell" className="muted small">
              {dateFmt.format(new Date(r.generated_at))}
            </span>
            <span role="cell">
              <span className={`pill ${v.cls}`}>{v.label}</span>
            </span>
            <span role="cell" className="report-actions">
              {r.pdf_path && (
                <button type="button" className="link-btn" onClick={() => void openPath(r.pdf_path!)}>
                  PDF
                </button>
              )}
              {r.html_path && (
                <button type="button" className="link-btn" onClick={() => void openPath(r.html_path!)}>
                  HTML
                </button>
              )}
            </span>
          </div>
        );
      })}
    </div>
  );
}
