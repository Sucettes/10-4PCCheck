# Disques

Écran **Disques** : un onglet par disque. Deux disques du même modèle se distinguent par leur lettre de lecteur et la fin de leur numéro de série.

## Santé SMART

Lue par **smartctl** (smartmontools). Chaque attribut SMART a :

- **Actuel** : note du fabricant (de 1 à 100, 200 ou 253 selon la marque ; plus haut = mieux) ;
- **Pire** : la note la plus basse jamais atteinte ;
- **Seuil** : si *Actuel* descend à ce niveau, le disque se déclare défaillant (0 : jamais pour cet attribut) ;
- **Brut** : le vrai compteur (heures, secteurs, démarrages).

Les compteurs les plus parlants : secteurs réalloués (5), secteurs en attente (197) et non corrigibles (198), erreurs de câble (199). Sur un SSD, la **vie restante** (usure des cellules) ; sur un NVMe, les erreurs de média et l'avertissement critique.

La section **Cohérence des compteurs** vérifie que les valeurs se tiennent (par exemple les écritures par rapport aux heures d'utilisation).

## Âge et usure

- **Heures d'utilisation**, pour un disque dur : moins de 20 000 h peu utilisé, 20 000 à 40 000 h usé, plus de 40 000 h fin de vie probable (repères tirés des statistiques publiques de Backblaze). Sur un SSD, c'est la vie restante qui compte.
- **Âge estimé** : un disque n'enregistre presque jamais sa date de fabrication. L'année de sortie du modèle (quand il est connu) donne un âge **maximal** ; saisis l'année imprimée sur l'étiquette pour l'âge exact.
- **Usage moyen** : heures par jour sur toute sa vie (24 h sur 24 pendant 10 ans n'use pas comme 2 h par jour).

## Vitesse

Test d'environ une minute (avec 1 Go), taille d'écriture au choix : 1, 5 ou 10 Go.

1. **Lecture directe** au début, au milieu et à la fin du disque, après une lecture de réveil non comptée. Mesure indicative : un disque système occupé par Windows, ou une zone jamais écrite (un SSD ou un disque SMR répond sans lire), la fausse.
2. **Temps d'accès** : 100 lectures à des endroits pris au hasard. C'est la lenteur ressentie à l'ouverture de nombreux petits fichiers, souvent la première chose qui se dégrade sur un vieux disque dur.
3. **Écriture** d'un fichier **neuf** dans l'espace libre (jamais un fichier existant), avec une courbe du débit : sur un SSD, une chute nette marque la fin de son cache rapide.
4. **Relecture** du même fichier, sans cache, bloc par bloc : c'est la vitesse de lecture qui fait foi. Un bloc relu différent de ce qui a été écrit est un défaut grave.

Le fichier est supprimé à la fin, même si le test est arrêté. Il occupe de l'espace libre : **ne lance pas ce test sur un disque dont tu veux récupérer des fichiers supprimés.**

Repères par type (débit) :

| Type | Bon | Acceptable | Faible |
|---|---|---|---|
| Disque dur 7 200 tr/min | ≥ 120 Mo/s | 80 – 120 | < 80 |
| Disque dur 5 400 tr/min | ≥ 90 Mo/s | 60 – 90 | < 60 |
| SSD SATA | ≥ 450 Mo/s | 300 – 450 | < 300 |
| SSD NVMe | ≥ 1 500 Mo/s | 800 – 1 500 | < 800 |

Temps d'accès d'un disque dur : 7 200 tr/min bon ≤ 15 ms, acceptable ≤ 25 ms ; 5 400 tr/min bon ≤ 20 ms, acceptable ≤ 30 ms.

## Liaison (SATA, PCIe et USB)

Un débit bridé par la liaison est marqué **« bridé par le port »**, pas « faible » :

- port SATA ancien : SATA II environ 280 Mo/s au maximum ;
- SSD NVMe dans un emplacement plus lent que lui (par exemple un SSD PCIe 4.0 dans un emplacement PCIe 3.0, environ 3 500 Mo/s) ;
- USB : la vitesse **négociée** est la plus basse du port, du câble et de l'adaptateur.

| Liaison USB | Débit réel maximal |
|---|---|
| USB 2.0 (480 Mb/s) | ~40 Mo/s |
| USB 3.2 Gen 1 (5 Gb/s) | ~420 Mo/s |
| USB 3.2 Gen 2 (10 Gb/s) | ~1 000 Mo/s |
| USB 3.2 Gen 2x2 (20 Gb/s) | ~2 000 Mo/s |

La fiche technique indique aussi le mode de transfert (UAS moderne, ou ancien mode plus lent sur les petits fichiers) et ce qui freine quand c'est possible de faire mieux : câble, port USB 2 (branche sur un port USB 3, souvent bleu), ou adaptateur.

## Rotation, mode de transfert et fonctionnalités

Dans la **fiche technique** d'un disque (et en tête du rapport) :

- **Vitesse de rotation** (disques durs) : 7 200 tr/min pour un disque de bureau rapide ; 5 400 ou 5 900 tr/min pour un disque de portable, externe ou d'archivage, plus lent (temps d'accès plus long). « Non rapportée » : le disque ne la déclare pas (modèles d'avant 2009 environ) ou un boîtier USB bloque ces informations.
- **Mode de transfert** : vitesse du lien en ce moment, et la plus haute que le disque sait négocier. SATA : 1,5, 3,0 ou 6,0 Gb/s. NVMe : génération PCIe et nombre de voies, par exemple « PCIe 4.0 x4 ». Un lien plus lent que ce que sait faire le disque est signalé (port, câble, adaptateur ou emplacement plus lent ; certaines machines baissent aussi le lien au repos pour économiser l'énergie).
- **Fonctionnalités** (disques SATA) : NCQ, TRIM, S.M.A.R.T., GPL, APM, AAM, DevSleep, cache d'écriture, lecture anticipée, lues dans les données d'identification du disque. Une fonctionnalité prise en charge mais désactivée est indiquée.

| Liaison | Débit réel maximal |
|---|---|
| SATA 2 (3,0 Gb/s) | ~280 Mo/s |
| SATA 3 (6,0 Gb/s) | ~550 Mo/s |
| PCIe 3.0 x4 | ~3 500 Mo/s |
| PCIe 4.0 x4 | ~7 000 Mo/s |

## Scan de surface

Lit tout le disque, en lecture seule. Zones illisibles (défaut grave) et blocs lents (secteurs relus plusieurs fois, fragiles), courbe du débit selon la position. Un disque dur de 1 To prend environ 2 heures.

## Auto-tests SMART

Test court (1 à 2 min) ou long (plusieurs heures), exécutés par le disque lui-même. L'ordinateur reste utilisable pendant le test.

## Capacité réelle (clés USB, cartes SD)

Détecte les fausses capacités (clé annoncée 128 Go qui n'en a que 16), comme H2testw ou f3 : l'espace libre est rempli de blocs signés puis relu. Les fichiers existants ne sont pas touchés. Refusé sur le disque du système et celui de PCCheck.
