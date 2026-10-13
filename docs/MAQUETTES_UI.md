# MAQUETTES D'INTERFACE (refonte visuelle des menus et du cockpit)

Ces maquettes montrent ce que pourraient devenir les ecrans de gestion et la vue pilotage
**avant** de refondre quoi que ce soit. Elles sont dessinees par le vrai moteur de rendu du jeu
(rasteriseur CPU, police 5x7 du jeu), avec les **vraies donnees d'une partie** (`new_game` plus
quelques jours simules), a 1280x720. Rien dans le jeu normal ne les utilise.

- Code : `crates/game/src/mock_ui.rs` (boite a outils + menus) et `crates/game/src/mock_cockpit.rs`
  (cockpit). **A supprimer** apres decision : les deux fichiers, les deux `mod` et le bloc
  `mock_ui::resolve / after_setup / render` de `snapshot()` dans `main.rs`.
- Chaque maquette a son pendant actuel pour comparer : `mock_<nom>` (maquette) et `now_<nom>`
  (ecran actuel, meme partie, meme etat).
- Commandes : `vector-game --snapshot shots/mock_colony.png mock_colony` (idem `now_colony`).
  Les PNG sont dans `shots/` (non versionne).

## Ce que les maquettes changent, en un coup d'oeil

| Probleme actuel | Reponse des maquettes |
|---|---|
| Un seul style : texte vert sur noir, tout au meme niveau | Un theme par ESPACE du menu F10 (couleur d'accent, fond teinte, grille discrete), meme famille partout |
| Rien ne regroupe les infos | Panneaux a titre (bandeau + filet fin + coins), tableaux a lignes alternees, ligne selectionnee en surbrillance |
| Les nombres sont `####----` en ASCII | Jauges segmentees colorees par seuils (vert/ambre/rouge), minces barres, sparklines, pastilles de statut |
| Les touches sont du texte dans une ligne | Barre de commandes : touche en pave plein, libelle a cote, **chaque commande reste cliquable** |
| Beaucoup d'etats en texte | Pastilles (ON/OFF, REPOS, AU TRAVAIL), voyants a halo, avatars a initiales, silhouette coloree |

Regle respectee : dans les **menus** les boutons restent du texte cliquable (soulignes au survol) ;
dans le **cockpit** seulement, ce sont de vrais organes d'instrument.

## Liste des images

Menus (maquette / actuel) :

| Maquette | Actuel | Contenu |
|---|---|---|
| `shots/mock_colony.png` | `shots/now_colony.png` | Ecran colonie en panneaux : 6 tuiles (population, moral, caisse, energie, emplois, productivite), batiments, stocks, politiques (selecteur a crans), construire, energie et reseau, journal |
| `shots/mock_hub.png` | `shots/now_hub.png` | Menu F10 : bandeau « vous etes » + jauges, bandeau « QUE PUIS-JE FAIRE ICI » cliquable, 8 espaces en cartes colorees (un fond par espace), page de l'espace en liste stylee (touche, description, raison en rouge si indisponible) |
| `shots/mock_crew.png` | `shots/now_crew.png` | Equipage, onglet LISTE : cartes d'equipage a avatars et 4 barres, fiche (jauges, competences, traits en pastilles, organes, equipement), quarts et relations |
| `shots/mock_crew_med.png` | `shots/now_crew_med.png` | Equipage, onglet MEDECINE : silhouette coloree par partie, signes vitaux en jauges, blessures, soins cliquables |
| `shots/mock_surface.png` | `shots/now_surface.png` | Carte de surface : chaque case teintee du sol, curseur en cadre, onglets de couches, panneaux (position, monde, sol et geologie, gisements) |
| `shots/mock_station.png` | `shots/now_station.png` | Marche : onglets, cartes de contexte, tableau trie avec stock en barre, achat/vente, ecart au cours de base (triangles), courbe 30 jours par ligne, fiche du bien (modele filaire qui tourne, cours, actions) |
| `shots/mock_dash.png` | `shots/now_dash.png` | Tableau de bord a 9 panneaux : jauges par zones, modele du vaisseau qui tourne, modules, valeur nette reelle, nouvelles, alertes, esprit de l'IA |

Cockpit (vol libre en orbite basse, plan de navigation pret, cible, contacts, carburant et coque
usés pour que les voyants disent quelque chose) :

| Image | Mise en page |
|---|---|
| `shots/mock_cockpit_a.png` | **A** : console basse large, type avion |
| `shots/mock_cockpit_b.png` | **B** : panneaux lateraux + console basse compacte |
| `shots/mock_cockpit_c.png` | **C** : HUD de verriere minimal, console repliee en bandeau |
| `shots/mock_cockpit_c_open.png` | **C** console deployee |
| `shots/now_cockpit_a.png` | le cockpit actuel, meme scene |

Approche d'atterrissage (pilote de descente engage, 262 m, impact dans 3 s) :
`shots/mock_cockpit_land.png` (A), `shots/mock_cockpit_land_b.png` (B), `shots/mock_cockpit_land_c.png`
(C), `shots/mock_cockpit_land_c_open.png` (C deployee), `shots/now_cockpit_land.png` (actuel).
En approche, l'ecran de navigation devient l'ecran **DESCENTE** (altitude radar, impact, point
d'allumage, TWR, pente, site) et l'ecran de plan devient **PILOTE ET AIDES** (boutons pilote, site
plat, vue sol, accelerateur) : meme emplacement, contenu selon la situation.

## Le cockpit en detail (commun aux trois mises en page)

La scene 3D/vectorielle (`hud::draw_cockpit_scene`) est rendue dans un framebuffer de la taille de
la verriere puis copiee ; les zones cliquables qu'elle declare (corps, vaisseaux) sont decalees a
leur place. Autour : une console faite de modules independants, que les trois mises en page placent
differemment.

| Organe | Realisation | Action (cliquable) |
|---|---|---|
| Interrupteurs a bascule (TRAIN, BOUCLIER, RADAR, LEURRE, LUMIERES, PILOTE AUTO) | levier haut/bas, voyant a halo, libelle grave | `Numpad3`, *(maquette)*, `.`, `,`, *(maquette)*, `G` |
| Bloc SAS (COUPE, MAINTIEN, PROGRADE, RETROGRADE, NORMAL, RADIAL, CIBLE, VERTICALE) | boutons-poussoirs lumineux, l'actif est allume et enfonce | `0` a `9` |
| Manette de poussee 0-100 % | fente graduee, bande lumineuse, poignee ; trim (decor) | clic sur un cran = `set_throttle` |
| Horizon artificiel | calcule de l'attitude reelle contre l'horizon local (monde en dessous, sinon plan du systeme), echelle de tangage, echelle de roulis | |
| Cap | bande de compas N/E/S/O | |
| Vitesse | cadran, echelle automatique, 3 selecteurs SOL / ORB / CIB | selecteurs |
| Variometre | cadran, zero a 9 h, echelle automatique | |
| Altimetre radar, vitesses vert./horiz. | fenetres numeriques | |
| Systemes | 8 colonnes de LED (carburant, delta-v, batterie, coque, bouclier, temperature, radiation, charge G), couleur par zone, etiquette verticale, clignotement en alarme | |
| Cible | nom, type, distance, approche (fleche verte si l'ecart se ferme), radiation, coque/bouclier d'un vaisseau ; boutons AMARRER / ATTERRIR / APPELER allumes quand c'est possible | `D`, `Pav4`, `Entree` |
| Radar / scanner | echelle logarithmique (1 km a 1 Mkm), balayage anime, contacts par couleur (station, planete, PNJ), cible entouree, bande verticale = hauteur | clic = cible suivante (`Tab`) |
| Ecran de navigation | schema de l'orbite avec le vaisseau, peri/apoapsis, periode, excentricite, prochain evenement, **temps avant allumage** | |
| Ecran de plan automatique | etat (pret / en cours / pause / arrete), ordre courant et suivants | LANCER/PAUSE, ECRAN, TRAJ (`nav_click_cockpit`) |
| Voyants d'alerte | COLLISION, CARBURANT, SURCHAUFFE, RADIATION, COQUE, ENERGIE, BOUCLIER, AIR/VIE ; ambre fixe, rouge qui clignote | |
| Touches de console | CARTE, GALAXIE, SOUTE, EQUIPAGE, JOURNAL, CODEX, VAISSEAU, NAV AUTO, TECH, MENU | `M L I K J F6 H Pav* F4 F10` |

Anime : voyants rouges qui clignotent, balayage du radar, cible entouree qui pulse, aiguilles et
bandes qui suivent la simulation. Les interrupteurs BOUCLIER et LUMIERES et le trim n'existent pas
encore dans la simulation : la maquette les bascule seulement a l'ecran (message « COMMANDE A CREER »).

### Les trois mises en page

**A : console basse large (type avion)**. Verriere 1280x380 en forme de pare-brise (montants,
bossage du pare-soleil), console de 340 px : systemes | commandes (interrupteurs, SAS, voyants) |
instruments de vol | radar, navigation, cible, plan | manette.
- Pour : tout est visible en permanence, le plus lisible et le plus « vrai cockpit » ; chaque
  organe a sa place fixe (memoire musculaire).
- Contre : la verriere ne garde que 53 % de la hauteur ; il faut qu'elle reste agreable a regarder
  (un monde qui occupe le bas de l'image, comme sur la maquette, aide).

**B : panneaux lateraux + console basse compacte**. Verriere 780x458, colonnes de 238 px de part et
d'autre (systemes, interrupteurs, SAS, navigation / radar, plan, cible, alertes), console basse
reduite aux instruments de vol, a la manette et a 8 touches.
- Pour : les panneaux lateraux sont lisibles sans cacher le bas de l'image, bonne densite,
  grande lisibilite des alertes (grosses touches a droite).
- Contre : verriere la plus petite (environ 40 % de l'ecran) ; l'oeil fait de longs trajets.

**C : HUD minimal + console repliable**. Scene plein ecran, HUD de verriere (bande de cap, bandes
de vitesse et d'altitude, echelle de tangage, reticule) et un bandeau bas de 34 px (4 jauges, 8
voyants, 4 touches, poignee CONSOLE). La poignee deploie la console (version compacte de A).
- Pour : immersion maximale, tres proche de ce qui existe (peu de risque), la console est la
  quand on en a besoin (atterrissage, amarrage, combat) et on la replie pour voler a vue.
- Contre : en vol le joueur ne voit plus les interrupteurs ni le SAS ; un clic de plus ; le HUD
  additif se lit moins bien sur un sol clair (ombrages sous les chiffres dans la maquette).

### Recommandation

**Choisir A pour la vue pilotage, avec le repli de C** : A est la seule qui donne vraiment la
sensation de tableau de bord avec de vrais organes (c'est ce qui est demande), et le repli en
bandeau (touche ou poignee) rend la verriere plein ecran quand on veut voyager. B est la plus
simple a lire mais sacrifie trop de verriere. Si la verriere doit rester reine, partir de C et
ajouter les modules de A dans la console deployee (les modules sont les memes).

Attention a la taille : les maquettes sont dessinees pour 1280x720, alors que la fenetre du jeu
s'ouvre en 1024x576 (la barre de commandes actuelle le suppose). Il faudra soit une mise a
l'echelle d'ensemble de la console, soit deux gabarits (>= 1280 de large : A ; en dessous : B ou C).

## Cout d'implementation estime (en jours de travail, avec tests)

| Chantier | Estimation | Remarque |
|---|---|---|
| Boite a outils commune (theme par espace, panneaux, tableaux, jauges, sparklines, pastilles, barre de commandes, onglets) | 2 a 3 j | base de tout le reste ; la maquette en contient deja une version |
| Colonie (ecran principal) | 1,5 j | + 2 j pour les 4 onglets de travaux (industrie, agriculture, ville, logistique), chaines, sante, projets |
| Menu F10 | 1 j | structure deja separee des ecrans (`hub.rs`) |
| Equipage : liste et fiche | 2 j | + 1,5 j medecine (silhouette), + 2 j pour cryo, organes, equipement, reseau, arbre, recrutement |
| Surface | 1 j | + 1 j globe et biosphere |
| Station : marche | 1,5 j | + 2 j noir, bourse, courtage (9 onglets) |
| Tableau de bord | 1,5 j | `dash_view.rs` garde ses donnees ; seul le rendu change |
| **Cockpit A** | **7 a 9 j** | instruments (cadrans, horizon, radar, ecrans) 3 j ; console et clics 2 j ; fenetre, etats reels (descente, amarrage) et tests 2 j ; reglages visuels 1 a 2 j |
| Cockpit B (apres A) | +2 j | memes modules, autre placement |
| Cockpit C (repli, HUD de verriere) | +3 a 4 j | le HUD de verriere est nouveau ; le repli reutilise A |
| Nouvelles commandes de simulation (bouclier on/off, lumieres, trim, mode du cadran de vitesse memorise) | 1 a 2 j | le reste est cable sur des touches existantes |
| Adaptation a plusieurs tailles de fenetre | 2 j | voir l'avertissement ci-dessus |

Total pour « toute l'interface » : environ 25 a 30 j ; pour « le cockpit seulement » : 8 a 10 j.
Une refonte progressive (boite a outils, puis un ecran a la fois) reste jouable a tout moment :
chaque ecran restyle ne touche que son fichier `*_view.rs`.

## Ce qui serait reutilise

- `landing_view.rs` : marqueurs (PRO, RET, VERT., SITE, IMPACT) separes en `draw_markers` pour
  etre dessines dans la verriere ; `Readout` (descente), `sas_key`, `landing_gear_key`,
  `throttle_key`, `pilot_key`, `find_site`. Le panneau texte de `draw` est remplace par les ecrans
  DESCENTE et PILOTE.
- `hud.rs` : `draw_cockpit_scene` (la scene sans le radar), `body_label`, `kind_label`,
  `kind_color`, `npc_color`, `fmt_dist`. `draw_status` disparait au profit des instruments.
- `nav_view.rs` : `with_pred` et `event_label` (evenements predits), `nav_click_cockpit`
  (LANCER/PAUSE/ECRAN/TRAJ) ; le panneau `draw_panel` est remplace par l'ecran de plan.
- `dash_view.rs` : `alerts()` (voyants et alertes), donnees de l'esprit de l'IA, des finances et
  des colonies ; son decoupage en neuf panneaux sert de modele au tableau de bord restyle.
- `hub.rs`, `context_menu.rs` (`suggestions`), `ship_model.rs` (`turntable`), `wire_shape.rs`,
  `surface_view.rs` (`cell`, `lat_lon`) : donnees et dessins deja existants, juste re-habilles.

Modifications faites hors des fichiers de maquette (comportement inchange) :
`hud::draw_cockpit` est scinde (`draw_cockpit_scene` + radar), `landing_view::draw` est scinde
(`draw_markers` + panneau), quelques fonctions passent en `pub(crate)` (`nav_view::with_pred`,
`event_label`, `surface_view::cell`, `lat_lon`, `RELIEF_M`), et `snapshot()` accepte les noms
`mock_*` et `now_*`.
