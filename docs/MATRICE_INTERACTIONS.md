# Matrice des interactions entre systemes (audit de l'emergence)

Audit fait sur la branche `wt-auemerg` (base `master` d6f6659). Principe du projet : le gameplay
emergent vient de systemes qui interagissent, un systeme = une representation, chacun chez soi.
Ce document dit ou en est chaque couple de systemes, ou le lien manque, ce qui est ecrit sans
jamais etre appele, et ce qui a ete branche pendant l'audit (un test par lien, fichier
`crates/sim/src/emergence_tests.rs` et tests voisins, noms `link_*`).

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
| **EPI** |  |  | E |  |  | . | E | P | E | E |  | E |  | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **COL** | A |  | E | A | P | E | . | E | E | E |  |  |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  |
| **SUR** |  |  | E | E |  |  | E | . | E |  |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **NEW** |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **ALE** |  |  |  |  |  |  |  |  |  | . |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  |
| **RAD** |  |  |  |  |  |  |  | + |  | E | . | E | E |  |  |  |  |  |  |  |  |  | E |  |  |  |  |
| **COR** |  |  | E |  |  |  |  |  |  | E |  | . |  | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **CRY** |  |  | E |  |  |  | E | + | P | E |  |  | . | E |  |  |  |  |  |  | E |  |  |  |  |  |  |
| **EQU** |  |  | E |  |  |  |  | E |  |  |  | + |  | . |  |  |  |  | E |  |  |  |  |  |  |  |  |
| **TER** | E |  |  |  |  |  | E | + | P | A |  |  |  |  | . | E |  |  |  |  |  |  |  |  |  |  |  |
| **BIO** |  | E |  |  |  |  | E |  |  | + |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |  |  |
| **GEO** | E |  |  |  |  |  | E |  | A |  |  |  |  |  |  |  | . | P |  |  |  |  | P |  |  |  |  |
| **MIN** |  |  | E |  |  |  |  |  |  |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |  |  |
| **LAN** |  |  | E | E |  |  |  | + |  |  |  |  |  |  |  |  |  |  | . | + |  |  |  |  |  |  | E |
| **DIA** |  |  | E | E |  |  |  |  | A |  |  |  |  |  |  |  |  |  |  | . |  |  |  |  |  |  |  |
| **LOI** |  |  | E | E |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  |  | . |  | E |  |  |  |  |
| **MND** |  |  | A |  |  |  |  |  |  | E |  |  |  |  |  |  |  |  | E |  | E | . | P |  |  | E | E |
| **NAV** |  |  | P |  |  |  |  |  |  |  | E |  | E |  |  |  |  |  |  |  |  |  | . |  |  |  |  |
| **SHP** |  |  | E |  |  |  |  |  |  |  |  |  | E |  |  |  |  |  |  |  |  |  | E | . | E |  |  |
| **VOL** | P |  | E |  |  |  |  |  |  | E |  | + |  |  |  |  |  |  |  |  |  |  |  |  | . |  |  |
| **HSF** |  |  | A |  |  |  |  |  | P |  |  |  |  | E |  |  |  |  |  |  |  |  |  |  |  | . | P |
| **HIS** |  |  | E | E |  |  |  |  | A |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  |  | . |

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

### Liens partiels
- CLI -> EQU : body::Exposure (temp_k) : seulement l'habitacle et la marche a pied, pas la meteo du site d'atterrissage
- GUE -> AGR : le blocus coupe les convois et l'import, mais aucun champ n'est detruit
- EPI -> SUR : Cond::Epidemic ne lit que les colonies du joueur, pas l'epidemie du secteur
- COL -> GUE : megastructure::apply_mega_threats (les grands ouvrages attirent pirates et guerre)
- TER -> NEW : les journaux de megastructure passent par les journaux de colonie (news), pas les seuils de Terra
- GEO -> MIN : les gisements des mondes servent aux colonies, pas au laser de minage du joueur (asteroides seuls)
- GEO -> NAV : astro/cosmos -> science.rs (environnement, ondes gravitationnelles) mais pas la planification
- CRY -> NEW : au reveil, le dormeur 'trouve le monde change' (nouvelles depuis), mais le flux de nouvelles n'est pas prefiltre
- MND -> NAV : autopilot::Autopilot::new (pilotage) seulement ; nav_auto/nav_plan/nav_predict ignorent le budget de calcul
- NAV -> ECO : refuel au prix local, contrats ; le plan de route ne lit pas les prix du carburant le long du trajet
- VOL -> CLI : la meteo du site (vent, poussiere, orage) ne perturbe pas l'approche ni le touche
- HSF -> HIS : exotic : ruines precurseurs, mais pas dans history::provenance
- HSF -> NEW : rumeurs certaines apres un retour en arriere

