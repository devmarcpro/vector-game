# DESIGN — jeu spatial vectoriel solo, sandbox total (working title: VECTOR)

## Principes
1. **Solo uniquement.** Pas de réseau, pas de serveur. Tout le budget va à la profondeur de simulation et à la perf.
2. **Sandbox sans objectifs imposés.** Aucun scénario, aucune quête obligatoire, aucune limite artificielle (pas de niveau max, pas de zones verrouillées). Le joueur peut être mineur, commerçant, pirate, explorateur, bâtisseur d'empire ou simple contemplatif.
3. **Systèmes émergents plutôt que contenu scripté.** Peu de règles simples qui interagissent (énergie, matière, besoins, économie, physique), d'où des situations imprévues.
4. **Tout est de la donnée.** Matériaux, modules, vaisseaux, ressources, recettes, personnalités d'équipage, factions : fichiers de données (RON/TOML) chargés au démarrage. Ajouter une arme ou une ressource = ajouter une entrée, pas du code. Ça permet aussi le modding gratuitement.
5. **Simulation déterministe séparée du rendu**, à pas fixe. Intérêt en solo : sauvegardes minuscules, replays, tests reproductibles, accélération du temps fiable.
6. **Tout est dérivé d'une graine.** `hash(seed, coord)` → contenu infini. On ne sauvegarde que les modifications du joueur et de la sim (deltas).
7. **Simulation à niveaux de détail (LOD).** Proche du joueur : équipage individuel, modules, atmosphère. Loin : équations agrégées par colonie/flotte/marché. Promotion et dégradation transparentes.
8. **Perf d'abord.** ECS maison (tableaux contigus, SoA), zéro allocation dans la boucle chaude, multithread via découpage des systèmes (rayon) quand c'est utile.

## Séparation commandes / sim (gardée sans réseau)
Entrées joueur → `Command` → sim. Pas de réseau, mais cette frontière garde : sauvegardes par replay, tests, pause/accélération propres, et IA qui joue avec les mêmes commandes que le joueur (les PNJ ont les mêmes règles que lui).

## Temps hybride
- Cockpit : temps réel.
- Cartes/menus : pause active ou accélération (x1 → x1000+), possible car solo.
- Voyages longs : trajectoires analytiques (orbites, transferts), donc accélération presque gratuite.

## Liberté du joueur (sandbox)
- Possession d'un ou plusieurs vaisseaux, stations, colonies, flottes ; on peut déléguer (IA d'équipage/flotte) ou tout piloter soi-même.
- Modification libre des vaisseaux : modules, disposition, énergie, armement.
- Économie ouverte : commerce, minage, production, piraterie, contrats générés par les marchés (pas de quêtes scriptées).
- Mode créatif/debug accessible : spawn, téléportation, édition de la graine, contrôle du temps. Le joueur choisit son niveau de contrainte (survie stricte ↔ bac à sable permissif) via des réglages de règles.
- Monde réaliste : physique newtonienne, échelles réelles compressées, ressources et chimie plausibles.

## Architecture (workspace Cargo)
| crate | rôle |
|---|---|
| `core` | ECS, maths, hash/RNG déterministe, IDs générationnels |
| `data` | chargement des définitions (matériaux, modules, recettes…) |
| `world` | génération procédurale (galaxie→secteur→système→corps), orbites |
| `sim` | systèmes : physique, énergie, air, cargaison, équipage, minage, économie, colonies |
| `render` | rasteriseur de lignes + grille de texte |
| `game` | fenêtre, entrées, UI cockpit/cartes/menus, sauvegardes (deltas + graine) |

## Rendu
- Engine custom pur : rasteriseur CPU de lignes (Bresenham/Wu), projection perspective, clipping, culling. Framebuffer via `softbuffer`/`pixels`. Pas de moteur de jeu.
- Texte : police bitmap monospace embarquée, grille de caractères ; effet phosphore optionnel.
- Wireframes générés procéduralement (aucun asset).
- Cockpit 3D + panneaux ASCII ; carte galactique/système, gestion, journal.

## Minage (v1, volontairement basique)
Cibler un astéroïde → laser → minerai en soute (masse/volume) → vente/stockage/production. Les ressources et leurs propriétés viennent des fichiers de données.

## Feuille de route
- **M0** Toolchain, workspace, boucle fixe, fenêtre, rasteriseur de lignes, texte. *(cube wireframe + HUD)*
- **M1** Vol newtonien, cockpit, un système solaire généré, carte système.
- **M2** Minage, soute, station, marché minimal, sauvegarde/chargement.
- **M3** Équipage + modules + énergie/air (premier niveau de simulation profonde), éditeur de vaisseau.
- **M4** Galaxie infinie, voyage interstellaire, LOD, accélération du temps.
- **M5** Colonies, production, flottes, commerce, factions/IA.
- **M6+** Couches de réalisme (chimie, biologie, climat, médecine…) ajoutées une à une, à la demande.

## Prérequis (état actuel)
- Rust non installé ; pas de linker MSVC détecté. À installer : `rustup` + Visual Studio Build Tools (C++).

## Questions ouvertes
- Maths : f64 partout (simple, déterministe sur un même binaire/CPU).
- Échelle : coordonnées de secteur `i64` + position locale `f64` (pas de perte de précision à grande distance).
- Format de sauvegarde : binaire compact (`bincode`/`postcard`) versionné.
