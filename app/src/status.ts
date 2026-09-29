// Niveaux d'état affichés (couleur + libellé). Les seuils de vie restante suivent la table
// du verdict (docs/PLAN.md, section 4) ; le moteur de verdict de la phase 2 les remplacera.
import type { AttributeStatus, DiskEntry, DiskInfo, NvmeHealth } from "./types";

export type Level = "good" | "warn" | "bad" | "neutral";

export function smartVerdict(disk: DiskInfo): { label: string; level: Level } {
  if (disk.smart_passed === true) return { label: "Bon", level: "good" };
  if (disk.smart_passed === false) return { label: "Critique", level: "bad" };
  return { label: "Inconnu", level: "neutral" };
}

export function lifeLevel(pct: number): Level {
  if (pct >= 90) return "good";
  if (pct >= 70) return "warn";
  return "bad";
}

export const attributeLevel: Record<AttributeStatus, { label: string; level: Level }> = {
  ok: { label: "OK", level: "good" },
  watch: { label: "À surveiller", level: "warn" },
  failing: { label: "Échec", level: "bad" },
};

/** État NVMe lu dans le journal de santé : mêmes niveaux que les attributs ATA. */
export function nvmeStatus(h: NvmeHealth): Partial<Record<keyof NvmeHealth, AttributeStatus>> {
  const spareLow =
    h.available_spare !== null && h.available_spare_threshold !== null && h.available_spare < h.available_spare_threshold;
  return {
    critical_warning: h.critical_warning ? "failing" : "ok",
    available_spare: spareLow ? "failing" : "ok",
    percentage_used: h.percentage_used !== null && h.percentage_used >= 100 ? "watch" : "ok",
    media_errors: h.media_errors ? "watch" : "ok",
  };
}

/** Pire niveau d'un disque, pour la pastille de son onglet. */
export function entryLevel(entry: DiskEntry): Level {
  const disk = entry.info;
  if (!disk) return "neutral";
  const statuses: AttributeStatus[] = [
    ...disk.ata_attributes.map((a) => a.status),
    ...Object.values(disk.nvme_health ? nvmeStatus(disk.nvme_health) : {}),
  ];
  if (disk.smart_passed === false || statuses.includes("failing")) return "bad";
  if (disk.life_remaining_pct !== null && lifeLevel(disk.life_remaining_pct) === "bad") return "bad";
  if (statuses.includes("watch")) return "warn";
  if (disk.life_remaining_pct !== null && lifeLevel(disk.life_remaining_pct) === "warn") return "warn";
  return disk.smart_passed === true ? "good" : "neutral";
}