### Liens absents
- COL -> CLI : la pollution (works.pollution) ne touche que les cultures (smog) : elle n'ajoute aucun gaz a Terra
- COL -> REL : aucune colonie ne change la reputation aupres de la faction qui tient le secteur
- TER -> ALE : aucune alerte de monde en emballement (effet de serre) ou d'habitabilite qui chute
- GEO -> NEW : aucune nouvelle de seisme/eruption hors journal de colonie
- DIA -> NEW : un dialogue ne laisse aucune trace dans le fil des nouvelles
- MND -> ECO : le calcul ne coute ni ne rapporte rien (pas de location de puissance, pas de minage de cycles)
- HSF -> ECO : aucun effet des dimensions/portails sur les prix
- HIS -> NEW : l'histoire des secteurs (ages, ruines) n'alimente pas les nouvelles


Autres changements de cet audit, qui ne sont pas des couples de la matrice :

- `found_colony` refuse un corps sans site d'habitation (`colony.rs`) : plus de kit perdu sur un geant gazeux.
- L'entretien d'une colonie sans population ne court plus (`colony.rs`).
- `fleet_unload` : la colonie paie le transporteur sur sa tresorerie (`fleet.rs`).
- Alerte de cargaison interdite par la loi locale (`alerts.rs`, LOI -> ALE), alerte d'esprit ralenti
  (`Topic::Mind`, ecran TECH), alerte de ravageurs de colonie, alerte de dose aigue par personne.
- `modules.ron` : ISP des moteurs exotiques sous 0,2 c ; delta-v en km/s dans le concepteur et la
  creation de partie.

## Liens manquants, classes par valeur de gameplay

Valeur = ce que le joueur (l'IA du vaisseau) verrait changer. Effort : S (quelques lignes), M (un
module), L (un systeme de plus).

| # | Lien | Pourquoi | Effort |
|---|---|---|---|
| 1 | COL -> CLI : l'industrie d'une colonie ajoute ses gaz et sa suie aux stocks de `Terra` | Le changement climatique d'origine humaine, le plus emergent possible : la pollution (`works.pollution`) ne sert aujourd'hui qu'a abimer les cultures. Il faut brûler des ressources (carbone des carburants) pour respecter le bilan de masse que `terraform` verifie | L |
| 2 | COL -> REL : une colonie en territoire d'une faction (impots, ordre, agitation, port ouvert) change la relation avec elle | Aujourd'hui fonder trois colonies ne change rien a la politique | M |
| 3 | Combat : tirs et degats par `arms::hit_chance`, `arms::damage_from_energy` | La physique des armes (dispersion, energie cinetique) existe et n'est jamais appelee : le combat utilise un DPS x precision abstrait. Deux representations du meme tir | M |
| 4 | MND -> NAV : le budget `T_PILOTAGE` aux plans de `nav_auto`/`nav_plan`/`nav_predict` (marge de reserve, qualite de la trajectoire prevue) | Seul l'ancien `autopilot` lit le calcul ; la navigation automatique, le systeme le plus recent, l'ignore | S |
| 5 | TER -> ALE / NEW : monde en emballement (gaz a effet de serre), habitabilite qui chute, seuils franchis | Un chantier de terraformation peut mal tourner sans que rien ne le dise | S |
| 6 | GEO -> MIN : un forage pose sur un monde lit `geology.ron` (gisement du site) | Les gisements ne servent qu'aux colonies | M |
| 7 | VOL <- CLI : vent, poussiere, orage du site (`planet::weather_at`) sur l'approche et le touche | La meteo existe pour les villes, pas pour le pilote | M |
| 8 | HIS -> NEW : ruines decouvertes, ages et chutes des secteurs dans le fil des nouvelles | L'histoire est calculee et jamais racontee | S |
| 9 | COL <-> ECO volume : imports/exports de vivres d'une colonie vont dans le stock du marche de la station du systeme | Aujourd'hui la colonie achete au prix local sans le deplacer | M |
| 10 | EPI -> SUR : `Cond::Epidemic` lisant aussi l'epidemie du secteur (pas seulement les colonies) | Les surprises sanitaires ne partent que des colonies | S |
| 11 | MND -> ECO : louer ou miner du calcul | Le calcul de l'IA ne rapporte ni ne coute | M |
| 12 | HSF -> ECO : prix et contrats dans les dimensions et apres un retour en arriere | Les portails ne touchent pas l'economie | M |
| 13 | DIA -> NEW : un dialogue important laisse une trace dans le fil | | S |

