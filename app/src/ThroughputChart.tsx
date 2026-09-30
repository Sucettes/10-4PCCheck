import type { Band, RateSample } from "./moreTypes";

const W = 640;
const H = 160;
const PAD = { left: 44, right: 12, top: 10, bottom: 22 };
const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

/**
 * Débit (Mo/s) selon la position dans le fichier de test (Go) : écriture, puis relecture.
 * Les lignes pointillées sont les repères « bon » et « acceptable » du type de disque. Sur un SSD,
 * une chute nette de l'écriture en cours de route marque la fin de son cache rapide.
 */
export function ThroughputChart({
  totalBytes,
  write,
  readback,
  band,
}: {
  totalBytes: number;
  write: RateSample[];
  readback: RateSample[];
  band: Band | null;
}) {
  if (write.length + readback.length < 2) return null;
  const all = [...write, ...readback].map((s) => s.mbps);
  const top = Math.max(...all, band ? band.good * 1.1 : 0) * 1.1 || 1;
  const x = (bytes: number) => PAD.left + (bytes / (totalBytes || 1)) * (W - PAD.left - PAD.right);
  const y = (mbps: number) => PAD.top + (1 - mbps / top) * (H - PAD.top - PAD.bottom);
  // Chaque relevé couvre l'intervalle qui le précède : tracé en marches, depuis le début du fichier.
  const steps = (samples: RateSample[]) => {
    let prev = 0;
    return samples
      .map((s) => {
        const seg = `${x(prev).toFixed(1)},${y(s.mbps).toFixed(1)} ${x(s.at_bytes).toFixed(1)},${y(s.mbps).toFixed(1)}`;
        prev = s.at_bytes;
        return seg;
      })
      .join(" ");
  };
  // Même unité que le choix de taille (1, 5, 10 Go = 1, 5, 10 × 1024³ octets).
  const gb = totalBytes / 1024 ** 3;
  const ticks = [0, 0.25, 0.5, 0.75, 1];
  return (
    <figure className="stress-chart">
      <figcaption className="chart-legend small">
        {write.length > 0 && <span className="legend-rate">Écriture</span>}
        {readback.length > 0 && (
          <>
            {" · "}
            <span className="legend-read">Relecture</span>
          </>
        )}
        <span className="muted"> · débit en Mo/s selon la position dans le fichier</span>
        {band && <span className="muted"> · pointillés : repères bon et acceptable</span>}
      </figcaption>
      <svg viewBox={`0 0 ${W} ${H}`} role="img" aria-label="Débit d'écriture et de relecture selon la position dans le fichier de test">
        {[0, 0.5, 1].map((f) => (
          <g key={f}>
            <line x1={PAD.left} x2={W - PAD.right} y1={y(top * f)} y2={y(top * f)} className="chart-grid" />
            <text x={PAD.left - 6} y={y(top * f) + 3} className="chart-axis" textAnchor="end">
              {nf0.format(top * f)}
            </text>
          </g>
        ))}
        {band &&
          [band.good, band.acceptable].map((v) => (
            <line key={v} x1={PAD.left} x2={W - PAD.right} y1={y(v)} y2={y(v)} className="chart-band" />
          ))}
        {ticks.map((f) => (
          <text
            key={f}
            x={x(totalBytes * f)}
            y={H - 6}
            className="chart-axis"
            textAnchor={f === 0 ? "start" : f === 1 ? "end" : "middle"}
          >
            {nf1.format(gb * f)} Go
          </text>
        ))}
        {write.length > 0 && <polyline points={steps(write)} className="chart-rate" />}
        {readback.length > 0 && <polyline points={steps(readback)} className="chart-read" />}
      </svg>
    </figure>
  );
}
