# Analyse complète

Écran **Analyse complète**, bouton *Lancer l'analyse* (environ 10 minutes, plus les tests interactifs). Les étapes s'enchaînent seules ; *Passer ce test* saute l'étape en cours. Quitter l'écran n'arrête pas l'analyse.

| Étape | Durée | Ce qui est mesuré |
|---|---|---|
| Inventaire | ~10 s | Processeur, mémoire, carte mère, BIOS, carte graphique, réseau, batterie, licence Windows, BitLocker, Secure Boot, TPM, gestion d'entreprise (Intune, Autopilot, domaine) |
| Disques | quelques s | Santé SMART de chaque disque (voir [[Disques]]) |
| Vitesse des disques | ~1 min par disque | Lecture, temps d'accès, écriture de 1 Go dans l'espace libre, relecture vérifiée |
| Mémoire | ~1 min | Test partiel sur la moitié de la mémoire libre (5 motifs) |
| Processeur | 5 min | Charge sur tous les cœurs : bridage thermique, erreurs de calcul, courbe débit et température |
| Carte graphique | 2 min | Rendu 3D intensif (WebGL) : bridage, erreurs de rendu, température (cartes NVIDIA) |

## Tests interactifs

Clavier (chaque touche), pixels morts, webcam, micro, haut-parleurs gauche et droite, pavé tactile. Le résultat de chaque test (réussi, échec avec note, non fait) va dans le rapport.

## Devant le vendeur

Liste de vérifications manuelles enregistrée dans le rapport : compte Microsoft du vendeur retiré, pas de mot de passe BIOS, numéro de série identique à l'étiquette, chargeur fonctionnel, ports USB, charnières et écran, Wi-Fi.

## Limites

- Le test mémoire depuis Windows ne couvre que la mémoire libre : pour un test complet, MemTest86+ depuis la [[Clé bootable]].
- Températures du processeur sous Windows : seulement les zones ACPI, souvent absentes (pas de pilote noyau).
- Test graphique : sur un portable à deux cartes, le rendu peut se faire sur la carte intégrée ; le moteur de rendu utilisé est indiqué.
