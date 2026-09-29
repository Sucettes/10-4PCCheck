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

const nfBytes = new Intl.NumberFormat("fr-CA", { maximumFractionDigits: 1 });

export function bytesWrittenHint(disk: DiskInfo): HintText {
  const perDay =
    disk.bytes_written !== null && disk.power_on_hours !== null && disk.power_on_hours >= 24
      ? disk.bytes_written / 1e9 / (disk.power_on_hours / 24)
      : null;
  return {
    title: "Données écrites",
    body: [
      disk.protocol === "nvme"
        ? "Total écrit par l'ordinateur sur le disque depuis sa fabrication (« Data Units Written », norme NVMe, unités de 512 000 octets)."
        : "Total écrit par l'ordinateur sur le disque depuis sa fabrication, lu dans l'attribut du fabricant (LBA écrits × taille d'un secteur, ou compteur en Gio).",
      "C'est ce qui use un SSD. Compare avec l'endurance garantie (TBW) de la fiche du fabricant : un SSD de 500 Go est souvent garanti pour 150 à 300 To écrits.",
      "Un total très bas pour beaucoup d'heures peut indiquer des compteurs remis à zéro.",
    ],
    ...(perDay !== null && { here: `Environ ${nfBytes.format(perDay)} Go écrits par jour d'utilisation.` }),
  };
}

export const checksHint: HintText = {
  title: "Vérifications de cohérence",
  body: [
    "Comparaison des compteurs entre eux : écritures et heures, heures et démarrages, usure et écritures, compteurs d'erreurs.",
    "Un disque dont les compteurs ne se tiennent pas a peut-être été remis à zéro pour paraître neuf, ou a eu un usage particulier (serveur, boîtier USB).",
    "« Attention » est une question à poser au vendeur, pas une preuve.",
  ],
};

export const selfTestHint: HintText = {
  title: "Auto-tests SMART",
  body: [
    "Le disque se teste lui-même, en lecture seule : aucune donnée n'est modifiée et l'ordinateur reste utilisable.",
    "Court : vérifie l'électronique et un échantillon de la surface, en 1 à 2 minutes. Long : lit toute la surface, de 10 minutes (SSD) à plusieurs heures (gros disque dur).",
    "Chez un vendeur, lance au moins le court. Un échec de lecture au test long veut dire des secteurs illisibles : à éviter.",
    "Un disque dans un boîtier USB peut interrompre le test s'il se met en veille.",
  ],
};

export const serialHint: HintText = {
  title: "Numéro de série",
  body: [
    "Identifiant unique du disque, masqué par défaut pour les captures d'écran partagées.",
    "À l'achat, compare-le avec l'étiquette collée sur le disque : un numéro différent veut dire que le disque lu n'est pas celui qu'on te vend.",
  ],
};

export function interfaceHint(disk: DiskInfo): HintText {
  return disk.protocol === "nvme"
    ? {
        title: "Interface",
        body: [
          "Disque NVMe : branché en PCI Express (connecteur M.2 ou boîtier USB).",
          "Dans un boîtier USB, le débit est limité par l'USB, pas par le disque.",
        ],
      }
    : {
        title: "Interface",
        body: [
          "Génération SATA prise en charge par le disque, puis vitesse réelle du lien en ce moment.",
          "SATA 3 = 6.0 Gb/s. Un lien à 3.0 ou 1.5 Gb/s sur un disque SATA 3 indique un vieux port, un câble abîmé ou un pont USB lent.",
        ],
      };
}

export const trimHint: HintText = {
  title: "TRIM",
  body: [
    "Commande qui permet au système de dire au SSD quels blocs sont libres. Elle garde les performances stables dans le temps.",
    "Effet secondaire : les fichiers supprimés sont effacés rapidement, donc presque impossibles à récupérer.",
  ],
};

export const standardHint: HintText = {
  title: "Norme",
  body: [
    "Version de la norme de commandes que le disque respecte (ATA/ACS pour SATA, NVMe pour les SSD PCIe).",
    "Utile surtout pour dater le disque : une norme récente sur un disque annoncé ancien est suspecte, et inversement.",
  ],
};

export const attributeColumnHints = {
  status: {
    title: "État",
    body: [
      "OK : rien à signaler.",
      "À surveiller : compteur d'erreurs non nul (secteurs réalloués, en attente, erreurs CRC…) ou attribut déjà passé sous son seuil par le passé.",
      "Échec : l'attribut est sous le seuil du fabricant en ce moment.",
    ],
  },
  id: {
    title: "ID",
    body: [
      "Numéro de l'attribut SMART, en hexadécimal comme dans CrystalDiskInfo.",
      "Les ID 1 à 199 ont en général le même sens partout. Au-delà, chaque fabricant fait ce qu'il veut : le nom affiché vient de la base de smartctl.",
    ],
  },
  value: {
    title: "Actuel",
    body: [
      "Valeur normalisée par le fabricant, souvent de 100 (ou 200, ou 253) quand le disque est neuf, qui baisse avec l'usure.",
      "Elle ne se compare qu'au seuil du même attribut, pas d'un disque à l'autre.",
    ],
  },
  worst: {
    title: "Pire",
    body: ["Plus basse valeur normalisée jamais atteinte par cet attribut."],
  },
  threshold: {
    title: "Seuil",
    body: [
      "Limite fixée par le fabricant. Si « Actuel » descend à ce seuil ou en dessous, l'attribut est en échec.",
      "Un seuil de 0 veut dire que l'attribut est informatif et ne peut pas échouer.",
    ],
  },
  raw: {
    title: "Brut",
    body: [
      "Valeur brute du compteur : secteurs, heures, octets, degrés… L'unité dépend de l'attribut et du fabricant.",
      "C'est souvent la valeur la plus parlante : par exemple 0 secteur réalloué, 8 démarrages. Bascule en hexadécimal pour les valeurs codées (certains fabricants y rangent plusieurs compteurs).",
    ],
  },
} satisfies Record<string, HintText>;

export const nvmeHints = {
  critical_warning: {
    title: "Avertissement critique",
    body: [
      "Drapeaux levés par le disque lui-même : réserve épuisée, température hors limites, fiabilité dégradée, passage en lecture seule.",
      "Toute valeur autre que 0 est un problème sérieux.",
    ],
  },
  available_spare: {
    title: "Réserve disponible",
    body: [
      "Part des blocs de rechange encore disponibles pour remplacer les blocs usés. 100 % sur un disque sain.",
      "Sous le seuil du fabricant, le disque est considéré en fin de vie.",
    ],
  },
  percentage_used: {
    title: "Usure",
    body: [
      "Estimation du fabricant de la durée de vie consommée. Peut dépasser 100 % : le disque a dépassé sa durée prévue.",
    ],
  },
  unsafe_shutdowns: {
    title: "Coupures brutales",
    body: [
      "Nombre de fois où le courant a été coupé sans arrêt propre (bouton maintenu, panne, batterie vide).",
      "Informatif : un nombre élevé n'endommage pas forcément le disque, mais renseigne sur l'usage.",
    ],
  },
  media_errors: {
    title: "Erreurs de média",
    body: ["Erreurs de données non corrigées par le disque. Doit être 0 ; sinon, des données ont pu être perdues."],
  },
  error_log_entries: {
    title: "Entrées du journal d'erreurs",
    body: [
      "Nombre d'erreurs de commande enregistrées. Souvent non nul sans gravité (commandes non prises en charge envoyées par le système).",
      "À regarder seulement si les erreurs de média ou l'avertissement critique sont aussi non nuls.",
    ],
  },
} satisfies Record<string, HintText>;
