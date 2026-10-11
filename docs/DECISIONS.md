# DECISIONS

Journal des choix de conception non triviaux (le plus récent en bas de chaque thème).

## Outillage et dépendances
- Nom de travail conservé : VECTOR.
- Toolchain : `stable-x86_64-pc-windows-msvc`, éditeur de liens via VS Build Tools 2022.
- Dépendances limitées à : winit, softbuffer (fenêtre + framebuffer), serde, ron, postcard (données et sauvegardes), cpal (audio). Aucune bibliothèque de rendu, de physique, d'ECS ou de texte : rasteriseur de lignes, police bitmap 5x7 et simulation sont maison. rayon n'est pas utilisé : la simulation tient dans le budget en mono-thread (voir `docs/SIM_MODEL.md`).
- `cpal` : seul le chemin "flux de sortie par défaut" est utilisé ; si aucun périphérique n'existe, le jeu tourne en silence (message sur stderr). Le synthétiseur (`audio.rs`) est indépendant du périphérique et testé.

## Moteur et simulation
- Pas fixe de 30 Hz ; l'accélération du temps utilise des sous-pas adaptatifs (`safe_dt`) plutôt qu'un pas plus grand, pour garder la stabilité (orbites képlériennes analytiques, vaisseau intégré en f64 dans le repère inertiel centré sur l'étoile).
- Déterminisme : SplitMix64 et `hash_coord(seed, coord)` partout ; aucune horloge murale dans la simulation. La galaxie est une fonction pure de la graine ; seuls les deltas (systèmes visités, colonies, marchés modifiés) sont sauvegardés.
- Sauvegarde `postcard` avec en-tête magique `VECT` + version (v11). Pas de migration entre versions : une version inconnue est refusée proprement. La version 12 (E2) ajoute l'écologie des territoires de colonie, la dernière colonie vivante touchée par le vaisseau et les pertes dues aux espèces introduites.
- À quai (`docked`), le pas de simulation monte jusqu'à 1 h de jeu (au lieu de 10 min) : les économies et l'équipage n'ont pas besoin de plus de résolution, et la marche de 100 ans en est 30 % plus rapide.
- La scène de départ (`scene.rs`) vit dans la crate `sim` pour que les outils et les tests la partagent avec le jeu ; le fond d'étoiles reste côté jeu (pure vue).

## Contenu
- Données `.ron` dans `data/`. Une partie est générée par `tools/*.py` (reproductibles, idempotents) : `gen_content.py`, `gen_colony.py`, `gen_science.py`, `gen_scale.py`. Les fichiers `*_extra.ron` (événements, recettes, textes) sont fusionnés au chargement ; `gen_scale.py` ajoute aussi des recettes aux bâtiments existants (ré-exécuter `gen_scale.py` après `gen_colony.py`).
- Les recettes dupliquées issues de la montée en charge portent le suffixe `_2` (variantes de rendement/énergie), pour ne pas casser les saves ni les ids existants.
- Classe stellaire D (naine blanche) : prise sur 3 % de la part des naines M pour ne pas déplacer les autres tirages d'une graine donnée (seules les étoiles tirées dans les 3 % de queue changent de classe).
- Les textes sont majuscules/ASCII : la police 5x7 n'a pas d'accents ; les générateurs normalisent (`unicodedata`).

## Économie
- Prix = valeur · modificateur régional · (référence/stock)^0.35, écart achat/vente de 10 %. Les PNJ agissent sur les mêmes marchés que le joueur : pas de prix scriptés.
- La conservation de la masse est validée au chargement pour toute recette de transformation (sorties ≤ entrées).
- `balance_check` rapporte les recettes à marge extrême. Les produits très transformés (nanomatériaux, barres de combustible, diamants) ont un ratio valeur ajoutée élevé par construction ; ils sont limités par le débit des bâtiments et les entrées rares, pas par le prix : documenté plutôt que "corrigé" pour éviter de casser les équilibres existants.

## Règles de partie (bac à sable)
- 13 options (carburant, besoins, mort permanente, dégâts, coûts, recherche, agressivité, événements, carte révélée, ressources infinies, invulnérable, créatif, accélération max), toutes testées sur leur effet dans la simulation (`rules_effects.rs`).
- Toute valeur différente du défaut affiche "PARTIE MODIFIEE" ; la console de debug exige le mode créatif (sauf `help` et `list`).
- Depuis le menu pause les règles s'appliquent en direct ; depuis le menu principal elles s'appliquent à la prochaine partie.

## Interface
- Écrans texte sur grille 8x10 px ; souris uniquement pour la visée en vol (F7, mouvement relatif). F1 = aide contextuelle par écran, F6 = codex généré depuis les données (aucune valeur écrite à la main), tutoriel de 16 étapes piloté par l'état (F10 l'ignore, option pour le désactiver).
- Post-traitement (lignes de balayage, halo, palettes monochromes) en option, appliqué sur le framebuffer avant affichage ; réglages dans `saves/settings.cfg` (clé=valeur, tolérant aux erreurs).
- Mode `--script fichier` : exécute des touches et des assertions sur la vraie logique du jeu, sans fenêtre. Les scripts de `scripts/` sont rejoués par `cargo test`.

