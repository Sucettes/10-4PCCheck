import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./format";
import { Hint } from "./Hint";
import { ageHint, speedHint } from "./hints";
import { cancelJob, isOk, useJob } from "./jobs";
import { errorMessage } from "./load";
import type { DiskAge, SpeedProgress, SpeedResult } from "./moreTypes";
import { ScaleBar } from "./ScaleBar";
import type { DiskInfo } from "./types";

const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

const STEPS: Record<SpeedProgress["step"], string> = {
  read: "Lecture au début, au milieu et à la fin",
  access: "Temps d'accès",
  write: "Écriture de 1 Go dans l'espace libre",
};

/** Panneau « Vitesse » de la fiche d'un disque : test rapide et résultats sur leur échelle. */
export function SpeedTest({ disk }: { disk: DiskInfo }) {
  const id = `speed:${disk.device.name}`;
  const [job, setJob] = useJob<SpeedProgress, SpeedResult>(id);
  const [startError, setStartError] = useState<string | null>(null);

  const start = () => {
    setStartError(null);
    setJob({ running: true, progress: null, result: null });
    invoke("start_speed_test", { device: disk.device.name }).catch((e: unknown) => {
      setJob({ running: false, progress: null, result: null });
      setStartError(errorMessage(e));
    });
  };

  const p = job.progress;
  return (
    <section className="panel" aria-label="Vitesse">
      <div className="panel-head">
        <h3>
          <Hint hint={speedHint}>Vitesse</Hint>
        </h3>
        {!job.running && disk.capacity_bytes !== null && (
          <button type="button" className="btn" onClick={start}>
            {job.result ? "Relancer le test" : "Tester la vitesse"}
            <span className="btn-sub">~1 min</span>
          </button>
        )}
      </div>
      {startError && <p className="text-bad small">{startError}</p>}
      {job.running && (
        <div className="test-progress" role="status">
          <div className="test-progress-head">
            <span>{p ? STEPS[p.step] : "Démarrage…"}</span>
            <span className="mono">{p ? `${p.pct} %` : ""}</span>
          </div>
          <div className="bar" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${Math.max(p?.pct ?? 0, 2)}%` }} />
          </div>
          <button type="button" className="btn" onClick={() => void cancelJob(id)}>
            Arrêter (le fichier de test est supprimé)
          </button>
        </div>
      )}
      {!job.running && isOk(job.result) && <SpeedSummary r={job.result.ok} />}
      {!job.running && !job.result && (
        <p className="muted small">
          Environ une minute. Écrit un fichier temporaire de 1 Go dans l'espace libre, supprimé à la fin : ne pas lancer sur un disque dont
          tu veux récupérer des fichiers supprimés.
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
  return (
    <div className="speed-summary">
      {scale && <p className="muted small">Repères : {scale.class}.</p>}
      {r.read_error && <p className="text-bad small">{r.read_error}</p>}
      {start &&
        (scale ? (
          <ScaleBar label="Lecture (début du disque)" value={start.mbps} unit="Mo/s" band={scale.read} linkCap={scale.link_cap_mbps} />
        ) : (
          <p>Lecture : {nf0.format(start.mbps)} Mo/s</p>
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
      {r.write &&
        (scale ? (
          <ScaleBar label="Écriture" value={r.write.mbps} unit="Mo/s" band={scale.write} linkCap={scale.link_cap_mbps} />
        ) : (
          <p>Écriture : {nf0.format(r.write.mbps)} Mo/s</p>
        ))}
      {r.write && (
        <p className="muted small">
          {formatBytes(r.write.bytes)} écrits sur {r.write.volume}, puis supprimés.
        </p>
      )}
      {!r.write && r.write_skipped && <p className="muted small">Écriture non mesurée : {r.write_skipped}.</p>}
      {scale?.link_cap_mbps && scale.link_cap_mbps < scale.read.good && (
        <p className="muted small">
          Port SATA ancien : environ {nf0.format(scale.link_cap_mbps)} Mo/s au maximum, quel que soit le disque.
        </p>
      )}
      {r.cancelled && <p className="muted small">Test arrêté avant la fin.</p>}
    </div>
  );
}

/** Libellés de la jauge des heures (mêmes seuils que `age::hours_rating`). */
const HOURS_WORDS = { good: "Peu utilisé", acceptable: "Usé", weak: "Fin de vie probable", limited: "" };

/** Panneau « Âge et usure » : heures sur leur jauge (disque dur), âge estimé, année de l'étiquette. */
export function DiskAgePanel({ disk }: { disk: DiskInfo }) {
  const [age, setAge] = useState<DiskAge | null>(null);
  const [year, setYear] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    invoke<DiskAge>("disk_age", { device: disk.device.name })
      .then((a) => {
        setAge(a);
        setYear(a.label_year ? String(a.label_year) : "");
      })
      .catch(() => setAge(null));
  }, [disk.device.name]);

  const save = () => {
    setError(null);
    const y = year.trim() === "" ? null : Number(year);
    if (y !== null && !Number.isInteger(y)) {
      setError("Année invalide.");
      return;
    }
    invoke<DiskAge>("set_disk_year", { device: disk.device.name, year: y })
      .then(setAge)
      .catch((e: unknown) => setError(errorMessage(e)));
  };

  if (!age) return null;
  const hours = age.power_on_hours;
  return (
    <section className="panel" aria-label="Âge et usure">
      <div className="panel-head">
        <h3>
          <Hint hint={ageHint}>Âge et usure</Hint>
        </h3>
      </div>
      {hours !== null && age.hours_rating && (
        <ScaleBar
          label={`Heures d'utilisation (≈ ${nf0.format(hours / 24)} jours allumé)`}
          value={hours}
          unit="h"
          band={{ good: 20000, acceptable: 40000, higher_is_better: false }}
          words={HOURS_WORDS}
          legend="Disque dur : moins de 20 000 h peu utilisé · 20 000 à 40 000 h usé · plus de 40 000 h fin de vie probable"
        />
      )}
      {hours !== null && !age.hours_rating && (
        <p className="small muted">
          {nf0.format(hours)} h d'utilisation. Sur un SSD, l'usure se lit dans la vie restante plutôt que dans les heures.
        </p>
      )}
      <dl className="rows">
        <div className="row">
          <dt>Âge estimé</dt>
          <dd>
            {age.age_years === null
              ? "Inconnu"
              : `${age.age_is_maximum ? "au plus " : ""}≈ ${nf0.format(age.age_years)} ans` +
                (age.label_year ? ` (étiquette : ${age.label_year})` : age.model_year ? ` (modèle sorti vers ${age.model_year})` : "")}
          </dd>
        </div>
        {age.hours_per_day !== null && (
          <div className="row">
            <dt>Usage moyen</dt>
            <dd>
              {age.age_is_maximum ? "au moins " : ""}
              {nf1.format(age.hours_per_day)} h par jour
            </dd>
          </div>
        )}
      </dl>
      <div className="tsk-bar">
        <label className="field inline">
          <span className="muted small">Année sur l'étiquette du disque</span>
          <input
            type="number"
            inputMode="numeric"
            min={1980}
            max={2100}
            placeholder="ex. 2012"
            value={year}
            onChange={(e) => setYear(e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && save()}
          />
        </label>
        <button type="button" className="btn" onClick={save}>
          Enregistrer
        </button>
      </div>
      {error && <p className="text-bad small">{error}</p>}
    </section>
  );
}
