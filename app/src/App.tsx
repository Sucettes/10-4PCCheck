import { useEffect, useState } from "react";
import { getAppInfo, isCommandError, reportSelfTest, scanDisks } from "./api";
import { commandErrorMessage, formatBytes, formatNumber, mediaLabel, smartctlErrorMessage } from "./format";
import type { AppInfo, DiskEntry, DiskInfo } from "./types";

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
            <div className="eyebrow">Phase 0 · prototype</div>
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

        <DiskList disks={disks} />
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

function DiskList({ disks }: { disks: Load<DiskEntry[]> }) {
  if (disks.state === "loading") return <p className="muted">Lecture des disques…</p>;
  if (disks.state === "error") {
    return (
      <div className="banner banner-bad" role="alert">
        <strong>Lecture impossible.</strong> {disks.message}
      </div>
    );
  }
  if (disks.value.length === 0) {
    return <p className="muted">Aucun disque détecté. Vérifie les droits administrateur et les branchements.</p>;
  }
  return (
    <section className="disk-grid" aria-label="Disques détectés">
      {disks.value.map((entry) => (
        <article className="card" key={entry.device.name}>
          {entry.info ? (
            <DiskCard disk={entry.info} />
          ) : (
            <>
              <div className="card-head">
                <h2>{entry.device.info_name}</h2>
                <span className="pill pill-neutral">Illisible</span>
              </div>
              <p className="muted">{entry.error ? smartctlErrorMessage(entry.error) : "Erreur inconnue."}</p>
            </>
          )}
        </article>
      ))}
    </section>
  );
}

function DiskCard({ disk }: { disk: DiskInfo }) {
  const verdict =
    disk.smart_passed === true
      ? { label: "Bon", cls: "pill-good" }
      : disk.smart_passed === false
        ? { label: "Critique", cls: "pill-bad" }
        : { label: "Inconnu", cls: "pill-neutral" };
  return (
    <>
      <div className="card-head">
        <h2>{disk.model ?? disk.device.info_name}</h2>
        <span className={`pill ${verdict.cls}`}>{verdict.label}</span>
      </div>
      <p className="muted">
        {disk.device.info_name} · {mediaLabel(disk)} · {formatBytes(disk.capacity_bytes)}
      </p>
      <dl className="stats">
        <div>
          <dt>Température</dt>
          <dd>{formatNumber(disk.temperature_c, " °C")}</dd>
        </div>
        <div>
          <dt>Heures</dt>
          <dd>{formatNumber(disk.power_on_hours, " h")}</dd>
        </div>
        <div>
          <dt>Démarrages</dt>
          <dd>{formatNumber(disk.power_cycles)}</dd>
        </div>
        <div>
          <dt>Firmware</dt>
          <dd className="mono">{disk.firmware ?? "Inconnu"}</dd>
        </div>
      </dl>
      {disk.warnings.length > 0 && (
        <ul className="warnings">
          {disk.warnings.map((w) => (
            <li key={w}>{w}</li>
          ))}
        </ul>
      )}
    </>
  );
}
