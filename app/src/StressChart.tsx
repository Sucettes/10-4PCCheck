import type { StressSample } from "./moreTypes";

const W = 640;
const H = 150;
const PAD = { left: 38, right: 38, top: 10, bottom: 22 };

/**
 * Courbe du test de charge : débit de calcul (en % du maximum, axe de gauche) et température
 * (°C, axe de droite) en fonction du temps. Une baisse durable du débit pendant que la
 * température plafonne est la signature du bridage thermique.
 */
export function StressChart({ samples, seconds }: { samples: StressSample[]; seconds: number }) {
  if (samples.length < 2) return null;
  const maxRate = Math.max(...samples.map((s) => s.iterations_per_sec)) || 1;
  const temps = samples.map((s) => s.max_celsius).filter((t): t is number => t !== null);
  const tMin = temps.length ? Math.max(0, Math.floor((Math.min(...temps) - 5) / 10) * 10) : 0;
  const tMax = temps.length ? Math.ceil((Math.max(...temps) + 5) / 10) * 10 : 100;
  const duration = Math.max(seconds, samples[samples.length - 1]!.elapsed_ms / 1000);
  const x = (ms: number) => PAD.left + (ms / 1000 / duration) * (W - PAD.left - PAD.right);
  const yRate = (r: number) => PAD.top + (1 - r / maxRate) * (H - PAD.top - PAD.bottom);
  const yTemp = (t: number) => PAD.top + (1 - (t - tMin) / (tMax - tMin || 1)) * (H - PAD.top - PAD.bottom);
  const rateLine = samples.map((s) => `${x(s.elapsed_ms).toFixed(1)},${yRate(s.iterations_per_sec).toFixed(1)}`).join(" ");
  const tempLine = samples
    .filter((s) => s.max_celsius !== null)
    .map((s) => `${x(s.elapsed_ms).toFixed(1)},${yTemp(s.max_celsius!).toFixed(1)}`)
    .join(" ");
  const last = samples[samples.length - 1]!;
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => Math.round(duration * f));

  return (
    <figure className="stress-chart">
      <figcaption className="chart-legend small">
        <span className="legend-rate">Débit de calcul</span> {Math.round((last.iterations_per_sec / maxRate) * 100)} % du maximum
        {last.max_celsius !== null && (
          <>
            {" · "}
            <span className="legend-temp">Température</span> {Math.round(last.max_celsius)} °C
          </>
        )}
        {temps.length === 0 && " · température non disponible sur cette machine"}
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Débit et température pendant le test de charge">
        {[0, 0.5, 1].map((f) => (
          <g key={f}>
            <line x1={PAD.left} x2={W - PAD.right} y1={yRate(maxRate * f)} y2={yRate(maxRate * f)} className="chart-grid" />
            <text x={PAD.left - 6} y={yRate(maxRate * f) + 3} className="chart-axis" textAnchor="end">
              {Math.round(f * 100)} %
            </text>
            {temps.length > 0 && (
              <text x={W - PAD.right + 6} y={yTemp(tMin + (tMax - tMin) * f) + 3} className="chart-axis">
                {Math.round(tMin + (tMax - tMin) * f)}°
              </text>
            )}
          </g>
        ))}
        {ticks.map((t) => (
          <text key={t} x={x(t * 1000)} y={H - 6} className="chart-axis" textAnchor="middle">
            {t < 60 ? `${t} s` : `${Math.floor(t / 60)} min${t % 60 ? ` ${t % 60}` : ""}`}
          </text>
        ))}
        <polyline points={rateLine} className="chart-rate" />
        {tempLine && <polyline points={tempLine} className="chart-temp" />}
      </svg>
    </figure>
  );
}
