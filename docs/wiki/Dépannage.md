# Dépannage

**« Droits limités » en haut de l'écran**
L'invite administrateur a été refusée. Relance `PCCheck.exe` et accepte-la ; sous Linux, `sudo ./PCCheck.AppImage`.

**SmartScreen bloque le lancement**
*Informations complémentaires*, puis *Exécuter quand même*. L'outil n'est pas signé numériquement.

**Un disque n'apparaît pas, ou sans données SMART**
Certains adaptateurs USB-SATA ne transmettent pas le SMART, et les contrôleurs RAID le masquent. Branche le disque directement en SATA si possible.

**La lecture directe d'un SSD semble lente, alors que la relecture est bonne**
Normal sur le disque où Windows tourne (il y lit et écrit en permanence) ou juste après une période d'inactivité. La note de lecture se fonde sur la relecture, plus fiable. Voir [[Disques]].

**Un disque dur neuf lit anormalement vite**
Les zones jamais écrites d'un disque SMR (ou d'un SSD) répondent sans être lues. C'est pour cela que la relecture d'un fichier réellement écrit fait foi.

**Vitesse « bridée par le port »**
Le disque est limité par sa liaison (USB 2.0, port SATA ancien) : branche-le sur un port plus rapide pour mesurer sa vraie vitesse. La fiche technique du disque indique la liaison.

**Test d'écriture « non mesuré »**
Aucun volume de ce disque n'est accessible en écriture, ou il reste moins de 3 Go libres (le fichier de test plus 2 Go de marge).

**Téléphone non détecté**
Débogage USB activé ? Câble de données (pas seulement de charge) ? Clé de l'ordinateur acceptée sur le téléphone ? Voir [[Téléphone Android]].

**PhotoRec, TestDisk ou The Sleuth Kit introuvable**
Le dossier `windows/tools/` est incomplet : retélécharge `PCCheck-windows.zip` depuis les *Releases* (voir [[Installation]]).

**Signaler un problème**
Onglet *Issues* du dépôt, modèle « Problème ». Joins le rapport JSON si possible, **après avoir vérifié qu'il ne contient rien de personnel** (numéros de série, noms de volumes) : les issues sont publiques.
