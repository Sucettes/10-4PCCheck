import { useEffect, useState, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { formatBytes } from "./format";
import { errorMessage, type Load } from "./load";
import type { AdbDevice, FindingLevel, PhoneAnalysis, PhoneDevices } from "./phoneTypes";
import { ReportButton } from "./ReportButton";
import { VerdictBanner } from "./Verdict";

/** Dernière analyse gardée entre deux passages sur l'écran (changement d'onglet). */
let cache: { analysis: PhoneAnalysis; checks: Record<string, boolean> } | null = null;

const LEVELS: Record<FindingLevel, { label: string; cls: string }> = {
  ok: { label: "OK", cls: "status-good" },
  info: { label: "Info", cls: "status-info" },
  warn: { label: "Attention", cls: "status-warn" },
  bad: { label: "Critique", cls: "status-bad" },
};

function stateLabel(d: AdbDevice): string {
  if (d.state === "device") return "Prêt";
  if (d.state === "unauthorized") return "Autorisation à accepter sur le téléphone";
  if (d.state === "offline") return "Ne répond pas";
  if (d.state === "no_permissions") return "Accès USB refusé";
  return d.state.other;
}

/** Écran « Téléphone Android » (maquette Phone.dc.html). */
export function PhonePage() {
  const [devices, setDevices] = useState<Load<PhoneDevices>>({ state: "loading" });
  const [analysis, setAnalysis] = useState<Load<PhoneAnalysis> | null>(
    cache ? { state: "ok", value: cache.analysis } : null,
  );
  const [checks, setChecks] = useState<Record<string, boolean>>(cache?.checks ?? {});

  const refresh = () => {
    setDevices({ state: "loading" });
    invoke<PhoneDevices>("phone_devices")
      .then((value) => setDevices({ state: "ok", value }))
      .catch((e: unknown) => setDevices({ state: "error", message: errorMessage(e) }));
  };
  useEffect(refresh, []);

  const analyse = (serial: string) => {
    setAnalysis({ state: "loading" });
    invoke<PhoneAnalysis>("phone_collect", { serial })
      .then((value) => {
        cache = { analysis: value, checks: {} };
        setChecks({});
        setAnalysis({ state: "ok", value });
      })
      .catch((e: unknown) => setAnalysis({ state: "error", message: errorMessage(e) }));
  };

  const toggle = (id: string) =>
    setChecks((c) => {
      const next = { ...c, [id]: !c[id] };
      if (cache) cache.checks = next;
      return next;
    });

  const a = analysis?.state === "ok" ? analysis.value : null;
  const id = a?.report.identity;
  const title = id ? [id.manufacturer ?? id.brand, id.model].filter(Boolean).join(" ") : "Téléphone Android";

  return (
    <>
      <header className="page-header">
        <div>
          <div className="eyebrow">Téléphone branché en USB · lecture ADB</div>
          <h1>{title}</h1>
          {id && (
            <p className="muted">
              {[id.android_version && `Android ${id.android_version}`, a?.report.storage && formatBytes(a.report.storage.total_bytes), a?.report.serial_masked]
                .filter(Boolean)
                .join(" · ")}
            </p>
          )}
        </div>
        <div className="header-actions">
          <button type="button" className="btn" onClick={refresh} disabled={devices.state === "loading"}>
            Actualiser
          </button>
          {a && (
            <ReportButton
              command="save_phone_report"
              args={{ checklist: a.checklist.map((c) => ({ label: c.label, checked: !!checks[c.id], note: null })) }}
            />
          )}
        </div>
      </header>

      <DeviceList devices={devices} onAnalyse={analyse} busy={analysis?.state === "loading"} />

      {analysis?.state === "loading" && <p className="muted">Lecture du téléphone… (quelques secondes)</p>}
      {analysis?.state === "error" && (
        <div className="banner banner-bad" role="alert">
          <strong>Analyse impossible.</strong> {analysis.message}
        </div>
      )}
      {a && <Analysis a={a} checks={checks} toggle={toggle} />}
    </>
  );
}

function DeviceList({
  devices,
  onAnalyse,
  busy,
}: {
  devices: Load<PhoneDevices>;
  onAnalyse: (serial: string) => void;
  busy: boolean;
}) {
  if (devices.state === "loading") return <p className="muted">Recherche d'un téléphone…</p>;
  if (devices.state === "error") return <p className="text-bad">{devices.message}</p>;
  const d = devices.value;
  if (d.adb_error) {
    return (
      <div className="banner banner-warn" role="status">
        <strong>ADB indisponible.</strong> {d.adb_error} Les outils Android (adb.exe) doivent être dans le dossier
        tools de la clé : voir tools/fetch-tools-windows.ps1.
      </div>
    );
  }
  if (d.devices.length === 0) {
    return (
      <section className="panel">
        <h3>Aucun téléphone détecté</h3>
        <p className="guidance">{d.guidance}</p>
      </section>
    );
  }
  return (
    <section className="panel" aria-label="Téléphones branchés">
      <h3>Téléphones branchés</h3>
      <ul className="device-list">
        {d.devices.map((dev) => (
          <li key={dev.serial}>
            <div>
              <div className="device-name">{dev.model?.replace(/_/g, " ") ?? dev.product ?? "Appareil Android"}</div>
              <div className="muted small">
                {dev.serial_masked} · {stateLabel(dev)}
              </div>
              {dev.guidance && <p className="guidance">{dev.guidance}</p>}
            </div>
            {dev.state === "device" && (
              <button type="button" className="btn btn-primary" onClick={() => onAnalyse(dev.serial)} disabled={busy}>
                Analyser
              </button>
            )}
          </li>
        ))}
      </ul>
    </section>
  );
}

function Analysis({ a, checks, toggle }: { a: PhoneAnalysis; checks: Record<string, boolean>; toggle: (id: string) => void }) {
  const r = a.report;
  const problems = a.findings.filter((f) => f.level === "bad" || f.level === "warn");
  const counts = {
    ok: a.findings.filter((f) => f.level === "ok").length,
    warn: a.findings.filter((f) => f.level === "warn").length,
    bad: a.findings.filter((f) => f.level === "bad").length,
  };
  const summary =
    problems.length === 0
      ? "Aucun problème détecté par l'analyse. Termine les vérifications manuelles ci-dessous."
      : problems.map((f) => f.label).join(" · ");
  const google = r.accounts?.filter((x) => x.activation_lock).reduce((n, x) => n + x.count, 0) ?? null;
  const bool = (v: boolean | null, yes: string, no: string) => (v === null ? "Inconnu" : v ? yes : no);
  const boot = r.security.verified_boot;
  const bootLabel = boot === null ? "Inconnu" : typeof boot === "string" ? boot : boot.other;
  const b = r.battery;

  return (
    <>
      <VerdictBanner level={a.verdict} summary={summary} counts={counts} />

      <section className="panel" aria-label="Constats">
        <h3>Constats</h3>
        <ul className="checks">
          {a.findings.map((f) => (
            <li key={f.label + f.detail}>
              <span className={`check-level ${LEVELS[f.level].cls}`}>{LEVELS[f.level].label}</span>
              <span>
                <strong>{f.label}.</strong> {f.detail}
              </span>
            </li>
          ))}
        </ul>
      </section>

      <div className="card-grid">
        <Card title="Batterie">
          <Row label="Santé Android" value={b?.health_label ?? "Inconnue"} />
          <Row label="Niveau" value={b?.level_pct != null ? `${b.level_pct} %` : "Inconnu"} />
          <Row label="Cycles" value={b?.cycle_count != null ? String(b.cycle_count) : "Non exposé"} />
          <Row label="Température" value={b?.temperature_c != null ? `${b.temperature_c.toFixed(1)} °C` : "Inconnue"} />
          <Row label="Capacité réelle" value={b?.capacity_pct != null ? `${b.capacity_pct} % de l'origine` : "Non exposée"} />
        </Card>
        <Card title="Système">
          <Row label="Patch de sécurité" value={r.security.security_patch_raw ?? "Inconnu"} />
          <Row label="Chargeur de démarrage" value={bool(r.security.bootloader_locked, "Verrouillé", "Déverrouillé")} />
          <Row label="Démarrage vérifié" value={bootLabel} />
          <Row label="Root détecté" value={bool(r.security.root.su_found, "Oui", "Non")} />
          {r.security.knox_warranty_void !== null && (
            <Row label="Knox (Samsung)" value={r.security.knox_warranty_void ? "Déclenché" : "Intact"} />
          )}
        </Card>
        <Card title="Verrous et comptes">
          <Row label="Comptes à retirer" value={google === null ? "Inconnu" : google === 0 ? "Aucun" : `${google} connecté(s)`} />
          {r.accounts
            ?.filter((x) => x.count > 0)
            .map((x) => <Row key={x.account_type} label={x.label ?? x.account_type} value={String(x.count)} />)}
          <Row
            label="Gestion d'entreprise"
            value={r.owners === null ? "Inconnue" : r.owners.device_owner ? "Présente" : r.owners.profile_owners.length > 0 ? "Profil géré" : "Aucune"}
          />
          <Row label="IMEI" value="Composer *#06#" />
        </Card>
        <Card title="Stockage">
          {r.storage ? (
            <>
              <div className="bar" aria-hidden="true">
                <div className="bar-fill" style={{ width: `${(r.storage.used_bytes / r.storage.total_bytes) * 100}%` }} />
              </div>
              <Row label="Utilisé" value={`${formatBytes(r.storage.used_bytes)} sur ${formatBytes(r.storage.total_bytes)}`} />
            </>
          ) : (
            <Row label="Utilisé" value="Inconnu" />
          )}
          <Row label="Usure de la puce" value="Root requis" />
        </Card>
      </div>

      <section className="panel" aria-label="Vérifications manuelles">
        <h3>À tester à la main</h3>
        <p className="muted small">Coche au fur et à mesure : la liste sera enregistrée dans le rapport.</p>
        <ul className="manual-checks">
          {a.checklist.map((c) => (
            <li key={c.id}>
              <label>
                <input type="checkbox" checked={!!checks[c.id]} onChange={() => toggle(c.id)} />
                <span>
                  <span className="manual-label">{c.label}</span>
                  <span className="muted small">{c.help}</span>
                </span>
              </label>
            </li>
          ))}
        </ul>
        <p className="muted small">
          {r.imei_note} {r.flash_wear_note}
        </p>
      </section>

      {r.issues.length > 0 && (
        <p className="muted small">Lectures incomplètes : {r.issues.map((i) => i.message).join(" ")}</p>
      )}
    </>
  );
}

function Card({ title, children }: { title: string; children: ReactNode }) {
  return (
    <section className="panel info-card" aria-label={title}>
      <h3>{title}</h3>
      <dl className="rows">{children}</dl>
    </section>
  );
}

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="row">
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

