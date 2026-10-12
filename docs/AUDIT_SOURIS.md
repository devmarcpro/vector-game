# Audit souris : tout est-il navigable sans clavier ?

Etat apres la branche `wt-auux`. Methode : pour chaque ecran de `Screen` (game.rs), les touches de sa
barre de commandes (`tutorial::hint_line`) et de son aide F1 (`tutorial::help_lines`), croisees avec
les zones qu'il declare (`Game::hit`). Le test `audit_mouse_dump` (context_menu.rs, `--ignored
--nocapture`) imprime, par ecran, le nombre de zones et les mots de la barre qui ne se cliquent pas.

Regles de lecture : **OUI** = une zone cliquable fait la meme chose que la touche (meme `Act`, une
seule implementation). **BARRE** = le mot de la barre de commandes du bas se clique (il appuie la
touche). **FLECHES** = les mots `HAUT/BAS` et `GAUCHE/DROITE` de la barre se cliquent maintenant par
moitie (gauche du mot = premiere touche, droite = seconde) ; les lignes de liste se cliquent aussi
et la molette fait defiler. **CLIC DROIT** = le menu contextuel (context_menu.rs) propose l'action.

## Ce qui a change dans cette branche

| Avant | Apres |
|---|---|
| Clic droit = retour partout | Clic droit sur un element = menu d'actions ; sur une zone vide = retour (cockpit : menu du cockpit) |
| `HAUT/BAS`, `GAUCHE/DROITE`, `+/-`, `HAUT`, `BAS` non cliquables dans la barre | Cliquables (moitie gauche / droite du mot) |
| `FIN`, `ACCUEIL`, `INSER`, `ENT` non reconnus | Reconnus |
| Reponses aux surprises du journal : touches 1-3 seulement | Lignes cliquables |
| Marche a la premiere personne : fleches seulement | Quatre mots : tourner a gauche / droite, avancer, reculer |
| Cockpit : rotations, RCS, missiles, leurre, radar, abordage, colonisation, epave, visee souris, zoom carte, nav auto au clavier | Tous dans le menu de pilotage (F11) : tangage, lacet, roulis, RCS (maintenir), missiles (maintenir), leurre, radar, abordage, coloniser, epave, F7, zoom, nav auto |
| Pas de bouton d'aide | `? AIDE (F1)` en haut a droite de chaque ecran ; le panneau d'aide se ferme au clic |
| Pieds de page de touches dupliques avec la barre | Masques quand la barre est affichee (ils redeviennent cliquables si la barre est coupee) |

## Tableau ecran x action

| Ecran | Actions clavier | Cliquable ? | Reste au clavier / plan |
|---|---|---|---|
| Menu principal | Haut/Bas, Entree, taper une graine | OUI (lignes) | Saisie de la graine (texte) : clavier seulement |
| Nouvelle partie | Haut/Bas ligne, Gauche/Droite changer, Tab alea, Espace action, Entree suivant, Echap, saisie | OUI (lignes, `<` `>`, barre, FLECHES) | Saisie de texte (graine, noms) |
| Menu F10 (hub) | 1-8, Entree, Echap, 0 | OUI (espaces, entrees, tableau de bord, retour) ; CLIC DROIT : ouvrir / aide de l'ecran | Chiffres 1-8 = doublons des lignes |
| Tableau de bord | Tab, fleches, Entree, 1-9, Echap | OUI (panneaux, liens, retour) ; CLIC DROIT : ouvrir l'ecran detaille | Chiffres 1-9 = doublons des panneaux |
| Cockpit (vol) | Rotations WASD/QE, poussee Espace/X, RCS fleches, Maj boost, C frein, F assistance, V laser, Ctrl tir, B missiles, Tab/T/Y/R cibles, G pilote, D amarrer, M carte, +/- zoom, P pause, PgUp/PgDn temps, Entree appeler, Inser orbite, F7 visee souris, SAS 0-9, pave num. | OUI : menu de pilotage F11 (maintenir le clic = maintenir la touche) ; clic = cibler, double clic = y aller ; CLIC DROIT sur un corps / vaisseau : menu ; CLIC DROIT dans le vide : menu du cockpit | SAS 0-9 et pave numerique de l'atterrissage : mots `[0] SAS OFF`, `[PAV3] TRAIN`... cliquables dans le HUD d'atterrissage (landing_view) |
| Carte du systeme | M, +/-, C, Tab, G | OUI (corps cliquables, molette = zoom, barre) ; CLIC DROIT : menu du corps | - |
| Station / marche | Haut/Bas, Entree, S, B, F, R, U, H, Y, N, K, C, E, T, D, L, A, Fin | OUI (lignes ; mots de la barre ; double clic = tout vendre) ; CLIC DROIT sur une ressource : acheter / vendre / plein / reparer / decoller | - |
| Soute | Haut/Bas, J, F | OUI (lignes, barre) ; CLIC DROIT : jeter / verser | - |
| Vaisseau (modules) | Haut/Bas, Entree, X, Y | OUI (emplacements) ; CLIC DROIT : installer / retirer / chantier (grise hors quai avec la raison) | - |
| Chantier naval | Haut/Bas, Entree, C | OUI (coques) ; CLIC DROIT : acheter / concevoir | - |
| Concepteur | grille, couches, M symetrie, N nommer, Tab onglet, glisser | OUI (grille, pieces, onglets, apercu en glissant) | Nommer le plan (N) : saisie de texte |
| Equipage | Haut/Bas, Gauche/Droite poste, T, X, M, C, E, O, G, V, J, N | OUI (lignes + barre) ; CLIC DROIT : soigner, cryo, poste suivant/precedent, quart, equipement, organes, arbre, reseau, debarquer | Sous-modes (medecine, cryo, organes, equipement) : lignes + barre cliquables ; le menu propose seulement CHOISIR dans ces modes |
| Recrutement | Haut/Bas, Entree, K | OUI ; CLIC DROIT : embaucher | - |
| Journal / nouvelles / archives | defiler, 1-3, Tab, D C M R T | OUI (molette = defiler, barre, reponses aux surprises cliquables) | - |
| Galaxie | Haut/Bas, +/-, Entree, Retour arriere | OUI (etoiles, barre, FLECHES, +/-) ; CLIC DROIT : details, saut FTL | Retour arriere (annuler une charge) : non cliquable (plan : mot dans la barre du vol en charge) |
| Contrats | Haut/Bas, Entree, X | OUI ; CLIC DROIT : accepter / livrer / abandonner | - |
| Banque | Haut/Bas (emprunter/rembourser), Entree, H, C, V, B | OUI (barre : HAUT et BAS cliquables) | - |
| Elevage | B, P, H, Gauche/Droite, Haut/Bas | OUI (barre + FLECHES) | - |
| Casino | Entree, Gauche/Droite, Haut/Bas, +/- | OUI (barre + FLECHES + `+/-`) | - |
| Courtage (onglets) | Gauche/Droite onglet, Haut/Bas, +/-, Entree | OUI (onglets, texte vert cliquable, FLECHES) | - |
| Portail / temps | Entree, A, R, S, N, K, +/- | OUI (barre, FLECHES) | - |
| Exploration a pied | fleches/ZQSD, Entree, X, B, N, V, R, T, E, G, C, Tab, F (vue 1ere pers.) | OUI : clic = marcher (case vue), foncer = attaquer, mots de commande a droite, 4 mots de marche en vue 1ere personne ; CLIC DROIT sur une case : marcher, attaquer/parler, ramasser, creuser, batir, sol, escalier, borne, drone, rappel, fiche | Tir vise sur une case precise : `T` tire sur la cible la plus proche (pas de choix de case) |
| Empire / colonies | Haut/Bas, Entree, F3 | OUI ; CLIC DROIT : detail, flotte | - |
| Colonie | Tab zone, Haut/Bas, Espace, R, X, D, W, Entree, T, U, P, F, H | OUI (listes, `<` `>` des politiques, barre) ; CLIC DROIT : deposer / retirer / caisse / construire / politique plus-moins / projets / schema / sanitaire | - |
| Flotte | Haut/Bas, Tab, N, A, U, X, B, O, R, V, 1-4 | OUI (navires, boutons de l'escadre) ; CLIC DROIT : ordre, ravitailler, reparer, affecter, retirer, revendre | Editeur de route (champs Haut/Bas + Gauche/Droite) : FLECHES de la barre |
| Technologies | fleches, Entree | OUI (cases, double clic = rechercher) ; CLIC DROIT : details / rechercher | - |
| Regles / Options | Haut/Bas, Gauche/Droite, R | OUI (`<` `>` par ligne) | - |
| Codex | Gauche/Droite categorie, Haut/Bas, 1-9 liens, Retour arriere, L | OUI (categories, entrees, liens `[n: ...]`) | Retour arriere : mot de la barre |
| Surface (carte du monde) | fleches, Tab couche, V, E, +/- | OUI (clic sur la carte, couches, especes) | Fleches : clic sur la carte |
| Bourse | Haut/Bas, Tab, B, N, E | OUI ; CLIC DROIT : courbe, acheter / vendre a terme, actions | - |
| Famille | Gauche/Droite | OUI (membres de l'arbre) | - |
| Factions / societe / guerre | Gauche/Droite, P, B, L, S, G, Tab, ... | OUI (peuples, boutons de la societe et de la guerre) ; CLIC DROIT : amendes, pot-de-vin, avocat, registre, guerre | - |
| Proto | clics, 1-2, F12 | OUI | prototype |
| Dialogue radio | 1-6, Haut/Bas, Entree, Echap | OUI (reponses cliquables, couper) | - |
| Expedition | Haut/Bas, Espace, Entree, Echap | OUI (cases a cocher, PARTIR, RETOUR) | - |
| Navigation auto | Haut/Bas, Gauche/Droite, +/-, Entree, Espace, Suppr, PgUp/PgDn | OUI (mots, FLECHES, `+/-`) | - |
| Pause | Haut/Bas, Entree, Echap | OUI (lignes) | - |
| Vaisseau detruit | Entree | OUI (2 zones) | - |

## Ecrans ou une action reste uniquement au clavier

1. **Saisie de texte** : graine de la partie rapide et de la nouvelle partie, nom d'un plan du
   concepteur, console `~`. Par nature clavier (la graine peut aussi rester celle par defaut).
2. **Retour arriere** (annuler la charge d'un saut FTL) : pas de bouton dedie.
3. **Visee souris F7** : le pilotage a la souris est lui-meme une entree souris ; les clics ne
   servent plus a l'interface pendant ce mode (on en sort par F7, bouton de la liste F11 du cockpit
   quand le curseur est libre).
4. **Chiffres 1-9** (panneaux, espaces, reponses 1-6 du dialogue) : doublons de lignes cliquables.
5. **Tir sur une case choisie** a pied (`T`) : seulement sur la cible la plus proche.
6. Les actions du cockpit qui se **maintiennent** (poussee, rotations, tir, boost) se maintiennent
   au clic gauche maintenu sur leur mot du menu F11 ; il faut que ce menu soit visible (option
   « menu de pilotage »).

## Plan restant

- Mot `RETOUR ARRIERE` cliquable pendant une charge de saut.
- Variantes du menu contextuel pour les ecrans les moins visites (Surface, Cosmos, Livestock, Casino) :
  ils ont le menu generique (ACTIVER / AIDE / RETOUR) et leurs barres cliquables.
- Infobulles sur les valeurs cles et les jauges (aujourd'hui : lignes de listes, corps, boutons du menu,
  entrees du menu F10 avec la raison de l'indisponibilite).
