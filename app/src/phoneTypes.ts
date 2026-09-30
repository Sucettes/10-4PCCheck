// Miroir des types de crates/android (sérialisés par le moteur Rust).

export type DeviceState = "device" | "unauthorized" | "offline" | "no_permissions" | { other: string };

export interface AdbDevice {
  serial: string;
  serial_masked: string;
  state: DeviceState;
  model: string | null;
  product: string | null;
  device: string | null;
  transport_id: number | null;
  guidance: string | null;
}

export interface PhoneDevices {
  adb_version: string | null;
  adb_error: string | null;
  devices: AdbDevice[];
  guidance: string;
}

export type FindingLevel = "ok" | "info" | "warn" | "bad";

export interface Finding {
  level: FindingLevel;
  label: string;
  detail: string;
}

export interface PhoneReport {
  serial_masked: string;
  identity: {
    manufacturer: string | null;
    brand: string | null;
    model: string | null;
    product_name: string | null;
    device_code: string | null;
    android_version: string | null;
    sdk: number | null;
    first_api_level: number | null;
    build_fingerprint: string | null;
    sales_code: string | null;
  };
  security: {
    security_patch: string | null;
    security_patch_raw: string | null;
    verified_boot: string | { other: string } | null;
    bootloader_locked: boolean | null;
    knox_warranty_void: boolean | null;
    root: { debuggable: boolean | null; build_type: string | null; test_keys: boolean | null; su_found: boolean | null };
  };
  battery: {
    level_pct: number | null;
    status_label: string | null;
    health_label: string | null;
    temperature_c: number | null;
    voltage_mv: number | null;
    technology: string | null;
    cycle_count: number | null;
    charge_full_uah: number | null;
    charge_full_design_uah: number | null;
    capacity_pct: number | null;
    updates_stopped: boolean;
  } | null;
  accounts: { account_type: string; label: string | null; count: number; activation_lock: boolean }[] | null;
  owners: { device_owner: string | null; profile_owners: { user_id: number; package: string; managed_profile: boolean }[] } | null;
  storage: { total_bytes: number; used_bytes: number; free_bytes: number } | null;
  imei_note: string;
  flash_wear_note: string;
  issues: { step: string; message: string }[];
}

export interface ChecklistItem {
  id: string;
  label: string;
  help: string;
}

export interface PhoneAnalysis {
  report: PhoneReport;
  findings: Finding[];
  verdict: FindingLevel;
  checklist: ChecklistItem[];
}
