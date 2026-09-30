# Sorties de test de l'inventaire

Tous les fichiers sont **inventés** : aucun ne vient d'une vraie machine. Les formats suivent la
documentation des outils et la structure observée sur Windows 11 (clés de `dsregcmd`, sans
reprendre de valeur). Numéros de série, identifiants et noms de machine sont fictifs (zéros,
« EXEMPLE »).

| Fichier | Origine |
|---|---|
| `dsregcmd_azure_intune.txt` | Inventé : `dsregcmd /status` d'un poste joint à Azure AD et inscrit à Intune (`MdmUrl`). Locataire et identifiants fictifs. |
| `dsregcmd_personal.txt` | Inventé : poste personnel, seul un compte professionnel ajouté (`WorkplaceJoined : YES`), valeurs vides dans la section SSO. |
| `dsregcmd_domain.txt` | Inventé : poste joint à un domaine Active Directory local. |
| `cpuinfo_intel_4c8t.txt` | Généré : `/proc/cpuinfo` d'un processeur à 4 cœurs et 8 fils (nom de modèle public, champs raccourcis). |
| `meminfo.txt` | Inventé : début de `/proc/meminfo`. |
| `os_release_ubuntu.txt` | Reconstruit : `/etc/os-release` d'Ubuntu 24.04.1 selon le format public de la distribution. |
| `dmidecode_memory.txt` | Inventé : `dmidecode -t memory` avec 3 emplacements (un vide, un en « Configured Clock Speed » d'une ancienne version de dmidecode, un fabricant en code JEDEC). Numéros de série à zéro. |
| `lspci_mm.txt` | Inventé : `lspci -mm` d'un portable avec GPU intégré Intel et GPU NVIDIA (classe « 3D controller »). |
| `uevent_bat_energy.txt` | Inventé : `/sys/class/power_supply/BAT0/uevent` en µWh (`ENERGY_*`). |
| `uevent_bat_charge.txt` | Inventé : batterie en µAh (`CHARGE_*`) avec tension de conception. |
| `uevent_mouse.txt` | Inventé : pile d'une souris sans fil (`SCOPE=Device`), à ignorer. |
| `uevent_ac.txt` | Inventé : adaptateur secteur (`TYPE=Mains`), à ignorer. |

Toute sortie réelle ajoutée ici doit d'abord être anonymisée (numéros de série, UUID, adresses
MAC, noms de machine, d'utilisateur ou de locataire).
