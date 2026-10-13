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
