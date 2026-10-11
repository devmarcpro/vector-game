# VECTOR

Bac Ã  sable spatial solo pour PC, en lignes vectorielles. Tout est Ã©crit en Rust avec un moteur maison : rasteriseur de lignes sur CPU, police bitmap, simulation dÃ©terministe Ã  pas fixe (30 Hz), galaxie infinie gÃ©nÃ©rÃ©e par fonction de hachage de la graine.

## Lancer
- Windows : lancer `vector-game.exe` depuis le dossier du jeu (le dossier `data/` doit se trouver Ã  cÃ´tÃ©, ou dans un dossier parent).
- Depuis les sources : `cargo run --release -p vector-game`.

## Jouer en 2 minutes
1. **Nouvelle partie** (taper une graine avant de valider si vous voulez une galaxie prÃ©cise). Le tutoriel guidÃ© dÃ©marre ; `F10` l'ignore.
2. Pilotage : `W/S` tangage, `A/D` lacet, `Q/E` roulis, `Espace` poussÃ©e, `F` assistance de vol. `Tab`/`T`/`Y` ciblent, `G` lance le pilote automatique, `D` amarre prÃ¨s d'une station.
3. Ã€ quai : `Haut/Bas` choisit une ressource, `EntrÃ©e` vend, `B` achÃ¨te, `F` fait le plein, `R` rÃ©pare.
4. Minez un astÃ©roÃ¯de (`T`, puis `V` maintenu Ã  moins de 2 km), vendez, achetez de meilleurs modules (`H`) ou une autre coque (`Y`).
5. `L` ouvre la carte de la galaxie (saut FTL), `F2` l'empire (colonies), `F3` la flotte dÃ©lÃ©guÃ©e, `F4` les technologies, `F6` le codex, `F1` l'aide de l'Ã©cran courant, `Ã‰chap` le menu.

Toutes les touches : [docs/CONTROLS.md](docs/CONTROLS.md).

## Ce que contient le jeu
- SystÃ¨me solaire complet Ã  orbites kÃ©plÃ©riennes, gravitÃ© et rÃ©fÃ©rentiels, pilote automatique brachistochrone, accÃ©lÃ©ration du temps jusqu'Ã  Ã—100 000.
- Vaisseau modulaire (74 modules, 21 armes et dÃ©fenses, 15 coques) : bus d'Ã©nergie avec dÃ©lestage, chaleur, radiateurs, santÃ© par module.
- Ã‰quipage simulÃ© (besoins, maladies, 155 Ã©vÃ©nements, journal), Ã©conomie vivante pilotÃ©e par les stocks (le trafic PNJ agit sur les mÃªmes marchÃ©s que vous), factions, rÃ©putation, douane, contrats Ã©mergents, banque.
- Galaxie infinie par secteurs, sauts FTL, scan progressif, anomalies, Ã©ruptions, mÃ©tÃ©oroÃ¯des.
- Colonies (32 bÃ¢timents, 106 recettes), flotte dÃ©lÃ©guÃ©e avec routes, combat (faisceaux, balistique, missiles, mines, abordage, assurance), arbre de 73 technologies.
- RÃ¨gles de partie (13 options) pour jouer en bac Ã  sable, codex gÃ©nÃ©rÃ© depuis les donnÃ©es, tutoriel, aide contextuelle, audio procÃ©dural, options graphiques (CRT, halo, palettes).

## DonnÃ©es et modding
Tout le contenu est dans `data/*.ron` (ressources, modules, coques, recettes, bÃ¢timents, technologies, factions, Ã©vÃ©nements, textes...). Modifier un fichier suffit : le chargement valide les rÃ©fÃ©rences croisÃ©es, les masses des recettes et les graphes de technologies et refuse un fichier incohÃ©rent avec un message clair. Les fichiers `*_extra.ron` sont gÃ©nÃ©rÃ©s par `tools/*.py` (voir [docs/CONTENT.md](docs/CONTENT.md)).

## Outils
```
cargo run --release -p vector-tools --bin data_check      # valide les donnÃ©es et les seuils de contenu
cargo run --release -p vector-tools --bin sim_bench 1 10 100   # stabilitÃ© et vitesse sur 1/10/100 ans
cargo run --release -p vector-tools --bin balance_check   # rapport d'Ã©quilibrage de l'Ã©conomie
vector-game --script scripts/first_flight.txt             # jeu scriptÃ© sans fenÃªtre
vector-game --snapshot image.png cockpit                  # rendu headless (PNG)
cargo test                                                 # toute la suite (â‰ˆ 200 tests)
```

## Architecture
| Crate | RÃ´le |
|---|---|
| `vector-core` | maths (Vec3, Mat3), RNG SplitMix64, hachage de coordonnÃ©es |
| `vector-render` | framebuffer, lignes avec dÃ©coupage, police 5x7, camÃ©ra, PNG |
| `vector-world` | gÃ©nÃ©ration procÃ©durale : Ã©toiles, planÃ¨tes, orbites, galaxie |
| `vector-data` | chargement et validation des `.ron` |
| `vector-sim` | simulation : vol, vaisseau, Ã©quipage, Ã©conomie, colonies, combat, science, sauvegarde |
| `vector-game` | fenÃªtre, Ã©crans, HUD, console, audio, tutoriel, mode script |
| `vector-tools` | `data_check`, `sim_bench`, `balance_check` |

Voir aussi [docs/SIM_MODEL.md](docs/SIM_MODEL.md) (modÃ¨le de simulation), [docs/DECISIONS.md](docs/DECISIONS.md) et [docs/PROGRESS.md](docs/PROGRESS.md).

