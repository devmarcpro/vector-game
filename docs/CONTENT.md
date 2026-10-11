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

Biomes : 20 entrées dans data/biomes.ron (catégorie BIOMES du codex, entièrement générée depuis les données ; la table est validée par couverture complète).
Espèces : 58 dans data/species.ron (python tools/gen_life.py, à lancer avec les autres générateurs ; catégories FLORE et FAUNE du codex). Politiques de colonie : 7 (cueillette et élevage, quarantaine biologique ajoutées).
Mœurs : data/ethos.ron (8 axes, 8 étiquettes morales, 8 profils de faction), généré à la main et validé par data_check.
## Organes
`data/organs.ron` : coeur, poumons, foie, reins, cerveau (vitaux), estomac, yeux, muscles, os, peau. Chacun : poids dans l'état général, guérison par jour, valeur marchande, effets quand il est détruit, maladies qui l'attaquent. Codex : catégorie ORGANES.

## Dialogues
`data/dialogues.ron` : noeuds avec textes par ton, options (`cond`, `effects`, `next`). Conditions : rep, credits, cargo:<res>, ethos:<axe>, role:<ROLE>, hostile. Effets : rep, credits, give, news, region, tip, peace, hostile, bluff, bribe.

## Sites d'expédition
`data/sites.ron` : id, nom, symbole, couleur, poids, butin, danger, soin, texte.

## Civilisations
`data/civilisations.ron` : id, nom, technologie 1-4, description, mœurs (une valeur par axe de ethos.ron), biens offerts et demandés, syllabes des noms. Dialogues : rôle CIVILISATION, effets `mores` et `civgift`.

## Équipement
`data/gear.ron` : id, nom, emplacement (weapon, armor, tool), attaque, défense (0 à 0.9), récolte, valeur, texte. Chaque objet est aussi une marchandise `gear_<id>` générée par `tools/gen_organs.py`.
