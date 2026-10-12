# ACCEPTANCE — critères de la section 12, avec preuves

Mesures prises sur la machine de développement (Windows 11, build `--release`), le 2026-10-11.

| Critère | État | Preuve |
|---|---|---|
| Compte de contenu (ressources, recettes, modules, coques, bâtiments, armes, technologies, factions, compétences+traits, maladies, événements, gabarits de texte, types de planètes, classes stellaires, anomalies) | atteint | `tools/data_check` (voir `docs/CONTENT.md`) ; test `shipped_data_is_valid` |
| Moteur 100 % maison, dépendances limitées | atteint | `Cargo.toml` : winit, softbuffer, serde, ron, postcard, cpal |
| Simulation déterministe à pas fixe 30 Hz, séparation sim / rendu / UI | atteint | crates `sim` (aucune dépendance fenêtre) et `game` ; tests de déterminisme de la galaxie et des sauvegardes |
| Galaxie infinie procédurale, LOD | atteint | `world::galaxy` (secteurs hachés), `peek_star` vs `generate_system` (test de cohérence) |
| Bac à sable à règles | atteint | 13 options, chacune testée sur son effet réel (`rules_effects.rs`) ; menus et écran de règles (`menus_reach_rules_options_and_codex`) |
| Stabilité sur 100 ans | atteint | `sim_bench 100` : 26 s pour 100 ans de jeu (≈ 1,2·10⁸ × le temps réel), aucune valeur non finie, marchés sains |
| Stabilité en vol libre | atteint (lent, compris) | `sim_bench drift 1` : 17 s par an de jeu (≈ 1,8·10⁶ ×) = 1,06 M de pas de 29,6 s en moyenne à 16 µs ; à ×100 000 (plafond du jeu) cela fait environ 55 pas par image, soit 1 ms. Seul le banc de 100 ans en vol libre (≈ 30 min) est lent ; à quai : 8 760 pas par an |
| Codex pour chaque ressource, module, bâtiment, technologie | atteint | F6 ; `every_entry_of_every_category_has_real_data` (350+ entrées) |
| Tutoriel | atteint | 16 étapes pilotées par l'état ; `tutorial_advances_on_actions_and_can_be_skipped` |
| Aide contextuelle | atteint | F1 sur chaque écran ; `f1_toggles_contextual_help_on_every_screen` |
| Performance | atteint | rendu 1280×720 : 0,67 ms/image ; pas de simulation : 0,028 ms ; sauvegarde : 2,9 Ko en 0,22 ms (`vector-game --bench`) |
| Tests automatisés | 265 verts | `cargo test` ; `cargo clippy --all-targets -- -D warnings` propre ; `cargo fmt` appliqué |
| Vérification sans voir la fenêtre | atteint | `--snapshot` (PNG, `docs/snap_*.png`), `--script` (scripts rejoués par `cargo test`) |
| Documentation | atteint | `README.md`, `docs/` (PROGRESS, DECISIONS, CONTENT, CONTROLS, SIM_MODEL, ACCEPTANCE) |
| Paquet livrable | atteint | `tools/package.ps1` → `dist/VECTOR/` et `dist/VECTOR.zip` (1,8 Mo) |

## Limites connues (honnêtes)
- La souris fonctionne sur tous les écrans (clic, double clic, molette, clic droit, glisser le globe, menu de pilotage avec clic maintenu) mais n'a été vérifiée que par tests : aucune session humaine. La police 5×7 est en capitales avec accents (données accentuées par 	ools/accents.py ; les chaînes écrites dans le code Rust sont encore sans accents).
- Équilibrage : `balance_check` : 0 avertissement ; le recyclage est listé à part (perte voulue) et les chaînes très rentables (nanomatériaux, barres de combustible, thorium) sont plafonnées par le débit (profit par bâtiment et par jour < 20 000 cr). Pas de campagne de test humaine : l'équilibrage a été vérifié par simulation, pas par des joueurs.
- Audio : synthèse procédurale simple (grondement, moteur, minage, alarme, impact, bips) ; pas de musique. Non vérifié à l'oreille dans cet environnement : le test couvre le synthétiseur (borné, silencieux à volume 0, réactif aux paramètres) et l'ouverture du flux a réussi, sans plus.
- Pas de migration de sauvegarde entre versions du format : une sauvegarde d'une autre version est refusée avec un message qui donne les deux versions et conseille une nouvelle partie.
- L'ouverture réelle de la fenêtre a été vérifiée en lançant le binaire quelques secondes (processus vivant, aucune erreur) ; le rendu fin et la jouabilité ont été vérifiés par captures et scripts, pas par une session humaine.

