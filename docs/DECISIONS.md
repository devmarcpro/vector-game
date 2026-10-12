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

## Pilote automatique : plan analytique plutôt que contrôleur (2026-10-11)
Le contrôleur en vitesse réagissait mal aux planètes qui bougent, aux grands pas de temps et aux puits de gravité (réservoirs vidés, traversée de planètes). Le pilote suit désormais un plan calculé (le vaisseau est placé selon le profil, la poussée brûle le vrai carburant). Compromis : pas de collisions pendant le transfert lui-même, et la physique reprend à l'arrivée (vaisseau garé). Un obstacle sur la ligne droite n'est pas évité : choix assumé pour un bac à sable.

## Un seul moteur d'exploration (E9)
La marche planétaire (E5) et l'exploration roguelike des lieux (E9) faisaient double emploi (deux écrans, deux boucles de tour, deux butins). Elles sont fusionnées dans `rogue.rs` : une surface est un étage particulier (carte du monde, air, climat, faune, plantes, sites), les lieux sont des étages générés, les sites de surface sont des entrées de lieux. Conséquences : plus de touche G (le butin se ramasse en marchant), équipe groupée au lieu de positions séparées (l'équipier de tête compte à plein dans le combat), tout lieu a la même interface, la même sauvegarde et les mêmes règles de portage. Principe pour la suite : avant d'ajouter un système, chercher s'il existe déjà un équivalent à étendre.

## Chacun chez soi : la propriété décide de ce qu'on peut faire
Un service n'existe que là où il a un sens. Station étrangère : marché, chantier, banque, bourse, courtage, contrats, recrutement, marché aux bêtes (acheter et vendre des animaux), casino, infirmerie ; on n'y entre à pied que si l'on y est amarré. Colonie du joueur : gestion, recrutement, infirmerie, portail relais. Vaisseau du joueur : pont, soute, équipage, ingénierie, laboratoire, carte, ancres temporelles, et selon les modules l'infirmerie ou les **enclos** (élevage, reproduction, abattage ; une serre est nécessaire). Les animaux vivent à bord ; on les achète et les vend à un éleveur de station. Pas d'élevage ni d'ancres dans une station qui n'est pas à soi.

## Une chose = une représentation (E10)
Un monde se dessine de la même façon partout : le globe topographique (points par biome, courbes de niveau, côtes, graticule) est `globe.rs`, utilisé par l'écran de surface (F8) et par la vue de pilotage dès qu'un monde solide relevé est assez gros à l'écran (la face nocturne est assombrie d'après la position de l'étoile). Même principe pour les nouvelles : un seul fil (`news.rs`), rempli par la politique des secteurs (`politics.rs`), les incidents d'entreprises (`incidents.rs`) et la bourse ; l'univers est infini, donc la portée est locale (secteurs de 240 al, écoute de 2 secteurs autour du vaisseau) et ce qui est loin arrive en retard (4 al par jour au-delà du secteur) ; chaque fait a un effet réel sur les prix de son secteur.

## Eau, saisons et nuages : une seule source (`world/hydro.rs`, `world/climate.rs`)

