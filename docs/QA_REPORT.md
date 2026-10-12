# Rapport QA : robot de test `vector-qa`

Robot : `crates/tools/src/bin/qa.rs` (binaire `vector-qa`, aucune dependance nouvelle).

```
cargo build --release -p vector-tools --bin qa
target/release/qa --days 100 --seeds 1,2,3 [--scenario voyageur] [--policy chaos] [--no-det] [--timeout 400]
target/release/qa --probe          # dose de rayonnement au depart de chaque scenario x 8 graines
QA_TRACE=1 / QA_CREW=1 qa ...      # traces (actions, blocs de temps, etat de l'equipage heure par heure)
```

## Ce que fait le robot

- Cree une partie par scenario de `data/scenarios.ron` (voyageur, marchand, explorateur, hors-la-loi,
  colon, naufrage) x graines x politiques, avec `new_sim_with` (le vrai chemin de depart du jeu).
- Politiques : `idle` (ne bouge pas, non amarre), `pacifiste` (s'amarre vite puis ne fait rien),
  `random` (actions + pilotage au hasard seede), `trader` (achat/vente/contrats/prets/autopilote),
  `explorer` (sauts, rogue_*, fouilles), `builder` (triche : argent + kit, fonde 2 colonies, invulnerable,
  puis terraformation, projets, flotte, chantier naval, titres, societe, actions), `chaos` (une
  quarantaine d'actions de la Sim tirees au hasard : commerce, contrats, bourse/titres, casino, marche noir,
  loi, colonies, flotte, chantier, medecine/chirurgie, haute SF, multivers, rogue, archives, elevage...).
- Chaque action et chaque bloc d'avance est enveloppe dans `catch_unwind` (message et position de la panique
  rapportes). Le temps avance par pas adaptatifs avec un plafond de pas par heure simulee (detecte les
  ralentissements) et un budget de pas par partie ; un thread de garde coupe une partie qui depasse
  `--timeout` secondes (blocage).
- Invariants verifies chaque jour simule : physique, rotation, carburant (0..reservoir), coque (0..max),
  batterie, temperature, dose, sante des modules, soute (masses finies >= 0, total = somme, <= capacite quand
  les modules sont intacts), banque, marches (stock fini >= 0, prix fini > 0), equipage (valeurs finies,
  organes, pas de « fantome » : vivant avec 0 de sante deux fins de jour de suite), colonies (population,
  bonheur, tresor, stocks, terraformation finis).
- Sauvegarde : tous les 40 jours `save_bytes` -> `load_bytes` -> `save_bytes` doit redonner les memes octets
  (`vector_sim::save::diff_fields` nomme le champ fautif).
- Determinisme : chaque partie est jouee deux fois (memes actions) ; l'etat final doit avoir le meme hash.
- Performance : ms par jour simule, pire jour, microsecondes par pas, actions les plus lentes.
- Equilibrage : bilan de fin de partie (credits, carburant, coque, equipage vivant, colonies, flotte) et
  signalement d'un equipage entierement mort avant le jour 40.

## Bugs trouves et corriges (chacun avec un test qui echoue avant et passe apres)

1. **Rechargement : les PNJ du trafic ne reviennent pas a l'identique** (graine 1..8, tous scenarios, jour 39).
   `load_bytes` appelait `place_npcs()` avant de restaurer l'horloge et les corps (temps 0), si bien que le
   champ `npcs` d'une sauvegarde rechargee differait de l'original. Correction : `place_npcs()` apres
   `time` et `refresh_bodies()`. Test : `save::tests::reload_keeps_npcs_exactly`.
2. **Pilote automatique : le pas se reduit a 1/240 s quand le trajet traverse un autre monde** (cause de
   parties qui « rament » : 200 000 pas pour une heure, jusqu'a des dizaines de secondes de calcul par jour
   simule). `safe_dt` limite maintenant le pas par la seule destination sous pilote automatique (le plan est
   analytique). Test : `autopilot_crossing_another_world_keeps_long_steps`.
3. **Pilote automatique -> amarrage = coque a 0 et equipage mort** (regression de `dock_damage`, fusionnee
   depuis master) : en fin de trajet le nez pointait dans une direction quelconque, `try_dock` calculait un
   amarrage brutal de 1000 points. A l'arrivee a une station, le nez est maintenant mis sur l'axe du port.
   Test : `autopilot::tests::a_ride_to_a_station_ends_ready_to_dock_without_hull_damage`.
