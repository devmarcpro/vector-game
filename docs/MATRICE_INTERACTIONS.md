# Matrice des interactions entre systemes (audit de l'emergence)

Audit fait sur la branche `wt-auemerg` (base `master` d6f6659). Principe du projet : le gameplay
emergent vient de systemes qui interagissent, un systeme = une representation, chacun chez soi.
Ce document dit ou en est chaque couple de systemes, ou le lien manque, ce qui est ecrit sans
jamais etre appele, et ce qui a ete branche pendant l'audit (un test par lien, fichier
`crates/sim/src/emergence_tests.rs` et tests voisins, noms `link_*`).

Deuxieme passe (branche `wt-links`, base `master` 74c3b0b) : les 13 liens manquants de la liste
du bas sont branches (cases `+` de la matrice, section "Liens branches par la deuxieme passe"),
les 46 orphelins sont resolus (section "Orphelins"). Un test par lien dans
`crates/sim/src/links_tests.rs` (noms `link_*`) et, pour le minage a pied,
`rogue::tests::the_ore_bodies_under_the_landing_cell_give_their_ore_to_the_pick`.

## Methode

1. Couplage statique : pour chaque module de `crates/sim/src`, quels noms publics (fonctions,
   types, champs) des autres modules il utilise hors tests. Cela donne la carte des appels reels ;
   les modules a faible entree (dialogue, gear, treat, geo, astro, history, intrusion, atmosphere)
   ont ete lus de pres.
2. Lecture des fonctions de liaison : chaque lien de la matrice cite le fichier et la fonction qui
   lit ou ecrit l'etat de l'autre systeme. Un lien est EXISTANT s'il y en a au moins un, PARTIEL
   s'il ne couvre qu'un cas, ABSENT sinon.
3. Orphelins : toute `pub fn` et tout champ `pub` dont le nom n'apparait nulle part hors tests
   (script du scratchpad `auemerg_orph.py`), fichiers `.ron` jamais charges, ecrans de la boucle de
   jeu inaccessibles (aucun).
4. Scenarios joues avec les vraies APIs : `crates/tools/src/bin/emerg.rs` (nouveau, voir plus bas),
   `qa.rs` (existant), tests Rust.

## Lire la matrice

Ligne = le systeme qui agit, colonne = celui sur lequel il agit (lit ou ecrit son etat).
`E` le lien existe, `P` partiel, `A` absent mais utile, `+` branche par cet audit,
`.` la diagonale, vide = pas de lien ou sans interet pour le jeu. Les cases `E` ou l'audit a
seulement AJOUTE un cas (alertes, nouvelles) portent `AJOUT` dans la liste plus bas.

| de \ vers | CLI | AGR | ECO | REL | GUE | EPI | COL | SUR | NEW | ALE | RAD | COR | CRY | EQU | TER | BIO | GEO | MIN | LAN | DIA | LOI | MND | NAV | SHP | VOL | HSF | HIS |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **CLI** | . | E |  |  |  |  | E |  |  |  |  |  |  | P |  | E |  |  |  |  |  |  |  |  | E |  |  |
| **AGR** |  | . | E |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **ECO** |  | E | . | E | E |  | + |  |  |  |  |  |  |  |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **REL** |  |  | E | . |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  | E |  |  |  |  |
| **GUE** |  | P | E | E | . |  | E | E | E | E |  |  |  | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **EPI** |  |  | E |  |  | . | E | + | E | E |  | E |  | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **COL** | + |  | + | + | P | E | . | E | E | E |  |  |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  |
| **SUR** |  |  | E | E |  |  | E | . | E |  |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **NEW** |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **ALE** |  |  |  |  |  |  |  |  |  | . |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **RAD** |  |  |  |  |  |  |  | + |  | E | . | E | E |  |  |  |  |  |  |  |  |  | E |  |  |  |  |
| **COR** |  |  | E |  |  |  |  |  |  | E |  | . |  | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **CRY** |  |  | E |  |  |  | E | + | P | E |  |  | . | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **EQU** |  |  | E |  |  |  |  | E |  |  |  | + |  | . |  |  |  |  | E |  |  |  |  |  |  |  |  |
| **TER** | E |  |  |  |  |  | E | + | P | + |  |  |  |  | . | E |  |  |  |  |  |  |  |  |  |  |  |
| **BIO** |  | E |  |  |  |  | E |  |  | + |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |  |  |
| **GEO** | E |  |  |  |  |  | E |  | A |  |  |  |  |  |  |  | . | + |  |  |  |  | P |  |  |  |  |
| **MIN** |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |
| **LAN** |  |  | E | E |  |  |  | + |  |  |  |  |  |  |  |  |  |  | . | + |  |  |  |  |  |  | E |
| **DIA** |  |  | E | E |  |  |  |  | + |  |  |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |
| **LOI** |  |  | E | E |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  |  | . |  | E |  |  |  |  |
| **MND** |  |  | + |  |  |  |  |  |  | E |  |  |  |  |  |  |  |  | E |  | E | . | + |  |  | E | E |
| **NAV** |  |  | P |  |  |  |  |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  | . |  |  |  |  |
| **SHP** |  |  | E |  |  |  |  |  |  |  |  |  | E |  |  |  |  |  |  |  |  |  | E | . | E |  |  |
| **VOL** | + |  | E |  |  |  |  |  |  | E |  | + |  |  |  |  |  |  |  |  |  |  |  |  | . |  |  |
| **HSF** |  |  | + |  |  |  |  |  | P |  |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  | . | P |
| **HIS** |  |  | E | E |  |  |  |  | + |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | . |

