// Miroir des types des crates report, inventory et recovery (sérialisés par le moteur Rust).

// ---------- Rapport (crates/report) ----------

export type Level = "ok" | "info" | "warn" | "bad" | "neutral";

export interface ReportItem {
  label: string;
  value: string;
  level: Level;
  detail: string | null;
}

export interface ReportSection {
  id: string;
  title: string;
  items: ReportItem[];
  tables: { title: string; columns: string[]; rows: string[][] }[];
}

export interface Report {
  schema_version: number;
  tool_version: string;
  generated_at: string;
  title: string;
  subject: { kind: "machine" | "disk" | "phone" | "recovery"; name: string; details: { label: string; value: string }[] };
  verdict: { level: Level; summary: string; ok: number; warn: number; bad: number };
  sections: ReportSection[];
}

export interface ReportSummary {
  id: string;
  title: string;
  subject_kind: "machine" | "disk" | "phone" | "recovery";
  generated_at: string;
  verdict_level: Level;
  json_path: string;
  html_path: string | null;
  pdf_path: string | null;
}

export interface ChecklistEntry {
  label: string;
  checked: boolean;
  note: string | null;
}

// ---------- Inventaire (crates/inventory) ----------

export interface MachineInventory {
  os: { name: string | null; version: string | null; build: string | null; display_version: string | null } | null;
  computer: { manufacturer: string | null; model: string | null; serial_number: string | null } | null;
  board: { manufacturer: string | null; product: string | null } | null;
  cpu: { name: string | null; cores: number | null; threads: number | null; base_mhz: number | null } | null;
  memory: { total_bytes: number | null; modules: unknown[] } | null;
  battery: { count: number; health_pct: number | null; cycle_count: number | null } | null;
  gpus: { name: string }[];
  errors: string[];
}

export interface StressSample {
  elapsed_ms: number;
  iterations_per_sec: number;
  max_celsius: number | null;
}

export interface StressResult {
  threads: number;
  duration_ms: number;
  cancelled: boolean;
  computation_errors: number;
  max_celsius: number | null;
  throttling: { baseline: number; final_rate: number; drop_pct: number; level: "none" | "brief" | "sustained" };
}

export interface RamProgress {
  phase: string;
  pass: number;
  total_passes: number;
  bytes_done: number;
  bytes_total: number;
  errors: number;
}

export interface RamTestResult {
  tested_bytes: number;
  errors: number;
  duration_ms: number;
  cancelled: boolean;
}

/** Nom de la machine, même règle que le rapport (crates/assemble, `machine_name`) : modèle, sinon
 *  carte mère (PC monté), sinon fabricant. */
export function machineName(inv: MachineInventory): string {
  const join = (...p: (string | null | undefined)[]) => p.filter(Boolean).join(" ");
  if (inv.computer?.model) return join(inv.computer.manufacturer, inv.computer.model);
  if (inv.board?.product) return join(inv.board.manufacturer, inv.board.product);
  return inv.computer?.manufacturer ?? "Ordinateur";
}

// ---------- Récupération (crates/recovery) ----------

export type FileFamily = "photos" | "documents" | "videos" | "audio" | "archives" | "everything";

export interface VolumeView {
  path: string;
  label: string;
  filesystem: string;
  total_bytes: number;
  free_bytes: number;
  removable: boolean;
  on_source: boolean;
}

export interface RecoveryStatus {
  photorec: string | null;
  photorec_error: string | null;
  default_destination: string;
  help: string;
  volumes: VolumeView[];
  tsk: boolean;
  testdisk: boolean;
}

export interface DeletedFile {
  inode: string;
  path: string;
  is_dir: boolean;
  size: number | null;
  modified: string | null;
}

export interface DeletedList {
  files: DeletedFile[];
  total: number;
  truncated: boolean;
}

export interface TskProgress {
  running: boolean;
  elapsed_s: number;
  files_found: number;
  bytes_found: number;
  reported: number | null;
  exit_code: number | null;
  stopped_by_user: boolean;
}

export interface FoundFile {
  path: string;
  name: string;
  extension: string;
  size: number;
}

export interface RecoveryProgress {
  running: boolean;
  elapsed_s: number;
  files_found: number;
  bytes_found: number;
  by_extension: Record<string, number>;
  last_files: FoundFile[];
  exit_code: number | null;
  stopped_by_user: boolean;
  problem: string | null;
  scan_error: string | null;
}

// ---------- Capacité réelle (crates/core) ----------

export interface CapacityProgress {
  phase: "write" | "verify";
  done_bytes: number;
  total_bytes: number;
  rate_bps: number;
  bad_bytes: number;
}

export interface CapacityResult {
  target: string;
  written_bytes: number;
  verified_bytes: number;
  ok_bytes: number;
  corrupted_bytes: number;
  overwritten_bytes: number;
  write_error: string | null;
  write_mbps: number;
  read_mbps: number;
  cancelled: boolean;
  verdict: { kind: "genuine" } | { kind: "fake"; real_bytes: number } | { kind: "damaged" } | { kind: "incomplete" };
}
