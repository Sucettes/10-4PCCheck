import { Icon } from "./icons";

export type VerdictLevel = "ok" | "warn" | "bad" | "info" | "neutral";

const TITLES: Record<VerdictLevel, string> = {
  ok: "Bon achat",
  warn: "À négocier",
  bad: "À éviter",
  info: "À vérifier",
  neutral: "Analyse incomplète",
};

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
        <Icon name={level === "ok" ? "check" : level === "bad" ? "x" : "alert"} size={24} />
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
