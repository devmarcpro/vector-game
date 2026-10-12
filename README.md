# VECTOR

Bac à sable spatial solo pour PC, en lignes vectorielles. Tout est écrit en Rust avec un moteur maison : rasteriseur de lignes sur CPU, police bitmap, simulation déterministe à pas fixe (30 Hz), galaxie infinie générée par fonction de hachage de la graine. Vous jouez l'IA du vaisseau : elle pilote, parle, commande l'équipage et peut prendre le contrôle de n'importe quel équipier.

## Lancer
- Windows : lancer `vector-game.exe` depuis le dossier du jeu (le dossier `data/` doit se trouver à côté, ou dans un dossier parent).
- Depuis les sources : `cargo run --release -p vector-game`.

## Jouer en 2 minutes
Le clavier est en **AZERTY** par défaut (Options > Clavier pour passer en QWERTY).
1. **Nouvelle partie** : sept étapes claires (explications, scénario de départ — Voyageur, Marchand, Explorateur, Hors-la-loi, Colon ou Naufrage —, vaisseau, équipage, galaxie, options, récapitulatif). « Partie rapide » lance le voyageur standard (taper une graine avant de valider si vous voulez une galaxie précise). Le tutoriel guidé démarre ; `Suppr` l'ignore. **`F10` ouvre le menu principal** : huit espaces (pilotage, navigation, vaisseau, équipage, commerce, exploration à pied, colonies, codex/journal) qui regroupent tous les écrans et disent où vous êtes et ce que vous pouvez faire ; `Échap` remonte d'un niveau partout.
2. Pilotage : `Z/S` tangage, `Q/D` lacet, `A/E` roulis, `Espace` poussée, `Maj` boost, `C` frein, `F` assistance de vol. `Tab` cible l'objet le plus proche, `Maj+Tab` une planète, `T` un astéroïde, `Y` une station, `R` un vaisseau ; `G` lance le pilote automatique, `D` amarre près d'une station. **Tout est aussi cliquable** : le menu de pilotage à droite du cockpit ouvre tous les écrans et tient les commandes tant que le bouton est enfoncé.
3. À quai : `Haut/Bas` choisit une ressource, `Entrée` vend, `B` achète, `F` fait le plein, `R` répare ; `T` ouvre la bourse (contrats à terme : `B` hausse, `N` baisse).
4. Minez un astéroïde (`T`, puis `V` maintenu à moins de 2 km), vendez, achetez de meilleurs modules (`H`) ou une autre coque (`Y`). Aucune technologie n'est obligatoire.
5. `L` ouvre la carte de la galaxie : `Entrée` lance un saut (le vaisseau s'éloigne seul des corps massifs, puis saute). `F8` ouvre la surface d'un monde relevé, `E` y lance une expédition à pied. `Entrée` en vol sur un vaisseau ou une planète ciblés ouvre la radio. `F2` l'empire, `F3` la flotte, `F4` les technologies, `F6` le codex, `F1` l'aide de l'écran courant, `Échap` le menu.

Toutes les touches : [docs/CONTROLS.md](docs/CONTROLS.md).

## Ce que contient le jeu
- Système solaire complet à orbites képlériennes, gravité et référentiels, pilote automatique (transfert planifié), accélération du temps jusqu'à ×100 000.
- Vaisseau modulaire (modules, armes, coques) : bus d'énergie avec délestage, chaleur, radiateurs, santé par module ; schémas visuels du vaisseau et des flux d'énergie.
- Équipage simulé : besoins, maladies, **organes** avec santé propre, greffes, prothèses, clonage, **implants cybernétiques**, mutations, génétique et **arbre généalogique**, réseau de relations, **équipement** (arme, armure, outil), journal de bord.
- Économie vivante pilotée par les stocks, **bourse** (courbes, événements du monde, contrats à terme), factions avec **mœurs** et lois locales, réputation, douane, contrats émergents, banque, commerce d'organes.
- Mondes détaillés : biomes, saisons, météo, catastrophes, flore et faune, écosystèmes, **civilisations indigènes** avec qui l'on parle (ton selon leurs mœurs), épidémies, cultures de colonies.
- **Expéditions à pied** en vue roguelike : l'IA contrôle un équipier à la fois, rover, sites d'intérêt, prédateurs.
- Galaxie infinie par secteurs, sauts FTL, scan progressif, anomalies, éruptions, météoroïdes ; radio avec les autres vaisseaux (dialogues pilotés par les données), intrusions contre l'IA.
- Colonies (bâtiments, recettes, politiques, schéma des chaînes de production), flotte déléguée avec routes et carte, combat, arbre de technologies.
- Règles de partie pour jouer en bac à sable libre, codex généré depuis les données (avec rendus en fil de fer qui tournent et liens croisés), tutoriel, aide contextuelle, audio procédural, options graphiques (CRT, halo, palettes), souris partout.

## Données et modding
Tout le contenu est dans `data/*.ron` (ressources, modules, coques, recettes, bâtiments, technologies, factions, organes, implants, équipements, dialogues, civilisations, sites, événements, textes...). Modifier un fichier suffit : le chargement valide les références croisées et refuse un fichier incohérent avec un message clair. Les fichiers générés (`*_extra.ron`, `resources_organs.ron`...) viennent de `tools/*.py` (voir [docs/CONTENT.md](docs/CONTENT.md)).

## Outils
```
cargo run --release -p vector-tools --bin data_check      # valide les données et les seuils de contenu
cargo run --release -p vector-tools --bin sim_bench 1 10 100   # stabilité et vitesse sur 1/10/100 ans
cargo run --release -p vector-tools --bin balance_check   # rapport d'équilibrage de l'économie
vector-game --script scripts/first_flight.txt             # jeu scripté sans fenêtre
vector-game --snapshot image.png cockpit                  # rendu headless (PNG)
cargo test                                                 # toute la suite (≈ 270 tests)
```

## Architecture
| Crate | Rôle |
|---|---|
| `vector-core` | maths (Vec3, Mat3), RNG SplitMix64, hachage de coordonnées |
| `vector-render` | framebuffer, lignes avec découpage, police 5x7, caméra, PNG |
| `vector-world` | génération procédurale : étoiles, planètes, orbites, galaxie, surfaces |
| `vector-data` | chargement et validation des `.ron` |
| `vector-sim` | simulation : vol, vaisseau, équipage, économie, colonies, combat, science, expéditions, sauvegarde |
| `vector-game` | fenêtre, écrans, HUD, console, audio, tutoriel, mode script |
| `vector-tools` | `data_check`, `sim_bench`, `balance_check` |

Voir aussi [docs/SIM_MODEL.md](docs/SIM_MODEL.md) (modèle de simulation), [docs/DECISIONS.md](docs/DECISIONS.md), [docs/ROADMAP.md](docs/ROADMAP.md) et [docs/PROGRESS.md](docs/PROGRESS.md).
