import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { Page } from "./App";
import { diskTag, formatBytes, mediaLabel } from "./format";
import { Icon, type IconName } from "./icons";
import type { Load } from "./load";
import { machineName, type MachineInventory, type ReportSummary } from "./moreTypes";
import { listReports, ReportTable } from "./ReportsPage";
import type { DiskEntry } from "./types";

/** Écran « Accueil » (maquette Main.dc.html) : machine détectée, modes d'analyse, rapports récents. */
export function HomePage({ disks, go }: { disks: Load<DiskEntry[]>; go: (p: Page) => void }) {
  const [inv, setInv] = useState<MachineInventory | null>(null);
  const [reports, setReports] = useState<ReportSummary[]>([]);

  useEffect(() => {
    invoke<MachineInventory>("machine_inventory", { refresh: false })
      .then(setInv)
      .catch(() => {});
    listReports()
      .then((r) => setReports(r.slice(0, 3)))
      .catch(() => {});
  }, []);

  const name = inv ? machineName(inv) : null;
  const diskList = disks.state === "ok" ? disks.value : [];
  const summary = [
    inv?.cpu?.name,
    inv?.memory?.total_bytes ? formatBytes(inv.memory.total_bytes) : null,
    `${diskList.length} disque(s)`,
    inv?.battery ? "Batterie détectée" : inv ? "Sans batterie" : null,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Machine détectée</div>
          <h1 className="home-title">{name ?? (inv ? "Ordinateur" : "Lecture du matériel…")}</h1>
          <p className="muted">{summary}</p>
        </div>
        <div className="muted small">Que veux-tu analyser ?</div>
      </header>

      <section aria-label="Modes d'analyse" className="modes">
        <ModeCard
          icon="pulse"
          title="Analyse complète"
          time="~7 min + tests interactifs"
          text="Matériel, disques, batterie, charge du processeur, mémoire, licence et sécurité, puis tests interactifs clavier, écran, audio et ports."
          action="Lancer l'analyse"
          onClick={() => go("analyse")}
          primary
        />
        <ModeCard icon="disk" title="Un seul disque" time="1 à 60 min" action="Choisir un disque" onClick={() => go("disques")}>
          <div className="mode-disks">
            {diskList.slice(0, 4).map((e) => (
              <div key={e.device.name}>
                <span>
                  {e.info?.model ?? e.device.info_name}
                  <span className="muted"> · {diskTag(e, diskList)}</span>
                </span>
                <span className="muted">{e.info ? mediaLabel(e.info) : "Illisible"}</span>
              </div>
            ))}
            {disks.state === "loading" && <div className="muted">Lecture des disques…</div>}
          </div>
        </ModeCard>
        <ModeCard
          icon="phone"
          title="Téléphone Android"
          time="~1 min"
          text="Batterie, stockage, patch de sécurité, comptes encore connectés, gestion d'entreprise. Branche le téléphone et autorise le débogage USB."
          action="Analyser un téléphone"
          onClick={() => go("telephone")}
        />
        <ModeCard
          icon="restore"
          title="Récupération de fichiers"
          time="PhotoRec"
          text="Retrouve photos, documents et vidéos supprimés. Efficace sur disque dur, clé USB et carte SD. Faible sur SSD avec TRIM."
          action="Récupérer des fichiers"
          onClick={() => go("recuperation")}
        />
      </section>

      {reports.length > 0 && (
        <section aria-label="Rapports récents" className="recent">
          <div className="panel-head">
            <h2 className="section-title">Rapports récents</h2>
            <button type="button" className="link-btn" onClick={() => go("rapports")}>
              Tous les rapports
            </button>
          </div>
          <ReportTable reports={reports} />
        </section>
      )}
    </>
  );
}

function ModeCard({
  icon,
  title,
  time,
  text,
  action,
  onClick,
  primary = false,
  children,
}: {
  icon: IconName;
  title: string;
  time: string;
  text?: string;
  action: string;
  onClick: () => void;
  primary?: boolean;
  children?: ReactNode;
}) {
  return (
    <button type="button" className={primary ? "mode-card mode-primary" : "mode-card"} onClick={onClick}>
      <div className="mode-top">
        <div className="mode-icon">
          <Icon name={icon} size={22} />
        </div>
        <span className="muted small">{time}</span>
      </div>
      <div className="mode-title">{title}</div>
      {text && <div className="mode-text">{text}</div>}
      {children}
      <div className="mode-action">{action} →</div>
    </button>
  );
}
