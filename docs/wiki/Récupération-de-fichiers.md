# Récupération de fichiers

Écran **Récupération**, trois méthodes. Règle absolue : la **destination** est toujours sur un **autre disque** que la source, sinon les fichiers récupérés écraseraient ceux qu'on cherche. PCCheck le vérifie et refuse sinon (y compris un disque virtuel VHD ou un partage réseau de la même machine).

## PhotoRec (par signatures)

Retrouve les fichiers d'après leur contenu (photos, documents, vidéos, archives), même après un formatage. Les **noms et dossiers sont perdus** : les fichiers sont rangés par type dans `recup_dir.1`, `recup_dir.2`...

Choisis les familles de fichiers à chercher pour aller plus vite. Sur un SSD avec TRIM, les chances sont faibles : le disque efface réellement les blocs libérés.

## Noms conservés (The Sleuth Kit)

Lit le système de fichiers (NTFS, FAT, exFAT, ext) et retrouve les fichiers supprimés **avec leur nom et leur dossier**, tant qu'ils n'ont pas été écrasés. Tu peux cocher des fichiers précis ou tout récupérer. La Corbeille vidée est incluse.

Ne fonctionne pas sur un volume chiffré BitLocker, ni après un formatage (utilise alors PhotoRec). Sous FAT, la première lettre du nom est remplacée par « _ ».

Deux fichiers supprimés du même nom ne s'écrasent pas : le second devient « nom (2).ext ».

## TestDisk (partitions perdues)

TestDisk s'ouvre dans un terminal intégré, au clavier. Pour une partition disparue : *Analyse*, puis *Quick Search*. Pour des fichiers supprimés : choisis la partition, *Advanced*, puis *Undelete*.

## Rapport de récupération

Chaque récupération peut produire un rapport : source, méthode, nombre de fichiers par type, fichiers illisibles, destination.