## Pilotage (session 2)
- Boost et frein sans état persistant (pas de jauge) pour garder la sauvegarde v11. Le frein passe par le RCS : fiable mais lent à très haute vitesse ; un retournement automatique viendra si le ressenti l'exige.
- D reste lacet ET amarrage (toutes les lettres sont prises) ; l'échec d'amarrage est muet quand aucune station n'est à portée. Limite connue : un lacet à droite près d'une station lente amarre.

## Équilibrage (session 2)
- alance_check signale maintenant : une recette déficitaire seulement si valeur sortie/entrée < 0.5 (énergie à 0.1 cr/kWh), et un exploit seulement si ratio > 6 ET profit > 20 000 cr par bâtiment et par jour (c'est le débit qui plafonne l'exploitation, pas le ratio). Le recyclage perd de la valeur par construction (élimination, pas commerce) : listé à part, non compté.
- Plafonds de débit : nanomatériaux 24 h -> 72 h (v2 : 96 h), barres de combustible 24 h -> 60 h (v2 et thorium : 48 h). Le diamant synthétique était déjà sous le plafond (48 h).
- Conversions corrigées : eau 0.05 -> 0.12 cr/kg (l'eau potable valait moins que la glace brute), fusion de la glace 800 -> 100 kWh, carburant de fusion 20 -> 4 kg de deutérium et 6000 -> 1500 kWh, électrolyse 4800 -> 3000 kWh, alliage de tungstène (40 kg de tungstène au lieu de 600), méthanisation, distillation du deutérium (1.3 kg de D pour 3000 kg d'eau, hors réalisme assumé), biocarburant, soufre.
- Résultat : 36 -> 0 avertissement (objectif : moins de 10). Remarque : gen_colony.py et gen_scale.py écrivent l'ordre des recettes d'un bâtiment depuis un set Python (ordre instable, sans effet de jeu) ; ne pas commiter ce bruit dans uildings.ron.

## Écologie (E2)
- Écosystème régional par colonie plutôt que planétaire : à l'échelle d'un monde (millions de km2) une colonie ne pèse rien ; un territoire de 25 km2 autour de la ville rend la chasse, la cueillette et les invasions perceptibles. L'écologie planétaire (carte de surface) reste sans état : l'équilibre se recalcule depuis la graine à l'ouverture de l'écran.
- La compétition entre producteurs est pondérée 0.5 (au lieu de 1) : avec 1, l'espèce la plus rapide éliminait toutes les autres (exclusion compétitive) et les herbivores s'éteignaient. Les proies ont un refuge (0.2 % de la capacité) : sans lui, les oscillations proies-prédateurs menaient à l'extinction.
## Le joueur est l'IA du vaisseau (précisé le 2026-10-11)
- Le joueur ne joue ni un personnage ni un capitaine : il est l'IA de bord. L'équipage est composé de personnes ; l'IA les dirige, ainsi que le vaisseau, des drones et des véhicules. Textes du jeu : « IA de bord » ou « vaisseau », jamais « capitaine ».
- Conséquences pour la suite : E3 (organes, cybernétique) concerne l'équipage ; E4 : les dialogues passent par les communications du vaisseau, les civilisations réagissent à une IA ; E5 : l'IA ne marche pas, elle envoie équipage, drones et véhicules au sol (choix de l'équipe et du véhicule avant l'expédition) ; E6 : journal de bord de l'IA, le piratage et la guerre électronique visent l'IA elle-même.

