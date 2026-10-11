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
