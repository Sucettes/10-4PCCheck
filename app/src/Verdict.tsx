export type VerdictLevel = "ok" | "warn" | "bad" | "info" | "neutral";

const TITLES: Record<VerdictLevel, string> = {
  ok: "Bon achat",
  warn: "À négocier",
  bad: "À éviter",
  info: "À vérifier",
  neutral: "Analyse incomplète",
};

// Jauge du logo (src-tauri/icons/icon.svg) : centre (64, 84), rayon 40. Angles en convention écran
// (y vers le bas, sens horaire) : 180° à gauche, 270° en haut, 360° à droite.
const CX = 64;
const CY = 84;
const R = 40;
const NEEDLE = 32;

// Zones de la jauge, séparées par 4° de vide.
const ZONES = [
  { level: "bad", from: 180, to: 238 },
  { level: "warn", from: 242, to: 298 },
  { level: "ok", from: 302, to: 360 },
] as const;

// Angle de l'aiguille ; pas d'aiguille quand l'analyse est incomplète (rien de mesuré).
const NEEDLE_ANGLE: Record<VerdictLevel, number | null> = {
  bad: 210,
  warn: 270,
  info: 270,
  ok: 330,
  neutral: null,
};

function polar(radius: number, degrees: number): [number, number] {
  const a = (degrees * Math.PI) / 180;
  return [CX + radius * Math.cos(a), CY + radius * Math.sin(a)];
}

function arc(from: number, to: number): string {
  const [x1, y1] = polar(R, from);
  const [x2, y2] = polar(R, to);
  // Petit arc (< 180°), parcouru dans le sens horaire (sweep = 1).
  return `M${x1.toFixed(2)} ${y1.toFixed(2)}A${R} ${R} 0 0 1 ${x2.toFixed(2)} ${y2.toFixed(2)}`;
}

/** Jauge du logo, aiguille sur la zone du verdict. La zone active est pleine, les autres estompées. */
export function GaugeIcon({ level, width = 44 }: { level: VerdictLevel; width?: number }) {
  const active = level === "info" ? "warn" : level;
  const angle = NEEDLE_ANGLE[level];
  const tip = angle === null ? null : polar(NEEDLE, angle);
  return (
    // Cadré sur la jauge seule (x 12 à 116, y 32 à 96), sans le fond carré de l'icône.
    <svg className={`gauge gauge-${level}`} width={width} height={(width * 64) / 104} viewBox="12 32 104 64" aria-hidden="true">
      {ZONES.map((z) => (
        <path key={z.level} d={arc(z.from, z.to)} className={`gauge-arc gauge-${z.level}${z.level === active ? "" : " gauge-dim"}`} />
      ))}
      {tip && <line x1={CX} y1={CY} x2={tip[0]} y2={tip[1]} className="gauge-needle" />}
      <circle cx={CX} cy={CY} r="9" className="gauge-hub" />
    </svg>
  );
}

/** Bandeau de verdict en haut des écrans de résultat (plan §6) : icône, titre, résumé, compteurs. */
export function VerdictBanner({
  level,
  title,
  summary,
  counts,
}: {
  level: VerdictLevel;
  title?: string;
  summary: string;
  counts?: { ok: number; warn: number; bad: number };
}) {
  return (
    <section aria-label="Verdict" className={`verdict verdict-${level}`}>
      <div className="verdict-icon">
        <GaugeIcon level={level} />
      </div>
      <div className="verdict-text">
        <div className="verdict-title">{title ?? TITLES[level]}</div>
        <div className="verdict-summary">{summary}</div>
      </div>
      {counts && (
        <div className="verdict-counts">
          <div>
            <span className="count count-ok">{counts.ok}</span>OK
          </div>
          <div>
            <span className="count count-warn">{counts.warn}</span>Avertis.
          </div>
          <div>
            <span className="count count-bad">{counts.bad}</span>Critique
          </div>
        </div>
      )}
    </section>
  );
}