Les rivieres et les lacs ne sont pas des decors : ils sont derives de la carte d'altitude et d'humidite
(remplissage par la priorite depuis la mer, direction de chaque case vers un voisin strictement plus bas,
pluie cumulee), donc deterministes et jamais sauvegardes. `Surface.hydro` est lu par la carte, le globe,
le terrain 3D et la grille a pied : la meme ligne de fleuve (du centre d'une case au centre de la suivante)
sert aux quatre. L'etat gele ne vient que de `Sim::temp_now` (une formule de saison dans `climate.rs`) et
de `climate::frozen` (la mer gele plus froid que l'eau douce). Les nuages sont une fonction pure de
(carte, pression, saison, jours) : deux couches decalees par le vent zonal du monde, fondues l'une dans
l'autre pour que le cisaillement ne les etire pas sans fin ; la meteo (`weather_at`) ne fait pleuvoir que
sous un ciel couvert. Choix assume : un fleuve est une ligne par case de 300 km, pas un reseau fin ; sa
largeur sur la grille a pied (6 a 36 m) et sa largeur lointaine (au moins un pixel) viennent de la meme
fonction `river_half_width_m`.

## Une case = une matiere (`data/materials.ron`)

Chaque case d'un etage (sol, mur, liquide) est faite d'une matiere (`Floor.mats`, index dans
`materials.ron`). Sa couleur et ses caracteres viennent de la matiere, dans la carte comme dans la vue
a la premiere personne (murs et sol, motif pierre/panneau/bruit/lisse) : une seule representation,
deux vues, qu'on change quand on veut (F). Sur une planete, le sol vient du biome (taches de matieres
ponderees par `ground_of`), la neige et la glace apparaissent quand il fait froid, l'eau gele ou la
lave remplace l'eau selon la temperature. Les lieux construits nomment leur matiere (`floor_mat`,
`wall_mat`). La matiere agit : `slow` multiplie le temps de marche (boue x2, neige x1,5, glace x0,8) et
un rover la ressent 2,5 fois moins. Sauvegardes : version 35.

## Reseau d'energie des colonies
Un seul systeme d'energie : `Colony::tick` (colony.rs). La source d'une centrale est une donnee (`source` : solar, wind, geothermal; sinon constante ou a combustible), pas un identifiant code en dur. Ordre : production -> pertes de reseau (`grid_loss`) -> batteries (decharge du deficit, charge du surplus a `BATTERY_EFFICIENCY`) -> delestage : la charge vitale (batiments avec logement, recyclage ou medical) est servie avant le reste; `power_sat` ne concerne que le reste (productivite), `vital_sat` le recyclage. Les rafales viennent de hash(graine du systeme, colonie, jour): aucune horloge reelle. Sauvegardes : `Colony` (pressure, geo, charge) et `ColonyStats` (power_sat, vital_sat, battery_charge, battery_cap, grid_loss) ont change; `Building` (donnees) gagne `source` et `battery`.
## La matiere donne, protege et blesse (suite de « Une case = une matiere »)

La composition d'une case pilote le reste, par donnees de `materials.ron` et non par code special :
`yields` (ressource, kg, rarete) ce que creuser la case donne, `under` le sol mis a nu, `harm` la
sante perdue par tour (sol caustique ; un liquide brule a cote, au tiers), `slick` la chance de deraper
d'une case (glace), `fertility` ce que les plantes y trouvent, `cost` ce que coute de la batir. Une
seule representation : un mur creuse devient un sol (la matiere de dessous), une case batie est une
matiere comme les autres (tole, pierre taillee), vue pareille en carte et en premiere personne ; seule
la marque de propriete (`Floor.owned`) est en plus (teinte ambre sur la carte, compteur dans le panneau).
La physique du monde module le rendement (`DigMods`, rogue.rs) : metaux, precieux, nucleaires suivent la
densite (rapport a la Terre, 0,4 a 2), soufre/phosphore/terres rares/diamant l'activite interne (inerte
x0,3 ... volcanique x2,5), glace et eau le froid de la surface (x0,3 en temperature douce, jusqu'a x3), et
les etages d'une mine sont plus riches (+25 % par etage). La durete donne le temps (1 + 6 x durete /
(1 + 2 x outil) tours) ; au-dessus de 0,75 il faut un outil de fouille (pioche, scanner : champ `gather`).
Les animaux evitent les sols glissants ou brulants ; les plantes ne s'installent que sur sol fertile.
Le froid : le gear a un champ `warmth` qui s'ajoute a la defense sous 250 K. Sauvegardes : `Floor.owned`
ajoute (le coordinateur bumpe la version).
## Physique de vol : un seul bilan, pas de systeme parallele (2026-10-11)
La chaleur exterieure (soleil, air) entre dans `Systems::step` par `Activity.ext_heat_kw`, la dose passe par `hazard_tick`/`irradiate` existants, la sphere d'influence remplace `60 rayons` dans `update_ref`. Le flux solaire des panneaux tient compte des eclipses. La dose cumulee est dans `Systems` (sauvegardee). Non fait : J2 appliquee a la trajectoire (seule la precession est calculee), co-rotation du vaisseau pose, brouillage des communications, effet de la dose sur la detection NPC.
- Droit : les lois dérivent des axes de mœurs existants (`ethos.ron`) au lieu d'une table de lois par faction : une seule représentation de la morale, et une faction nouvelle ou une colonie qui dérive change de lois sans contenu à écrire. Le casier vit dans `TradeState` (pas de nouvelle structure de sauvegarde de premier niveau). Pas de prison pour l'instant : mandat d'arrêt (ports fermés) et saisie, pour ne jamais bloquer le joueur ; les amendes se paient de partout.
## Haute science-fiction
Un seul systeme : les phenomenes exotiques sont des resultats d'exploration d'anomalies (`science.rs`) et ne creent aucun systeme parallele; les modules exotiques restent les `exo_*` de `multiverse.rs`. Rien n'est scripte : le phenomene, les vestiges (20 % des systemes) et la reponse au paradoxe de Fermi sortent de `hash(graine, coordonnees)`. Antimatiere : E = 2 m c2 exact; un piege magnetique non alimente la laisse fuir et s'annihiler contre la coque. Cout du warp : m c2 v2 (1 + v2/4), plus que quadratique. Sauvegardes : `Multiverse.exotic_clues` (serde default) ; le coordinateur bumpe VERSION.

