import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Hint } from "./Hint";
import { speedHint } from "./hints";
import { cancelJob, isOk, useJob } from "./jobs";
import { useKept } from "./kept";
import { errorMessage } from "./load";
import type { RateSample, SpeedProgress, SpeedResult } from "./moreTypes";
import { ScaleBar } from "./ScaleBar";
import { ThroughputChart } from "./ThroughputChart";
import type { DiskInfo } from "./types";

const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

/** Tailles d'écriture proposées, en Go (même liste que `WRITE_SIZES_GIB` côté moteur). */
const SIZES = [1, 5, 10] as const;
const GIB = 1024 ** 3;

const STEPS: Record<SpeedProgress["step"], string> = {
  read: "Lecture au début, au milieu et à la fin",
  access: "Temps d'accès",
  write: "Écriture dans l'espace libre",
  readback: "Relecture et vérification du fichier",
};

/** Débit typique du type de disque, pour estimer la durée du test. */
function typicalMbps(disk: DiskInfo): number {
  if (disk.protocol === "nvme") return 1500;
  if (disk.media.kind === "ssd") return 450;
  return 100;
}

/** Courbe en direct : relevés reçus pendant le test, gardés si l'écran est quitté. */
interface Live {
  totalBytes: number;
  write: RateSample[];
  readback: RateSample[];
}

