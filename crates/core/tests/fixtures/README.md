# Sorties smartctl de test

| Fichier | Origine |
|---|---|
| `sata_samsung_860evo.json` | Reconstruit à partir d'une capture CrystalDiskInfo réelle (valeurs exactes, numéro de série masqué). Fiche technique (norme, lien SATA, format, TRIM) ajoutée selon les caractéristiques publiques du modèle. À remplacer par une vraie sortie `smartctl -a -j` au test final. |
| `nvme_generic.json` | Reconstruit selon le format JSON de smartctl 7.x (structure vérifiée sur smartctl 8.0 sous Windows). Valeurs plausibles, pas un vrai disque. |
| `hdd_failing.json` | Reconstruit : disque dur en fin de vie (secteurs réalloués sous le seuil, code de sortie 88). |
| `usb_bridge_unsupported.json` | Reconstruit : pont USB sans passthrough SMART (code de sortie 1). |
| `scan.json` | Reconstruit : sortie de `--scan-open -j` avec SATA, NVMe et pont Realtek. |
| `real_scan_virtio_empty.json` | **Réel** : smartctl 7.5 statique sur la VM cloud de développement (disque virtio, aucun disque SMART détecté). |
| `real_virtio_unknown_type.json` | **Réel** : `smartctl -a -j /dev/vda` sur la même VM (type de périphérique non détecté, code de sortie 1). |

Toute sortie réelle ajoutée ici doit avoir son numéro de série masqué.
| `selftest_*.json` | Reconstruits : sorties de `smartctl -c -l selftest -j` (auto-test ATA en cours et historique avec échec, auto-test NVMe en cours et historique). Format selon smartctl 7.x. |
