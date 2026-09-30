# Clé bootable

La même clé sert aussi au **démarrage** de l'ordinateur, grâce à **Ventoy** (https://www.ventoy.net), qui garde une partition de données normale, visible depuis Windows :

- **MemTest86+** (https://www.memtest.org) : test complet de la mémoire, impossible depuis Windows. Au moins un passage complet (15 à 60 min) ; une seule erreur = barrette défectueuse.
- **Ubuntu 24.04 LTS** (https://ubuntu.com/download/desktop) : Linux de secours quand Windows est absent, cassé ou verrouillé. Démarre avec Secure Boot actif : rien à changer dans le BIOS du vendeur. Lance ensuite `sudo ./PCCheck.AppImage` depuis le dossier `linux/`.

## Préparer (une fois)

1. **Sauvegarde** le contenu de la clé : l'installation de Ventoy l'efface.
2. `Ventoy2Disk.exe` : choisis la clé (vérifie deux fois la lettre et la taille), coche **Secure Boot Support**, puis *Install*.
3. Copie à la racine de la partition Ventoy : l'ISO de MemTest86+, l'ISO d'Ubuntu, et le dossier de PCCheck (voir [[Installation]]).

## Démarrer chez un vendeur

1. Éteins, branche la clé, rallume en appuyant sur la touche du menu de démarrage (F12 Dell/Lenovo, F9 HP, F8 Asus, Échap sur certains portables).
2. Secure Boot actif : au premier démarrage, Ventoy demande d'enrôler sa clé (*Enroll key from disk*, fichier `ENROLL_THIS_KEY_IN_MOKMANAGER.cer`). Environ une minute ; Windows n'est pas modifié.
3. Choisis MemTest86+ ou Ubuntu (« Essayer Ubuntu »).

## Limites

- Mot de passe BIOS sur un PC d'entreprise : démarrage externe bloqué, analyse depuis Windows seulement (et c'est un indice : machine encore gérée par une entreprise ?).
- Mac Intel : Ventoy fonctionne en général ; Apple Silicon : non.
- Le Linux live ne lit pas les volumes chiffrés BitLocker (le SMART et le scan de surface restent possibles).