## Orphelins

Mesures par recherche de noms hors tests ; "orphelin" veut dire : ni la boucle de jeu, ni un ecran,
ni un outil n'appelle la chose (elle n'est utilisee que par des tests, ou par rien).

### Fonctions publiques jamais appelees (46)

- Physique d'armes (`arms.rs`) : `hit_chance`, `damage_from_energy` (voir lien 3).
- Physique orbitale et de navigation (`navigation.rs`) : `tsiolkovsky`, `propellant_for` (le calcul
  du delta-v est refait a la main dans `lib.rs`, `autopilot.rs`, `orbit.rs`, `shipwright.rs`),
  `light_delay_to` (retard de la lumiere : pourrait nourrir le retard des nouvelles/dialogues),
  `ir_detection_range` (arms a son propre modele thermique), `launch_bonus`, `j2_of`,
  `nodal_precession`, `equilibrium_k`, `burn_energy_gain`.
- Navigation auto (`nav_auto.rs`, `nav_predict.rs`) : `is_macro`, `nav_halt`, `nav_set_guard`,
  `predict` (appeles par les tests ou l'ecran via d'autres chemins).
- Economie : `macroeco::price_level` (l'inflation est calculee ailleurs a partir des memes
  grandeurs), `securities::after_fee`, `megastructure::project_years`, `beamed_power_w`,
  `terraform::mean_mass`, `archive::artefact_true_value`.
- Lois et gens : `law::is_illegal` (les actes passent par `offend`), `dialogue::npc_tone`,
  `npc::color_class`, `start::crew_pool`, `start::crew_valid`.
- Chimie : `chemistry::react`, `max_extent` (utilises par `lab_run` en interne seulement),
  `lab_reactions` (l'ecran liste toutes les reactions).
- Vaisseau et monde : `lib::gravity_excluding`, `lib::shop_sell_module`, `landing::time_to_burn`,
  `shipwright::buildable`, `mirror_y`, `fleet::buy_fleet_ship_free_pub`, `planet::sun_declination`,
  `world::galaxy::distant_galaxy_name`, `world::physics::star_physics`, `world::stellar::is_compact`.
- Donnees : `GameData::{reaction_index, age_index, cyber_index, gear_index, industry_index}`,
  `startopts::{ai_trait_index, background_index}` (seuls les tests les appellent).

Aucun champ `pub` du sim n'est ecrit sans etre lu. Aucun champ de `data/*.ron` n'a un nom que
personne ne lit (controle grossier par nom).

### Donnees jamais consommees

- `data/names.ron` : aucun code ne le charge (les noms viennent de `cultures_names.ron`).
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

## Structures sauvegardees modifiees

Aucune (VERSION de `save.rs` inchangee : 43). Champs ajoutes :

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
