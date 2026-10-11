# CONTENT (inventaire réel — vérifié par `data_check`)

| Élément | Actuel | Cible |
|---|---|---|
| Ressources | 78 | >= 40 OK |
| Recettes | 106 | >= 100 OK |
| Modules de vaisseau (hors armes) | 74 | >= 60 OK |
| Armes / défenses | 21 | >= 20 OK |
| Coques | 15 | >= 15 OK |
| Bâtiments de colonie | 32 | >= 25 OK |
| Technologies | 73 | >= 60 OK |
| Factions | 8 | >= 6 OK |
| Compétences + traits | 12 + 32 = 44 | >= 40 OK |
| Maladies / blessures / états | 38 | >= 30 OK |
| Événements d'équipage | 155 | >= 150 OK |
| Gabarits de texte (variantes) | 300+ | >= 300 OK |
| Types de planètes | 12 | >= 12 OK |
| Classes stellaires | 8 (O B A F G K M D) | >= 8 OK |
| Anomalies | 12 | >= 10 OK |

Autres : 4 classes d'astéroïdes, 6 cultures de noms, 10 postes, 8 industries de station, 5 politiques de colonie.

Chaque ressource, module, arme, coque, bâtiment, recette, technologie, faction, anomalie, maladie et industrie a une entrée de codex (F6) générée depuis les données ; le test `every_entry_of_every_category_has_real_data` en vérifie plus de 350.

Régénération : `python tools/gen_content.py && python tools/gen_colony.py && python tools/gen_science.py && python tools/gen_scale.py` (dans cet ordre).

Régénération complète : python tools/gen_content.py ; python tools/gen_colony.py ; python tools/gen_science.py ; python tools/gen_scale.py ; python tools/accents.py (le résultat est identique octet pour octet à data/).
