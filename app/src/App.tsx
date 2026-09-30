import { useEffect, useState } from "react";
import { getAppInfo, reportSelfTest, scanDisks } from "./api";
import { DisksPage } from "./DisksPage";
import { FullScanPage } from "./FullScanPage";
import { HomePage } from "./HomePage";
import { RecoveryPage } from "./RecoveryPage";
import { ReportsPage } from "./ReportsPage";
import { PhonePage } from "./Phone";
import { Icon, type IconName } from "./icons";
import { errorMessage, type Load } from "./load";
import type { AppInfo, DiskEntry } from "./types";

export type Page = "accueil" | "analyse" | "disques" | "telephone" | "recuperation" | "rapports";

const NAV: { id: Page; label: string; icon: IconName }[] = [
  { id: "accueil", label: "Accueil", icon: "home" },
  { id: "analyse", label: "Analyse complète", icon: "pulse" },
  { id: "disques", label: "Disques", icon: "disk" },
  { id: "telephone", label: "Téléphone", icon: "phone" },
  { id: "recuperation", label: "Récupération", icon: "restore" },
  { id: "rapports", label: "Rapports", icon: "report" },
];

function pageFromHash(): Page {
  const id = window.location.hash.replace("#", "");
  return NAV.some((n) => n.id === id) ? (id as Page) : "accueil";
}

/** Écran courant dans l'ancre de l'URL (#disques) : pas besoin de bibliothèque de routage. */
function usePage(): [Page, (p: Page) => void] {
  const [page, setPage] = useState<Page>(pageFromHash);
  useEffect(() => {
    const onHash = () => setPage(pageFromHash());
    window.addEventListener("hashchange", onHash);
    return () => window.removeEventListener("hashchange", onHash);
  }, []);
  return [page, (p) => (window.location.hash = p)];
}

export default function App() {
  const [info, setInfo] = useState<Load<AppInfo>>({ state: "loading" });
  const [disks, setDisks] = useState<Load<DiskEntry[]>>({ state: "loading" });
  const [page, go] = usePage();

  const refresh = (): Promise<void> => {
    setDisks({ state: "loading" });
    return scanDisks()
      .then((value) => setDisks({ state: "ok", value }))
      .catch((e: unknown) => setDisks({ state: "error", message: errorMessage(e) }));
  };

  useEffect(() => {
    getAppInfo()
      .then((value) => setInfo({ state: "ok", value }))
      .catch((e: unknown) => setInfo({ state: "error", message: errorMessage(e) }));
    void refresh();
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
      <Sidebar info={info} page={page} go={go} />
      <main className="main">
        {info.state === "ok" && !info.value.elevated && (
          <div className="banner banner-warn" role="status">
            <strong>Droits limités.</strong> Relance l'outil en administrateur pour lire les données SMART et les
            disques.
          </div>
        )}
        {page === "accueil" && <HomePage disks={disks} go={go} />}
        {page === "analyse" && <FullScanPage onRefreshDisks={refresh} />}
        {page === "disques" && <DisksPage info={info} disks={disks} onRefresh={() => void refresh()} />}
        {page === "telephone" && <PhonePage />}
        {page === "recuperation" && <RecoveryPage disks={disks} />}
        {page === "rapports" && <ReportsPage />}
      </main>
    </div>
  );
}

function Sidebar({ info, page, go }: { info: Load<AppInfo>; page: Page; go: (p: Page) => void }) {
  return (
    <nav className="sidebar" aria-label="Navigation principale">
      <div className="brand">
        <div className="brand-mark">10-4</div>
        <div className="brand-name">PCCheck</div>
      </div>
      {NAV.map((n) => (
        <a
          key={n.id}
          className={page === n.id ? "nav-item active" : "nav-item"}
          href={`#${n.id}`}
          aria-current={page === n.id ? "page" : undefined}
          onClick={(e) => {
            e.preventDefault();
            go(n.id);
          }}
        >
          <Icon name={n.icon} />
          {n.label}
        </a>
      ))}
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