Haute SF, suite (`exotic_field.rs`) : rien de nouveau n'est sauvegarde. Le propulseur Alcubierre remplace le carburant par de l'antimatiere (energie `warp_energy_j` x 1e-9 d'echelle de jeu, convertie a 2 c2 par kg : un seul bilan d'energie) et irradie l'equipage au sillage ; la liaison quantique supprime le delai des couriers (`news.rs`) jusqu'a 60 al ; singularite nue, corde cosmique, chambre de stase, floraison de nanites et graine de strangelet agissent chaque pas en fonction de la distance (fonctions pures de l'identifiant) ; Kardashev = (log10 P - 6) / 10 pour le vaisseau et pour les civilisations (champ `kardashev` de exotic.ron). Le clonage (nanites + credits) est un recrutement : copie a 80 % des competences, relation -15 avec l'original, moral du bord -3 %. Armes sorties pendant un premier contact : traduction x0,6.
## Noms generes (une grammaire, une source)
Tous les noms viennent de `world::names` (`Spec::new(genre).culture(..).host(..).name(hash)`), jamais d'une horloge : meme graine, memes noms. La grammaire `data/names.ron` est embarquee (`include_str!`) et validee au premier usage (jetons connus, listes existantes, caracteres affichables par la police 5x7, probabilites, genres obligatoires); les tests refusent une grammaire cassee. `cultures_names.ron` ne garde que id et nom d'affichage; tout le reste de la culture (syllabes, sexe, particules, surnoms, titres, mots de lieux) est dans `names.ron` sous le meme id. La generation des systemes tire exactement les memes nombres aleatoires qu'avant (le nom d'etoile vient d'un hash de l'etat du generateur; le trafic PNJ tire deux nombres comme avant) : la physique des systemes et le trafic ne changent pas, seuls les noms changent. Les noms de personnes, d'etoiles, de planetes, de lunes, de stations, d'anomalies, de PNJ et de vaisseaux de flotte sont donc differents d'avant pour une graine donnee (les noms deja enregistres dans une sauvegarde restent tels quels).
## Chimie : une table, la loi de Hess, un laboratoire (`data/chemistry.ron`, `reactions.ron`)

