import type { CommandError, DiskEntry, DiskInfo, SmartctlError } from "./types";

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

/** Valeur brute ATA en hexadécimal : 48 bits, soit 12 chiffres. */
export function formatHex(n: number): string {
  return n.toString(16).toUpperCase().padStart(12, "0");
}

/** Numéro de série masqué sauf les 4 premiers caractères (captures d'écran partagées). */
export function maskSerial(serial: string): string {
  return serial.length <= 4 ? serial : serial.slice(0, 4) + "•".repeat(serial.length - 4);
}

/**
 * Étiquette qui distingue un disque des autres : ses lettres de lecteur et leur nom (« C: · D: Data »),
 * plus la fin du numéro de série si un autre disque a le même modèle.
 */
export function diskTag(entry: DiskEntry, all: DiskEntry[]): string {
  const volumes = (entry.volumes ?? []).map((v) => {
    const name = v.path.replace(/[\\/]+$/, "") || v.path;
    return v.label ? `${name} ${v.label}` : name;
  });
  const parts = volumes.length > 0 ? [volumes.join(" · ")] : ["sans lettre"];
  const model = entry.info?.model;
  const twins = model ? all.filter((e) => e.info?.model === model).length > 1 : false;
  const serial = entry.info?.serial;
  if (twins && serial) parts.push(`n° …${serial.slice(-4)}`);
  return parts.join(" · ");
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
