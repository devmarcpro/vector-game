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
Lois : data/laws.ron (7 lois, conditions sur les axes de mœurs, amende, gravité ; validées par data_check ; catégorie LOIS du codex générée depuis les données).
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

## Croyances
`data/religions.ron` : id, nom, intervalle de foi (axe foi des mœurs), bonus de moral, bonus de recherche, texte.

## Entreprises
`data/companies.ron` : id, nom, siège (faction), cours de base, rendement annuel, volatilité, biens (id, poids, total 1), texte.

## Energie (batiments)
`data/buildings.ron` : champ `source` ("solar", "wind", "geothermal") pour les centrales a production variable, champ `battery` (kWh) pour le stockage. Batiments ajoutes : `wind_turbine` (Parc eolien), `battery_bank` (Parc de batteries), techs `bld_wind_turbine`, `bld_battery_bank`. Entrees de codex generees depuis ces champs.
## Materiaux : champs de matiere (materials.ron)
`yields` [(ressource, kg, rarete)], `under` (sol mis a nu), `harm`, `slick`, `fertility`, `cost`
[(ressource, kg)] : tous optionnels, valides au chargement (ressources connues, `under` est un sol,
valeurs 0..1, un mur qui donne peut etre creuse). Nouveau : `soufre` (sol caustique des plaines de
lave). Le gear a `warmth` (protection contre le froid). Le codex des matieres liste ce qu'elles donnent,
laissent a nu, leurs dangers et leur cout de construction.
## Phenomenes exotiques
`data/exotic.ron` : id, nom, genre (matter, propulsion, field, entity, precursor, fermi), danger 0..1, recherche (min,max), credits (min,max), butin (ressource, min kg, max kg), texte. Champ optionnel `kardashev` (0..3,5) pour les civilisations. Les entrees `fermi` sont les reponses possibles au paradoxe de Fermi (une seule vraie par galaxie). Valide au chargement.
## Noms
`data/names.ron` : `voices` (id, start, mid, end, mid_chance), `words` (id, list), `patterns` (kind, voice, forms = (modele, poids)) et `cultures` (id, place = voix des lieux, words = listes propres a la culture, first_start/first_mid/first_end_m/first_end_f, last_start/last_mid/last_end, particles, nick_m/nick_f, title_m/title_f). Jetons des modeles : {root} {root2} {gr} {rom} {n} {n3} {n4} {cat} {l} {idx} {idx_l} {host} {label} {given} {surname} {person} {w:liste}. Chaque culture de `cultures_names.ron` doit avoir la meme id dans `names.ron` (teste). Le codex a une entree NOMS par culture, generee depuis la grammaire (exemples de prenoms, noms, surnoms, titres, villes, stations, fleuves, entreprises, vaisseaux).
## Chimie
`data/chemistry.ron` : `elements` (symbole, nom, z, masse molaire, electronegativite, fusion, ebullition, abondance solaire, texte) et `compounds` (id, nom, formule, classe, enthalpie de formation, fusion, ebullition, ressources, limite de toxicite, corrosivite, auto-inflammation + limite basse d'inflammabilite, comburant, texte). `data/reactions.ron` : id, nom, genre (synthese, combustion, explosion, electrolyse, reduction, corrosion, decomposition), reactifs et produits (corps, coefficient), t_act, p_min, catalyseur, texte. `data/resources_chemistry.ron` : ressources des reactifs. Codex : categories ELEMENTS, COMPOSES, REACTIONS (laboratoire) et ligne CHIMIE dans chaque ressource pure.
## Matières liées à l'eau
`data/materials.ron` : la matière `banquise` (sol, dureté 0,9) est celle des étangs, fleuves et lacs pris par un gel profond (< 262 K) : la grille à pied les rend praticables. L'eau (`eau`, `eau_gelee`) reste liquide. Les fleuves, lacs et nuages ne sont pas du contenu : ils sont dérivés de la carte (`world/hydro.rs`, `world/climate.rs`).
## Agents pathogènes
`data/epidemics.ron` : id, nom, origine (natural | lab | alien), déclencheur (any | crowd | hunger | water | wild | lab | alien), maladie de symptômes (`diseases.ron`, une par agent), R0, incubation, durée infectieuse, durée d'immunité (0 = à vie), létalité, poids de tirage, efficacité du vaccin, perte de production, effets sur les prix (ressource, variation à 10 % de malades), texte. Validé au chargement (12 au moins). Ressource `vaccine` (recette `make_vaccine`, usine chimique). Entrée du codex : AGENTS PATHOGENES.
- Armes (modules `kinetic`/`beam`) : `proj_mass`, `spread` (mrad), `wavelength` (nm), `aperture` (m), `flash` (kW) ; equipement : `noise` (cases). Fiches du codex generees.
- Monnaies : une par peuple (entrée MONNAIES du codex, générée depuis `data/factions.ron`) ; les paramètres macro sont calculés, pas écrits en données.
## Surprises
`data/surprises.ron` : id, nom, etiquette (une du fil de nouvelles), poids, delai (jours), `when` (conditions toutes vraies : WorldEvent, AtWar, Unrest, CartelHere, Incident, Epidemic, ColonyNear, ColonyUnrest, Territory, Claimed, Civ, Anomaly, AnomalyOf, Docked, InFlight, Credits, Contracts, Rel, Morale, Crew, Hold, Seat, Wanted, InDebt, Shares, Herd), `effects` (Credits, Rel, Bond, Shock, Cargo, Morale, Contract, Research, ColonyHappiness), `choices` (libelle, resultat, effets ; au plus 3, avec `window` jours pour repondre), texte avec {here} {ally} {rival} {place}. Cibles : Here, Ally, Rival, Contact, Civ, Faction(id), Cartel(id), Company(id). Validation au chargement : ids connus, bornes des effets, au moins une condition, un effet ou un choix. Chaque surprise a une entree SURPRISES du codex (conditions et effets decrits depuis les donnees) et un test qui la rend vraie, la declenche et verifie qu'elle change le monde.
Xenobiologie : 13 especes dans data/species_xeno.ron (ecrit a la main, ajoute a `species` au chargement) et 4 chimies dans data/xenobio.ron ; entrees FLORE/FAUNE (avec biochimie, metabolisme, cycle de vie, comportement) et categorie XENOBIOLOGIE du codex ; fiches de variantes generees par `codex::variant_card` (couche BIOSPHERE de l'ecran de surface, Entree).
## Esprit de l'IA
`data/mind.ron` : `tasks` (5 taches fixes : pilotage, analyse, traduction, simulation, defense) et `software` (id, nom, tache, bonus, memoire, prix, technologie requise ou "", texte). Validation : ordre des taches, logiciels uniques, technologies connues. Codex : LOGICIELS, TACHES DE L'IA.

## Histoire et artefacts
`data/history.ron` : `ages` (6 ages fixes), `causes` (id, nom, poids, texte) et `artefacts` (texte, carte, technologie, relique, outil : valeur de base, jours de traduction, texte). Codex : AGES DE L'HISTOIRE, CAUSES D'EFFONDREMENT, ARTEFACTS.
## Corps, blessures, medicaments
`data/body.ron` : id, nom, chance d'etre touchee (`hit`), extremite, organes abrites, effets du handicap total. `data/wounds.ron` : id, nom, saignement par heure, douleur, jours de guerison, risque d'infection, interne (invisible sans medecin competent ou infirmerie), demande un pansement, parties possibles, cause. `data/drugs.ron` : id, nom, ressource et dose, demi-vie, effets (pain, clot, infection, stress, fatigue, rad, heal, blood_now, resp, liver), tolerance et dependance par dose, interactions, texte. Valides au chargement (`medical.rs`) ; une entree de codex est generee pour chacun.
## Scénarios de départ
`data/scenarios.ron` : id, nom, difficulté indicative (1 à 5), vaisseaux au choix (coque, modules dans l'ordre de montage ; le premier est celui par défaut), crédits, soute (ressource, kg), équipage par défaut (postes) et points à dépenser, technologies connues, réputations de départ (« home » = le territoire du système de départ), jours de prime, `wreck` (départ au sol sur un vaisseau cassé), texte. Validé au chargement (références connues, au moins un départ en vol, au moins 6 scénarios) ; chaque vaisseau proposé est vérifié viable par `every_ship_option_fits_its_hull_and_can_live_and_fly`. Catégorie SCENARIOS du codex.
## Geologie
`data/geology.ron` : id, nom, groupe (limite, roche, gisement, volcan, erosion, relief), texte, rendements (id de ressource, part, total 1 pour un gisement). Les nombres vivent dans `world/geology.rs` ; chaque id d'un type du code (frontiere, roche, gisement, volcan) a son entree (test), le codex tire ses chiffres d'un monde type genere.

## Astronomie
`data/cosmos.ron` : id, nom, groupe (etoile, systeme, milieu, galaxie, asteroide), lien (classe d'asteroide de `asteroids.ron`), texte. Chaque stade d'etoile, chaque type d'objet du milieu interstellaire et chaque classe d'asteroide du code a son entree (test).

`data/society.ron` : langues (famille, peuples et cultures qui la parlent, annees par ere de derive, trois mots), fetes (moment de l'annee locale, duree, moral, moeurs requises), plats, arts (prix de base, gout sur un axe d'ethos, titres), sports (concurrents, marge), etudes (competence, langue ou art, jours, frais, gain). Valides au chargement ; une entree de codex est generee pour chacun (categories 47 a 52). `data/laws.ron` gagne la loi de quarantaine.
