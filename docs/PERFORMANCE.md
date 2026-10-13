# Performance des voyages longue distance

Mesures du profil de test (`optimized + debuginfo`), machine partagee et chargee : ordres de grandeur, pas des garanties. Reproduire : `cargo test -p vector-sim --lib far_tests -- --nocapture` (lignes `PERF` et `PROFILE`).

| Mesure | Valeur |
|---|---|
| Planification d'une route de ~150 sauts (portee 300 al, 40 000 al), secteurs a froid | ~0,3 s |
| La meme, secteurs en cache | ~9 ms |
| Generation d'un secteur de galaxie (a froid) | ~20 microsecondes |
| Pas de simulation pendant un voyage (transit, halte, mele) | ~0,15 ms en moyenne (transit) ; un pas de vol libre ordinaire coute ~6 ms |
| Voyage de 52 sauts de bout en bout (a x100000 equivalent, ~1 an de jeu) | ~24 s de calcul |
| Arrivee dans un systeme (generation du systeme, trafic, factions) | < 0,2 ms |

Ce qui borne le cout :

- La galaxie n'est jamais stockee : chaque secteur est regenere depuis la graine. `Galaxy::sector_cached` garde jusqu'a 32 768 secteurs par thread (vide quand plein) ; `nearby`, `star` et le planificateur lisent ces listes partagees sans les copier.
- Le planificateur parcourt, pour chaque saut, les seuls secteurs dont la boite est a portee du point vise (carre des distances, pas de tri) et elimine un candidat des que `distance au but` depasse le meilleur score ; le territoire (27 hachages) n'est evalue que pour un candidat qui pourrait gagner. Le carburant est lineaire en la distance : un seul coefficient.
- Un saut long est franchi en ~40 pas (`total/40` entre 600 s et 6 h) et les haltes en pas de 6 h : le nombre de pas ne depend presque pas de la duree du voyage (une traversee intergalactique de ~2 000 pas).
- Test de budget (`route_planning_and_stepping_stay_fast`) : plan de 150 sauts < 30 s a froid et < 3 s en cache, pas moyen < 20 ms.
# Performance

Mesures en release, 1280x720, `vector-game --bench` (machine partagee, donc pessimiste).

## Ciel reel et carte de l'univers (wt-univ)

`vector-game --bench` (release, 1280x720, rendu logiciel). Deux series sur la meme machine partagee, charge differente : A (machine calme) et B (machine tres chargee, reference de rendu 7,55 ms au lieu de 2,38 ms).

| Mesure | A | B |
|---|---|---|
| Reference : rendu du cockpit d'avant | 2,38 ms | 7,55 ms |
| Reconstruction du catalogue du ciel (changement de secteur, ~2000 etoiles + bande + constellations) | 66 ms | 256 ms |
| Cockpit avec le ciel reel | 1,90 ms | 5,77 ms |
| Carte, echelle 1 voisinage (50 al) | 1,43 ms | 5,18 ms |
| Carte, echelle 2 region (1000 al) | 1,46 ms | 5,52 ms |
| Carte, echelle 3 galaxie entiere | 2,30 ms | 6,55 ms |
| Carte, echelle 4 groupe local | 3,01 ms | 18,3 ms |
| Carte, echelle 5 univers | 1,52 ms | 11,3 ms |

Tout reste sous 17 ms (60 images/s) sur la machine calme et sous 33 ms sur la machine chargee. Le ciel et la carte sont en O(n) borne (~4000 points de bande, ~2000 etoiles, 9000 points de galaxie, 8000 points locaux, quelques milliers de galaxies voisines), sans allocation par image : les listes sont des caches (`SkyCache` par secteur, `UMap` par graine et secteur). La reconstruction du ciel est payee une fois par saut ; le premier affichage d'une echelle lointaine construit son cache (nuage de la galaxie, galaxies voisines) une seule fois. Test de budget tolerant : `sky::tests::the_sky_budget_is_tolerant`.
Le jeu est rendu entierement par le processeur (framebuffer 1280x720, plot additif) et simule a 30 Hz.
Cible : >= 60 images/s (16,7 ms) en release a 1280x720 partout, >= 30 au pire, pas de pic > 50 ms,
demarrage < 2 s, pas de simulation > 1 ms en vol normal. Ce document dit comment mesurer, ce qui
etait lent, ce qui a ete change, et ce qui reste lent.

