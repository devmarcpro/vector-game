# PROGRESS — VECTOR

## Jalons
- [x] M0 Fondations (toolchain, workspace, boucle, fenêtre, lignes, police, cube+HUD, snapshot PNG) (cube+HUD+snapshot OK ; reste : benchmark rendu, mode --script)
- [x] M1 Vol et cockpit (reste : vues exterieure/carte, son)
- [x] M2 Système solaire complet (reste : ceinture d'astéroïdes générée en corps, rendu anneaux)
- [x] M3 Minage, soute, station, sauvegarde (menu, marche, amarrage ; reste : son)
- [x] M4 Vaisseau modulaire et énergie (reste : support de vie lié à l'équipage en M5)
- [ ] M5 Équipage
- [ ] M6 Galaxie infinie
- [ ] M7 Économie vivante
- [ ] M8 Colonies et empire
- [ ] M9 Combat, menaces, science
- [ ] M10 Contenu et finition

## Journal
- 2026-10-11 : rustup installé (rustc 1.99, cargo 1.99), git init. Build Tools MSVC en cours d'installation (winget, en arrière-plan).
- 2026-10-11 : M0 : MSVC OK, workspace core/render/game, rasteriseur+police 5x7+PNG, cube wireframe, snapshot (docs/snapshot_m0.png), fmt/clippy/test verts.
- 2026-10-11 : M1 : crate sim (vol newtonien, Tsiolkovski, assist, collisions, 6 tests), cockpit+HUD+radar+console. Bench release 1280x720 : 1.9 ms/frame rendu, 0.0002 ms/pas sim.
- 2026-10-11 : M2 : crate world (Kepler, 7 classes stellaires, 12 types de planetes, lunes, stations, noms), sim avec gravite/referentiel/pas adaptatif/pilote auto, carte systeme, temps jusqu'a x100000. 23 tests verts.
- Depot GitHub prive : https://github.com/devmarcpro/vector-game (branche master).
- 2026-10-11 : M3 : data crate (40 ressources, 4 classes d'asteroides, validation), minage laser, soute, marches, amarrage, sauvegarde postcard, menus (principal/pause/station/inventaire). Bench release : rendu 2.9 ms, pas sim 0.036 ms, sauvegarde 1.1 Ko en 0.06 ms.
- 2026-10-11 : M4 : 70 modules + 15 coques (données générées), ShipStats, bus d'énergie avec délestage, chaleur/radiateurs/SCRAM, dégâts par module, chantier naval et écran vaisseau, pilote auto avec gravité. 27 tests sim verts.
