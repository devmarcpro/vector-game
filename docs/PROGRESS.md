# PROGRESS — VECTOR

## Jalons
- [x] M0 Fondations (toolchain, workspace, boucle, fenêtre, lignes, police, cube+HUD, snapshot PNG) (cube+HUD+snapshot OK ; reste : benchmark rendu, mode --script)
- [x] M1 Vol et cockpit (reste : vues exterieure/carte, son)
- [x] M2 Système solaire complet (reste : ceinture d'astéroïdes générée en corps, rendu anneaux)
- [x] M3 Minage, soute, station, sauvegarde (menu, marche, amarrage ; reste : son)
- [x] M4 Vaisseau modulaire et énergie (reste : support de vie lié à l'équipage en M5)
- [x] M5 Équipage (30 événements ; objectif final 150 -> M10)
- [x] M6 Galaxie infinie
- [x] M7 Économie vivante (reste : escortes/convois de la flotte joueur en M8, combat pirates en M9)
- [x] M8 Colonies et empire (reste : individus détaillés des colonies, convois armés en M9)
- [x] M9 Combat, menaces, science (reste : IA tactique plus riche, escorte de flotte)
- [x] M10 Contenu et finition (voir docs/ACCEPTANCE.md)

## Journal
- 2026-10-11 : rustup installé (rustc 1.99, cargo 1.99), git init. Build Tools MSVC en cours d'installation (winget, en arrière-plan).
- 2026-10-11 : M0 : MSVC OK, workspace core/render/game, rasteriseur+police 5x7+PNG, cube wireframe, snapshot (docs/snapshot_m0.png), fmt/clippy/test verts.
- 2026-10-11 : M1 : crate sim (vol newtonien, Tsiolkovski, assist, collisions, 6 tests), cockpit+HUD+radar+console. Bench release 1280x720 : 1.9 ms/frame rendu, 0.0002 ms/pas sim.
- 2026-10-11 : M2 : crate world (Kepler, 7 classes stellaires, 12 types de planetes, lunes, stations, noms), sim avec gravite/referentiel/pas adaptatif/pilote auto, carte systeme, temps jusqu'a x100000. 23 tests verts.
- Depot GitHub prive : https://github.com/devmarcpro/vector-game (branche master).
- 2026-10-11 : M3 : data crate (40 ressources, 4 classes d'asteroides, validation), minage laser, soute, marches, amarrage, sauvegarde postcard, menus (principal/pause/station/inventaire). Bench release : rendu 2.9 ms, pas sim 0.036 ms, sauvegarde 1.1 Ko en 0.06 ms.
- 2026-10-11 : M4 : 70 modules + 15 coques (données générées), ShipStats, bus d'énergie avec délestage, chaleur/radiateurs/SCRAM, dégâts par module, chantier naval et écran vaisseau, pilote auto avec gravité. 27 tests sim verts.
- 2026-10-11 : M5 : crew.rs (génération, besoins, maladies, paie, postes, 30 événements data-driven, journal, recrutement), écrans équipage/recrutement/journal, sauvegarde v3. 37 tests sim verts.
- 2026-10-11 : M6 : galaxie par secteurs (spirales, Poisson), peek_star, FTL (charge/transit/arrivée, incidents), deltas de systèmes, scan progressif 0-3, carte galactique avec brouillard. 42 tests sim + 14 world verts. Génération système < 1 ms.
- 2026-10-11 : M7 : factions + territoires, industries et prix régionaux, réputation/douane/contrebande, contrats émergents, banque, trafic PNJ (traders/mineurs/patrouilles/pirates) agissant sur les marchés, stations multiples. 57 tests sim verts.
- 2026-10-11 : M8 : 40 recettes + 32 bâtiments + 5 politiques (data générée, masse conservée), colonies (pop/moral/stocks/énergie/construction/émeutes), astroports, flotte déléguée avec routes inter-systèmes, écrans Empire/Colonie/Flotte, sauvegarde v8. 71 tests sim verts.
- 2026-10-11 : M9 : 21 armes/défenses, combat complet (faisceaux, balistique, missiles, mines, boucliers/blindage, IA pirates/patrouilles, épaves, abordage, assurance), arbre de 73 technologies avec verrouillage, 12 anomalies, éruptions/météoroïdes/radiations, 38 maladies. Tests de raccourcis clavier ajoutés (ils ont révélé des raccourcis L/F2/F3/O mal câblés depuis M6, corrigés). 88 tests sim verts.
- 2026-10-11 : M10 (a) : règles de partie (13 options testées), menus principal/pause étendus, écrans Règles/Options/Codex, console réservée au mode créatif, accélération max, mort permanente, marqueur PARTIE MODIFIEE.
- 2026-10-11 : M10 (b) : montée en charge du contenu (`tools/gen_scale.py`) : 155 événements, 106 recettes, 300+ gabarits de texte, classe stellaire D ; codex F6 (11 catégories, 350+ entrées générées) ; options graphiques (CRT, halo, palettes) ; outils `data_check`, `sim_bench`, `balance_check` (crate `tools`).
- 2026-10-11 : M10 (c) : tutoriel guidé de 15 étapes, aide contextuelle F1 sur tous les écrans, audio procédural (cpal), mode `--script` avec scripts de régression rejoués par `cargo test`.
- 2026-10-11 : M10 (d) : README, docs/ACCEPTANCE.md, paquet `dist/VECTOR.zip` (`tools/package.ps1`). `sim_bench 100` : 100 ans en 26 s, stable.
- 2026-10-11 : Session 2, environnement (Rust 1.99, Build Tools 2022, gh) installé sur la nouvelle machine : build, 147 tests, clippy, data_check, first_flight OK au départ. Étape 0 / pilotage (a) : boost (Maj) et frein relatif (C maintenu), rotation des coques x1.3, amarrage D silencieux hors portée. 150 tests verts.
- 2026-10-11 : Étape 0 / pilotage (b) : poussière spatiale en traînées (le fond d'étoiles est à l'infini, rien ne défilait), FOV élargi au boost, visée souris F7 (DeviceEvent::MouseMotion, curseur capturé en vol), tutoriel à 16 étapes, aide F1 mise à jour.
- 2026-10-11 : Étape 0 / jouabilité : scripts de régression full_loop (miner, vendre, remplacer un module, fonder une colonie), ftl_jump, combat, colony, save_reload ; commandes de script near/give/mark/deepspace/pirate/aim/engage. Blocage corrigé : Entrée sur un emplacement occupé du vaisseau refusait d'installer ; swap_module remplace en une étape avec annulation. Constats : FTL refusé près d'un corps massif (message clair) ; 20 s de minage = environ 68 cr (à équilibrer).
- 2026-10-11 : Étape 0 / lisibilité : couleur par type de corps (relevé niveau 2), halo d'atmosphère, anneaux de géantes, réticule de cible ; barre des commandes permanente contextuelle (option, test de largeur par écran) ; tutoriel hors vol en haut d'écran.
- 2026-10-11 : Étape 0 / équilibrage : balance_check 36 -> 0 avertissement (seuil de profit par jour, recyclage séparé, plafonds de débit nanomatériaux/barres/thorium, conversions d'énergie corrigées), 147+ tests verts.
- 2026-10-11 : Étape 0 / accents : police 5x7 avec accents (tests), 	ools/accents.py appliqué aux données (18 fichiers), normalisation ASCII retirée de gen_scale. Performance (drift) : 1,06 M de pas par an en vol libre, pas moyen 29,6 s, 16 us par pas (docked : 8 760 pas) ; le jeu plafonné à x100 000 reste sous 1 ms par image, seul le banc de 100 ans en vol libre est lent (~30 min) ; compteur Sim::steps ajouté.
- 2026-10-11 : E1 (a) planètes détaillées : world::surface (carte 128x64 par bruit 3D sur la sphère, niveau de mer par fraction d'eau), 20 biomes data-driven (data/biomes.ron, validation de couverture complète, catégorie BIOMES du codex), sim::planet (biomes, saisons par inclinaison axiale, météo par jour et région, catastrophes rares, placement des villes des colonies), écran de surface F8 (5 couches, curseur, globe fil de fer à courbes de niveau), script surface.txt, tutoriel à 17 étapes. Aucun état sauvegardé ajouté (v11 inchangée).
- 2026-10-11 : E1 (b) : catastrophes appliquées aux colonies (population, moral, caisse, journal de colonie), caches de cartes non sauvegardés ; 171 tests, sim_bench 100 ans stable (19 s).
- 2026-10-11 : E2 faune et flore : 58 espèces data-driven (régimes validés), chaîne trophique par biomasse, territoire écologique par colonie sauvegardé (sauvegarde v12), politiques cueillette/élevage et quarantaine, espèces envahissantes via l'amarrage, pertes de production, couche VIE et liste des espèces sur l'écran de surface, catégories FLORE et FAUNE du codex. 124 tests.

- 2026-10-11 : Bourse : 20 événements du monde data-driven (régionaux et globaux) qui font bouger les prix, calendrier déterministe sans état sauvegardé, dérive de marché, écran Bourse (touche T à quai) avec courbes sur 30/90/180 jours, bandes d'événements et actualités, annonces dans le jeu, catégorie EVENEMENTS du codex. Le joueur est l'IA du vaisseau (textes mis à jour).
- 2026-10-11 : Pilier mœurs et moralité (couche 1) : 8 axes de valeurs, profils des 8 factions, 8 étiquettes morales de marchandises (demande et interdits), prix de marché et contrebande qui en découlent, affichage dans la Bourse et le codex.
- 2026-10-11 : E3.1 génétique et généalogie : 20 gènes (14 additifs, 6 prédispositions), identités familiales déterministes (génération dans la clé de famille), génomes recombinés (Mendel, mutations), noms de famille patrilinéaires, parents recrutables (frères, sœurs, cousins), deuil familial, risque héréditaire de maladie, écran arbre généalogique (G à l'équipage) avec onglet des gènes, catégorie GENES du codex, sauvegarde v13.
- 2026-10-11 : Passe d'interfaces visuelles (1) : arbre des technologies en graphe (colonnes par profondeur de prérequis, bandes par domaine, traits des prérequis vers les déblocages, navigation spatiale, détail), à la place de la liste ; tests de mise en page et de navigation.
- 2026-10-11 : Souris partout : zones cliquables déclarées au dessin (clic, double clic, survol), molette, clic droit = retour, barre des commandes cliquable sur tous les écrans, menus, listes (station, soute, vaisseau, chantier, équipage, recrutement, codex, options, règles), arbre des technologies, bourse (périodes, lignes), carte de surface, carte galactique, objets du cockpit et de la carte système, arbre généalogique. Tests de bout en bout.

- 2026-10-11 : Passe visuelle : schéma du vaisseau (écran H), chaque emplacement est une boîte reliée au bus d'énergie, colorée par santé/alimentation, cliquable.

- 2026-10-11 : Passe visuelle : diagramme des chaînes de production d'une colonie (touche F dans l'écran colonie) : stocks consommés -> bâtiments/recettes -> stocks produits, fils rouges si un intrant manque.

- 2026-10-11 : Passe visuelle : écran des factions (X en vol) : graphe en anneau des alliances/ennemis, moeurs sur 8 axes, interdits moraux, contrebande, réputation ; cliquable.

- 2026-10-11 : Plan de soute visuel (écran I) : une case = un volume égal, remplissage de la case selon la densité, couleur par catégorie, jauges volume/masse, blocs cliquables.

- 2026-10-11 : Passe visuelle : réseau des relations de l'équipage (V dans l'écran équipage) : anneau de boîtes, fils verts/rouges selon l'amitié, pointillés ambre pour la parenté, liens détaillés de la personne choisie.

- 2026-10-11 : Passe visuelle : carte de la flotte (M dans l'écran flotte) : systèmes à leur vraie position, colonies, routes (vert actif, gris arrêté, ambre choisie).

- 2026-10-11 : Sandbox libre : plus besoin de rechercher des technologies pour acheter coques, modules et bâtiments (règle `tech_locks` désactivée par défaut, activable dans les règles). Sauvegarde v14.

- 2026-10-11 : Passe visuelle : schéma des flux d'énergie et de chaleur (écran vaisseau H) : producteurs -> bus/batterie -> consommateurs (rouge si éteint), chaleur -> radiateurs avec jauge.

- 2026-10-11 : Passe visuelle (fin) : liens croisés du codex (« VOIR AUSSI » : clic ou chiffres 1-9, Retour arrière pour revenir). La passe visuelle est terminée.

- 2026-10-11 : E3.2 Organes : 10 organes (`data/organs.ron`, validés) avec intégrité par personne ; les maladies usent les organes qu'elles ciblent, les autres guérissent (plus vite avec médecin/infirmerie), un organe vital à zéro tue, un organe défaillant coûte des effets (vitesse de travail, stress...), santé globale plafonnée par l'état des organes ; panneau ORGANES dans l'écran équipage, catégorie ORGANES du codex. Sauvegarde v15.

- 2026-10-11 : E3.3 (1re partie) Greffes, prothèses et commerce d'organes : 20 nouvelles marchandises (greffons et prothèses, générées par `tools/gen_organs.py`) vendues aux marchés comme tout le reste (quelques pièces en stock, prix très élevés), interdites par les mœurs « corps » sacralisé (étiquette Greffons de `ethos.ron`) ; opérations `implant` (greffe avec risque de rejet selon le chirurgien, prothèse fixe à 85 %, jamais usée) et `harvest` (sur les morts ; sur un vivant seulement un organe non vital, moral et loyauté en chute chez la victime et l'équipage) ; touche O de l'écran équipage. Sauvegarde v16.

- 2026-10-11 : E3.3 (2e partie) Cybernétique : 12 implants (`data/cybernetics.ron`, validés, pièces vendues comme marchandises) en 6 emplacements ; effets sur traits et compétences, besoin de courant (support de vie) et de pièces détachées, contrainte constante sur le porteur, arrêt (et stress) sans courant ni pièces, piratage (`hack_implants`) pour les implants piratables ; installation/retrait à l'hôpital ou avec médecin + infirmerie ; catégorie IMPLANTS du codex. Sauvegarde v17.

- 2026-10-11 : E3.4 Mutations : instabilité génétique par personne (la radiation l'augmente, le temps la réduit), mutations d'un allèle au hasard au-delà de 15 % (journal), cancers précoces possibles, mutagène (marchandise, touche M de la liste des organes) qui mute un gène immédiatement. Limite connue : l'hérédité d'une mutation n'est pas modélisée (les parents d'un recruté sont recalculés depuis la généalogie canonique). Sauvegarde v18.

- 2026-10-11 : E4 (1re partie) Dialogues radio : `data/dialogues.ron` (15 noeuds, conditions et effets validés au chargement), arbres par type de vaisseau (cargo, mineur, patrouille, pirate), ton formel/cordial/méfiant/neutre selon les mœurs de la faction, options conditionnées (cargaison, crédits, réputation, valeurs), effets (réputation, dons, nouvelles du monde, rumeurs de prix, étoile voisine, tribut, bluff, pot-de-vin refusé par les factions honorables) ; écran COMMUNICATIONS (Entrée en vol sur un vaisseau ciblé), catégorie COMMUNICATIONS du codex.

- 2026-10-11 : Clavier AZERTY par défaut (option Clavier : AZERTY/QWERTY dans les options) : les touches sont lues par position puis nommées par leur lettre (A/Q, W/Z et M permutés sur AZERTY) ; rotation Z/S, Q/D, A/E sur AZERTY (W/S, A/D, Q/E sur QWERTY) ; textes d'aide et tutoriel réécrits selon la disposition ; le tribut aux pirates se paie par la radio (Entrée) sur AZERTY car Z sert à piloter. Option Langue (FRANCAIS seul pour l'instant, prête pour les traductions). Les scripts restent en QWERTY.

- 2026-10-11 : E5 (1re partie) Vue de marche roguelike : touche E sur l'écran de surface (case de terre, à proximité de la planète) : choix de l'équipe et du rover, puis carte 96x52 générée de la case (sol, rochers, mares selon l'humidité, plantes et animaux du biome d'après les espèces), tour par tour (1 tour = 60 s de jeu, le temps du monde passe), l'IA contrôle un équipier à la fois (Tab ou clic sur un nom), foncer sur un animal l'attaque (compétence combat), G cueille, les prédateurs poursuivent et blessent (santé et un organe, un organe vital à zéro tue), butin limité par la charge (30 kg par personne, +250 kg avec le rover), R rentre au vaisseau : le butin va en soute. Clic sur la carte = un pas vers la case. Pas sauvegardé : l'expédition est interne à la session.

- 2026-10-11 : E5 (suite) Sites d'intérêt (`data/sites.ron`, validés) : campement, épave, ruine, grotte, source chaude, balise ; 2 à 4 par carte loin du point de chute, fouillés une fois avec G (butin, embuscade ou éboulement possible, la source soigne l'équipe) ; catégorie SITES du codex.