Une seule representation de la matiere chimique : les corps purs (`chemistry.ron`) portent une formule dont
la masse molaire est calculee (jamais saisie) et une enthalpie de formation ; une reaction (`reactions.ron`)
ne porte pas son energie : elle se deduit de la difference des enthalpies de formation (loi de Hess), donc
une table ne peut pas se contredire. A la lecture, chaque reaction doit equilibrer les atomes de chaque
element, une combustion ou une explosion doit degager de la chaleur, une electrolyse en absorber. Les
ressources de `resources.ron` restent la monnaie du jeu : un corps pur liste les ressources qu'il est
(`resources:[...]`, la premiere est celle qu'une reaction fabrique) ; les alliages et composants ne sont pas des
corps purs et n'ont pas de composition. Les atmospheres (`world`) gardent leurs formules (`"CO2"`) : on les
rattache aux corps par la formule. Nouvelles ressources dans `resources_chemistry.ron` (fichier a part pour
ne pas heurter les autres ajouts de ressources). Le laboratoire reutilise le codex (categorie REACTIONS) au
lieu d'un nouvel ecran. Le reacteur du bord monte a 2400 K et 300 bar ; l'efficacite electrique est de 0,6 ;
l'impulsion specifique reelle = 0,78 x sqrt(2 q) / g0 (q = chaleur par kg d'ejecta). Sauvegardes : aucune
structure sauvegardee n'a change (corrosion = sante des modules existante ; stock = soute existante).
## Epidemies : un modele, trois sortes de lieux
Un seul modele SEIR (`epidemic::step`) sert les colonies (etat sauvegarde), les secteurs (fonction de la graine, rien de stocke, comme `politics` et `incidents`) et le vaisseau (les symptomes sont des maladies de `diseases.ron` portees par des personnes). Pas de seconde representation de la maladie : l'agent pathogene reference la maladie, la maladie d'equipage dit si une personne est porteuse. Les effets locaux (prix, nouvelles, import dans les colonies) ne dependent que du secteur de l'observateur. Limite assumee : le joueur contamine les colonies qu'il visite mais pas les secteurs (sans etat, rien a ecrire).
## Militaire : physique des armes et campagnes
- Pas de `weapons.ron` parallele : les armes restent des modules (`modules.ron`), etendus par `proj_mass`, `spread`, `wavelength`, `aperture`, `flash` ; une arme a pied est un `Gear` avec `noise`. Un test verifie que 1/2 m v^2 / 10 kJ ~ degats des donnees (une constante, `arms::DAMAGE_J`).
- Les guerres de `politics.rs` ne sont pas dupliquees : `arms::fronts` resout chaque front par la loi carree de Lanchester, avec une brume de guerre hachee (graine, cellules, periode), identique vue des deux camps. Le vaincu est assiege (`Turmoil::besieged`).
- Detection : la signature thermique est calculee (moitie de la puissance consommee + flash des armes qui viennent de tirer), jamais stockee.
- Non fait : escadres du joueur, mercenariat, agents persistants, leurres thermiques en vol, abordage a grille de coque (voir PROGRESS).
## Macroéconomie par secteur
L'économie d'un secteur est une fonction pure de (graine, secteur, date), comme la politique : pas d'état à sauvegarder, la même crise frappe tout le monde le même jour. Un seul système de monnaie et de banque (`Bank`) : le taux, le plafond et les cours en dépendent, il n'y a pas de banque parallèle. Le tableau de bord vit sur l'écran banque (chacun chez soi : les services financiers sont à la station) plutôt que dans un écran neuf. Les lois (taxes, interdits) restent à l'agent société/droit : on lit seulement `Faction::tax`.
## Un seul registre de relations (`relations.rs`)

