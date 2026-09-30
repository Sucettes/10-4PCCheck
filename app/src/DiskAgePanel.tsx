import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Hint } from "./Hint";
import { ageHint } from "./hints";
import { errorMessage } from "./load";
import type { DiskAge } from "./moreTypes";
import { ScaleBar } from "./ScaleBar";
import type { DiskInfo } from "./types";

const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

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
