import { useMemo, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./format";
import { cancelJob, isOk, useJob } from "./jobs";
import { errorMessage } from "./load";
import type { DeletedList, RecoveryStatus, TskProgress } from "./moreTypes";

const JOB = "tsk";
const SHOWN = 400;

function stamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}_${p(d.getHours())}${p(d.getMinutes())}`;
}

function join(dir: string, name: string): string {
  const sep = dir.includes("\\") ? "\\" : "/";
  return dir.endsWith(sep) ? dir + name : dir + sep + name;
}

/**
 * Onglet « Noms conservés » : The Sleuth Kit lit les fichiers supprimés dans le système de fichiers
 * d'un volume, avec leur nom et leur dossier, puis les copie ailleurs.
 */
export function TskPanel({ status }: { status: RecoveryStatus }) {
  const volumes = status.volumes;
  const [volume, setVolume] = useState<string>(volumes[0]?.path ?? "");
  const [list, setList] = useState<{ state: "idle" } | { state: "loading" } | { state: "ok"; value: DeletedList } | { state: "error"; message: string }>({ state: "idle" });
  const [filter, setFilter] = useState("");
  const [dest, setDest] = useState(() => join(status.default_destination, `${stamp()}_noms`));
  const [startError, setStartError] = useState<string | null>(null);
  const [job, setJob] = useJob<TskProgress, TskProgress>(JOB);

  const scan = () => {
    setList({ state: "loading" });
    invoke<DeletedList>("tsk_list", { volume })
      .then((value) => setList({ state: "ok", value }))
      .catch((e: unknown) => setList({ state: "error", message: errorMessage(e) }));
  };

  const files = useMemo(() => {
    if (list.state !== "ok") return [];
    const f = filter.trim().toLowerCase();
    return list.value.files.filter((x) => !x.is_dir && (!f || x.path.toLowerCase().includes(f)));
  }, [list, filter]);
  const totalBytes = files.reduce((n, f) => n + (f.size ?? 0), 0);

  const start = () => {
    setStartError(null);
    setJob({ running: true, progress: null, result: null });
    invoke("start_tsk", { volume, destination: dest }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };
  const p = job.progress ?? (isOk(job.result) ? job.result.ok : null);

  if (!status.tsk) {
    return (
      <div className="banner banner-warn" role="status">
        <strong>The Sleuth Kit absent de la clé.</strong> Lance tools/fetch-tools-windows.ps1 puis tools/assemble-usb.ps1.
      </div>
    );
  }

  return (
    <>
      <section className="panel" aria-label="Volume à analyser">
        <div className="panel-head">
          <p className="muted small help-text">
            Retrouve les fichiers supprimés <strong>avec leur nom et leur dossier</strong>, tant que le système de fichiers
            ne les a pas écrasés. Fonctionne sur NTFS, FAT, exFAT et ext ; pas sur un volume chiffré BitLocker ni après un
            formatage (utilise alors PhotoRec). Sous FAT, la première lettre du nom est remplacée par « _ ».
          </p>
        </div>
        <div className="tsk-bar">
          <label className="field inline">
            <span className="muted small">Volume</span>
            <select className="select" value={volume} onChange={(e) => setVolume(e.target.value)}>
              {volumes.map((v) => (
                <option key={v.path} value={v.path}>
                  {v.path} {v.label && `(${v.label})`} · {v.filesystem} · {formatBytes(v.total_bytes)}
                </option>
              ))}
            </select>
          </label>
          <button type="button" className="btn btn-primary" onClick={scan} disabled={!volume || list.state === "loading"}>
            {list.state === "loading" ? "Lecture du système de fichiers…" : "Lister les fichiers supprimés"}
          </button>
        </div>
        {list.state === "error" && <p className="text-bad small">{list.message}</p>}
      </section>

      {list.state === "ok" && (
        <section className="panel" aria-label="Fichiers supprimés">
          <div className="panel-head">
            <h3>
              {list.value.total} fichier(s) supprimé(s) retrouvé(s)
              {list.value.truncated && " (liste tronquée)"}
            </h3>
            <input
              type="search"
              className="select"
              placeholder="Filtrer : .jpg, Documents…"
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
              aria-label="Filtrer les fichiers"
            />
          </div>
          <p className="muted small">
            {files.length} fichier(s) affiché(s){files.length > SHOWN && ` (${SHOWN} premiers)`}, {formatBytes(totalBytes)}.
          </p>
          {files.length > 0 && (
            <div className="scroll-table">
              <table className="attr-table">
                <thead>
                  <tr>
                    <th scope="col">Chemin d'origine</th>
                    <th scope="col">Modifié</th>
                    <th scope="col" className="num">
                      Taille
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {files.slice(0, SHOWN).map((f) => (
                    <tr key={f.inode + f.path}>
                      <td className="mono">{f.path}</td>
                      <td className="muted-cell">{f.modified ?? ""}</td>
                      <td className="num">{f.size === null ? "" : formatBytes(f.size)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="tsk-bar">
            <label className="field inline grow">
              <span className="muted small">Copier vers (autre disque)</span>
              <input type="text" value={dest} onChange={(e) => setDest(e.target.value)} spellCheck={false} />
            </label>
            {!job.running && (
              <button type="button" className="btn btn-primary" onClick={start} disabled={list.value.total === 0}>
                Récupérer tous les fichiers supprimés
              </button>
            )}
          </div>
          {startError && <p className="text-bad small">{startError}</p>}
          {p && (
            <div className="test-progress" role="status">
              <div className="test-progress-head">
                <span>
                  {job.running ? "Copie en cours" : p.stopped_by_user ? "Arrêtée" : "Terminée"} · {p.files_found} fichier(s),{" "}
                  {formatBytes(p.bytes_found)}
                  {p.reported !== null && ` · ${p.reported} annoncé(s) par The Sleuth Kit`}
                </span>
                <span className="header-actions">
                  {job.running && (
                    <button type="button" className="btn" onClick={() => void cancelJob(JOB)}>
                      Arrêter
                    </button>
                  )}
                  {!job.running && (
                    <button type="button" className="btn" onClick={() => void invoke("open_folder", { path: dest })}>
                      Ouvrir le dossier
                    </button>
                  )}
                </span>
              </div>
              {job.running && (
                <div className="bar" aria-hidden="true">
                  <div className="bar-fill bar-indeterminate" />
                </div>
              )}
            </div>
          )}
        </section>
      )}
    </>
  );
}
