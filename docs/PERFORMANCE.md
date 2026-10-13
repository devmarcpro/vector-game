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

## Menus du style NOUVEAU (toile 1280x720)
Mesure (release, machine chargee, menu F10, une image) : 1024x576 11 ms (reduction bilineaire), 1280x720 4,6 ms (direct), 1920x1080 8,4 ms et 2560x1440 12,3 ms (agrandissement au plus proche voisin : un simple parcours de table). Le bilineaire en agrandissement coutait 35 ms a 1920x1080 : abandonne. Toile, table d echantillonnage et zone d accueil des ecrans classiques sont gardees d une image a l autre. Test : `cargo test --release ui_frame_cost -- --ignored --nocapture`.
