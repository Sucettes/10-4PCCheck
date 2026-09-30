import { useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { cancelJob, isOk, useJob } from "./jobs";
import { commandErrorMessage, formatBytes } from "./format";
import { Hint } from "./Hint";
import { surfaceHint } from "./hints";
import type { CommandError, DiskInfo } from "./types";

interface Progress {
  done_bytes: number;
  total_bytes: number;
  elapsed_ms: number;
  rate_bps: number;
  bad_bytes: number;
}

interface Result {
  total_bytes: number;
  read_bytes: number;
  duration_ms: number;
  cancelled: boolean;
  bad_ranges: { offset: number; len: number }[];
  bad_bytes: number;
  slow_blocks: number;
  median_block_ms: number;
  max_block_ms: number;
  profile_mbps: (number | null)[];
}

type SurfaceError = { code: "unsupported_device" | "permission_denied" | "open"; detail?: string };

const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });

function duration(ms: number): string {
  const s = Math.round(ms / 1000);
  if (s < 60) return `${s} s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m} min ${s % 60} s`;
  return `${Math.floor(m / 60)} h ${m % 60} min`;
}

/** Débit typique pour l'estimation affichée sur le bouton, en octets par seconde. */
function typicalRate(disk: DiskInfo): number {
  if (disk.media.kind === "hdd") return 150e6;
  return disk.protocol === "nvme" ? 1500e6 : 450e6;
}

function surfaceErrorMessage(e: unknown): string {
  const err = e as SurfaceError;
  switch (err?.code) {
    case "permission_denied":
      return "Accès refusé : relance l'outil en administrateur.";
    case "unsupported_device":
      return "Ce disque ne peut pas être lu directement (contrôleur RAID ou chemin CSMI).";
    case "open":
      return `Ouverture du disque impossible : ${err.detail ?? ""}`;
    default:
      return "Échec du scan.";
  }
}

export function SurfaceScan({ disk }: { disk: DiskInfo }) {
  const id = `surface:${disk.device.name}`;
  const [job, setJob] = useJob<Progress, Result>(id);
  const [startError, setStartError] = useState<string | null>(null);
  const capacity = disk.capacity_bytes;

  const start = () => {
    if (capacity === null) return;
    setStartError(null);
    setJob({ running: true, progress: null, result: null });
    invoke("start_surface_scan", { device: disk.device, totalBytes: capacity }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(commandErrorMessage(e as CommandError));
    });
  };

  const p = job.progress;
  const pct = p && p.total_bytes > 0 ? (p.done_bytes / p.total_bytes) * 100 : 0;
  const eta = p && p.rate_bps > 0 ? ((p.total_bytes - p.done_bytes) / p.rate_bps) * 1000 : null;

  return (
    <section className="panel" aria-label="Scan de surface">
      <div className="panel-head">
        <h3>
          <Hint hint={surfaceHint}>Scan de surface (lecture seule)</Hint>
        </h3>
        {!job.running && capacity !== null && (
          <button type="button" className="btn" onClick={start}>
            {job.result ? "Relancer le scan" : "Lancer le scan"}
            <span className="btn-sub">~{duration((capacity / typicalRate(disk)) * 1000)}</span>
          </button>
        )}
      </div>

      {capacity === null && <p className="muted small">Capacité inconnue : scan impossible.</p>}
      {startError && <p className="text-bad small">{startError}</p>}

      {job.running && (
        <div className="test-progress" role="status">
          <div className="test-progress-head">
            <span>
              {p ? `${formatBytes(p.done_bytes)} sur ${formatBytes(p.total_bytes)}` : "Démarrage…"}
              {p && p.bad_bytes > 0 && <span className="text-bad"> · {formatBytes(p.bad_bytes)} illisibles</span>}
            </span>
            <span className="mono">
              {p ? `${nf0.format(pct)} % · ${nf0.format(p.rate_bps / 1e6)} Mo/s` : ""}
              {eta !== null && ` · reste ${duration(eta)}`}
            </span>
          </div>
          <div className="bar" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${Math.max(pct, 1)}%` }} />
          </div>
          <button type="button" className="btn" onClick={() => void cancelJob(id)}>
            Arrêter le scan
          </button>
        </div>
      )}

      {!job.running && job.result && !isOk(job.result) && (
        <p className="text-bad small">{surfaceErrorMessage(job.result.error)}</p>
      )}
      {!job.running && isOk(job.result) && <ScanSummary r={job.result.ok} />}
    </section>
  );
}

function ScanSummary({ r }: { r: Result }) {
  const avg = r.duration_ms > 0 ? r.read_bytes / 1e6 / (r.duration_ms / 1000) : 0;
  const lines: { level: "ok" | "warn" | "bad" | "info"; text: string }[] = [];
  if (r.cancelled) lines.push({ level: "info", text: `Scan arrêté après ${formatBytes(r.read_bytes)} sur ${formatBytes(r.total_bytes)}.` });
  lines.push(
    r.bad_bytes === 0
      ? { level: "ok", text: "Aucune zone illisible." }
      : {
          level: "bad",
          text: `${r.bad_ranges.length} zone(s) illisible(s), ${formatBytes(r.bad_bytes)} au total. Ces secteurs ne rendent plus leurs données : disque à éviter.`,
        },
  );
  lines.push(
    r.slow_blocks === 0
      ? { level: "ok", text: "Aucun bloc anormalement lent." }
      : {
          level: "warn",
          text: `${r.slow_blocks} bloc(s) anormalement lent(s) (jusqu'à ${nf0.format(r.max_block_ms)} ms contre ${nf0.format(r.median_block_ms)} ms en temps normal) : secteurs relus plusieurs fois, signe de faiblesse.`,
        },
  );
  lines.push({ level: "info", text: `${formatBytes(r.read_bytes)} lus en ${duration(r.duration_ms)}, ${nf0.format(avg)} Mo/s en moyenne.` });
  const labels = { ok: ["OK", "status-good"], warn: ["Attention", "status-warn"], bad: ["Critique", "status-bad"], info: ["Info", "status-info"] } as const;
  return (
    <>
      <ul className="checks">
        {lines.map((l) => (
          <li key={l.text}>
            <span className={`check-level ${labels[l.level][1]}`}>{labels[l.level][0]}</span>
            <span>{l.text}</span>
          </li>
        ))}
      </ul>
      <Profile values={r.profile_mbps} />
    </>
  );
}

/** Courbe du débit en fonction de la position sur le disque (tranches de 1 %). */
function Profile({ values }: { values: (number | null)[] }) {
  const known = values.filter((v): v is number => v !== null);
  if (known.length < 2) return null;
  const max = Math.max(...known) * 1.1;
  const W = 600;
  const H = 120;
  const x = (i: number) => (i / (values.length - 1)) * W;
  const y = (v: number) => H - (v / max) * H;
  const points = values.flatMap((v, i) => (v === null ? [] : [`${x(i).toFixed(1)},${y(v).toFixed(1)}`])).join(" ");
  return (
    <figure className="profile">
      <figcaption className="muted small">
        Débit selon la position sur le disque (début à gauche, fin à droite). Maximum {nf0.format(Math.max(...known))} Mo/s,
        minimum {nf0.format(Math.min(...known))} Mo/s.
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" role="img" aria-label="Profil de débit du disque">
        <line x1="0" y1={H} x2={W} y2={H} className="profile-axis" />
        <polyline points={points} className="profile-line" />
      </svg>
    </figure>
  );
}
