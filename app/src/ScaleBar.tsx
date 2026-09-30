import type { Band } from "./moreTypes";

export type Rating = "good" | "acceptable" | "weak" | "limited";

const WORDS: Record<Rating, string> = {
  good: "Bon",
  acceptable: "Acceptable",
  weak: "Faible",
  limited: "Bridé par le port",
};
const CLASSES: Record<Rating, string> = {
  good: "status-good",
  acceptable: "status-warn",
  weak: "status-bad",
  limited: "status-info",
};

/** Même règle que `Band::rate` et `SpeedScale::rate_throughput` (crates/core/src/speed.rs). */
export function rate(band: Band, value: number, linkCap: number | null = null): Rating {
  const good = band.higher_is_better ? value >= band.good : value <= band.good;
  const ok = band.higher_is_better ? value >= band.acceptable : value <= band.acceptable;
  const r: Rating = good ? "good" : ok ? "acceptable" : "weak";
  if (r !== "good" && linkCap !== null && band.higher_is_better && band.good > linkCap && value >= linkCap * 0.85) {
    return "limited";
  }
  return r;
}

/**
 * Barre à trois zones (faible, acceptable, bon) avec un repère à la valeur mesurée.
 * Débit : faible à gauche, bon à droite. Temps d'accès : bon à gauche, faible à droite.
 */
export function ScaleBar({
  label,
  value,
  unit,
  band,
  linkCap = null,
  digits = 0,
  words = WORDS,
  legend,
}: {
  label: string;
  value: number;
  unit: string;
  band: Band;
  linkCap?: number | null;
  digits?: number;
  /** Libellés des zones (par défaut : Bon, Acceptable, Faible). */
  words?: Record<Rating, string>;
  /** Légende sous la barre (par défaut : les deux seuils). */
  legend?: string;
}) {
  const max = band.higher_is_better ? band.good * 1.5 : band.acceptable * 1.6;
  const pct = (v: number) => Math.min(100, Math.max(0, (v / max) * 100));
  const zones = band.higher_is_better
    ? [
        { cls: "zone-weak", from: 0, to: pct(band.acceptable) },
        { cls: "zone-ok", from: pct(band.acceptable), to: pct(band.good) },
        { cls: "zone-good", from: pct(band.good), to: 100 },
      ]
    : [
        { cls: "zone-good", from: 0, to: pct(band.good) },
        { cls: "zone-ok", from: pct(band.good), to: pct(band.acceptable) },
        { cls: "zone-weak", from: pct(band.acceptable), to: 100 },
      ];
  const r = rate(band, value, linkCap);
  const fmt = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: digits, minimumFractionDigits: digits });
  const sign = band.higher_is_better ? "≥" : "≤";
  return (
    <div className="scale-row">
      <div className="scale-head">
        <span>{label}</span>
        <span>
          <strong className="mono">
            {fmt.format(value)} {unit}
          </strong>{" "}
          <span className={`status ${CLASSES[r]}`}>{words[r]}</span>
        </span>
      </div>
      <div className="scale-bar" role="img" aria-label={`${label} : ${fmt.format(value)} ${unit}, ${words[r]}`}>
        {zones.map((z) => (
          <span key={z.cls} className={`scale-zone ${z.cls}`} style={{ left: `${z.from}%`, width: `${z.to - z.from}%` }} />
        ))}
        <span className="scale-marker" style={{ left: `${pct(value)}%` }} />
      </div>
      <div className="scale-legend muted small">
        {legend ?? `Bon ${sign} ${fmt.format(band.good)} ${unit} · acceptable ${sign} ${fmt.format(band.acceptable)} ${unit}`}
      </div>
    </div>
  );
}
