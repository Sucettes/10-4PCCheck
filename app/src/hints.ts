// Textes des info-bulles : d'où vient la valeur, ce qu'elle représente, comment la lire.
// Les repères chiffrés suivent la table du verdict (docs/PLAN.md, section 4), encore à valider.
import type { DiskInfo } from "./types";

export interface HintText {
  title: string;
  body: string[];
  /** Lecture de la valeur de ce disque, quand elle aide à interpréter. */
  here?: string;
}

const nf1 = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

export function smartStatusHint(disk: DiskInfo): HintText {
  const status =
    disk.smart_passed === true
      ? "Le disque déclare réussir son autodiagnostic SMART (« PASSED »)."
      : disk.smart_passed === false
        ? "Le disque déclare lui-même une défaillance SMART (« FAILED ») : un attribut critique est sous le seuil du fabricant. Sauvegarde les données et évite l'achat."
        : "Le disque ne fournit pas de résultat SMART global (pont USB, disque virtuel ou SMART désactivé).";
  const body = [
    status,
    "Ce résultat bascule très tard, souvent quand le disque est déjà en train de lâcher. Un disque très usé peut encore afficher « PASSED ».",
  ];
  if (disk.life_remaining_pct === null) {
    return { title: "État SMART", body };
  }
  const source =
    disk.protocol === "nvme"
      ? "Le % est la vie restante : 100 moins l'usure déclarée par le disque (« Percentage Used », norme NVMe)."
      : "Le % est la vie restante, lue dans l'attribut d'usure du fabricant (177 Samsung, 202 Crucial, 231 Kingston, 233 Intel).";
  return {
    title: "État SMART et vie restante",
    body: [
      ...body,
      source,
      "C'est l'estimation du fabricant selon les écritures prévues par la garantie. Repères : 90 % et plus, bon ; 70 à 89 %, usé, à négocier ; sous 70 %, à éviter. À 0 %, le disque a dépassé sa durée prévue mais peut encore fonctionner.",
    ],
    here: `Il reste environ ${disk.life_remaining_pct} % de la durée de vie prévue.`,
  };
}

export function temperatureHint(disk: DiskInfo): HintText {
  const t = disk.temperature_c;
  return {
    title: "Température",
    body: [
      "Température actuelle mesurée par le capteur du disque.",
      "Repères au repos : moins de 50 °C, normal ; 50 à 60 °C, chaud (vérifie la ventilation) ; plus de 60 °C, trop chaud. Elle monte pendant les gros transferts, c'est normal.",
      "Un SSD trop chaud ralentit pour se protéger (bridage thermique).",
    ],
    ...(t === null && { here: "Le disque ne rapporte pas de température." }),
  };
}

export function powerOnHoursHint(disk: DiskInfo): HintText {
  const h = disk.power_on_hours;
  return {
    title: "Heures d'utilisation",
    body: [
      "Nombre total d'heures pendant lesquelles le disque a été alimenté depuis sa fabrication (compteur interne, non modifiable par l'utilisateur).",
      "8 760 h = 1 an allumé 24 h sur 24. Compare avec ce que dit le vendeur : un disque « presque neuf » à plusieurs milliers d'heures est suspect.",
    ],
    ...(h !== null && {
      here: `Soit environ ${nf1.format(h / 24)} jours, ou ${nf1.format(h / 8760)} an(s) allumé en continu.`,
    }),
  };
}

export function powerCyclesHint(disk: DiskInfo): HintText {
  const h = disk.power_on_hours;
  const c = disk.power_cycles;
  return {
    title: "Démarrages",
    body: [
      "Nombre de fois où le disque a été mis sous tension.",
      "Heures ÷ démarrages = durée moyenne d'une session. Beaucoup d'heures pour peu de démarrages : machine rarement éteinte (serveur, minage, NAS). Beaucoup de démarrages pour peu d'heures : usage court et fréquent, typique d'un portable.",
    ],
    ...(h !== null && c !== null && c > 0 && { here: `Ici, environ ${nf1.format(h / c)} h par démarrage.` }),
  };
}

export const firmwareHint: HintText = {
  title: "Firmware",
  body: [
    "Version du logiciel interne du disque.",
    "Certaines versions ont des bogues connus (ralentissements, perte de données). Cherche « modèle + firmware » sur le site du fabricant pour savoir si une mise à jour existe.",
  ],
};

export const unreadableHint: HintText = {
  title: "Disque illisible",
  body: [
    "smartctl n'a pas pu lire les données SMART de ce disque.",
    "Causes fréquentes : boîtier USB dont le pont ne transmet pas le SMART, ou outil lancé sans droits administrateur. Essaie un autre boîtier ou branche le disque en interne.",
  ],
};
