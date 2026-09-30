import { useRef, useState, type KeyboardEvent } from "react";
import { DiskDetail } from "./DiskDetail";
import { Hint } from "./Hint";
import { unreadableHint } from "./hints";
import { diskTag, smartctlErrorMessage } from "./format";
import type { Load } from "./load";
import { entryLevel } from "./status";
import type { AppInfo, DiskEntry } from "./types";

/** Écran « Disques » : onglets par disque et détail du disque choisi (maquette Disk.dc.html). */
export function DisksPage({
  info,
  disks,
  onRefresh,
}: {
  info: Load<AppInfo>;
  disks: Load<DiskEntry[]>;
  onRefresh: () => void;
}) {
  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Un seul disque</div>
          <h1>Disques</h1>
        </div>
        <button type="button" className="btn" onClick={onRefresh} disabled={disks.state === "loading"}>
          Actualiser
        </button>
      </header>
      {info.state === "ok" && info.value.smartctl.status === "unavailable" && (
        <div className="banner banner-bad" role="alert">
          <strong>smartctl indisponible.</strong> {smartctlErrorMessage(info.value.smartctl.error)}
        </div>
      )}
      <DisksView disks={disks} />
    </>
  );
}

function DisksView({ disks }: { disks: Load<DiskEntry[]> }) {
  const [selected, setSelected] = useState<string | null>(null);
  const tabs = useRef<(HTMLButtonElement | null)[]>([]);

  if (disks.state === "loading") return <p className="muted">Lecture des disques…</p>;
  if (disks.state === "error") {
    return (
      <div className="banner banner-bad" role="alert">
        <strong>Lecture impossible.</strong> {disks.message}
      </div>
    );
  }
  const entries = disks.value;
  if (entries.length === 0) {
    return <p className="muted">Aucun disque détecté. Vérifie les droits administrateur et les branchements.</p>;
  }
  // Sélection gardée après « Actualiser » si le disque est toujours là, sinon le premier.
  const current = entries.find((e) => e.device.name === selected) ?? entries[0]!;
  const index = entries.indexOf(current);

  // Motif ARIA « onglets » : flèches gauche/droite, Début et Fin déplacent la sélection.
  const onKeyDown = (e: KeyboardEvent) => {
    const next = { ArrowRight: index + 1, ArrowLeft: index - 1, Home: 0, End: entries.length - 1 }[e.key];
    if (next === undefined) return;
    e.preventDefault();
    const i = (next + entries.length) % entries.length;
    setSelected(entries[i]!.device.name);
    tabs.current[i]?.focus();
  };

  return (
    <>
      <div className="disk-tabs" role="tablist" aria-label="Disques détectés" onKeyDown={onKeyDown}>
        {entries.map((entry, i) => {
          const active = entry === current;
          const pct = entry.info?.life_remaining_pct ?? null;
          return (
            <button
              key={entry.device.name}
              ref={(el) => {
                tabs.current[i] = el;
              }}
              type="button"
              role="tab"
              id={`tab-${i}`}
              aria-selected={active}
              aria-controls="disk-panel"
              tabIndex={active ? 0 : -1}
              className={active ? "disk-tab active" : "disk-tab"}
              onClick={() => setSelected(entry.device.name)}
            >
              <span className={`dot dot-${entryLevel(entry)}`} aria-hidden="true" />
              <span className="disk-tab-name">{entry.info?.model ?? entry.device.info_name}</span>
              <span className="muted small">{diskTag(entry, entries)}</span>
              {pct !== null && <span className="disk-tab-pct">{pct} %</span>}
            </button>
          );
        })}
      </div>
      <section id="disk-panel" role="tabpanel" aria-labelledby={`tab-${index}`}>
        {current.info ? (
          <DiskDetail key={current.device.name} disk={current.info} />
        ) : (
          <div className="panel">
            <div className="card-head">
              <h2>{current.device.info_name}</h2>
              <Hint hint={unreadableHint}>
                <span className="pill pill-neutral">Illisible</span>
              </Hint>
            </div>
            <p className="muted">{current.error ? smartctlErrorMessage(current.error) : "Erreur inconnue."}</p>
          </div>
        )}
      </section>
    </>
  );
}
