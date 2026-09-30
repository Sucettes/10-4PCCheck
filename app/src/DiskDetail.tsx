import { useEffect, useState, type CSSProperties, type ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Hint } from "./Hint";
import {
  attributeColumnHints,
  bytesWrittenHint,
  checksHint,
  firmwareHint,
  interfaceHint,
  nvmeHints,
  powerOnHoursHint,
  serialHint,
  smartStatusHint,
  standardHint,
  temperatureHint,
  trimHint,
  type HintText,
  transferHint,
  featuresHint,
  usbHint,
} from "./hints";
import { formatBytes, formatHex, formatNumber, mediaLabel } from "./format";
import { SelfTests } from "./SelfTests";
import { SurfaceScan } from "./SurfaceScan";
import { CapacityTest } from "./CapacityTest";
import { DiskAgePanel } from "./DiskAgePanel";
import { SpeedTest } from "./SpeedTest";
import { ReportButton } from "./ReportButton";
import { attributeLevel, lifeLevel, nvmeStatus, smartVerdict } from "./status";
import type { AtaAttribute, AtaFeature, AttributeStatus, Check, DiskInfo, NvmeHealth } from "./types";
import type { PcieLink, UsbLink, UsbSpeed } from "./moreTypes";

const nf0 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 0 });

/** Écran « Un disque » (maquette : Disk.dc.html) : résumé, fiche technique, attributs SMART. */
export function DiskDetail({ disk }: { disk: DiskInfo }) {
  return (
    <div className="detail">
      <header className="detail-header">
        <div>
          <h2 className="detail-title">{disk.model ?? disk.device.info_name}</h2>
          <p className="muted">
            {formatBytes(disk.capacity_bytes)} · {mediaLabel(disk)} · {disk.device.info_name}
          </p>
        </div>
        <ReportButton command="save_disk_report" args={{ device: disk.device.name }} />
      </header>

      <section className="summary" aria-label="Résumé">
        <HealthCard disk={disk} />
        <div className="summary-stats">
          <Stat
            label="Température"
            hint={temperatureHint(disk)}
            value={formatNumber(disk.temperature_c, " °C")}
            sub="Repère au repos : moins de 50 °C"
          />
          <Stat
            label="Données écrites"
            hint={bytesWrittenHint(disk)}
            value={disk.bytes_written === null ? "Non rapportées" : formatBytes(disk.bytes_written)}
            sub={disk.bytes_read === null ? "Lectures : non rapportées" : `Lues : ${formatBytes(disk.bytes_read)}`}
          />
          <Stat
            label="Heures d'utilisation"
            hint={powerOnHoursHint(disk)}
            value={formatNumber(disk.power_on_hours, " h")}
            sub={
              disk.power_on_hours === null
                ? ""
                : `≈ ${nf0.format(disk.power_on_hours / 24)} jours · ${formatNumber(disk.power_cycles)} démarrages`
            }
          />
        </div>
      </section>

      <div className="detail-row">
        {disk.checks.length > 0 && <Checks checks={disk.checks} />}
        <SelfTests disk={disk} />
      </div>

      <div className="detail-row">
        <DiskAgePanel disk={disk} />
        <SpeedTest disk={disk} />
      </div>

      <SurfaceScan disk={disk} />
      <CapacityTest disk={disk} />

      <TechSheet disk={disk} />

      {disk.warnings.length > 0 && (
        <section className="panel panel-warn" aria-label="Messages de smartctl">
          <h3>Messages de smartctl</h3>
          <ul className="warnings">
            {disk.warnings.map((w) => (
              <li key={w}>{w}</li>
            ))}
          </ul>
        </section>
      )}

      {disk.ata_attributes.length > 0 && <AtaTable attributes={disk.ata_attributes} />}
      {disk.nvme_health && <NvmeTable health={disk.nvme_health} />}
    </div>
  );
}

