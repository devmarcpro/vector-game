# CONTROLES (touches physiques, positions QWERTY ; sur AZERTY : ZQSD = WASD)
| Touche | Action |
|---|---|
| W / S | Tangage (piquer / cabrer) |
| A / D | Lacet |
| Q / E | Roulis |
| Espace / X | Poussee avant (moteur principal) / recul (RCS) |
| Fleches | RCS lateral (gauche/droite) et vertical (haut/bas) |
| F | Assistance de vol ON/OFF (amortit rotation et derive) |
| Maj gauche (maintenir) | Boost : poussee x2.5, rotation x1.4, carburant x3 sur la poussee principale |
| C (maintenir, carte fermee) | Frein : annule la vitesse relative a la cible (sinon au corps de reference, sinon au repere de l'etoile), partout et a toute vitesse, par RCS |
| F7 | Visee souris ON/OFF : le mouvement relatif de la souris commande tangage et lacet (500 px/s = pleine deflexion), s'ajoute au clavier ; curseur capture en vol seulement |
| Tab | Cible suivante (la plus proche d'abord) |
| R | Cibler le vaisseau PNJ le plus proche (cycle) |
| T / Y | Cible : asteroide / station le(la) plus proche |
| V (maintenir) | Laser de minage (cible asteroide a < 2 km, visee dans le cone) |
| D | Amarrer (station < 500 m, vitesse relative < 8 m/s) |
| I | Soute / inventaire (J = jeter la ressource) |
| K | Equipage (Gauche/Droite poste, T quart, X débarquer à quai, J journal) |
| J | Journal de bord |
| L | Carte galactique (Haut/Bas étoile, +/- zoom, Entrée = saut FTL) ; Retour arrière annule une charge de saut |
| F2 | Empire (colonies) ; Entrée = détail ; F3 = Flotte |
| O | Fonder une colonie sur le corps ciblé (kit en soute) |
| F4 | Arbre technologique (Entrée lance une recherche) |
| Ctrl / Entrée (maintenir) | Tir principal (faisceaux, canons) |
| B (maintenir) | Missiles et mines |
| Z | Payer le tribut demandé par des pirates |
| N | Récupérer l'épave la plus proche |
| U | Aborder la cible neutralisée |
| R | Cibler le vaisseau PNJ le plus proche |
| H | Ecran vaisseau (emplacements, santé, énergie ; amarré : Entrée installer, X retirer, Y chantier naval) |
| F5 / F9 | Sauvegarde / chargement rapide (saves/slot1.sav) |
| Echap | Menu pause |
| G | Pilote auto vers la cible (brachistochrone limitee par le delta-v) |
| M | Carte systeme ; +/- zoom ; C centre etoile/vaisseau |
| P | Pause |
| PgUp / PgDn | Temps x0.25 .. x100000 (pas adaptatifs ; poussee manuelle coupee au-dela de x2) |
| ~ (Backquote) | Console de debug : help, list, tp, tpbody, stop, speed, fuel, repair, assist, time |
| F1 | Aide contextuelle de l'ecran courant (re-appuyer pour fermer) |
| F6 | Codex (Gauche/Droite categorie, Haut/Bas entree) |
| F10 | Ignorer le tutoriel |

Options de ligne de commande : `--snapshot fichier.png [scene]` (rendu headless), `--bench`, `--script fichier.txt` (jeu scripte sans fenetre, voir `crates/game/src/script.rs`).

## Station
Haut/Bas : ressource ; Entree : vendre tout ; S : vendre 100 kg ; B : acheter 100 kg ; F : plein de carburant ; R : reparer ; U : decoller.
## Menu principal
Haut/Bas + Entree ; taper une graine (nombre ou texte) avant Nouvelle partie.

Station : N = recruter (Entrée embauche au poste de sa meilleure compétence), K = équipage.
Station : T = bourse (Haut/Bas ressource, PgUp/PgDn x10, Tab ou Gauche/Droite période 30/90/180 jours, T ou Échap retour).
Station : C = contrats (Entrée accepter/livrer, X abandonner), E = banque (Haut emprunter 1000, Bas rembourser 1000, Entrée tout rembourser).

Ecran colonie : Tab zone (bâtiments / stocks / politiques / construction), Haut/Bas, Espace on/off, R recette, X démolir, D déposer / W retirer (stocks, amarré), Gauche/Droite politique, Entrée construire, T récupérer la caisse, U décoller.
Ecran flotte : Tab liste, N nouvelle route (Gauche/Droite valeur, Tab champ, Entrée), A affecter, U retirer, X supprimer/vendre, B acheter (à quai).

## Menus
Menu principal : Nouvelle partie / Continuer / Regles de partie / Options / Codex-Aide / Quitter. Menu pause : Reprendre / Sauvegarder / Charger / Regles / Options / Codex-Aide / Menu principal / Quitter.
Regles : Haut/Bas, Gauche/Droite modifier, R valeurs par defaut ; depuis la pause elles s'appliquent immediatement.
Options : lignes de balayage, halo, palette (couleur, vert, ambre, blanc, bleu), volume, tutoriel, barre des commandes ; sauvegardees dans saves/settings.cfg.


## Barre des commandes
La dernière ligne de l'écran affiche en permanence les touches utiles ici et maintenant (vol, carte, saut en charge, pilote automatique, station, module, colonie, menus...). Elle suit l'écran et son sous-état ; Options > Barre des commandes la coupe. Le bandeau du tutoriel passe alors en haut des écrans hors vol.

## Couleurs
Une fois un corps relevé (niveau 2), sa couleur donne son type : étoiles par classe (O bleu à M rouge), planètes (aride gris, désert sable, terrestre vert, océan bleu, toundra turquoise, glace blanc-bleu, lave rouge-orangé, vénusienne ocre, géante gazeuse orange, géante de glace bleue, Jupiter chaud magenta), astéroïdes par type (C brun, S sable, M métal, I glace), stations cyan. Non relevé : vert neutre. La cible est entourée de crochets ambre ; atmosphères et géantes ont un halo, certaines géantes des anneaux.