4. **Equipage qui meurt irradie en quelques jours, meme amarre** (« l'equipage meurt tout entier au jour 3 a
   14 » sur 7 graines sur 8 pour le scenario voyageur, pacifiste amarre) :
   - les abris de station ne laissaient passer que 5 % des ceintures et des eruptions ; un quai dans une
     ceinture a 0,03-0,14 Gy/h donnait de quoi tuer en un a deux mois sans rien faire : couverture a
     `DOCKED_COVER = 0.5 %` (ceinture de 0,1 Gy/h : sous 1 mGy/h, seuil de malaise) et les eruptions au double
     de cette valeur (`science.rs`). Test : `navigation::tests::a_station_shelters_its_crew_from_the_belts_and_the_flares`.
   - le « mal des rayons » (12 jours, severite 0,07 par jour, soit presque mortel sans medicament) etait
     donne des 0,3 Gy avec une probabilite egale a la dose : une eruption moderee tuait un equipage d'un
     seul membre. Seuil realiste : malaise a partir de 1 Gy en une fois, probabilite (dose - 0,5). Test :
     `science::tests::a_moderate_dose_does_not_give_radiation_sickness_but_a_big_one_can`.

Structures sauvegardees changees : **aucune** (`VERSION` de `save.rs` non touche). `save.rs` recoit seulement
la fonction d'outil `diff_fields` (non sauvegardee) et un test.

## Constats restants (non corriges)

- **Depart dans une ceinture de radiation (graines 1, 2, 4, 7 du voyageur ; 3 pour le naufrage a ciel
  ouvert)** : le vaisseau commence non amarre, a 3 km de la station, parfois dans une ceinture a
  0,03-0,15 Gy/h (ou un vent stellaire de 1,4 Gy/h sur la graine 3 : mortel en 5 heures). Un joueur qui
  s'amarre dans les premieres minutes s'en sort maintenant ; un joueur qui attend (ou la politique `idle`)
  meurt en 2 a 9 jours. Piste : demarrer amarre, ou choisir un foyer sans ceinture a la station dans
  `scene::pick_home`. Je n'ai pas change `pick_home` : cela change le systeme de depart de toutes les graines
  et casse cinq tests existants (autopilote, marche noir, loi, navigation, naufrage) et ceux que les autres
  agents ecrivent en parallele sur les memes graines.
- **Pas minimal au ras d'un monde** : un vaisseau qui rebondit a 15 m d'une planete a 30 km/s (relatif au
  referentiel etoile) descend a `MIN_DT` (1/240 s) : `advance` plafonne a 200 000 pas par appel, mais une
  partie « chaos » qui reste la coute plusieurs secondes de calcul par jour simule (1,2 million de pas en
  une trentaine de jours simules). A traiter dans le pilotage (acceleration du temps interdite pres du sol).
- **Cache `ship.stats`** : `radiate_kw` differe de 1e-5 entre l'etat vivant et l'etat recharge (cache
  derive, rafraichi au chargement). Sans consequence, filtre dans le robot.
- **Soute au-dessus de la capacite apres destruction d'un module de soute** : le total reste, la capacite
  baisse. Comportement de conception (ignore tant que des modules sont abimes), mais le joueur n'a pas de
  message ni de largage automatique.
- Equipage seul qui part faute de paie (`vivant 0/0`) apres 20 a 60 jours sans revenu avec les politiques
  inactives : comportement voulu (salaires impayes), pas un bug.

## Couverture et limites (honnetete)

- Le robot appelle les APIs de la Sim : il ne couvre ni le rendu, ni la souris, ni les ecrans de `crates/game`,
  ni l'audio. Il ne joue pas l'interface (pas de soft-lock d'ecran detecte ici).
- Les politiques n'ont pas de but ; elles n'optimisent rien. Les actions echouent souvent (« trop loin »,
  « pas amarre »...) : c'est voulu, on verifie surtout qu'elles ne cassent rien. Les systemes qui demandent une
  longue mise en place (terraformation avancee, projets de megastructures, flotte en route, epidemies a
  grande echelle) ne sont atteints qu'en partie, surtout via `builder` (2 colonies pendant 100 a 200 jours) ;
  la haute SF (`exotic_field`, relais, multivers) n'est touchee que par des appels au hasard, pas par un
  scenario soutenu.
- Soft-lock : le robot ne sait pas dire « plus rien ne peut avancer alors que le joueur est en vie »
  de facon generale ; il detecte les temps de calcul extremes, les equipages morts et le credit/carburant nuls
  en bilan. Aucun blocage de la sim (boucle infinie) n'a ete trouve : toutes les lenteurs sont des pas minimaux
  plafonnes.
- Performance mesuree sur une machine partagee avec d'autres agents (mesures bruitees).

## Performance (voir `qa` pour les chiffres du moment)

- Amarre : 10 a 45 ms par jour simule (un pas d'une heure, 24 pas par jour).
- Libre et inactif : 70 a 250 ms par jour (environ 3 000 a 4 000 pas par jour de 22 s, 25 a 90 microsecondes
  par pas).
- Pire cas : au ras d'un monde (voir plus haut) 2 a 4 s par jour simule, 1,2 million de pas.
- Actions les plus lentes mesurees : `exotic` (groupe d'appels haute SF) 30 ms, `jump` 5 ms ; toutes les autres
  sous 1 ms. Aucun point chaud evident n'a ete profile par fonction (pas de profileur disponible) : le cout
  par pas (25 a 90 microsecondes) est stable d'une politique a l'autre.