function HealthCard({ disk }: { disk: DiskInfo }) {
  const verdict = smartVerdict(disk);
  const pct = disk.life_remaining_pct;
  const source =
    pct === null
      ? disk.media.kind === "hdd"
        ? "Disque dur : pas d'indicateur d'usure. Regarde les secteurs réalloués et en attente."
        : "Ce disque n'expose pas d'indicateur d'usure."
      : disk.protocol === "nvme"
        ? "Selon l'usure déclarée par le disque (norme NVMe)"
        : "Selon l'attribut d'usure du fabricant";
  return (
    <div className="health-card">
      {pct !== null ? (
        <div className={`ring ring-${lifeLevel(pct)}`} style={{ "--pct": `${pct}%` } as CSSProperties}>
          <div className="ring-inner">
            <span className="ring-value">{pct} %</span>
            <span className="ring-label">vie restante</span>
          </div>
        </div>
      ) : (
        <div className="ring ring-none">
          <div className="ring-inner">
            <span className="ring-label">Usure non mesurée</span>
          </div>
        </div>
      )}
      <Hint hint={smartStatusHint(disk)}>
        <span className={`pill pill-${verdict.level}`}>SMART : {verdict.label}</span>
      </Hint>
      <p className="health-source">{source}</p>
    </div>
  );
}

function Stat({ label, hint, value, sub }: { label: string; hint: HintText; value: string; sub: string }) {
  return (
    <div className="stat-card">
      <Hint hint={hint}>
        <span className="stat-label">{label}</span>
      </Hint>
      <span className="stat-value">{value}</span>
      {sub && <span className="stat-sub">{sub}</span>}
    </div>
  );
}

const checkLabels = {
  ok: { label: "OK", cls: "status-good" },
  info: { label: "Info", cls: "status-info" },
  warn: { label: "Attention", cls: "status-warn" },
} as const;

