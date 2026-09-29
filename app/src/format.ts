import type { CommandError, DiskInfo, SmartctlError } from "./types";

const nf = new Intl.NumberFormat("fr-CA");

export function formatBytes(bytes: number | null): string {
  if (bytes === null) return "Inconnue";
  const units = ["o", "Ko", "Mo", "Go", "To"];
  let value = bytes;
  let unit = 0;
  while (value >= 1000 && unit < units.length - 1) {
    value /= 1000;
    unit += 1;
  }
  return `${new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 }).format(value)} ${units[unit]}`;
}

export function formatNumber(n: number | null, suffix = ""): string {
  return n === null ? "Inconnu" : `${nf.format(n)}${suffix}`;
}

export function mediaLabel(disk: DiskInfo): string {
  switch (disk.media.kind) {
    case "ssd":
      return disk.protocol === "nvme" ? "SSD NVMe" : "SSD";
    case "hdd":
      return `Disque dur ${nf.format(disk.media.rpm)} tr/min`;
    case "unknown":
      return "Type inconnu";
  }
}

export function smartctlErrorMessage(e: SmartctlError): string {
  switch (e.code) {
    case "not_found":
      return `smartctl introuvable. Cherché dans : ${e.detail.searched.join(", ")}`;
    case "spawn":
      return `Impossible de lancer smartctl : ${e.detail.reason}`;
    case "timeout":
      return `smartctl n'a pas répondu en ${e.detail.seconds} s.`;
    case "invalid_json":
      return `Réponse illisible de smartctl : ${e.detail.reason}`;
    case "unsupported_json_version":
      return `Version du format smartctl non prise en charge : ${e.detail.join(".")}`;
    case "command_failed":
      if (e.detail.messages.length > 0) return e.detail.messages.join(" ");
      // Bit 1 : ouverture du périphérique impossible ou pas de réponse à IDENTIFY.
      if (e.detail.exit_status & 0b10) {
        return "Le périphérique ne répond pas aux commandes SMART. Souvent une clé USB, un lecteur de cartes ou un boîtier USB non pris en charge.";
      }
      return `smartctl a échoué (code ${e.detail.exit_status}).`;
  }
}

export function commandErrorMessage(e: CommandError): string {
  return e.kind === "smartctl" ? smartctlErrorMessage(e.detail) : e.detail;
}
