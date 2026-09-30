import { useEffect, useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { diskTag, formatBytes, mediaLabel } from "./format";
import { cancelJob, isOk, useJob } from "./jobs";
import { errorMessage, type Load } from "./load";
import type { FileFamily, RecoveryProgress, RecoveryStatus } from "./moreTypes";
import type { DiskEntry, DiskInfo } from "./types";
import { TestDiskTerminal } from "./TestDiskTerminal";
import { TskPanel } from "./TskPanel";

type Mode = "photorec" | "tsk" | "testdisk";
/** Onglet gardé entre deux passages sur l'écran (TestDisk peut tourner en arrière-plan). */
let savedMode: Mode = "photorec";

const MODES: { id: Mode; label: string; hint: string }[] = [
  { id: "photorec", label: "PhotoRec (guidé)", hint: "Par signatures : marche après formatage, noms perdus" },
  { id: "tsk", label: "Noms conservés (The Sleuth Kit)", hint: "Par le système de fichiers : noms et dossiers gardés" },
  { id: "testdisk", label: "TestDisk (terminal)", hint: "À la main : partitions perdues, fichiers supprimés" },
];

const FAMILIES: { id: FileFamily; label: string }[] = [
  { id: "photos", label: "Photos" },
  { id: "documents", label: "Documents" },
  { id: "videos", label: "Vidéos" },
  { id: "audio", label: "Audio" },
  { id: "archives", label: "Archives" },
  { id: "everything", label: "Tout (480 formats)" },
];

const JOB = "recovery";

function stamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}`;
}

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

/** Écran « Récupération de fichiers » (maquette Recovery.dc.html). */
export function RecoveryPage({ disks }: { disks: Load<DiskEntry[]> }) {
  const readable = useMemo(() => (disks.state === "ok" ? disks.value.flatMap((e) => (e.info ? [e.info] : [])) : []), [disks]);
  const [source, setSource] = useState<string | null>(null);
  const [families, setFamilies] = useState<Set<FileFamily>>(new Set(["photos", "documents"]));
  const [status, setStatus] = useState<Load<RecoveryStatus>>({ state: "loading" });
  const [dest, setDest] = useState<string>("");
  const [trim, setTrim] = useState<string | null>(null);
  const [startError, setStartError] = useState<string | null>(null);
  const [job, setJob] = useJob<RecoveryProgress, RecoveryProgress>(JOB);
  const [mode, setModeState] = useState<Mode>(savedMode);
  const setMode = (m: Mode) => {
    savedMode = m;
    setModeState(m);
  };

  const disk: DiskInfo | undefined = readable.find((d) => d.device.name === source);

  useEffect(() => {
    setStatus({ state: "loading" });
    invoke<RecoveryStatus>("recovery_status", { source })
      .then((value) => {
        setStatus({ state: "ok", value });
        setDest((d) => d || join(value.default_destination, stamp()));
      })
      .catch((e: unknown) => setStatus({ state: "error", message: errorMessage(e) }));
    if (disk) {
      invoke<string | null>("recovery_trim_warning", { isSsd: disk.media.kind === "ssd", trimSupported: disk.trim_supported })
        .then(setTrim)
        .catch(() => setTrim(null));
    } else {
      setTrim(null);
    }
  }, [source, disk]);

  const toggle = (f: FileFamily) =>
    setFamilies((s) => {
      const n = new Set(s);
      if (f === "everything") return n.has(f) ? new Set() : new Set(["everything"]);
      n.delete("everything");
      if (n.has(f)) n.delete(f);
      else n.add(f);
      return n;
    });

  const st = status.state === "ok" ? status.value : null;
  const destVolume = st?.volumes
    .filter((v) => dest.toLowerCase().startsWith(v.path.toLowerCase()))
    .sort((a, b) => b.path.length - a.path.length)[0];
  const sameDisk = destVolume?.on_source ?? false;

  const start = () => {
    if (!source) return;
    setStartError(null);
    setJob({ running: true, progress: null, result: null });
    invoke("start_recovery", { disk: source, destination: dest, families: [...families], paranoid: false }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };

  const p = job.progress ?? (isOk(job.result) ? job.result.ok : null);

  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Lecture seule sur la source · écriture uniquement sur la destination</div>
          <h1>Récupération de fichiers</h1>
        </div>
      </header>

      <div className="segmented mode-tabs" role="group" aria-label="Méthode de récupération">
        {MODES.map((m) => (
          <button key={m.id} type="button" aria-pressed={mode === m.id} onClick={() => setMode(m.id)} title={m.hint}>
            {m.label}
          </button>
        ))}
      </div>
      {mode === "tsk" && st && <TskPanel status={st} />}
      {mode === "testdisk" && <TestDiskTerminal available={st?.testdisk ?? false} />}
      {mode === "photorec" && (
        <>

      {st && !st.photorec && (
        <div className="banner banner-warn" role="status">
          <strong>PhotoRec absent de la clé.</strong> {st.photorec_error} Lance tools/fetch-tools-windows.ps1 puis
          tools/assemble-usb.ps1 pour l'ajouter.
        </div>
      )}

      <div className="recovery-steps">
        <section className="panel" aria-label="Source">
          <h3>1 · Source</h3>
          {readable.length === 0 && <p className="muted small">Aucun disque lisible.</p>}
          <div className="radio-list">
            {readable.map((d) => (
              <label key={d.device.name} className={source === d.device.name ? "radio-row active" : "radio-row"}>
                <input type="radio" name="src" checked={source === d.device.name} onChange={() => setSource(d.device.name)} />
                <span>
                  {d.model ?? d.device.info_name}
                  <span className="muted small">
                    {" "}
                    · {mediaLabel(d)} · {formatBytes(d.capacity_bytes)}
                    {disks.state === "ok" &&
                      ` · ${diskTag(disks.value.find((e) => e.device.name === d.device.name)!, disks.value)}`}
                  </span>
                </span>
              </label>
            ))}
          </div>
          {trim && <p className="small text-warn">{trim}</p>}
        </section>

        <section className="panel" aria-label="Types de fichiers">
          <h3>2 · Types de fichiers</h3>
          <div className="chips">
            {FAMILIES.map((f) => (
              <button key={f.id} type="button" className="chip" aria-pressed={families.has(f.id)} onClick={() => toggle(f.id)}>
                {f.label}
              </button>
            ))}
          </div>
        </section>

        <section className="panel" aria-label="Destination">
          <h3>3 · Destination</h3>
          <label className="field">
            <span className="muted small">Dossier où écrire les fichiers retrouvés</span>
            <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} spellCheck={false} />
          </label>
          {!source && <p className="small muted">Choisis d'abord la source (étape 1).</p>}
          {source && destVolume && (
            <p className={sameDisk ? "small text-bad" : "small text-good"}>
              {sameDisk
                ? "Sur le même disque que la source : interdit, les fichiers retrouvés écraseraient ceux à récupérer."
                : `Différente de la source · ${formatBytes(destVolume.free_bytes)} libres${
                    destVolume.filesystem.toUpperCase().includes("FAT32") ? " · FAT32 : fichiers limités à 4 Go" : ""
                  }`}
            </p>
          )}
        </section>
      </div>

      <section className="panel" aria-label="Progression">
        {!job.running && (
          <div className="panel-head">
            <p className="muted small help-text">{st?.help}</p>
            <button
              type="button"
              className="btn btn-primary"
              onClick={start}
              disabled={!source || families.size === 0 || !dest || sameDisk || !st?.photorec}
            >
              Lancer la récupération
            </button>
          </div>
        )}
        {startError && <p className="text-bad small">{startError}</p>}
        {p && (
          <>
            <div className="test-progress-head">
              <span>
                {job.running ? "Analyse en cours" : p.stopped_by_user ? "Arrêtée" : "Terminée"} · {p.files_found} fichier(s),{" "}
                {formatBytes(p.bytes_found)}
              </span>
              <span className="mono">
                {Math.floor(p.elapsed_s / 60)} min {p.elapsed_s % 60} s
              </span>
            </div>
            {job.running && (
              <>
                <div className="bar" aria-hidden="true">
                  <div className="bar-fill bar-indeterminate" />
                </div>
                <button type="button" className="btn" onClick={() => void cancelJob(JOB)}>
                  Arrêter
                </button>
              </>
            )}
            {p.problem && <p className="text-bad small">{p.problem}</p>}
            <div className="ext-counts">
              {Object.entries(p.by_extension)
                .sort((a, b) => b[1] - a[1])
                .slice(0, 8)
                .map(([ext, n]) => (
                  <div key={ext}>
                    <span className="count">{n}</span>
                    {ext}
                  </div>
                ))}
            </div>
          </>
        )}
      </section>

      {p && p.last_files.length > 0 && (
        <section className="panel" aria-label="Fichiers trouvés">
          <div className="panel-head">
            <h3>Derniers fichiers trouvés</h3>
            <button type="button" className="btn" onClick={() => void invoke("open_folder", { path: dest })}>
              Ouvrir le dossier
            </button>
          </div>
          <table className="attr-table">
            <thead>
              <tr>
                <th scope="col">Fichier (nom d'origine perdu)</th>
                <th scope="col">Type</th>
                <th scope="col" className="num">
                  Taille
                </th>
              </tr>
            </thead>
            <tbody>
              {p.last_files.map((f) => (
                <tr key={f.path}>
                  <td className="mono">{f.name}</td>
                  <td>{f.extension}</td>
                  <td className="num">{formatBytes(f.size)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </section>
      )}
        </>
      )}
    </>
  );
}