## Mesurer

| Outil | Usage |
|---|---|
| `vector-game --bench-planet [images] [--threads N] [--micro]` | un monde de rayon apparent 10 / 50 / 100 / 200 % de la hauteur d'ecran, de jour et de nuit, vivant (air + nuages), rocheux sans air, geante, lune ; puis en vol bas (relief marche). `--micro` : cout du pas de relief seul. |
| `vector-game --bench-all [images] [--threads N] [--only a,b]` | demarrage (Game::new, new_game, planet_map, flight_map), simulation (ms par pas, pas > 5 ms), sauvegarde/chargement, puis ~70 ecrans a 1280x720 (noms de `--snapshot`). Un ecran dont la scene met plus de 40 s a se construire est abandonne (certaines scenes jouent des jours de temps de jeu). |
| `vector-game --profile` ou la touche ArretDefil (Scroll Lock) en jeu | profileur leger (`perf.rs`) : temps par etape de dessin (`scene`, `hud`, `world`, `ball_pixels`, `terrain_march`, `sim_advance`, `draw_total`...), moyenne sur 30 images, affichee en haut a gauche. Hors profil : une lecture d'un booleen atomique par etape. |

Colonnes : `min` = meilleure image (la plus proche d'une machine au repos), `med` = mediane,
`cpu` = ms de processeur par image tous fils confondus (`GetProcessTimes` : ne grossit pas quand la
machine est occupee par autre chose ; resolution 15 ms, donc a lire sur >= 10 images), `pic` = la plus
longue des images de chauffe (construction des caches : l'accroc que le joueur sent une fois).

**Regle de projet** : toute modification du rendu, de la simulation par pas ou du chargement relance
`--bench-all` et `--bench-planet` avant fusion ; aucun cas ne doit passer de < 16 ms a > 16 ms, et un
nouvel ecran > 16 ms (un seul fil) doit etre justifie ici.

## Ce que la mesure a montre (avant)

Un seul fil, release, 1280x720, machine partagee (charge a 100 % pendant tout le travail : les temps
absolus sont pessimistes, les rapports sont fiables).

* `globe::fill_world` etait le goulot : pour chaque pixel (par blocs de `r_px/170`) un rayon, `asin`,
  `atan2`, 2 bruits de decalage, **3 echantillons de `Terrain::sample`** (16 octaves de bruit, 4 hachages
  chacune, craters), un autre bruit, **`Sim::cloud_cover` qui reconstruit `Atmo` (384 lectures) et
  calcule 2 x fbm 3D** pour chaque pixel, et `powf`. 4 us par pixel : 100 a 200 ms quand la boule
  prend l'ecran (une planete = 10 a 60 images/s de moins).
* La geante (pas de relief ni de nuages) etait deja a 16-29 ms a 100 % : meme boucle, memes `asin` /
  `atan2` / bruits.
* Le marcheur de relief (`terrain::render`, vol bas) : 640 colonnes x ~380 pas, chaque pas = 16 octaves
  hachees de zero + `inland_water` + 3 bruits de couleur, un seul fil : 145-170 ms ; 400-1000 ms sur un
  monde sans air (craters : 9 cases x 4 echelles hachees a chaque pas).
* Ecran de surface (F8), couches meteo / nuages / biosphere : `cell()` recalcule `Atmo::new` + fbm (ou la
  somme de toutes les especes) pour **chaque case affichee, a chaque image** : 22 ms (nuages), 48 ms
  (biosphere), 7 ms (carte de base).
* Tableau de bord : `net_worth` + `portfolio_value` (prix de marche de tout le portefeuille, deux
  fois) a chaque image : 15 ms.
* Ni la simulation (0,07-0,1 ms par pas en moyenne ; les pics > 5 ms observes tombent sur des appels
  quelconques d'un pas a l'autre : ce sont les coupures du systeme, pas du travail periodique), ni le
  demarrage (Game::new 15-40 ms, new_game 20-110 ms, premiere `planet_map` 25-125 ms), ni la
  sauvegarde (0,5 ms, 8 Ko) / le chargement (3 ms) ne posent probleme. Les pas par image en temps
  accelere (`Sim::advance`) ne sont pas la cause du lag devant une planete.

## Ce qui a ete fait

1. **Texture de monde cuite (`ball.rs`)**. Tout ce qui ne change pas d'une image a l'autre (case de la
   carte decalee par le bruit, hauteur detaillee, pente est/nord contre la lumiere, part de terre,
   variation de lieu, eau / sable corriges) est cuit une fois dans une texture equirectangulaire par
   monde et par taille (64x32 ... 1024x512 selon le rayon apparent : un texel ~ 1 a 2 pixels). La boule
   ne fait plus, par pixel, que : rayon (repere du monde, sans `normalize`), 2 `atan2` approches, une
   lecture bilineaire, l'eclairage de Lambert, le terminateur (le meme calcul qu'avant) et le bord d'air.
   Les couleurs (saison, terraformation) se recalculent sans refaire le relief (`recolor` : une table de
   8192 couleurs puis une passe sur les texels). Le niveau fin se cuit **sur plusieurs images** (2048
   texels par fil et par image) pendant que le niveau grossier est affiche : plus d'accroc.
   La cle du cache est `Paint.world` / `Paint.colors` / `Paint.cloud_stamp` (voir `globe::Paint`).
2. **Nuages en texture** (256x128, 128x64 si petit) reconstruite seulement quand l'heure de jeu change
   (`cloud_stamp`, une heure) ; la closure de nuages ne capture que `&Surface` + `Atmo` (calcule une
   fois par image), pas `Sim`.
3. **Parallelisme sans dependance** (`ball::bands`, `std::thread::scope`) : bandes de lignes tirees dans
   une file par les fils (le travail inegal se repartit), chaque bande ne depend que d'elle-meme : la
   sortie est identique pour 1 ou N fils (test). Utilise pour la cuisson, les nuages, les pixels de la
   boule et le marcheur de relief.
4. **Marcheur de relief** : un fil par groupe de colonnes (`terrain::render_par`, table de couleurs de
   cases ; `render` garde sa signature et reste sur un fil pour les closures non `Sync` de la vue a pied) ;
   `SampleCache` (la maille de bruit d'un octave ne change pas pendant des dizaines de pas : 4 hachages
   evites ; liste de craters par maille) ; `floor` / `fmod` remplaces par des versions sans appel a la
   bibliotheque mathematique ; pas de 4,5 % au-dela de 8 km ; variation fine de couleur omise au-dela de
   300 km ; ciel et sol composes par lignes (parallele) au lieu de `plot` par pixel.
5. **Ecran de surface** : grille 128x64 de (glyphe, couleur) gardee tant que (couche, monde, heure de
   jeu / 30 min, espece, etat de l'ecologie) ne change pas.
6. **Tableau de bord** : valeur nette et portefeuille gardes une heure de jeu (ou jusqu'a un changement
   de credits).

## Avant / apres (un seul fil ; `min`, ms/image)

Planetes (apres : `--threads 1`, apres tous les changements ; la ligne « N fils » est le calcul, pas une
mesure, voir plus bas).

| Cas | Avant | Apres |
|---|---|---|
| vivante (air, nuages) 10 % | 19 / 17,6 (jour / nuit) | 3,2 / 3,3 |
| vivante 50 % | 99 / 101 | 8,1 / 7,2 |
| vivante 100 % | 181 / 198 | 14,4 / 13,1 |
| vivante 200 % | 160 / 158 | 12,6 / 11,7 |
| rocheuse sans air 10 % | 10 / 11 | 2,8 / 3,4 |
| rocheuse sans air 50 % | 79 / 78 | 6,9 / 7,7 |
| rocheuse sans air 100 % | 139 / 149 | 15,4 / 16,6 |
| rocheuse sans air 200 % | 102 / 114 | 15,4 / 12,0 |
| geante 10 / 50 / 100 / 200 % (jour) | 3,4 / 12 / 18 / 24 | 2,9 / 6,9 / 10,9 / 10,1 |
| lune 100 % jour / nuit | 164 / 197 | 13,8 / 32,5 (deux boules : la lune et la geante derriere) |
| vol bas vivante (177 km / 18 km / 200 m) | 147 / 155 / 147 | 96 / 108 / 103 |
| vol bas sans air (37 km / 3,7 km) | 986 / 432 | 151 / 175 (cpu 147 / 133) |
| vol bas lune (26 km / 2,6 km / 200 m) | 171 / 170 / 121 | 146 / 122 / 119 (cpu 86 / 84 / 84) |

Les temps de vol bas sont le temps d'un seul fil sur une machine saturee (cpu : le temps de calcul).
Le marcheur est reparti en groupes de colonnes sur tous les coeurs : a 8 coeurs, 100 ms de calcul font
~13-15 ms ; a 4 coeurs ~26 ms (30+ images/s). **Je n'ai pas pu mesurer la mise a l'echelle a plusieurs
fils** : la machine de travail etait a 100 % de charge tout du long (12 compilations en parallele) et
les mesures a 8 fils y sont plus lentes qu'a un fil. L'egalite des sorties 1 fil / N fils est testee ;
le gain reel sur machine libre reste a confirmer avec `--bench-planet` (a refaire au repos).

Ecrans (un fil) : surface (couche nuages) 22 -> 2,4 ; biosphere 48 -> 2,4 ; surface (base) 7,3 -> 2,2 ;
globe 7,2 -> 4,1 ; tableau de bord 15 -> < 2.

## Reste lent

* Le relief marche (vol bas, atterrissage, marche a pied qui utilise `terrain::render`) : ~80-150 ms de
  calcul par image pour un fil, 10-20 ms a 8 coeurs, 25-40 ms a 4 coeurs. Le cout vient de 8 octaves de
  bruit hachees par pas (~330 ns/pas, la carte seule coute ~80 ns). Pistes : precalculer le relief en
  tuiles autour du joueur, ou rendre les colonnes eloignees une image sur deux.
* La premiere vue d'un monde construit sa carte (`Sim::planet_map` : 25-125 ms une fois par corps ;
  l'accroc est inevitable sans la calculer en fil de fond).
* `render` (vue a pied, closures `local` non `Sync`) n'est pas parallele ; passer a `render_par` quand la
  vue a pied saura donner une table.
* Scenes de la baterie `--snapshot` qui jouent 3 jours de temps de jeu (`market`, `contracts`, `bank`...) :
  longues a construire (millions de pas), non mesurees.

## Verification

Tests (`ball.rs`, `terrain.rs`) : boule eclairee de face symetrique et ronde (aire du disque a 6 %),
terminateur net du bon cote, meme image pour 1 et 5 fils, aucune panique pour toutes les tailles
(1x1 ... 333x77, distances de 1,0005 a 1e5 rayons, centre hors ecran), boule cuite proche de l'ancienne
boule pixel par pixel (ecart moyen < 12/255 sur un monde avec nuages ; ~8) et plus rapide (budget < 250 ms
en un fil sur une machine chargee), `sample_with` identique a `sample`, `render_par` identique a
`render` (1 et 4 fils).