Quatre systemes disaient la meme chose (reputation de faction -100..100, bienveillance des civilisations -1..1, confiance des contacts de contrebande 0..1, faveur des cartels) : ils sont maintenant des valeurs d'un meme registre (`Relations.values`, -1..1, par `Entity {kind, id}`), les anciennes API (`reputation_with`, `change_civ_reputation`, `contact_trust`) restent comme vues sur lui, pour ne rien dupliquer. Les liens entre entites viennent des donnees (allies/ennemis des factions, `hq` des cartels et des entreprises) et de liens dynamiques (`Bond`, formes par les surprises, qui expirent). Propagation : un changement se repercute sur les voisins avec le poids du lien (+1 allie, -1 rival) x `ATTENUATION` (0,3), un seul saut, sans boucle ; la confiance d'un contact n'a pas d'echo (elle est personnelle) et ne descend pas sous 0. Ce que la relation commande : prix (`trade_edge`), acces (accostage refuse sous -60), recompense des contrats, prix du siege et profits des cartels, conditions de dialogue, conditions de surprise.

## Surprises = regles en donnees, pas d'evenement ecrit

`data/surprises.ron` decrit des conditions sur l'etat du monde AUTOUR du vaisseau (evenement du monde actif, guerre, cartel actif, incident d'entreprise, colonie du joueur a moins de N al avec une epidemie, relation, anomalie du systeme, contrats, moral, soute, siege de cartel...), un poids, un delai, des effets (credits, relation, lien, choc de prix local, soute, moral, contrat propose, recherche, bonheur des colonies) et des choix. Chaque jour de jeu, chaque surprise dont toutes les conditions tiennent tire `hash(graine, secteur, jour, surprise)` contre `0,05 x poids` ; au plus une par jour (celle qui bat le plus largement sa chance) ; delai par surprise. Le monde etant infini, rien ne se passe hors du secteur du vaisseau. Pas d'horloge reelle ; un saut de temps rejoue au plus 10 jours. Une cible impossible a determiner (pas de territoire, pas d'allie) rend la condition fausse ou l'effet saute : jamais applique ailleurs. Les chocs de prix passent par `shock_factor`, lu avec les facteurs du monde (marche de la station, cours de reference). Elargit `world_events` et le journal au lieu d'ajouter un 4e generateur.
## Biosphere : tout est derive, rien n'est stocke (E2b/E2c)

