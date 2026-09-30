// Miroir des types sérialisés par le moteur Rust (crates/core et app/src-tauri).

export interface ScanDevice {
  name: string;
  info_name: string;
  dev_type: string;
  protocol: string;
}

export type MediaKind = { kind: "ssd" } | { kind: "hdd"; rpm: number } | { kind: "unknown" };

export type Protocol = "ata" | "nvme" | "scsi" | { other: string };

export interface AtaAttribute {
  id: number;
  name: string;
  value: number;
  worst: number;
  threshold: number;
  raw_value: number;
  raw_string: string;
  prefailure: boolean;
  when_failed: string | null;
  label_fr: string | null;
  status: AttributeStatus;
}

export type AttributeStatus = "ok" | "watch" | "failing";

export type SelfTestKind = "short" | "long";

export interface SelfTestResult {
  kind: string;
  passed: boolean | null;
  text: string;
  power_on_hours: number | null;
}

export interface SelfTestStatus {
  supported: boolean | null;
  running: boolean;
  remaining_pct: number | null;
  short_minutes: number | null;
  long_minutes: number | null;
  history: SelfTestResult[];
}

export interface Check {
  level: "ok" | "info" | "warn";
  text: string;
}

export interface NvmeHealth {
  critical_warning: number | null;
  available_spare: number | null;
  available_spare_threshold: number | null;
  percentage_used: number | null;
  data_units_written: number | null;
  data_units_read: number | null;
  unsafe_shutdowns: number | null;
  media_errors: number | null;
  error_log_entries: number | null;
}

export interface DiskInfo {
  device: ScanDevice;
  model: string | null;
  serial: string | null;
  firmware: string | null;
  capacity_bytes: number | null;
  protocol: Protocol;
  media: MediaKind;
  standard: string | null;
  sata_version: string | null;
  link_speed: string | null;
  form_factor: string | null;
  trim_supported: boolean | null;
  bytes_written: number | null;
  bytes_read: number | null;
  smart_available: boolean | null;
  smart_enabled: boolean | null;
  smart_passed: boolean | null;
  temperature_c: number | null;
  power_on_hours: number | null;
  power_cycles: number | null;
  ata_attributes: AtaAttribute[];
  nvme_health: NvmeHealth | null;
  life_remaining_pct: number | null;
  exit_status: number;
  warnings: string[];
  checks: Check[];
}

export type SmartctlError =
  | { code: "not_found"; detail: { searched: string[] } }
  | { code: "spawn"; detail: { path: string; reason: string } }
  | { code: "timeout"; detail: { seconds: number; args: string } }
  | { code: "invalid_json"; detail: { reason: string; excerpt: string } }
  | { code: "unsupported_json_version"; detail: number[] }
  | { code: "command_failed"; detail: { exit_status: number; messages: string[] } };

export type CommandError =
  | { kind: "smartctl"; detail: SmartctlError }
  | { kind: "tool"; detail: string }
  | { kind: "internal"; detail: string };

export type SmartctlStatus =
  | { status: "ready"; path: string; version: string }
  | { status: "unavailable"; error: SmartctlError };

export interface AppInfo {
  version: string;
  os: string;
  elevated: boolean;
  self_test: boolean;
  smartctl: SmartctlStatus;
}

export interface DiskEntry {
  device: ScanDevice;
  info: DiskInfo | null;
  error: SmartctlError | null;
}
