import { invoke } from "@tauri-apps/api/core";
import type { AppInfo, CommandError, DiskEntry } from "./types";

export const getAppInfo = (): Promise<AppInfo> => invoke<AppInfo>("app_info");

export const scanDisks = (): Promise<DiskEntry[]> => invoke<DiskEntry[]>("scan_disks");

export const reportSelfTest = (content: string): Promise<void> => invoke<void>("self_test_report", { content });

/** Les commandes Tauri rejettent avec l'objet d'erreur sérialisé par Rust. */
export function isCommandError(e: unknown): e is CommandError {
  return typeof e === "object" && e !== null && "kind" in e;
}
