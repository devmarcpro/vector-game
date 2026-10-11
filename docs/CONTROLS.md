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

## Souris
Tout se fait aussi à la souris (hors visée F7, où la souris pilote le vaisseau).
- **Clic gauche** : une ligne de menu ou de liste la sélectionne ; un bouton (barre des commandes du bas, onglets, flèches < > autour d'une valeur) fait ce que fait sa touche ; en vol, un objet du cockpit ou de la carte système devient la cible ; sur la carte de surface le curseur va sous le clic ; dans l'arbre des technologies une case est sélectionnée.
- **Double clic** : station, soute, vaisseau, chantier naval, recrutement = Entrée sur la ligne (vendre, installer, acheter, embaucher) ; équipage = arbre généalogique ; technologie = lancer la recherche ; objet en vol = pilote automatique ; étoile = saut.
- **Clic droit** : retour (Échap) sur tous les écrans sauf en vol et au menu principal.
- **Molette** : fait défiler la sélection de l'écran (Haut/Bas) ; en vol, change l'accélération du temps ; sur la carte système, zoome.
- **Barre des commandes** : chaque commande (« TOUCHE ce qu'elle fait ») est un bouton, sauf les touches maintenues en vol (poussée, boost, frein, lasers).
- Un arbre généalogique : un parent à bord est un bouton qui ouvre son propre arbre.

- **X** (en vol) : écran des factions et de leurs moeurs ; Gauche/Droite ou clic pour choisir, X ou Echap pour revenir.

- **Codex** : sous chaque entrée, « VOIR AUSSI » liste les entrées citées ; clic ou touches 1 à 9 pour les suivre, Retour arrière pour revenir.

- **Equipage, O** : liste des organes de la personne choisie ; Haut/Bas choisit l'organe, Entree greffe (ou prothèse s'il n'y a pas de greffon en soute), P prothèse, H prélève, O ou Echap revient à la liste. Il faut la pièce en soute et un hôpital (à quai, 1500 CR) ou un médecin et une infirmerie à bord.

- **Equipage, O, puis Gauche/Droite** choisit un implant, **I** le pose, **U** le retire (la pièce doit être en soute ; 3000 CR à quai).

- **Liste des organes, M** : injecter une dose de mutagène (0.1 kg) à la personne choisie.

- **Entrée** (en vol, vaisseau ciblé) : appeler ; 1-6 ou clic pour répondre, Haut/Bas + Entrée, Echap coupe.

## Clavier
Par défaut AZERTY : tangage Z/S, lacet Q/D, roulis A/E. Options > Clavier bascule en QWERTY (W/S, A/D, Q/E). Les autres commandes se nomment par la lettre imprimée sur la touche, quelle que soit la disposition. Le tribut des pirates se paie avec la touche Z en QWERTY, par la radio (Entrée) en AZERTY. Options > Langue : FRANCAIS (les traductions viendront plus tard).

## Expédition (surface, touche E)
Écran de préparation : Haut/Bas, Espace coche un équipier ou le rover, Entrée part. Ensuite c'est l'exploration commune (voir « Exploration à pied ») : flèches, pavé numérique ou Z/Q/S/D pour avancer (foncer sur un animal ou un monstre l'attaque), les plantes et le butin se ramassent en marchant, Entrée sur un site (ruine, grotte, épave…) y descend et sur < remonte, V lance un drone, R rappelle l'équipe depuis la surface, Tab change de chef, clic = marcher jusqu'à la case.

- **Globe de surface** : glisser avec le bouton gauche pour le faire tourner.

- **Menu de pilotage** (cockpit, à droite) : cliquer un mot ouvre l'écran ou exécute la commande ; les mots « (MAINT.) » agissent tant que le bouton de la souris reste enfoncé. F11 plie ou déplie le menu (Options > Menu de pilotage pour le retirer).

- **Maj + Tab** : cible la planète ou lune suivante. **Pilote auto (G)** : transfert assisté vers la cible, le vaisseau reste garé à l'arrivée (une poussée le libère) ; refusé pour les géantes gazeuses (approche à la main). **Galaxie (L) puis Entrée** : le vaisseau s'éloigne seul des corps massifs puis saute.

- **Entrée sur une planète ciblée** (Maj+Tab, assez près) : parler à son peuple s'il en a un (voir la ligne CIVILISATION de l'écran de surface F8).

- **Bourse** : B / N contrat à terme à la hausse / à la baisse sur la ressource choisie (100 kg, 30 j, marge 25 %), C clôt un contrat.

- **Equipage, E** : liste de l'équipement (flèches pour choisir, Entrée équipe depuis la soute, U retire, E ou Echap revient). Clic sur une ligne ARME / ARMURE / OUTIL.

- **Bourse, E** : bascule entre marchandises et entreprises ; sur les entreprises, B achète 10 actions et N en vend 10 (il faut être à quai).

- Station : G achete 50 kg, J vend 50 kg de la ressource choisie au marche noir (seulement pour les biens interdits du territoire ; prime ~x1,9 a l'achat, x1,35 a la vente, 12 % de descente de police : amende 600 CR, saisie, reputation -0,25).

- Station : L elevage (B acheter, P reproduire avec le meilleur partenaire, H abattre, Gauche/Droite espece), A casino (Gauche/Droite jeu, Haut/Bas mise, +/- numero, Entree jouer).

- Station : D courtage (Gauche/Droite onglet : rumeurs, arbitrage, cartels, appels d'offres, assurance ; Haut/Bas choix ; +/- mise ; Entree agir).

- Accueil (vol ou quai) : portails, dimensions et temps (Entree portail, A ancre, R remonter, X lever, +/- jours, S stase).

- Ecran Portail/Temps (Accueil) : 1/2/3 echo, B briser une boucle, O avant-poste, Gauche/Droite + J relais.

- Fin (vol ou quai) : explorer a pied (fleches / pave numerique, Entree escalier, Espace attendre, Tab chef, clic = marcher).
- Exploration à pied (un seul système pour les mondes, stations, épaves, ruines, grottes, portails, dimensions) : touche Fin en vol ou à quai, ou E sur la carte de surface (F8). Mondes : « Atterrir » ; la surface a ses sites qui mènent aux étages des lieux.
