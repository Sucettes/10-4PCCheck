import { useCallback, useEffect, useState } from "react";
import { abortSmartTest, isCommandError, smartTestStatus, startSmartTest } from "./api";
import { commandErrorMessage, formatNumber } from "./format";
import { Hint } from "./Hint";
import { selfTestHint } from "./hints";
import type { DiskInfo, SelfTestKind, SelfTestResult, SelfTestStatus } from "./types";

/** Intervalle de suivi pendant un test : l'état ATA n'avance que par pas de 10 %. */
const POLL_MS = 5000;
/** Délai avant la première lecture après le lancement : le disque met un instant à démarrer. */
const START_DELAY_MS = 1500;

type State =
  | { state: "loading" }
  | { state: "ok"; value: SelfTestStatus }
  | { state: "error"; message: string };

function message(e: unknown): string {
  if (isCommandError(e)) return commandErrorMessage(e);
  return e instanceof Error ? e.message : String(e);
}

const KIND_LABELS: Record<string, string> = {
  "Short offline": "Court",
  Short: "Court",
  "Short self-test": "Court",
  "Extended offline": "Long",
  Extended: "Long",
  "Extended self-test": "Long",
  "Conveyance offline": "Transport",
};

function resultLabel(r: SelfTestResult): { label: string; cls: string } {
  if (r.passed === true) return { label: "Réussi", cls: "status-good" };
  if (r.passed === false) return { label: "Échec", cls: "status-bad" };
  return { label: "Interrompu", cls: "status-neutral" };
}

/** Panneau « Tests » de la maquette : auto-tests SMART court et long, suivi, historique. */
export function SelfTests({ disk }: { disk: DiskInfo }) {
  const [status, setStatus] = useState<State>({ state: "loading" });
  const [busy, setBusy] = useState(false);
  const [actionError, setActionError] = useState<string | null>(null);
  const device = disk.device;

  const refresh = useCallback(() => {
    smartTestStatus(device)
      .then((value) => setStatus({ state: "ok", value }))
      .catch((e: unknown) => setStatus({ state: "error", message: message(e) }));
  }, [device]);

  useEffect(refresh, [refresh]);

  // Suivi tant qu'un test tourne. Le composant est recréé à chaque changement de disque
  // (clé sur DiskDetail) : le minuteur de l'ancien disque est annulé au démontage.
  useEffect(() => {
    if (status.state !== "ok" || !status.value.running) return;
    const id = setTimeout(refresh, POLL_MS);
    return () => clearTimeout(id);
  }, [status, refresh]);

  const act = (action: () => Promise<void>, delay: number) => {
    setBusy(true);
    setActionError(null);
    action()
      .then(() => new Promise((r) => setTimeout(r, delay)))
      .then(refresh)
      .catch((e: unknown) => setActionError(message(e)))
      .finally(() => setBusy(false));
  };
  const start = (kind: SelfTestKind) => act(() => startSmartTest(device, kind), START_DELAY_MS);
  const abort = () => act(() => abortSmartTest(device), 500);

  const s = status.state === "ok" ? status.value : null;
  const running = s?.running ?? false;
  const unsupported = s?.supported === false;
  const duration = (minutes: number | null | undefined, fallback: string) =>
    minutes ? `~${formatNumber(minutes)} min` : fallback;

  return (
    <section className="panel tests-panel" aria-label="Tests">
      <h3>
        <Hint hint={selfTestHint}>Auto-tests SMART</Hint>
      </h3>

      {status.state === "loading" && <p className="muted">Lecture de l'état des tests…</p>}
      {status.state === "error" && <p className="text-bad small">{status.message}</p>}
      {unsupported && <p className="muted small">Ce disque ne prend pas en charge les auto-tests.</p>}

      {running && s && (
        <div className="test-progress" role="status">
          <div className="test-progress-head">
            <span>Test en cours</span>
            <span className="mono">
              {s.remaining_pct === null ? "…" : `${100 - s.remaining_pct} % fait`}
            </span>
          </div>
          <div className="bar" aria-hidden="true">
            <div className="bar-fill" style={{ width: `${s.remaining_pct === null ? 5 : 100 - s.remaining_pct}%` }} />
          </div>
          <button type="button" className="btn btn-block" onClick={abort} disabled={busy}>
            Interrompre le test
          </button>
        </div>
      )}

      {!running && !unsupported && status.state !== "loading" && (
        <div className="test-buttons">
          <button type="button" className="test-btn" onClick={() => start("short")} disabled={busy}>
            <span>Auto-test court</span>
            <span className="test-btn-sub">{duration(s?.short_minutes, "~2 min")}</span>
          </button>
          <button type="button" className="test-btn" onClick={() => start("long")} disabled={busy}>
            <span>Auto-test long</span>
            <span className="test-btn-sub">{duration(s?.long_minutes, "10 min à plusieurs heures")}</span>
          </button>
        </div>
      )}
      {actionError && <p className="text-bad small">{actionError}</p>}

      {s && s.history.length > 0 && (
        <div className="test-history">
          <h4>Derniers résultats</h4>
          <ul>
            {s.history.map((r, i) => {
              const res = resultLabel(r);
              return (
                <li key={`${i}-${r.power_on_hours}`}>
                  <span className={`status ${res.cls}`}>{res.label}</span>
                  <span>
                    {KIND_LABELS[r.kind] ?? r.kind}
                    {r.power_on_hours !== null && <span className="muted"> · à {formatNumber(r.power_on_hours)} h</span>}
                    {r.passed === false && <span className="test-detail">{r.text}</span>}
                  </span>
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </section>
  );
}