function Checks({ checks }: { checks: Check[] }) {
  return (
    <section className="panel" aria-label="Vérifications de cohérence">
      <h3>
        <Hint hint={checksHint}>Vérifications de cohérence</Hint>
      </h3>
      <ul className="checks">
        {checks.map((c) => (
          <li key={c.text}>
            <span className={`check-level ${checkLabels[c.level].cls}`}>{checkLabels[c.level].label}</span>
            <span>{c.text}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

const USB_LABELS: Record<UsbSpeed, string> = {
  usb1: "USB 1.1 (12 Mb/s)",
  usb2: "USB 2.0 (480 Mb/s)",
  gen1: "USB 3.2 Gen 1 (5 Gb/s)",
  gen2: "USB 3.2 Gen 2 (10 Gb/s)",
  gen2x2: "USB 3.2 Gen 2x2 (20 Gb/s)",
};
const USB_ORDER: UsbSpeed[] = ["usb1", "usb2", "gen1", "gen2", "gen2x2"];

/** Liaison USB en une ligne : vitesse négociée, mode, et ce qui la limite s'il y a mieux possible. */
function usbText(l: UsbLink): string {
  const parts = [USB_LABELS[l.speed]];
  if (l.uas !== null) parts.push(l.uas ? "mode UAS" : "ancien mode (pas UAS)");
  const rank = (s: UsbSpeed | null) => (s === null ? -1 : USB_ORDER.indexOf(s));
  const now = rank(l.speed);
  if (rank(l.device_capable) > now && rank(l.port_capable) > now) parts.push("limité par le câble");
  else if (rank(l.device_capable) > now) parts.push(`port plus lent que l'adaptateur (${USB_LABELS[l.device_capable!]})`);
  else if (rank(l.port_capable) > now) parts.push(`adaptateur plus lent que le port (${USB_LABELS[l.port_capable!]})`);
  return parts.join(" · ");
}

const PCIE_VERSIONS: Record<number, string> = { 1: "1.0", 2: "2.0", 3: "3.0", 4: "4.0", 5: "5.0", 6: "6.0" };
const pcieLabel = (gen: number, lanes: number) => `PCIe ${PCIE_VERSIONS[gen] ?? "?"} x${lanes}`;

/** « 6.0 Gb/s » → 6 ; pour comparer vitesse actuelle et maximale. */
const gbps = (s: string | null) => (s ? Number.parseFloat(s) : Number.NaN);

/** Mode de transfert : lien actuel, puis ce que le disque sait faire s'il est plus rapide. */
function transferText(disk: DiskInfo, pcie: PcieLink | null): string | null {
  if (pcie) {
    const now = pcieLabel(pcie.current_gen, pcie.current_lanes);
    const max = pcieLabel(pcie.max_gen ?? pcie.current_gen, pcie.max_lanes ?? pcie.current_lanes);
    return now === max ? now : `${now} · le disque sait faire ${max} (emplacement plus lent, ou lien en économie d'énergie)`;
  }
  if (!disk.link_speed) return null;
  if (!disk.link_speed_max || disk.link_speed_max === disk.link_speed) return disk.link_speed;
  return gbps(disk.link_speed) < gbps(disk.link_speed_max)
    ? `${disk.link_speed} · le disque sait faire ${disk.link_speed_max} (port, câble ou adaptateur plus lent)`
    : `${disk.link_speed} (maximum ${disk.link_speed_max})`;
}

/** Fonctionnalités prises en charge, avec leur état quand la norme en a un. */
function FeatureList({ features }: { features: AtaFeature[] }) {
  const supported = features.filter((f) => f.supported);
  if (supported.length === 0) return <span className="muted">Aucune déclarée</span>;
  return (
    <span className="feature-list">
      {supported.map((f) => (
        <span key={f.key} className={f.enabled === false ? "feature feature-off" : "feature"}>
          {f.label}
          {f.enabled === false && " (désactivé)"}
        </span>
      ))}
    </span>
  );
}

function TechSheet({ disk }: { disk: DiskInfo }) {
  const [usb, setUsb] = useState<UsbLink | null>(null);
  const [pcie, setPcie] = useState<PcieLink | null>(null);
  useEffect(() => {
    invoke<UsbLink | null>("disk_usb_link", { device: disk.device.name })
      .then(setUsb)
      .catch(() => setUsb(null));
    invoke<PcieLink | null>("disk_pcie_link", { device: disk.device.name })
      .then(setPcie)
      .catch(() => setPcie(null));
  }, [disk.device.name]);
  const iface =
    disk.protocol === "nvme"
      ? "PCIe NVMe"
      : [disk.sata_version, disk.link_speed && `lien à ${disk.link_speed}`].filter(Boolean).join(" · ") || null;
  // Lignes sans donnée masquées (format et TRIM ne sont pas rapportés pour un NVMe, par exemple).
  const rows: { label: string; hint?: HintText; value: ReactNode | null }[] = [
    { label: "Firmware", hint: firmwareHint, value: <span className="mono">{disk.firmware ?? "Inconnu"}</span> },
    {
      label: "Numéro de série",
      hint: serialHint,
      value: disk.serial ? (
        <span className="mono">{disk.serial}</span>
      ) : (
        "Inconnu"
      ),
    },
    { label: "Interface", hint: interfaceHint(disk), value: iface ?? "Inconnue" },
    { label: "Mode de transfert", hint: transferHint, value: transferText(disk, pcie) },
    { label: "Liaison USB", hint: usbHint, value: usb ? usbText(usb) : null },
    { label: "Fonctionnalités", hint: featuresHint, value: disk.features ? <FeatureList features={disk.features} /> : null },
    { label: "Norme", hint: standardHint, value: disk.standard ?? "Inconnue" },
    { label: "Format", value: disk.form_factor },
    {
      label: "TRIM",
      hint: trimHint,
      value: disk.trim_supported === null ? null : disk.trim_supported ? "Pris en charge" : "Non pris en charge",
    },
  ];
  return (
    <section className="panel" aria-label="Fiche technique">
      <h3>Fiche technique</h3>
      <dl className="tech">
        {rows
          .filter((r) => r.value !== null)
          .map((r) => (
          <div key={r.label}>
            <dt>{r.hint ? <Hint hint={r.hint}>{r.label}</Hint> : r.label}</dt>
            <dd>{r.value}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

function StatusCell({ status }: { status: AttributeStatus }) {
  const s = attributeLevel[status];
  return (
    <span className={`status status-${s.level}`}>
      <span className="dot" aria-hidden="true" />
      {s.label}
    </span>
  );
}

function ColumnHead({ hint, children, numeric = false }: { hint: HintText; children: ReactNode; numeric?: boolean }) {
  return (
    <th scope="col" className={numeric ? "num" : undefined}>
      <Hint hint={hint}>{children}</Hint>
    </th>
  );
}

function AtaTable({ attributes }: { attributes: AtaAttribute[] }) {
  const [hex, setHex] = useState(false);
  return (
    <section className="panel" aria-label="Attributs SMART">
      <div className="panel-head">
        <h3>Attributs SMART</h3>
        <div className="segmented" role="group" aria-label="Format des valeurs brutes">
          <button type="button" aria-pressed={!hex} onClick={() => setHex(false)}>
            Décimal
          </button>
          <button type="button" aria-pressed={hex} onClick={() => setHex(true)}>
            Hexa
          </button>
        </div>
      </div>
      <div className="table-wrap">
        <table className="attr-table">
          <thead>
            <tr>
              <ColumnHead hint={attributeColumnHints.status}>État</ColumnHead>
              <ColumnHead hint={attributeColumnHints.id}>ID</ColumnHead>
              <th scope="col">Attribut</th>
              <ColumnHead hint={attributeColumnHints.value} numeric>
                Actuel
              </ColumnHead>
              <ColumnHead hint={attributeColumnHints.worst} numeric>
                Pire
              </ColumnHead>
              <ColumnHead hint={attributeColumnHints.threshold} numeric>
                Seuil
              </ColumnHead>
              <ColumnHead hint={attributeColumnHints.raw} numeric>
                Brut
              </ColumnHead>
            </tr>
          </thead>
          <tbody>
            {attributes.map((a) => (
              <tr key={a.id}>
                <td>
                  <StatusCell status={a.status} />
                </td>
                <td className="mono muted-cell">{a.id.toString(16).toUpperCase().padStart(2, "0")}</td>
                <td>
                  <span className="attr-name">{a.label_fr ?? a.name}</span>
                  {a.label_fr && <span className="attr-en">{a.name}</span>}
                </td>
                <td className="num mono">{a.value}</td>
                <td className="num mono">{a.worst}</td>
                <td className="num mono muted-cell">{a.threshold}</td>
                <td className="num mono">{hex ? formatHex(a.raw_value) : a.raw_string}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

function NvmeTable({ health }: { health: NvmeHealth }) {
  const status = nvmeStatus(health);
  const pct = (n: number | null) => (n === null ? "Inconnu" : `${n} %`);
  const units = (n: number | null) => (n === null ? "Inconnu" : formatBytes(n * 512_000));
  const rows: { key: keyof NvmeHealth; label: string; hint?: HintText; value: string }[] = [
    {
      key: "critical_warning",
      label: "Avertissement critique",
      hint: nvmeHints.critical_warning,
      value: health.critical_warning === null ? "Inconnu" : health.critical_warning === 0 ? "Aucun" : `0x${health.critical_warning.toString(16).toUpperCase()}`,
    },
    {
      key: "available_spare",
      label: "Réserve disponible",
      hint: nvmeHints.available_spare,
      value: `${pct(health.available_spare)} (seuil ${pct(health.available_spare_threshold)})`,
    },
    { key: "percentage_used", label: "Usure", hint: nvmeHints.percentage_used, value: pct(health.percentage_used) },
    { key: "data_units_written", label: "Données écrites", value: units(health.data_units_written) },
    { key: "data_units_read", label: "Données lues", value: units(health.data_units_read) },
    { key: "unsafe_shutdowns", label: "Coupures brutales", hint: nvmeHints.unsafe_shutdowns, value: formatNumber(health.unsafe_shutdowns) },
    { key: "media_errors", label: "Erreurs de média", hint: nvmeHints.media_errors, value: formatNumber(health.media_errors) },
    {
      key: "error_log_entries",
      label: "Entrées du journal d'erreurs",
      hint: nvmeHints.error_log_entries,
      value: formatNumber(health.error_log_entries),
    },
  ];
  return (
    <section className="panel" aria-label="Journal de santé NVMe">
      <div className="panel-head">
        <h3>Journal de santé NVMe</h3>
      </div>
      <div className="table-wrap">
        <table className="attr-table">
          <thead>
            <tr>
              <ColumnHead hint={attributeColumnHints.status}>État</ColumnHead>
              <th scope="col">Mesure</th>
              <th scope="col" className="num">
                Valeur
              </th>
            </tr>
          </thead>
          <tbody>
            {rows.map((r) => (
              <tr key={r.key}>
                <td>{status[r.key] ? <StatusCell status={status[r.key] ?? "ok"} /> : <span className="status status-neutral">Info</span>}</td>
                <td>{r.hint ? <Hint hint={r.hint}>{r.label}</Hint> : r.label}</td>
                <td className="num mono">{r.value}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </section>
  );
}

