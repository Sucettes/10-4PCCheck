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
    setJob({ running: true, progress: null, result: null });
    invoke("start_capacity_test", { path: target }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };

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
      {job.running && (
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
      {!job.running && job.result && !isOk(job.result) && <p className="text-bad small">{errorMessage(job.result.error)}</p>}
      {!job.running && isOk(job.result) && <CapacitySummary r={job.result.ok} />}
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
