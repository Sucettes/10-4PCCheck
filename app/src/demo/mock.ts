/**
 * Mode démo : quand l'interface tourne hors de Tauri (navigateur, serveur de développement,
 * captures du wiki, vidéos d'AgentFly), les commandes du moteur reçoivent des réponses FICTIVES
 * au lieu d'échouer. Données : `data.json`, généré par le vrai moteur
 * (crates/assemble/examples/demo_data.rs).
 *
 * Les lectures (disques, inventaire, rapports...) répondent ; les actions (lancer un test, une
 * récupération, ouvrir un dossier) refusent avec un message clair, sans faire semblant.
 */
import { mockIPC } from "@tauri-apps/api/mocks";
import type { DiskAge, MachineInventory, RecoveryStatus, Report, ReportSummary, SpeedResult, StressResult } from "../moreTypes";
import type { PhoneAnalysis, PhoneDevices } from "../phoneTypes";
import type { AppInfo, DiskEntry, SelfTestStatus } from "../types";
import raw from "./data.json";

interface DemoData {
  disks: DiskEntry[];
  inventory: MachineInventory;
  stress: StressResult;
  ram: unknown;
  machine_report: Report;
  speed: Record<string, SpeedResult>;
  age: Record<string, DiskAge>;
  pcie: Record<string, unknown>;
  phone_devices: PhoneDevices["devices"];
  phone: PhoneAnalysis;
}

const data = raw as unknown as DemoData;

const appInfo: AppInfo = {
  version: "démo",
  os: "windows",
  elevated: true,
  self_test: false,
  smartctl: { status: "ready", path: "tools\\smartctl.exe", version: "smartctl 7.5 2025-04-30 r5714" },
};

const selfTests: SelfTestStatus = {
  supported: true,
  running: false,
  remaining_pct: null,
  short_minutes: 2,
  long_minutes: 110,
  history: [{ kind: "Court", passed: true, text: "Terminé sans erreur", power_on_hours: 41200 }],
};

const recoveryStatus: RecoveryStatus = {
  photorec: "tools\\testdisk\\photorec_win.exe",
  photorec_error: null,
  default_destination: "F:\\PCCheck\\recup",
  help: "Choisis le disque à analyser et une destination sur un autre disque.",
  volumes: [
    { path: "C:\\", label: "Système", filesystem: "NTFS", total_bytes: 1000e9, free_bytes: 412e9, removable: false, on_source: false },
    { path: "E:\\", label: "Archives", filesystem: "NTFS", total_bytes: 1000e9, free_bytes: 118e9, removable: false, on_source: true },
    { path: "F:\\", label: "PCCHECK", filesystem: "exFAT", total_bytes: 64e9, free_bytes: 51e9, removable: true, on_source: false },
  ],
  tsk: true,
  testdisk: true,
};

const reports: ReportSummary[] = [
  {
    id: "demo-machine",
    title: data.machine_report.title,
    subject_kind: "machine",
    generated_at: data.machine_report.generated_at,
    verdict_level: data.machine_report.verdict.level,
    json_path: "rapports\\demo.json",
    html_path: "rapports\\demo.html",
    pdf_path: "rapports\\demo.pdf",
  },
  {
    id: "demo-phone",
    title: "Téléphone Samsung Galaxy S10",
    subject_kind: "phone",
    generated_at: "2026-09-29T15:40:00+00:00",
    verdict_level: "warn",
    json_path: "rapports\\telephone.json",
    html_path: "rapports\\telephone.html",
    pdf_path: "rapports\\telephone.pdf",
  },
];

/** Refus d'une action, au format des erreurs du moteur (`CommandError::Tool`). */
function unavailable(): never {
  throw { kind: "tool", detail: "Mode démonstration : cette action a besoin de l'application PCCheck sur un vrai PC." };
}

/** État d'une tâche longue : terminée, avec son résultat de démonstration quand il existe. */
function jobState(id: string) {
  const done = (ok: unknown) => ({ running: false, progress: null, result: ok === undefined ? null : { ok } });
  if (id === "cpu") return done(data.stress);
  if (id === "ram") return done(data.ram);
  if (id.startsWith("speed:")) return done(data.speed[id.slice("speed:".length)]);
  return done(undefined);
}

const device = (args: unknown) => (args as { device?: string | { name: string } }).device;
const deviceName = (args: unknown) => {
  const d = device(args);
  return typeof d === "string" ? d : (d?.name ?? "");
};

export function installDemo(): void {
  mockIPC(
    (cmd, args) => {
      switch (cmd) {
        case "app_info":
          return appInfo;
        case "scan_disks":
          return data.disks;
        case "machine_inventory":
          return data.inventory;
        case "preview_machine_report":
          return data.machine_report;
        case "job_state":
          return jobState((args as { id: string }).id);
        case "disk_age":
        case "set_disk_year":
          return data.age[deviceName(args)] ?? null;
        case "disk_usb_link":
          return null;
        case "disk_pcie_link":
          return data.pcie[deviceName(args)] ?? null;
        case "speed_result":
          return data.speed[deviceName(args)] ?? null;
        case "smart_test_status":
          return selfTests;
        case "recovery_status":
          return recoveryStatus;
        case "recovery_trim_warning":
          return null;
        case "list_reports":
          return reports;
        case "phone_devices":
          return { adb_version: "35.0.2", adb_error: null, devices: data.phone_devices, guidance: "" } satisfies PhoneDevices;
        case "phone_collect":
          return data.phone;
        case "cancel_job":
          return null;
        default:
          return unavailable();
      }
    },
    { shouldMockEvents: true },
  );
}
