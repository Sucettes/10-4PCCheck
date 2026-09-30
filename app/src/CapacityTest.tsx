import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./format";
import { Hint } from "./Hint";
import { capacityHint } from "./hints";
import { cancelJob, isOk, useJob } from "./jobs";
import { errorMessage } from "./load";
import type { CapacityProgress, CapacityResult, RecoveryStatus, VolumeView } from "./moreTypes";
import type { DiskInfo } from "./types";

const JOB = "capacity";
/** Volume du test lancé depuis cet écran : un seul test à la fois, pour tous les disques. */
let runningTarget: string | null = null;

type CapacityError =
  | { code: "free_space"; detail: { path: string; reason: string } }
  | { code: "not_enough_space" }
  | { code: "create_dir"; detail: string };

function capacityErrorMessage(e: unknown): string {
  const err = e as Partial<CapacityError> | null;
  switch (err?.code) {
    case "free_space":
      return `Espace libre illisible sur ${(err as { detail: { path: string } }).detail.path}.`;
    case "not_enough_space":
      return "Pas assez d'espace libre pour un test (au moins 48 Mo).";
    case "create_dir":
      return `Création du dossier de test impossible : ${(err as { detail: string }).detail}. Volume protégé en écriture ?`;
    default:
      return errorMessage(e);
  }
}
const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });

function verdictText(r: CapacityResult): { level: "ok" | "bad" | "info"; text: string } {
  switch (r.verdict.kind) {
    case "genuine":
      return { level: "ok", text: `Capacité authentique : ${formatBytes(r.verified_bytes)} écrits et relus sans erreur.` };
    case "fake":
      return {
        level: "bad",
        text: `Fausse capacité : seuls ${formatBytes(r.verdict.real_bytes)} environ sont réels. Au-delà, les données écrasent le début de la mémoire.`,
      };
    case "damaged":
      if (r.corrupted_bytes + r.overwritten_bytes === 0 && r.write_error) {
        return {
          level: "bad",
          text: `Écriture refusée avant la fin de l'espace annoncé libre (${formatBytes(r.written_bytes)} écrits) : mémoire défectueuse ou fausse capacité.`,
        };
      }
      return {
        level: "bad",
        text: `Mémoire abîmée : ${formatBytes(r.corrupted_bytes + r.overwritten_bytes)} relus avec des erreurs sur ${formatBytes(r.verified_bytes)}.`,
      };
    case "incomplete":
      return { level: "info", text: "Test arrêté avant la fin : aucun verdict." };
  }
}

/** Test de capacité réelle (fausses clés USB et cartes SD) sur un volume du disque affiché. */
export function CapacityTest({ disk }: { disk: DiskInfo }) {
  const [volumes, setVolumes] = useState<VolumeView[]>([]);
  const [target, setTarget] = useState<string | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const [job, setJob] = useJob<CapacityProgress, CapacityResult>(JOB);

  useEffect(() => {
    invoke<RecoveryStatus>("recovery_status", { source: disk.device.name })
      .then((s) => {
        const mine = s.volumes.filter((v) => v.on_source);
        setVolumes(mine);
        setTarget(mine[0]?.path ?? null);
      })
      .catch(() => setVolumes([]));
  }, [disk.device.name]);

  const start = () => {
    if (!target) return;
    setStartError(null);
    runningTarget = target;
    setJob({ running: true, progress: null, result: null });
    invoke("start_capacity_test", { path: target }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };

  // Le test (identifiant unique) peut porter sur le volume d'un autre disque.
  const mine = (path: string | null) => path !== null && volumes.some((v) => v.path === path);
  const busyElsewhere = job.running && runningTarget !== null && !mine(runningTarget);
  const resultHere = !job.running && job.result && (!isOk(job.result) || mine(job.result.ok.target)) ? job.result : null;
  const p = job.progress;
  const pct = p && p.total_bytes > 0 ? (p.done_bytes / p.total_bytes) * 100 : 0;

  return (
    <section className="panel" aria-label="Capacité réelle">
      <div className="panel-head">
        <h3>
          <Hint hint={capacityHint}>Capacité réelle (fausses clés USB, cartes SD)</Hint>
        </h3>
        {!job.running && volumes.length > 0 && (
          <div className="header-actions">
            <select className="select" value={target ?? ""} onChange={(e) => setTarget(e.target.value)} aria-label="Volume à tester">
              {volumes.map((v) => (
                <option key={v.path} value={v.path}>
                  {v.path} {v.label && `(${v.label})`} · {formatBytes(v.free_bytes)} libres
                </option>
              ))}
            </select>
            <button type="button" className="btn" onClick={start}>
              Tester l'espace libre
            </button>
          </div>
        )}
      </div>
      {volumes.length === 0 && (
        <p className="muted small">
          Aucun volume monté sur ce disque : formate-le ou branche-le pour qu'il ait une lettre de lecteur, puis actualise.
        </p>
      )}
      {startError && <p className="text-bad small">{startError}</p>}
      {busyElsewhere && <p className="muted small">Un test de capacité est en cours sur {runningTarget} (autre disque).</p>}
      {job.running && !busyElsewhere && (
        <div className="test-progress" role="status">
          <div className="test-progress-head">
            <span>{p ? (p.phase === "write" ? "Écriture des blocs signés" : "Relecture et vérification") : "Démarrage…"}</span>
            <span className="mono">{p ? `${nf0.format(pct)} % · ${nf0.format(p.rate_bps / 1e6)} Mo/s` : ""}</span>
          </div>
          <div className="bar" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${Math.max(pct, 1)}%` }} />
          </div>
          <button type="button" className="btn" onClick={() => void cancelJob(JOB)}>
            Arrêter (les fichiers de test sont supprimés)
          </button>
        </div>
      )}
      {resultHere && !isOk(resultHere) && <p className="text-bad small">{capacityErrorMessage(resultHere.error)}</p>}
      {resultHere && isOk(resultHere) && <CapacitySummary r={resultHere.ok} />}
    </section>
  );
}

function CapacitySummary({ r }: { r: CapacityResult }) {
  const v = verdictText(r);
  const labels = { ok: ["OK", "status-good"], bad: ["Critique", "status-bad"], info: ["Info", "status-info"] } as const;
  return (
    <ul className="checks">
      <li>
        <span className={`check-level ${labels[v.level][1]}`}>{labels[v.level][0]}</span>
        <span>{v.text}</span>
      </li>
      <li>
        <span className="check-level status-info">Info</span>
        <span>
          Écriture {nf0.format(r.write_mbps)} Mo/s, lecture {nf0.format(r.read_mbps)} Mo/s sur {r.target}.
          {r.write_error && ` Écriture interrompue : ${r.write_error}.`}
        </span>
      </li>
    </ul>
  );
}
