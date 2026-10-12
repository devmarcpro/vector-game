# AUDIT des systèmes (2026-10-11)

Règle posée par le joueur : **pas de systèmes parallèles qui font la même chose ; tout doit interagir**. Cet audit liste ce qui existe (23 000 lignes de simulation, 16 600 d'interface, 33 écrans), ce qui fait double emploi, ce qui est isolé, et ce qui a été fait ou reste à faire.

## 1. Doublons constatés

| # | Doublon | Où | Décision | État |
|---|---------|----|----------|------|
| 1 | Marche planétaire (E5) **et** exploration roguelike (E9) | `walk.rs`, `rogue.rs`, deux vues | Un seul moteur : la surface est l'étage racine, les sites mènent aux lieux | **Fait** (commit `e1a14b8`) |
| 2 | 12 écrans de station isolés (marché, chantier, banque, bourse, courtage, contrats, recrutement, équipage, élevage, casino, cosmos, soute) | `game.rs` / `*_view.rs` | Devenir des **lieux** d'une station, d'une colonie, du vaisseau qu'on parcourt à pied (bornes d'accès dans les salles) | **Fait** (lieux sociaux : station, colonie, vaisseau ; bornes ouvrant les écrans existants) |
| 3 | Trois vues de l'équipage : écran Équipage, arbre généalogique (Family), réseau de l'équipage (V) | `crew`, `family_view`, `crew_net_view` | Un écran Équipage à onglets (état, liens, famille, organes) ; les personnes se parlent dans le vaisseau parcouru | À faire (refonte) |
| 4 | Deux systèmes de contrats : `Contract` (livraison) et `Tender` (appel d'offres) | `economy.rs`, `trading.rs` | Un appel d'offres gagné devient un `Contract` ordinaire (même suivi, même expiration, même journal) | À faire |
| 5 | Trois « confiances » : réputation de faction, réputation de civilisation, confiance des contacts de contrebande, sièges de cartel | `economy`, `civ`, `blackmarket`, `trading` | Un registre unique de **relations** (qui, valeur, historique) ; les autres lisent ce registre | **Fait** (`relations.rs`, vue : Tab dans Factions) |
| 6 | Trois générateurs d'événements : équipage (`events.ron`), monde (`world_events.ron`), éléments de lieux (`rogue_themes`) + découvertes d'anomalies | `crew`, `bourse`, `rogue`, `science` | Le système de surprises **étend `world_events` et le journal** au lieu d'ajouter un 4e générateur | **Fait** (`surprises.rs`, `data/surprises.ron`, 41) |
| 7 | Deux assurances : respawn d'assureur (combat) et assurance de soute (courtage) | `combat.rs`, `trading.rs` | Déjà reliées (`claim_before_wipe`) ; une seule police à terme couvrant coque et soute | Partiel |
| 8 | Animaux d'élevage (`livestock`) et faune des surfaces (`mon_from_species`) | `livestock`, `rogue` | Même table d'espèces ; **capturer** une bête vivante à la surface l'ajoute à l'élevage | À faire (prochain) |
| 9 | Quatre façons de voyager : pilote auto, saut FTL, trou de ver, relais de colonie | `autopilot`, `voyage`, `multiverse` | Volontaire (coûts et risques différents) ; tous passent par `arrive()` | OK |
| 10 | Rumeurs (courtage) et événements de marché (bourse) | `trading`, `bourse` | Déjà reliés (mêmes `world_events`) ; les passants des lieux sociaux relaient les rumeurs | En cours |
| 11 | Deux écrans de « liste de choses à vendre » : marché de la station et marché noir | `lib.rs` `sell/buy`, `blackmarket` | Même écran, une ligne de plus quand le bien est interdit | À faire (refonte) |

## 2. Systèmes isolés (qui devraient interagir)

- **Casino** : aucun lien avec les lois locales ni les mœurs (certaines factions devraient l'interdire).
- **Élevage** : les animaux ne sont pas visibles dans le vaisseau parcouru ; pas de vente à la colonie.
- **Naissances à bord** : les enfants ne sont pas visibles ; pas d'effet sur la colonie quand on les y dépose.
- **Dimensions** : aucune colonie ni flotte ne peut y travailler (seuls les avant-postes).
- **Compagnies cotées** : aucun événement du monde ne les ferme ni ne les fait fuser (OPA, faillite).

## 3. Écrans à regrouper pour la refonte (cible : 8 espaces)

Vol • Vaisseau (parcouru) • Station/Colonie/Ville (parcourues) • Carte (système, galaxie, multivers, surface) • Équipage • Économie (marché, bourse, courtage, banque) • Empire (colonies, flotte, factions) • Savoir (technologies, codex, journal).

## 4. Lieux sociaux (en cours)

Les stations, colonies et le vaisseau lui-même deviennent des lieux du moteur d'exploration :
- salles reliées par des couloirs, avec des **bornes** (marché, chantier, banque, bourse, courtage, recrutement, contrats, élevage, casino, infirmerie, équipage, soute, pont, portail…) qui ouvrent l'écran existant ;
- des **passants** non hostiles : parler à l'un donne une rumeur du courtage ou une remarque sur le lieu ; dans le vaisseau, ce sont les membres d'équipage, avec leur état réel ;
- le vaisseau est dessiné d'après ses modules (réacteur → salle des machines, soute, infirmerie, laboratoire…).

## 5. Ordre de travail

1. Lieux sociaux (§4), puis suppression des raccourcis de station devenus redondants (garder les touches comme raccourcis).
2. Appel d'offres → `Contract` (#4). Capture → élevage (#8). Registre de relations (#5).
3. Système de surprises sur `world_events` + journal (#6).
4. Refonte de l'interface (§3) et menus de début de partie.

## 6. Propriété (règle du joueur, 2026-10-11)
Beaucoup de services n'avaient pas de logique de lieu (de l'élevage dans une station étrangère, par exemple). Règle : un service n'existe que chez celui à qui il appartient ou qui le vend. Voir `docs/DECISIONS.md`, « Chacun chez soi ». Appliqué : élevage (enclos à bord, marché aux bêtes en station), ancres temporelles (vaisseau seulement), gestion de colonie (colonies seulement), marche à pied dans une station (seulement amarré). À vérifier encore : casino interdit par certaines mœurs, banque propre à une faction, relais seulement entre colonies à soi.

## 8. Audit des doublons (2026-10-12)
Passe complète sur la sim, les données et les vues : voir [AUDIT_DOUBLONS.md](AUDIT_DOUBLONS.md) (14 doublons corrigés : constantes physiques uniques (core::units), delta-v, chaleur du vaisseau, rareté des prix, aides de dessin ; 18 restants, classés par gravité). Les points 3, 4, 7 et 11 du tableau du §1 y sont repris (B14, B3, B4, B15).

## 7. Une chose, une représentation (2026-10-11)
- Planètes : `globe.rs` partagé (surface F8 + pilotage). Restent à unifier : carte système (disque), codex (texte).
- Nouvelles : un fil unique `news.rs` ; politique par secteurs (`politics.rs`), incidents d'entreprises (`incidents.rs`), événements de bourse ; ticker + onglet du journal.
- Exploration : un seul moteur (`rogue.rs`) pour surfaces, lieux, stations, colonies, vaisseau.