### Systemes

- CLI : Climat, meteo, atmosphere (planet.rs, atmosphere.rs, vector_world::climate)
- AGR : Agriculture (colony_works::farm_tick)
- ECO : Economie et prix (economy, macroeco, market, bourse, trading, futures)
- REL : Relations et reputation (relations, civ, economy::change_reputation)
- GUE : Militaire et guerre (arms, politics, fleet, combat)
- EPI : Epidemies (epidemic)
- COL : Colonies (colony, colony_life, colony_works)
- SUR : Surprises (surprises, data/surprises.ron)
- NEW : Nouvelles (news)
- ALE : Alertes (alerts)
- RAD : Radiation et dangers (science::radiation_tick, hazard_tick)
- COR : Corps et medecine (body, treat, surgery)
- CRY : Cryogenisation (cryo)
- EQU : Equipage (crew, births, genealogy)
- TER : Terraformation et megastructures (terraform, megastructure)
- BIO : Biosphere et ecosysteme (biosphere, ecology, livestock)
- GEO : Geologie et astronomie (geo, astro, cosmos)
- MIN : Minage et industrie (mine_tick, colony_works industrie)
- LAN : Langues et societe (society)
- DIA : Dialogues (dialogue)
- LOI : Lois et police (law, customs)
- MND : Esprit de l'IA (mind)
- NAV : Navigation (nav_auto, nav_plan, nav_predict, autopilot, navigation)
- SHP : Chantier et editeur de vaisseau (shipwright, yard, design)
- VOL : Vol, atterrissage et degats (landing, lib::hurt_ship, collide)
- HSF : Haute SF (exotic, multiverse, exotic_field)
- HIS : Histoire et archeologie (history, archive)

## Liens, avec fichiers et fonctions