## Session 2 (nouvelle machine)
Build release, 265 tests, clippy `-D warnings`, `data_check`, `balance_check` (0 avertissement), `sim_bench` 1/10/100 ans (stable, sans colonie) et les 10 scripts de `scripts/` (first_flight, dock_and_trade, full_loop, ftl_jump, leave_system, combat, colony, save_reload, surface, bourse) passent. Non vérifié : le ressenti du pilotage et de la souris, les couleurs en jeu réel, l'équilibrage des nouveautés (organes, implants, épidémies, civilisations, contrats à terme, équipement) par des joueurs ; `sim_bench` n'a pas tourné 100 ans avec des colonies. Aucune session jouée par un humain avant le retour du joueur (pilote automatique et sortie de système corrigés à sa suite).

## Départs et menu principal (2026-10-12)
| Critère | Etat | Preuve |
|---|---|---|
| Nouvelle partie en étapes claires (explications, scénario, vaisseau, équipage, galaxie, options, récapitulatif) | atteint | `the_new_game_flow_walks_back_and_forth_and_leaves_to_the_main_menu`, `the_flow_choices_reach_the_world_ship_crew_seed_and_rules` ; scènes `--snapshot x.png newgame0..6` |
| Chaque scénario produit une Sim valide et jouable | atteint | `every_scenario_builds_a_valid_sim_with_what_it_promises` (sim), `every_scenario_chosen_in_the_flow_starts_a_playable_game` (jeu : lance, dessine, avance, ouvre le menu) |
| Chaque vaisseau proposé est viable (moteur, énergie, réservoir, soute, logements, support de vie) | atteint | `every_ship_option_fits_its_hull_and_can_live_and_fly` |
| Le choix change l'état de départ (crédits, soute, modules, équipage, technologies, réputation, prime) | atteint | `a_scenario_is_not_the_default_one_it_changes_the_start`, `the_start_depends_on_the_choices_hull_crew_and_seed` |
| Budget de points de l'équipage | atteint | `the_crew_budget_is_enforced`, `the_crew_step_keeps_to_the_points_and_the_roles_change` |
| Naufrage : pas de vaisseau en état de vol, à pied, atteint l'espace avec ce qu'on trouve | atteint (mécanique testée ; équilibrage non joué) | `the_castaway_starts_on_the_ground_without_a_ship_that_flies`, `the_castaway_can_reach_space_by_mending_and_refuelling_with_what_he_finds`, `without_parts_the_wreck_stays_broken_and_an_empty_hold_gives_no_fuel` |
| Menu unique F10, huit espaces, sans doublon ; Echap remonte d'un niveau | atteint | `every_screen_has_a_place_and_every_space_screen_has_an_entry`, `the_menu_opens_on_every_screen_and_escape_climbs_one_level_at_a_time`, `menu_lines_that_need_a_dock_or_a_flight_say_why_and_work_when_they_can` |
| Tout cliquable (texte seulement) | atteint | `the_menu_and_the_flow_are_clickable_text` |
| Barre de commandes et aide F1 sur les nouveaux écrans | atteint | `every_screen_has_a_command_bar_a_help_and_a_place_in_the_menu` |
Non vérifié : le ressenti du Naufrage (combien de temps pour trouver les pièces et le carburant) par un joueur humain ; la lisibilité réelle des nouveaux écrans à d'autres résolutions que 1280x720.