## Interfaces visuelles plutôt que listes (demandé le 2026-10-11)
Le joueur trouve un arbre généalogique « beaucoup plus digeste que juste des listes » et veut ce genre d'interface plus souvent. Principe : pour toute information qui a une structure (hiérarchie, réseau, flux, calendrier, séries de valeurs), dessiner un schéma (cases reliées par des traits, arbres, graphiques, cartes, jauges) et garder les listes pour le détail. Déjà faits : arbre généalogique, courbes de la bourse, globe et carte de surface. À refaire en schémas (voir ROADMAP) : arbre des technologies, schéma du vaisseau et de ses modules, chaînes de production des colonies, routes de la flotte sur une carte, diplomatie des factions (alliés et ennemis), flux d'énergie et de chaleur du vaisseau, relations de l'équipage.
Précision (2026-10-11) : l'IA n'a pas de corps propre et peut tout contrôler, personnages compris. Le joueur prend la main sur n'importe quel membre d'équipage, drone, véhicule ou système du vaisseau et peut passer de l'un à l'autre ; les autres agissent seuls selon les ordres reçus, leur volonté et leur loyauté (une personne peut hésiter, refuser ou obéir de mauvaise grâce). Cela structure E5 (vue à pied : contrôle direct d'un personnage à la fois, bascule entre eux, équipe et véhicule choisis avant l'expédition) et E3 (le corps contrôlé est celui de la personne : blessures, organes, implants).
## Souris (2026-10-11)
Les zones cliquables sont déclarées par chaque écran en se dessinant (`Game::hit`, un `RefCell` vidé à chaque image) : un seul code décrit la mise en page et la zone, pas de table de coordonnées séparée à tenir à jour. Une action est une touche (`Act::Key`, donc tout ce qui marche au clavier marche à la souris) ou une fonction avec un numéro ou une position. La barre des commandes, déjà un texte par écran, devient d'un coup cliquable partout en lisant sa première touche de chaque commande. En vol les touches maintenues restent au clavier. La visée souris (F7) capture le pointeur et désactive les clics.

## Sandbox libre : plus de technologie obligatoire (2026-10-11)
Le joueur veut un sandbox totalement libre : coques, modules et bâtiments sont disponibles sans recherche. La règle « Technologies requises pour construire » (`Rules.tech_locks`) est désactivée par défaut ; l'activer rétablit les verrous. L'arbre des technologies reste là comme option de jeu. Format de sauvegarde passé en v14 (nouveau champ des règles).

## Le commerce d'organes passe par les ressources (2026-10-11)
Plutôt qu'un système de vente parallèle, greffons et prothèses sont des ressources (catégories `organes` et `protheses`) : bourse, prix mondiaux, douanes et mœurs fonctionnent tels quels. Leur stock de référence est compté en pièces (1.5 à 5), pas en tonnes. Format de sauvegarde v16.

## Disposition du clavier (2026-10-11)
Le jeu reçoit des touches physiques (winit). Plutôt que de dupliquer chaque commande, `Settings::translate` convertit la touche physique en « lettre imprimée » selon la disposition choisie (AZERTY par défaut), et seules les touches de rotation dépendent de la disposition (`move_keys`). Les textes d'aide sont écrits en QWERTY et réécrits par `Settings::localize`.