### Liens existants
- CLI -> AGR : colony_works::apply_works_world (pluie, chaleur, poussiere du jour), farm_tick (temperature saisonniere Env::temp_at)
- CLI -> COL : planet::apply_catastrophes (secheresse, blizzard, eruption, seisme) -> colony_works/colony; colony::solar_transmittance(pression)
- CLI -> VOL : lib::atmosphere_drag, landing (densite de l'air)
- CLI -> BIO : biosphere (plages de temperature des especes), ecology (biomes)
- AGR -> COL : farm_tick -> storage -> food_sat -> croissance et bonheur de colony::tick
- AGR -> ECO : colony_works::trade_tick (imports/exports de vivres au prix local), macroeco::sector_with(extra) via economy.rs:705 (production des colonies dans le PIB)
- ECO -> AGR : bourse::world_factors -> works.food_price ; AJOUT : macroeco::harvest -> works.harvest -> rendement des champs ouverts
- ECO -> REL : economy::trade_edge (reputation -> prix), lib::buy/sell (change_reputation), blackmarket
- ECO -> LOI : economy::customs_check (marchandises interdites), trading (fraude)
- ECO -> GUE : politics::Turmoil::factor (guerre, blocus +30 %), arms::BLOCKADE_MARKUP, arms::prize_tick
- GUE -> ECO : politics::Turmoil::factor via bourse::world_factors ; trading::cartel_factor
- GUE -> COL : colony_works::apply_works_world (works.siege : plus de convois), colony_works siege (obus)
- GUE -> REL : arms.rs (crimes de guerre, cessez-le-feu, victoire : change_reputation)
- GUE -> NEW : news::news_tick (PolKind::War/Peace) ; AJOUT : blocus (is_besieged) debut et fin
- GUE -> SUR : surprises Cond::AtWar
- GUE -> ALE : alerts::world_alerts (secteur en guerre et bloque : prix majores)
- GUE -> LOI : law::offend (agression, meurtre, crime_de_guerre)
- GUE -> EQU : combat::damage_player -> science::injure_crew
- EPI -> COL : epidemic::epidemics_tick (colony.rs:947), contagion croisee colonies/secteur
- EPI -> ECO : bourse::world_factors (epidemic::goods_factors), macroeco::production, trading (prime d'assurance)
- EPI -> NEW : news::news_tick (declarations_in_cell)
- EPI -> ALE : alerts (epidemie du secteur, colonie malade)
- EPI -> EQU : epidemic -> crew (maladie d'un equipier = diseases.ron), epidemic_tick (voyage)
- EPI -> COR : un seul systeme de maladie : Person.disease + body
- EPI -> LOI : society::quarantine_check -> law::offend(quarantaine)
- COL -> ECO : economy.rs:705 sector_with(extra) : le PIB du secteur inclut la production des colonies
- COL -> NEW : news::news_tick (journaux des colonies, avec retard de courrier)
- COL -> SUR : surprises Cond::ColonyNear/ColonyUnrest/Epidemic ; AJOUT : Cond::Terraform
- COL -> ALE : alerts::world_alerts (agitation, penurie, tresorerie, malades) ; AJOUT : ravageurs
- COL -> BIO : apply_introductions, apply_ecology, Colony.pest
- COL -> EPI : epidemic (contagion equipage <-> colonie a l'amarrage)
- COL -> MIN : colony_works (industries, recettes, drillers)
- TER -> CLI : terraform::Terra::apply_to_map -> planet::planet_map (temperature, niveau des mers, biomes)
- TER -> BIO : terraform::sync_terra_air, biosphere (O2 d'un monde vivant)
- TER -> COL : megastructure::open_air_housing, climat de la ville
- BIO -> AGR : colony.pest -> farm_tick (parasites)
- BIO -> COL : ecology (especes introduites, chasse, recoltes)
- GEO -> COL : geo.rs : gisements du site, chaleur du sol (geothermique), failles et volcans (planet::apply_catastrophes)
- GEO -> CLI : planet.rs (seismes et eruptions sur les failles)
- MIN -> ECO : lib::mine_tick -> cargo -> market::sell (le stock monte, le prix glisse)
- RAD -> COR : science::radiation_tick -> body.pending_dose / acute_dose
- RAD -> CRY : cryo : la dose s'accumule en dormant (DOSE_RISK, LETHAL_DOSE_SV)
- RAD -> ALE : alerts (debit de dose, dose cumulee du vaisseau) ; AJOUT : dose aigue de chaque personne, cryo compris
- RAD -> NAV : nav_auto limits.dose_return (retour d'urgence)
- COR -> EQU : crew.rs <-> body.rs (sante, blessures, soins)
- COR -> ALE : alerts::crew_alerts
- COR -> LOI : surgery::moral_offense (chirurgie, clonage, prelevement, mutation)
- COR -> ECO : surgery (greffes, organes en soute, prix)
- CRY -> EQU : cryo_tick, crew::update (ne mange plus, vieillit au rythme de la temperature)
- CRY -> ECO : retenue de salaire (RETAINER), pieces detachees des caissons
- CRY -> LOI : law::offend(cryo_forcee, cryo_interdite)
- CRY -> COL : colony::found_colony (colons en caissons = kit)
- NAV -> CRY : nav_auto : reveil planifie a l'arrivee (cryo_arrival_wake)
- CRY -> ALE : alerts (dormeurs sans caisson, pieces) ; cryo -> alertes critiques (reveil programme)
- EQU -> ECO : crew::daily_payroll, accounts_tick
- EQU -> SUR : surprises Cond::Morale/Crew
- EQU -> LAN : society::fluency (langue maternelle, apprise, famille)
- LAN -> ECO : society::language_edge -> economy::trade_edge (0,97 .. 1)
- LAN -> REL : civ::change_civ_reputation (une faveur non comprise vaut moins)
- LAN -> HIS : archive : traduction des trouvailles (budget traduction), langues qui derivent avec les ages
- DIA -> REL : dialogue Effect::Rep
- DIA -> ECO : dialogue Effect::Credits/Give/Bribe
- LOI -> REL : law::offend -> change_reputation
- LOI -> ECO : amendes, saisies, marchandises interdites (ethos), assurance (fraude)
- LOI -> NAV : lib::try_dock : mandat d'arret = quais fermes
- LOI -> ALE : alerts (mandat, notoriete, amendes) ; AJOUT : cargaison interdite a bord
- LOI -> SUR : surprises Cond::Wanted
- MND -> HIS : archive : analyse (datation), traduction
- MND -> LAN : society::ai_fluency (budget traduction)
- MND -> LOI : intrusion : defense contre les intrusions (mind_factor T_DEFENSE)
- MND -> ALE : alerts (logiciel malveillant) ; AJOUT : esprit ralenti par la chaleur ou le bus d'energie
- MND -> HSF : multiverse : sauvegardes de soi = ancres
- NAV -> RAD : nav_auto : dose_return
- SHP -> VOL : shipwright::analyse -> hulls (masse, inertie, couple parasite, ang_accel) -> landing/lib
- SHP -> NAV : delta-v, isp, poussee -> autopilot, nav_predict
- SHP -> CRY : les caissons sont des modules (category cryo)
- SHP -> ECO : yard : commandes, cout des modules
- VOL -> ALE : alerts : impact imminent, coque, structure
- VOL -> ECO : repair (cout), assurance
- HSF -> EQU : multiverse : paradoxe -> instabilite des equipiers
- HIS -> REL : archive : restituer aux heritiers, piller (change_reputation)
- HIS -> ECO : archive : collectionneurs, musees
- SUR -> ECO : surprises : chocs de prix, credits, contrats
- SUR -> REL : surprises : Effect::Rel/Bond
- SUR -> NEW : fire_surprise raconte dans les nouvelles
- SUR -> COL : Effect::ColonyHappiness
- SUR -> EQU : Effect::Morale
- REL -> ECO : economy::trade_edge (reputation -> prix d'achat et de vente)
- REL -> SUR : surprises Cond::Rel
- REL -> NAV : lib::try_dock : reputation hostile = quais fermes
- ALE -> CRY : cryo : reveil programme sur alerte critique

### Liens branches par cet audit
- TER -> SUR : Cond::Terraform (travaux en cours dans une colonie proche) + surprise vocations_de_chantier
- BIO -> ALE : alerts : ravageurs d'une colonie (Colony.pest)
- RAD -> SUR : Cond::Dose + surprise compteurs_inquietants
- CRY -> SUR : Cond::Cryo + surprise curieux_aux_caissons
- LAN -> DIA : dialogue::apply_dialogue_effect : meme regle pour les factions
- LAN -> SUR : Cond::Language + surprises malentendu_au_comptoir, porte_ouverte_par_la_langue
- VOL -> COR : lib::hurt_ship -> science::injure_crew (crash, bascule, amarrage dur blessent l'equipage)
- EQU -> COR : gear (armure portee, defense) -> science::injure_crew : l'armure arrete une part des blessures
- ECO -> COL : fleet::fleet_unload : la colonie paie le transporteur sur sa tresorerie (avant : credits crees de rien)

### Liens branches par la deuxieme passe (`wt-links`)

Chaque lien : fichier::fonction, effet, test. Toutes les grandeurs sont bornees (voir "Boucles").

- COL -> CLI (1) : `colony_works::city_tick` garde les unites d'emission du jour (`Works::emitted`),
  `megastructure::mega_tick` les remet a `terraform::Terra::emit_co2` (`CO2_PER_EMISSION_DAY`
  = 1e9 kg de CO2 par unite et par jour). Le carbone et l'oxygene qui manque a l'air viennent du
  sol (`Terra::ground`), l'oxygene de l'air qui brule est pris a l'air : `closed_ledger` reste
  constant, l'effet de serre fait le reste (chaleur, calottes, vapeur). Test :
  `link_pollution_to_atmosphere_*` (bilan de masse a 1e-9, colonie industrielle contre colonie
  propre, bornes).
- COL -> REL (2) : `links::colony_standing` (prosperite, impots, crimes, agitation, tresorerie a sec,
  malades ; x1,5 en temps de guerre) et `links::colony_relations_tick` (appele par
  `colony_works::apply_works_world`) : la bonne volonte de la faction du secteur derive par lots de
  0,5 point, plafonnee a +-60 (`COLONY_REP_CAP`). Test : `link_colony_to_faction_*`.
- GUE (3, un seul modele du tir) : `combat::Gun` (masse, vitesse, cadence, dispersion) pour les
  vaisseaux non joueurs. Les degats d'un obus sont `arms::damage_from_energy(arms::kinetic_energy_j)`,
  la probabilite de toucher est `arms::hit_chance(dispersion, distance, AIM_RADIUS)` ;
  `NpcStats::dps` en est derive (valeurs de l'ancienne table conservees a 1e-6) et
  `combat::npc_combat` n'a plus de formule de precision a part. Test : `link_combat_*`.
- MND -> NAV (4) : `nav_predict::nav_step_angle` (pas d'integration de `predict_ship`, jusqu'a 30 %
  plus gros quand le budget de pilotage est pauvre) et `nav_predict::nav_reserve` (la reserve de
  carburant du planificateur `nav_plan` x 0,5 a x 1,5 selon `mind_factor(T_PILOTAGE)`). Test :
  `link_mind_to_navigation_*`.
- TER -> ALE / NEW (5) : `terraform::Terra::alarm` (emballement de serre, oceans en ebullition,
  glaciation totale : critiques ; derive du climat >= 1 K/an sur un monde encore habitable :
  attention) lu par `alerts::world_alerts` ; les seuils franchis restaient dits par
  `Terra::notes` -> journal de la colonie -> `news::news_tick`. Test : `link_terraform_to_alerts_*`.
- GEO -> MIN (6) : `geo::veins_at` (gisements de `geology.ron` autour de la case d'atterrissage, avec
  le budget d'analyse du scanner `T_ANALYSE`) lu par `rogue::dig_context` : `rogue_dig` sur de la
  roche dure rend aussi le minerai du gisement. Les gisements restent visibles sur la carte de
  surface (`surface_view::geology_cell`). Test :
  `rogue::tests::the_ore_bodies_under_the_landing_cell_give_their_ore_to_the_pick`.
- VOL <- CLI (7) : `links::SiteWeather` (meteo de la case sous le vaisseau, tiree une fois par jour
  et region, gardee dans `Sim::site_weather_cache`) : le vent deplace l'air de
  `orbit::atmosphere_drag` (dernieres 8 km), l'adherence (`landing::Touch::grip`) diminue la vitesse
  tangentielle que le train absorbe (derapage, degats), `landing::Readout::weather` et
  `Topic::Weather` (alerte "METEO AU SOL"). Test : `link_weather_to_landing_*`.
- HIS -> NEW (8) : `history::anniversaries` (un siecle apres les sommets, chutes et reconquetes
  de la chronique du secteur) lu par `news::news_tick` avec le nom `Marche de ...` du lieu ;
  `archive::dig_site` raconte les ruines, epaves et stations fouillees (`provenance_line`). Test :
  `link_history_to_news_*`.
- COL <-> ECO volume (9) : `colony_works::trade_tick` note ce qui passe par le port (`Works::trade_flow`),
  `links::settle_station_food` (appele par `colony::update_colonies`) retire (importations) ou ajoute
  (exportations) ce volume au stock du marche de la station la plus proche, entre 0 et 6 fois son stock
  habituel (`MARKET_ROOM`) ; le reste vient du secteur par les lignes regulieres. Le joueur voit le prix
  glisser sur place (`Market::scarcity_at`). Le prix de la colonie reste celui du secteur. Test :
  `link_colony_trade_to_market_*` (et la borne sur dix ans).
- EPI -> SUR (10) : `Cond::SectorOutbreak` (epidemie du secteur du vaisseau, plus seulement des
  colonies) lu par `surprises::surprise_ctx`, surprise `foyer_sur_le_quai`. Test :
  `link_epidemic_to_surprise_*`.
- MND -> ECO (11) : `lanes::broker_fee` (frais du courtier x 0,5 a x 1,5 selon le budget de
  simulation) et `trading::arbitrage_breadth` (6 a 24 systemes etudies par le tableau d'arbitrage).
  Test : `link_mind_to_economy_*`.
- HSF -> ECO (12) : `multiverse::dimension_price_factor` (matiere brute moins chere ou la roche est
  plus riche, matiere exotique moins chere ou les champs sont plus forts) lu par `Sim::market` ;
  la balise morale `exotique` de `ethos.ron` fait de l'antimatiere, de la matiere noire et des
  nanites un produit de demande, de contrebande et de marche noir selon les moeurs. Test :
  `link_dimension_to_economy_*`.
- DIA -> NEW (13) : `dialogue::dialogue_trace` (pot-de-vin, bluff, tribut, cadeau d'un peuple) et
  `radio_region` qui nomme la marche de l'etoile voisine (`history::chronicle`). Tests :
  `link_dialogue_to_news_*`, `link_history_to_dialogue_*`.

### Liens partiels
- CLI -> EQU : body::Exposure (temp_k) : seulement l'habitacle et la marche a pied, pas la meteo du site d'atterrissage
- GUE -> AGR : le blocus coupe les convois et l'import, mais aucun champ n'est detruit
- COL -> GUE : megastructure::apply_mega_threats (les grands ouvrages attirent pirates et guerre)
- TER -> NEW : les seuils de Terra (emballement, glaciation, extinction) passent par le journal de la colonie puis les nouvelles ; la derive lente n'a pas de nouvelle propre (elle a l'alerte)
- GEO -> NAV : astro/cosmos -> science.rs (environnement, ondes gravitationnelles) mais pas la planification
- CRY -> NEW : au reveil, le dormeur 'trouve le monde change' (nouvelles depuis), mais le flux de nouvelles n'est pas prefiltre
- NAV -> ECO : refuel au prix local, contrats ; le plan de route ne lit pas les prix du carburant le long du trajet
- HSF -> HIS : exotic : ruines precurseurs, mais pas dans history::provenance
- HSF -> NEW : rumeurs certaines apres un retour en arriere

### Liens absents
- GEO -> NEW : aucune nouvelle de seisme/eruption hors journal de colonie
- MND -> ECO (reste) : louer ou miner du calcul n'existe pas ; seuls le courtage et l'arbitrage lisent le budget


Autres changements de cet audit, qui ne sont pas des couples de la matrice :

- `found_colony` refuse un corps sans site d'habitation (`colony.rs`) : plus de kit perdu sur un geant gazeux.
- L'entretien d'une colonie sans population ne court plus (`colony.rs`).
- `fleet_unload` : la colonie paie le transporteur sur sa tresorerie (`fleet.rs`).
- Alerte de cargaison interdite par la loi locale (`alerts.rs`, LOI -> ALE), alerte d'esprit ralenti
  (`Topic::Mind`, ecran TECH), alerte de ravageurs de colonie, alerte de dose aigue par personne.
- `modules.ron` : ISP des moteurs exotiques sous 0,2 c ; delta-v en km/s dans le concepteur et la
  creation de partie.

## Liens manquants, classes par valeur de gameplay

(Etat apres la deuxieme passe : les 13 lignes sont branchees, voir "Liens branches par la deuxieme passe".)

Valeur = ce que le joueur (l'IA du vaisseau) verrait changer. Effort : S (quelques lignes), M (un
module), L (un systeme de plus).

| # | Lien | Pourquoi | Effort |
|---|---|---|---|
| 1 (FAIT) | COL -> CLI : l'industrie d'une colonie ajoute ses gaz et sa suie aux stocks de `Terra` | Le changement climatique d'origine humaine, le plus emergent possible : la pollution (`works.pollution`) ne sert aujourd'hui qu'a abimer les cultures. Il faut brûler des ressources (carbone des carburants) pour respecter le bilan de masse que `terraform` verifie | L |
| 2 (FAIT) | COL -> REL : une colonie en territoire d'une faction (impots, ordre, agitation, port ouvert) change la relation avec elle | Aujourd'hui fonder trois colonies ne change rien a la politique | M |
| 3 (FAIT) | Combat : tirs et degats par `arms::hit_chance`, `arms::damage_from_energy` | La physique des armes (dispersion, energie cinetique) existe et n'est jamais appelee : le combat utilise un DPS x precision abstrait. Deux representations du meme tir | M |
| 4 (FAIT) | MND -> NAV : le budget `T_PILOTAGE` aux plans de `nav_auto`/`nav_plan`/`nav_predict` (marge de reserve, qualite de la trajectoire prevue) | Seul l'ancien `autopilot` lit le calcul ; la navigation automatique, le systeme le plus recent, l'ignore | S |
| 5 (FAIT) | TER -> ALE / NEW : monde en emballement (gaz a effet de serre), habitabilite qui chute, seuils franchis | Un chantier de terraformation peut mal tourner sans que rien ne le dise | S |
| 6 (FAIT) | GEO -> MIN : un forage pose sur un monde lit `geology.ron` (gisement du site) | Les gisements ne servent qu'aux colonies | M |
| 7 (FAIT) | VOL <- CLI : vent, poussiere, orage du site (`planet::weather_at`) sur l'approche et le touche | La meteo existe pour les villes, pas pour le pilote | M |
| 8 (FAIT) | HIS -> NEW : ruines decouvertes, ages et chutes des secteurs dans le fil des nouvelles | L'histoire est calculee et jamais racontee | S |
| 9 (FAIT) | COL <-> ECO volume : imports/exports de vivres d'une colonie vont dans le stock du marche de la station du systeme | Aujourd'hui la colonie achete au prix local sans le deplacer | M |
| 10 (FAIT) | EPI -> SUR : `Cond::Epidemic` lisant aussi l'epidemie du secteur (pas seulement les colonies) | Les surprises sanitaires ne partent que des colonies | S |
| 11 (FAIT) | MND -> ECO : louer ou miner du calcul | Le calcul de l'IA ne rapporte ni ne coute | M |
| 12 (FAIT) | HSF -> ECO : prix et contrats dans les dimensions et apres un retour en arriere | Les portails ne touchent pas l'economie | M |
| 13 (FAIT) | DIA -> NEW : un dialogue important laisse une trace dans le fil | | S |

## Orphelins

Mesures par recherche de noms hors tests ; "orphelin" veut dire : ni la boucle de jeu, ni un ecran,
ni un outil n'appelle la chose (elle n'est utilisee que par des tests, ou par rien).

### Fonctions publiques jamais appelees (46) : resolues

Chaque fonction est soit branchee, soit supprimee avec ses tests ; ce qui reste est dit a la fin.

- Branchees : `arms::hit_chance`, `arms::damage_from_energy` (combat, lien 3) ;
  `navigation::tsiolkovsky`, `navigation::propellant_for` (deja la seule loi du delta-v, audit des
  doublons A2) ; `landing::time_to_burn` (`Readout::t_burn` : "ALLUMAGE A x M (DANS y S)") ;
  `lib::shop_sell_module` (touche V du choix de module du chantier) ;
  `archive::artefact_true_value` (la fiche d'un faux datee donne sa valeur reelle) ;
  `law::is_illegal` (`offend` la consulte avant de tirer le de) ; `shipwright::Report::buildable`
  (concepteur et creation de partie) ; `GameData::reaction_index` (`rogue::blast_damage`).
- Supprimees (code mort ou deuxieme representation) : `navigation::{ir_detection_range,
  launch_bonus (la rotation du monde est dans la vitesse du sol), j2_of, j2_from,
  nodal_precession (rien dans la prediction ne precesse : l'afficher serait faux), equilibrium_k
  (doublon de terraform, B7), burn_energy_gain, equator_speed, IR_SENSOR_W_M2}` ;
  `nav_auto::{Order::is_macro, nav_set_guard, nav_halt}` ; `nav_predict::predict` ;
  `macroeco::price_level` ; `securities::after_fee` ; `terraform::Terra::mean_mass` (l'air a sa masse
  molaire dans `atmosphere`) ; `dialogue::npc_tone` ; `start::{crew_pool, crew_valid}` (enveloppes de
  `crew_pool_for` et `crew_valid_with`) ; `chemistry::{Mixture, ChemError, max_extent, react,
  lab_reactions}` (le laboratoire deplace de la vraie cargaison : `lab_run` ; les tests de bilan
  d'atomes, de Hess et de limites du reacteur passent par les donnees et par `lab_run`) ;
  `shipwright::Blueprint::mirror_y` ; `fleet::buy_fleet_ship_free_pub` ; `planet::sun_declination` ;
  `world::Galaxy::distant_galaxy_name` ; `world::System::star_physics` (et `StarPhysics`) ;
  `world::StarStage::is_compact` ; `GameData::{age_index, industry_index}`. (Deja supprimes par
  l'audit des doublons : `gravity_excluding`, `light_delay_to`, `color_class`, `project_years`,
  `beamed_power_w`.)
- Gardes (accesseurs de table de `GameData`, comme les quarante autres `*_index` ; seuls les tests
  des trois crates les appellent, un `#[cfg(test)]` ne se voit pas d'un crate a l'autre) :
  `gear_index`, `cyber_index`, `ai_trait_index`, `background_index`.

Aucun champ `pub` du sim n'est ecrit sans etre lu. Aucun champ de `data/*.ron` n'a un nom que
personne ne lit (controle grossier par nom).

### Donnees jamais consommees

- `data/names.ron` : charge ; la grammaire est embarquee par `include_str!` dans
  `crates/world/src/names.rs` (`GRAMMAR`), c'est la meme source (une seule copie, pas de
  duplication) ; l'audit precedent s'etait trompe. Les noms de personnes viennent de
  `cultures_names.ron`.
- Tous les autres `.ron` sont charges ; les 12 actes de `laws.ron` sont reportes par au moins un
  appel a `offend` ou `moral_offense`.

### Ecrans et touches

Les 37 variantes de `Screen` sont atteignables (par le hub, un raccourci ou un ecran parent).
Le recensement des touches non documentees dans `docs/CONTROLS.md` n'a pas ete fait : il demande
un parcours des gestionnaires de touches de ~45 ecrans, a faire avec la capture d'ecran.

### Effets sans effet observable

- `colony.works.rain/harsh/siege/food_price/harvest` sont recalcules par le sim a chaque tick
  (non sauves) : ils changent bien le rendement et les imports.
- Corrige : `fleet::fleet_unload` ecrivait `c.treasury -= 0.0` (la colonie qui recevait des biens ne
  payait rien et le transporteur etait paye de rien).

## Boucles de retroaction et echelles

Mesures avec `vector-emerg` et le robot `qa` ; lecture du code pour les bornes.

- **Prix, inflation, taux, change** (`macroeco.rs`, `market.rs`) : fonctions pures de (graine,
  secteur, date), bornees par construction : inflation -30 %..+300 % par an, taux directeur 0..60 %,
  change 0,1..3, recolte 0,4..1,4, rarete d'un stock 0,25..4. Aucune divergence possible, meme sur
  des millenaires. La seule dynamique avec memoire est le stock du marche, qui revient vers sa cible
  avec une constante de 3 jours.
- **Arbitrage sans risque** : 692 allers-retours achat puis vente au meme marche (7 tailles, tous les
  biens) : aucun profitable, le meilleur rend 0,967 de la mise (l'ecart achat/vente et le glissement
  du prix suffisent). Pret contre obligations d'Etat : le taux de la banque
  (`0,002 x (0,6 + 4 x taux directeur)` par jour, soit 64 %/an composes a 2 %) depasse toujours le
  rendement souverain (plafonne a 90 %) : pas d'argent gratuit.
- **Argent cree de rien** : trouve et corrige, une route de flotte vers une colonie. Voir plus haut.
- **Colonie** : la population est bornee par le logement (`clamp(0, housing)`), la tresorerie vient
  d'impots reels. Piege trouve et corrige : fonder sur un geant gazeux (aucun batiment d'habitation
  n'y a de site) perdait le kit et les colons au premier tick, population ramenee a un logement de
  zero ; `found_colony` refuse maintenant ce corps sans rien depenser.
- **Cryo** : pas de repas gratuit : retenue de salaire de 10 %, 1,5 kg de cryoprotecteur par
  dormeur, caissons qui coutent et tombent en panne, dose de radiation qui s'accumule endormi
  (maintenant visible dans les alertes).
- **Delta-v et ISP** : le delta-v du vaisseau de depart va de 400 a 930 km/s (ISP des torches de
  150 000 a 340 000 s, vitesse d'ejection 1,5 a 3,3 Mm/s). L'echelle est coherente avec celle des
  distances du jeu (la physique est reelle, rien n'est comprime) : le pilote automatique mesure 163
  jours et 4 384 kg sur 8 000 kg de carburant pour 6,7 UA, soit 55 % du reservoir. Diviser les ISP par
  10 ramenerait la portee a 0,7 UA et rendrait les systemes infranchissables ; ils sont donc gardes.
  Corrige : les moteurs exotiques depassaient la vitesse de la lumiere (Bussard : vitesse d'ejection
  0,98 c, torche a antimatiere 0,29 c, sans plafond de vitesse dans l'integrateur) ; ils sont
  ramenes a 3 000 000 s (0,1 c) et 2 400 000 s (0,08 c), en gardant l'ordre torche < sans inertie
  (1 200 000 s) < antimatiere < Bussard, et un test garde `ve < 0,2 c` pour tout moteur. Les ecrans
  du concepteur et de la creation de partie affichaient le delta-v en m/s (1 546 425) alors que le
  HUD et la fiche vaisseau l'affichent en km/s : tout est en km/s maintenant.
- **Equilibrage** : avec les politiques aleatoires du robot (`trader`), l'equipage meurt de faim en 7
  a 25 jours (quelques dizaines de kg de vivres au depart) ; ce n'est pas une boucle, c'est le prix d'une partie jouee
  sans manger. A noter : `Sim::advance` collee a un sol (pas de temps minimal, garde de 200 000 pas) n'avance
  pas d'un jour : pour mesurer une colonie, sauter l'horloge (`s.time += ...; update_colonies_pub()`),
  comme le fait `emerg colony`.
- **Colonie abandonnee** : une colonie livree a elle-meme (sans vivres) s'eteint en moins de 90
  jours (population 0,2 au jour 90, agitation 100 %), puis reste inscrite avec 83 cr/jour d'entretien :
  tresorerie a -87 000 apres trois ans et alerte "tresorerie a sec" pour toujours. Corrige : sans
  population l'entretien ne court plus (les batiments se degradent seulement).

- **Pollution -> climat** (`terraform::emit_co2`) : emission bornee par la taille de la colonie (un
  batiment industriel a pleine charge = 1e9 kg de CO2 par jour), plafonnee a un millieme de la
  masse de l'air par appel ; la retroaction (plus de CO2 -> plus chaud -> les calottes rendent du
  CO2) est celle de `Terra` deja bornee (la temperature est a +150 K au plus apres un impact, les
  calottes sont finies). Test sur 120 mois (`link_ten_simulated_years_*`) : ecart de temperature < 40 K, bilan de masse ferme a 1e-6.
- **Colonies -> faction** : le gain est donne par lots de 0,5 point, au plus 0,05 x poids x |score|
  point par jour, et s'arrete a +-60 ; pas de retroaction sur la colonie (la relation ne change ni
  ses impots ni son agitation).
- **Meteo -> approche** : le vent est une fonction de (jour, region), il ne depend pas du vaisseau ;
  la trainee (`atmosphere_drag`) est deja bornee a 150 m/s^2 ; la meteo est tiree une fois par
  region et par jour (cache), donc pas de cout par pas de physique.
- **Marche de station** : le stock bouge de ce que la colonie importe ou exporte, borne a 0 et a 6 fois
  le stock habituel ; il relaxe vers sa cible en 3 jours, donc une colonie qui vide les rayons ne les
  vide que quelques jours (une colonie de 3 000 ames y prend 1 800 kg de vivres par jour).

## Structures sauvegardees modifiees

Deuxieme passe : aucune non plus (VERSION de `save.rs` inchangee : 43). Champs ajoutes, tous
derives et non sauves : `colony_works::Works::{emitted, rel_acc, trade_flow}`
(`#[serde(skip)]`), `Sim::site_weather_cache` (le `Sim` n'est pas serialise tel quel), `landing::Touch::grip`
et `landing::Readout::{t_burn, weather}`, `combat::NpcStats::gun` (valeurs calculees),
`surprises::Ctx::sector_outbreak`, `alerts::Topic::Weather`, `vector_data::surprise::Cond::SectorOutbreak`
(donnee de `surprises.ron`). Premiere passe, champs ajoutes :

- `colony_works::Works::harvest` : `#[serde(skip)]`, derive de la date et du secteur, jamais sauve.
- `surprises::Ctx` (lecture quotidienne du monde) : champs `cryo`, `terraform`, `sick`, `dose`,
  `lingo`, jamais sauves.
- `vector_data::surprise::Cond` : nouvelles variantes `Cryo`, `Terraform`, `Sick`, `Dose`,
  `Language` ; elles ne sont que des donnees de `surprises.ron`, pas du format de sauvegarde (la
  sauvegarde garde les index de surprises, pas les conditions).
- `alerts::Topic::Mind` : valeur calculee, non sauvee.

## Outil

`vector-emerg` (`crates/tools/src/bin/emerg.rs`) : `emerg dv` (delta-v et poussee de chaque vaisseau de
depart et moteur), `emerg arb` (allers-retours a un marche), `emerg sys` (pilote automatique vers
chaque corps : jours, carburant), `emerg colony` (une colonie livree a elle-meme), `emerg loop`.
