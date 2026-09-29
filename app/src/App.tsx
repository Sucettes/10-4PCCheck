import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { getAppInfo, isCommandError, reportSelfTest, scanDisks } from "./api";
import { DiskDetail } from "./DiskDetail";
import { Hint } from "./Hint";
import { unreadableHint } from "./hints";
import { commandErrorMessage, smartctlErrorMessage } from "./format";
import { entryLevel } from "./status";
import type { AppInfo, DiskEntry } from "./types";

type Load<T> = { state: "loading" } | { state: "ok"; value: T } | { state: "error"; message: string };

function errorMessage(e: unknown): string {
  if (isCommandError(e)) return commandErrorMessage(e);
  return e instanceof Error ? e.message : String(e);
}

export default function App() {
  const [info, setInfo] = useState<Load<AppInfo>>({ state: "loading" });
  const [disks, setDisks] = useState<Load<DiskEntry[]>>({ state: "loading" });

  const refresh = () => {
    setDisks({ state: "loading" });
    scanDisks()
      .then((value) => setDisks({ state: "ok", value }))
      .catch((e: unknown) => setDisks({ state: "error", message: errorMessage(e) }));
  };

  useEffect(() => {
    getAppInfo()
      .then((value) => setInfo({ state: "ok", value }))
      .catch((e: unknown) => setInfo({ state: "error", message: errorMessage(e) }));
    refresh();
  }, []);

  // Autotest : une fois tout chargé, on renvoie ce qui est affiché, puis l'app quitte.
  useEffect(() => {
    if (info.state !== "ok" || !info.value.self_test || disks.state === "loading") return;
    const frame = requestAnimationFrame(() => {
      const report = { ok: true, text: document.body.innerText, appInfo: info.value, disks };
      reportSelfTest(JSON.stringify(report, null, 2)).catch((e: unknown) => console.error(errorMessage(e)));
    });
    return () => cancelAnimationFrame(frame);
  }, [info, disks]);

  return (
    <div className="layout">
      <Sidebar info={info} />
      <main className="main">
        <header className="page-header">
          <div>
            <div className="eyebrow">Phase 1 · un seul disque</div>
            <h1>Disques</h1>
          </div>
          <button type="button" className="btn" onClick={refresh} disabled={disks.state === "loading"}>
            Actualiser
          </button>
        </header>

        {info.state === "ok" && !info.value.elevated && (
          <div className="banner banner-warn" role="status">
            <strong>Droits limités.</strong> Relance l'outil en administrateur pour lire les données SMART.
          </div>
        )}
        {info.state === "ok" && info.value.smartctl.status === "unavailable" && (
          <div className="banner banner-bad" role="alert">
            <strong>smartctl indisponible.</strong> {smartctlErrorMessage(info.value.smartctl.error)}
          </div>
        )}

        <DisksView disks={disks} />
      </main>
    </div>
  );
}

function Sidebar({ info }: { info: Load<AppInfo> }) {
  return (
    <nav className="sidebar" aria-label="Navigation principale">
      <div className="brand">
        <div className="brand-mark">10-4</div>
        <div className="brand-name">PCCheck</div>
      </div>
      <a className="nav-item active" href="#disques" aria-current="page">Disques</a>
      <div className="env">
        {info.state === "loading" && <div>Lecture de l'environnement…</div>}
        {info.state === "error" && <div className="text-bad">{info.message}</div>}
        {info.state === "ok" && (
          <>
            <div className={info.value.elevated ? "env-admin ok" : "env-admin warn"}>
              <span className="dot" aria-hidden="true" />
              {info.value.elevated ? "Mode administrateur" : "Droits limités"}
            </div>
            <div>Système : {info.value.os}</div>
            <div>
              {info.value.smartctl.status === "ready"
                ? info.value.smartctl.version.split(" ").slice(0, 2).join(" ")
                : "smartctl : indisponible"}
            </div>
            <div>10-4 PCCheck {info.value.version}</div>
          </>
        )}
      </div>
    </nav>
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