- **Un systeme, une representation** : le reseau trophique reste `ecology.rs` (biomasse par espece et biome) ; `biosphere.rs` ne fait que le lire (variantes, reseau vu d'une espece) et deriver des fonctions pures. La couche BIOSPHERE est une couche de plus de l'ecran de surface, pas un ecran parallele ; la xenobiologie est la meme table d'especes (un second fichier ajoute a `species`), pas un second moteur.
- **Evolution sans etat** : une population (espece x biome x monde) est une fonction de la graine du monde, du temps de jeu et de ses generations (age d'isolation tire de la graine entre 100 et 1 million d'annees, plus le temps de jeu, divise par la duree d'une generation ~ masse^0,25). Les traits (masse, pelage, durcissement aux radiations) s'adaptent vers l'optimum du lieu (regle de Bergmann, camouflage, blindage selon la pression) avec une derive aleatoire lissee ; au-dela d'un seuil de divergence la population compte comme variante. Resultat : aucune structure sauvegardee, et le temps accelere (anomalies, dimensions) fait evoluer plus vite parce qu'il fait avancer le meme temps de jeu. Limite assumee : sur une partie de quelques annees seules les petites especes (generations courtes) bougent visiblement.
- **Comportements derives, pas ecrits** : troupeau, meute, territoire, nocturne, hibernation sortent du niveau trophique, de la masse et des habitats (graine = hash de l'id), pour que toute nouvelle espece ait un comportement coherent sans table a maintenir. L'hibernation lit `temp_now` (donc les saisons du climat), la nuit lit le jour solaire reel du monde.
- **Air vivant** : la photosynthese transforme du CO2 en O2 mole pour mole (le carbone est enfoui) ; la quantite depend de l'age de la vie du monde (graine) et du temps de jeu. `air_of` lit `atmosphere_of`, pas la composition brute generee. Pas de combustion ni de pollution industrielle pour l'instant.
- **Journal par franchissement de seuil** : les evenements (extinction, famine, invasion) comparent l'etat avant/apres la mise a jour du territoire de la colonie, sans nouveau champ sauvegarde.
## L'histoire d'un secteur, une seule chronologie (`history.rs`)

Chaque secteur de 240 al a 8 ages de 400 ans (essor, apogee, effondrement, age sombre, migrations,
reconquete) tires par une petite machine a etats depuis `hash(graine, secteur)`, sans rien stocker : le
meme secteur raconte la meme histoire a chaque appel. Le dernier age s'accorde avec le present (le
peuple qui tient le secteur aujourd'hui, `politics::faction_of`) et les annees recentes sont celles de
`politics::events_in_cell` (dirigeants, guerres, paix) : une chronologie, pas deux moteurs. Une ruine, une
epave ou une station ancienne tire sa provenance (`provenance`) de cette chronique : batie dans un age
d'essor, d'apogee ou de reconquete par le peuple de cet age, tombee avec l'effondrement suivant et sa
cause ; une epave est recente. Le nom du lieu (`Marche de ...`) vient du plus grand personnage.

## Archeologie et ethique (`archive.rs`)

La fouille prudente documente les trouvailles (valeur x1,3, le musee les accepte) mais en sort moins ;
le pillage en sort davantage, sans contexte (x0,6), et coute de la reputation aupres des heritiers (le
peuple qui tient le secteur). Un faux est fabrique 150 a 400 ans avant aujourd'hui avec de vieux
materiaux : la datation donne un age mesure +/- une barre d'erreur qui fond avec le budget d'analyse de
l'IA ; au-dela de 2 barres d'ecart avec l'age apparent, la datation le trahit. Un collectionneur paie
80 % de la valeur apparente sans question (il peut flairer un faux : fraude denoncee, reputation en
baisse), un musee 50 % mais seulement du documente et authentique, la restitution aux heritiers ne paie
rien et rapporte leur reconnaissance.

## L'IA est une machine (`mind.rs`)

Puissance = 100 + 60 par laboratoire alimente (sante), x debit thermique (0,4 a la temperature de
degats) x bus electrique, x (1 - 0,6 malware), +10 % par IA alliee. Le budget se repartit par poids 0-10
entre pilotage, analyse, traduction, simulation, defense. Une tache lit sa part de deux manieres :
`mind_factor` (saturant, 0,5 a ~1,5, x1,00 a parts egales sur un vaisseau nu, utilise par le pilote
automatique (delta-v du plan), les laboratoires (`ship_research_rate`) et le pare-feu des intrusions) et
`mind_rate` (lineaire, pour les travaux de plusieurs jours : traduction, cassage de donnees chiffrees,
nettoyage de malware, datation). Une intrusion reussie laisse du malware d'autant plus gros que la defense
est faible. Les logiciels (`mind.ron`) ajoutent un bonus a une tache et prennent de la memoire (6 + 2 par
laboratoire) ; trois versions payantes. Les autres IA d'un secteur (0 a 2) sont asservies (a un peuple),
libres ou rebelles ; en liberer une coute -6 de reputation a son proprietaire ; un rebelle non allie rend
les intrusions plus probables ; les sauvegardes de soi sont les ancres du multivers.
## Medecine et corps
Les blessures localisees sont une couche *au-dessus* des maladies (`diseases.ron`) et des organes (`organs.ron`), pas un systeme parallele : un coup abime l'organe de la partie touchee, une plaie infectee ou une dose de rayons declenche la maladie existante (septicemie, mal des rayons), la sante generale et le plafond par l'etat des organes restent ceux de `crew.rs`. Le corps (`body.rs`) ne connait pas la simulation : on lui donne une exposition (`Exposure` : pression, fraction d'oxygene, CO2, temperature, gravite, acceleration, rayonnement, isolation) et un contexte de soins (`Care` : competence du medecin, infirmerie, faim, fatigue, age, foie) et il rend les degats (organes, sante, stress). L'exposition vient de la cabine (acceleration lissee de la poussee, habitat = 0,5 g, chaleur du vaisseau en surchauffe) ou de l'air de la planete pour l'escouade a pied (composition de `system.bodies`, combinaison etanche = sa propre pression et son oxygene). Choix assumes : le froid met des heures a refroidir le corps (tau 2,5 h) et le vide asphyxie en quelques minutes ; sans combinaison, une planete sans oxygene tue en une vingtaine de minutes de jeu (l'ancien `environment_damage` reste en plus comme choc brutal). Les medicaments sont des doses de la ressource `medicine` ; la surdose depend du niveau, des interactions et du foie. Un blesse non panse guerit a 40 % de la vitesse et garde une sequelle s'il etait grave ; un medecin de bord et l'infirmerie accelerent les soins, la rehabilitation efface les handicaps lentement. Sauvegardes : `Person.body`.
## Un menu, huit espaces, des écrans qui ne se dupliquent pas (2026-10-12)
Le jeu avait une trentaine d'écrans atteints par des touches éparses (F2, F4, X, Y, T...) : « le bordel, rien n'est compréhensible ». Le menu principal (F10) les regroupe en huit espaces ; chaque ligne appelle la fonction d'ouverture de l'écran existant (jamais une copie), `place_of` (exhaustif) range chaque écran dans un espace et un test refuse un écran sans ligne ou listé deux fois. La navigation est uniforme : Echap remonte d'un niveau (écran ouvert depuis le menu -> page de son espace -> liste -> écran d'origine). Les écrans de commerce restent réservés au quai, ceux du cockpit au vol ; à pied seuls les écrans d'information s'ouvrent et reviennent à la marche (mécanisme `rogue_visit` existant). F10 étant pris, l'ignorance du tutoriel passe sur Suppr.

## Les départs sont des données et construisent la vraie Sim (2026-10-12)
Un scénario (`data/scenarios.ron`) = une liste de vaisseaux possibles (coque + modules, tous vérifiés viables par test), crédits, soute, points d'équipage, technologies, réputations, prime. `start::new_sim_with` ne contient aucune règle de plus : il règle l'état de départ. Le **Naufrage** n'a pas de système parallèle : c'est le vaisseau normal avec des modules endommagés (la santé par module existe déjà), réparé par les ingénieurs avec des pièces détachées (mécanique existante), posé sur un monde (atterrissage `parked` existant), exploré à pied (`rogue.rs`). Seul ajout de règle : verser le carburant de la soute au réservoir (`refuel_from_hold`), qui sert aussi à tout butin de carburant. Les aléas ordinaires (éruptions, météores) restent actifs : un vaisseau cloué au sol peut être abîmé de nouveau, c'est voulu.
## Vue a la troisieme personne
Un seul rendu de vaisseau : `ship_model::build(data, design, state)` est une fonction pure (design + etat -> triangles), utilisee par la camera externe, les PNJ, les stations, le codex et le chantier naval ; `hud::draw_cockpit` reste le seul dessinateur de la scene (la vue externe change la camera et remplace les filaires des vaisseaux/stations par les modeles, rien d'autre). Le rasteriseur est logiciel et z-buffer (`ship_model::draw`), distinct des lignes additives de `Framebuffer::plot` : les parties opaques ecrivent avec `px_set`, les flammes sont additives et testees en profondeur sans ecrire. La poussee de la flamme vient de la sim (`Sim.main_throttle`, part de la poussee principale utilisee, non sauvegardee) et non des touches. Le train sort quand le vaisseau est a moins de 600 m du sol reel (relief de `Terrain::sample`) et plus lent que 250 m/s ; l'ombre et la poussiere n'existent qu'au-dessus d'un monde solide ; le vaisseau n'est pas pose par la sim (l'atterrissage est le domaine d'un autre agent) : seul le rendu remonte le modele pour que les pieds touchent. Choix assume : la lumiere est plate par triangle (style vectoriel), sans ombre portee sur la coque elle-meme.