/** Panneau « Vitesse » de la fiche d'un disque : test, courbe en direct, résultats sur leur échelle. */
export function SpeedTest({ disk }: { disk: DiskInfo }) {
  const id = `speed:${disk.device.name}`;
  const [job, setJob] = useJob<SpeedProgress, SpeedResult>(id);
  const [startError, setStartError] = useState<string | null>(null);
  const [size, setSize] = useKept<number>("speed.size", () => 1);
  const [live, setLive] = useKept<Live>(`speed.live.${disk.device.name}`, () => ({ totalBytes: GIB, write: [], readback: [] }));

  // Chaque relevé reçu (une demi-seconde) prolonge la courbe de son étape.
  const p = job.progress;
  useEffect(() => {
    const sample = p?.sample;
    if (!p || !sample || !job.running || (p.step !== "write" && p.step !== "readback")) return;
    const key = p.step === "write" ? "write" : "readback";
    setLive((l) => {
      const last = l[key][l[key].length - 1];
      if (last && sample.at_bytes <= last.at_bytes) return l;
      return { ...l, [key]: [...l[key], sample] };
    });
  }, [p, job.running, setLive]);

  const start = () => {
    setStartError(null);
    setLive({ totalBytes: size * GIB, write: [], readback: [] });
    setJob({ running: true, progress: null, result: null });
    invoke("start_speed_test", { device: disk.device.name, writeGib: size }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };

  // Lecture directe, écriture puis relecture de la taille choisie au débit typique, plus l'accès.
  const minutes = Math.max(1, Math.round((15 + (3 * size * GIB) / 1e6 / typicalMbps(disk)) / 60));
  return (
    <section className="panel" aria-label="Vitesse">
      <div className="panel-head">
        <h3>
          <Hint hint={speedHint}>Vitesse</Hint>
        </h3>
        {!job.running && disk.capacity_bytes !== null && (
          <div className="header-actions">
            <select className="select" value={size} onChange={(e) => setSize(Number(e.target.value))} aria-label="Quantité à écrire">
              {SIZES.map((g) => (
                <option key={g} value={g}>
                  Écrire {g} Go
                </option>
              ))}
            </select>
            <button type="button" className="btn" onClick={start}>
              {job.result ? "Relancer le test" : "Tester la vitesse"}
              <span className="btn-sub">~{minutes} min</span>
            </button>
          </div>
        )}
      </div>
      {startError && <p className="text-bad small">{startError}</p>}
      {job.running && (
        <div className="test-progress" role="status">
          <div className="test-progress-head">
            <span>{p ? STEPS[p.step] : "Démarrage…"}</span>
            <span className="mono">
              {p ? `${p.pct} %` : ""}
              {p?.sample ? ` · ${nf0.format(p.sample.mbps)} Mo/s` : ""}
            </span>
          </div>
          <div className="bar" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${Math.max(p?.pct ?? 0, 2)}%` }} />
          </div>
          <ThroughputChart totalBytes={live.totalBytes} write={live.write} readback={live.readback} band={null} />
          <button type="button" className="btn" onClick={() => void cancelJob(id)}>
            Arrêter (le fichier de test est supprimé)
          </button>
        </div>
      )}
      {!job.running && isOk(job.result) && <SpeedSummary r={job.result.ok} />}
      {!job.running && !job.result && (
        <p className="muted small">
          Écrit un fichier temporaire dans l'espace libre, le relit en vérifiant chaque bloc, puis le supprime. Ne pas lancer sur un
          disque dont tu veux récupérer des fichiers supprimés. Avec 5 ou 10 Go, la courbe montre si un SSD ralentit une fois son
          cache rapide rempli.
        </p>
      )}
    </section>
  );
}

export function SpeedSummary({ r }: { r: SpeedResult }) {
  const scale = r.scale;
  const zones = r.read?.zones ?? [];
  const start = zones[0];
  const where = (pct: number) => (pct === 0 ? "début" : pct === 50 ? "milieu" : "fin");
  const w = r.write;
  // La relecture (données réelles) fait foi : la lecture directe devient indicative.
  const hasReadback = !!w?.readback;
  return (
    <div className="speed-summary">
      {scale && <p className="muted small">Repères : {scale.class}.</p>}
      {r.read_error && <p className="text-bad small">{r.read_error}</p>}
      {start && hasReadback && (
        <p className="small">
          Lecture directe : <strong>{nf0.format(start.mbps)} Mo/s</strong>{" "}
          <span className="muted">
            (indicative : un disque système occupé par Windows ou une zone jamais écrite la fausse ; la note se fonde sur la relecture)
          </span>
        </p>
      )}
      {start &&
        !hasReadback &&
        (scale ? (
          <ScaleBar
            label="Lecture directe (début du disque)"
            value={start.mbps}
            unit="Mo/s"
            band={scale.read}
            linkCap={scale.link_cap_mbps}
          />
        ) : (
          <p>Lecture directe : {nf0.format(start.mbps)} Mo/s</p>
        ))}
      {zones.length > 1 && (
        <p className="muted small">
          {zones.map((z) => `${where(z.position_pct)} ${nf0.format(z.mbps)} Mo/s`).join(" · ")}
          {scale?.class.startsWith("Disque dur") && " (la fin d'un disque dur est normalement environ 2 fois plus lente)"}
        </p>
      )}
      {r.read?.access &&
        (scale?.access ? (
          <ScaleBar label="Temps d'accès" value={r.read.access.avg_ms} unit="ms" band={scale.access} digits={1} />
        ) : (
          <p className="muted small">Temps d'accès : {nf1.format(r.read.access.avg_ms)} ms (quasi instantané sur un SSD).</p>
        ))}
      {w &&
        (scale ? (
          <ScaleBar label="Écriture" value={w.mbps} unit="Mo/s" band={scale.write} linkCap={scale.link_cap_mbps} />
        ) : (
          <p>Écriture : {nf0.format(w.mbps)} Mo/s</p>
        ))}
      {w?.readback &&
        (scale ? (
          <ScaleBar label="Relecture (données réelles)" value={w.readback.mbps} unit="Mo/s" band={scale.read} linkCap={scale.link_cap_mbps} />
        ) : (
          <p>Relecture : {nf0.format(w.readback.mbps)} Mo/s</p>
        ))}
      {w && <ThroughputChart totalBytes={w.bytes} write={w.samples} readback={w.readback?.samples ?? []} band={scale?.write ?? null} />}
      {w && (
        <p className="muted small">
          {nf1.format(w.bytes / GIB)} Go écrits sur {w.volume}, relus puis supprimés. Écriture de {nf0.format(w.min_mbps)} à{" "}
          {nf0.format(w.max_mbps)} Mo/s.
        </p>
      )}
      {w && w.readback_errors > 0 && (
        <p className="text-bad small">
          {w.readback_errors} bloc(s) relu(s) différent(s) de ce qui a été écrit : défaut grave (mémoire, contrôleur ou surface).
        </p>
      )}
      {w?.readback_error && <p className="text-bad small">Relecture impossible : {w.readback_error}</p>}
      {!w && r.write_skipped && <p className="muted small">Écriture non mesurée : {r.write_skipped}.</p>}
      {scale?.link_cap_mbps && scale.link_cap_mbps < scale.read.good && (
        <p className="muted small">
          Port SATA ancien : environ {nf0.format(scale.link_cap_mbps)} Mo/s au maximum, quel que soit le disque.
        </p>
      )}
      {r.cancelled && <p className="muted small">Test arrêté avant la fin.</p>}
    </div>
  );
}
