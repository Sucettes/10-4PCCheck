# Sorties adb de test

Aucun vrai téléphone n'a servi : **tous les fichiers sont reconstruits** à la main d'après le
format des commandes (code source d'AOSP, documentation d'adb). Numéros de série, comptes et
noms d'application de gestion sont inventés et visiblement factices (`R58N00000XX`,
`compte1@example.com`, `com.example.mdm`). À compléter par de vraies sorties au premier test sur
un téléphone, en masquant série, IMEI et adresses.

| Fichier | Commande | Origine |
|---|---|---|
| `adb_version.txt` | `adb version` | Reconstruit (platform-tools 35), chemin d'installation fictif. |
| `devices_mixed.txt` | `adb devices -l` | Reconstruit : prêt, non autorisé, hors ligne (Wi-Fi), émulateur, mode recovery. |
| `devices_daemon_start.txt` | `adb devices -l` | Reconstruit : premier appel qui démarre le serveur (lignes `* daemon ...`). |
| `devices_linux_no_permissions.txt` | `adb devices -l` | Reconstruit : Linux sans règles udev. |
| `devices_empty.txt` | `adb devices -l` | Reconstruit : aucun appareil. |
| `devices_short_crlf.txt` | `adb devices` | Reconstruit : format court séparé par des tabulations, fins de ligne Windows. |
| `getprop_samsung.txt` | `adb shell getprop` | Reconstruit, extrait : Galaxy S10 canadien (SM-G973W, CSC XAC), Android 12, correctif 2023-01-01. |
| `getprop_pixel.txt` | `adb shell getprop` | Reconstruit, extrait : Pixel 7, Android 16, correctif 2026-08-05. |
| `battery_old.txt` | `adb shell dumpsys battery` | Reconstruit : format Android 4 (`voltage:` sans espace, peu de champs). |
| `battery_new.txt` | `adb shell dumpsys battery` | Reconstruit : format Android 14 et plus (`Charge counter`, `Charging state`...). |
| `battery_samsung_stopped.txt` | `adb shell dumpsys battery` | Reconstruit : valeurs figées (`UPDATES STOPPED`), surchauffe, tension en µV, lignes propres à Samsung, champ `level` répété. |
| `account_with.txt` | `adb shell dumpsys account` | Reconstruit : 2 Google, 1 Samsung, 1 WhatsApp, et un profil de travail avec un compte Microsoft. |
| `account_none.txt` | `adb shell dumpsys account` | Reconstruit : aucun compte. |
| `dpm_none.txt` | `adb shell dpm list-owners` | Reconstruit : aucun propriétaire. |
| `dpm_device_owner.txt` | `adb shell dpm list-owners` | Reconstruit : propriétaire d'appareil (gestion d'entreprise). |
| `dpm_profile_owner.txt` | `adb shell dpm list-owners` | Reconstruit : profil de travail géré. |
| `dpm_unknown_old.txt` | `adb shell dpm list-owners` | Reconstruit : Android 11 et moins, commande inconnue. |
| `device_policy_old.txt` | `adb shell dumpsys device_policy` | Reconstruit, extrait : repli pour Android 11 et moins. |
| `df_toybox.txt` | `adb shell df /data` | Reconstruit : toybox (Android 6 et plus), blocs de 1 Kio. |
| `df_toolbox.txt` | `adb shell df /data` | Reconstruit : toolbox (Android 5 et moins), tailles lisibles. |
| `df_wrapped.txt` | `adb shell df /data` | Reconstruit : nom de partition long renvoyé à la ligne. |
