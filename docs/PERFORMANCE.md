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

## Monde unifie cockpit / vue externe (wt-scene)

`vector-game --bench-scene` (release, 1280x720, meilleur de 3 a 5 series sur machine partagee tres bruitee). Colonne "objets" : couche `scene_render::draw_objects` seule. Avant : meme bench sur le binaire de a8a06d2 (objets non mesurables seuls, total par image).

| Situation | Cockpit avant | Cockpit apres (objets) | Externe avant | Externe apres (objets) |
|---|---|---|---|---|
| Espace vide | 2,8 ms | 1,0 ms (0) | 7,5 ms | 2,8 ms (1,5 : vaisseau du joueur) |
| Station proche | 2,6 ms (filaire) | 4,1 ms (2,6, modele plein) | 97,7 ms | 82 ms (3,7) |
| 20 vaisseaux | 6,3 ms (filaire) | 4,5 ms (3,1, modeles pleins) | 95,6 ms | 74 ms (3,2) |
| Ceinture d'asteroides | 4,3 ms | 2,8 ms (0,9) | 30,8 ms | 4,1 ms (2,4) |
| Planete | 125 ms | 93 ms (0) | 161 ms | 108 ms (1,5) |

Les 75 a 110 ms restants en vue externe pres d'un monde et en cockpit face a une planete sont le dessin du globe (`draw_world`/terrain, domaine de wt-perf) : la couche des objets reste a 1 a 4 ms. Ce qui a change : les modeles (station en deux parties fixe + anneau tourne par la pose, rochers, vaisseaux PNJ et vaisseau du joueur) sont construits une fois et gardes dans un cache par thread (cle quantifiee ; flammes animees a 6 Hz seulement pres de la camera) ; le tampon de profondeur est reutilise et vide seulement si un objet plein est dessine ; `ship_model::draw` n'alloue plus par triangle ; `raster_tri` est un balayage de lignes (un intervalle par ligne, 1/z incremental) au lieu d'un test barycentrique sur tout le rectangle ; culling d'un modele par sphere avant tout travail. Avant, la vue externe reconstruisait la station et chaque vaisseau a chaque image. Non fait : rasterisation en bandes paralleles (la couche tient dans le budget sans).
