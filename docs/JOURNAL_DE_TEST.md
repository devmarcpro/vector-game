# Journal de test : une partie jouee pour de vrai

Ce journal est ecrit par un **joueur testeur** : une session de jeu pilotee de bout en bout
(menu principal, creation, tutoriel, vol, amarrage, commerce, contrats, naufrage, colonie, saut FTL,
equipage, combat, tableau de bord), avec des captures lues a chaque etape, comme le ferait un
nouveau joueur. Jusqu'ici personne n'avait *joue* : le jeu avait ete construit et teste par morceaux.

Tout a ete fait en **headless** (rendu logiciel dans un PNG, aucune fenetre). Voir
"Ce que je n'ai pas pu verifier" en fin de document.

## 1. L'outil : jouer sans fenetre

`vector-game --script fichier.txt` pilote la vraie logique du jeu (memes touches, memes ecrans, meme
rendu) et ecrit des PNG. J'ai etendu `crates/game/src/script.rs` **sans changer le comportement du
jeu** (seul ajout dans le moteur : `Framebuffer::rec`, un enregistreur de textes inactif par
defaut, `None`). Nouvelles commandes :

| Commande | Effet |
|---|---|
| `key shift+tab`, `key ctrl+x` | touche avec modificateurs ; touches ajoutees : chiffres, `numpad0-9`, `f11`, `f12`, `home`, `end`, `insert`, `backslash`, `comma`, `period`, `slash`, `numpadmultiply` |
| `click X Y`, `rclick X Y` | clic gauche / droit a des coordonnees (1280x720) |
| `click_text MOT`, `rclick_text MOT` | clic sur le premier texte affiche qui contient MOT (sans tenir compte de la casse) |
| `type TEXTE` | saisie de texte (champs graine, nom...) |
| `text` | imprime le texte visible a l'ecran, ligne par ligne (ce que lit le joueur) |
| `expect_text "MOT"`, `expect_no_text "MOT"` | verifications sur le texte affiche |
| `shot dossier/nom.png` | capture a l'instant courant (cree le dossier) |
| `lint` | detecte les textes qui s'ecrasent mutuellement sur l'ecran courant |
| `post TEXTE`, `messages` | pousse / liste les messages du bandeau (verifier qu'un retour d'action est lisible) |
| `azerty on/off` | clavier |
| `print` | ajoute maintenant le nombre de membres d'equipage vivants |

Exemples dans `scripts/play/` (non executes par les tests ; `start*.txt` = creation d'une partie par
les menus, un fichier par scenario). Lancer : `target/release/vector-game.exe --script scripts/play/b4.txt`
apres avoir concatene un `start*.txt` devant (voir `scripts/play/README.txt`). `scripts/first_dock.txt`,
lui, est un test de non-regression execute par `cargo test`.

Astuce : sur un ecran de carte (`F8`, `L`) la commande `text` deverse des centaines de lignes ; preferer
`shot` puis lire l'image.

## 2. Verdict d'un nouveau joueur (resume)

**Le jeu est impressionnant de profondeur, et la presentation est coherente** (vectoriel vert,
menus a mots cliquables, aide `F1` contextuelle sur chaque ecran, menu `F10` avec "QUE PUIS-JE FAIRE
ICI ?", clic droit avec des actions grisees *et la raison du grisage*). Mais **sur la graine par defaut,
une partie sur deux se terminait dans l'heure** pour des raisons que rien n'expliquait :

* Voyageur : eruption stellaire a ~29 h de jeu, 88 Gy, equipage mort (si on accelere le temps, on ne voit meme pas l'alerte) ;
* Hors-la-loi : les batteries orbitales detruisent le vaisseau en 62 s, sans avertissement, sans rien faire ;
* Naufrage : le survivant etouffe (air toxique) en ~40 tours, sans combinaison ;
* premier amarrage : 150 CR d'amende sur 500 (30 % du capital) pour 3 kg de medicaments *fournis par le scenario*.

Ces quatre cas sont corriges ou attenues (section 5). Il reste des problemes de comprehension (le premier
trajet vers un asteroide coute 4,7 t de carburant et 4 jours ; la liste de construction d'une colonie
est invisible ; la recherche ne progresse pas sans labo) listes en section 6.

## 3. Parcours joues

Les captures sont dans `shots/play_NN_*.png` (dossier ignore par git, genere par les scripts).
Graine par defaut 24263 (systeme Polmix, etoile M, territoire des Clans Errants), sauf mention.

### (a) Premier lancement : menu, creation, tutoriel

Etapes : `scripts/play/start.txt` (menu -> NOUVELLE PARTIE -> 9 ecrans -> lancer).

* Marche : menu principal clair (`play_01_menu`) ; creation en 9 etapes numerotees (explications,
  scenario, vaisseau, equipage, IA, galaxie, monde, options, recapitulatif) avec un resume de ce que
  donne chaque choix (`play_03_scenario` : credits, soute, equipage, difficulte indicative 2/5...) ;
  equipage composable avec budget de points ; recapitulatif complet avant de lancer
  (`play_05_recap`). L'intro dit bien "VOUS ETES L'IA DU VAISSEAU".
* Confus :
  * Le menu principal ouvre sur **"PARTIE RAPIDE  GRAINE: (DEFAUT)_"** : le mot "(DEFAUT)" suivi
    d'un curseur n'explique pas qu'on peut taper une graine ; la ligne d'aide du bas le dit, mais
    elle est en tout petit.
  * Les listes de ressources/contrats melangent majuscules (interface) et casse mixte accentuee
    (donnees) : "Pièces détachées", "Rations" ; et des textes de tutoriel sans accents.
  * Le tutoriel est **lineaire** : il attend des touches precises dans l'ordre. En allant directement
    a la station (Tab, G, D) on se retrouvait au quai avec la consigne "Appuyez sur ESPACE pour pousser"
    (corrige, voir section 5).
  * Les consignes du tutoriel "Ciblez un asteroide (T)" menent vers un objet a 22 milliards de metres.
* Casse (corrige) : le texte du tutoriel (2 a 4 lignes) s'ecrasait avec le titre de l'ecran, les
  boutons `? AIDE (F1)` et les messages du bandeau (`play_09_autopilot`, `play_21_galaxy`,
  `play_30_surface0` : titres illisibles).
* Ennuyeux : rien ; l'ecran de creation est dense mais il est facultatif (PARTIE RAPIDE).
* Equilibrage : Voyageur = 500 CR + 233 kg de soute, 1 ingenieur ; Marchand = 3000 CR + 22 000 CR de
  marchandises (vendues en 1 minute au premier quai : 3000 -> 25 000 CR) ; Colon = 24 000 CR et le
  kit ; Naufrage = rien. La "difficulte 2/5" du Marchand et du Voyageur est tres inegale.

### (b) Un voyage complet : decoller, cibler, voler, amarrer, commercer, contrat

Etapes : `scripts/play/b4.txt`, `b6.txt` (apres `start.txt`).

* Marche : `Tab` cible le plus proche, `G` engage le pilote auto qui freine seul et s'arrete a 300 m
  ("STATION A 300 M - [D] AMARRER" est un excellent repere), `D` amarre en un tour. L'ecran de quai
  est lisible ; `B` / `S` achetent/vendent 100 kg (retour : "ACHETE 100 KG POUR 335 CR", puis "VENDU
  100 KG POUR 287 CR" : ecart achat/vente ~14 % avec l'impact de prix, sain). Un contrat "LIVRER 200 kg
  de silicium ICI, 414 CR" s'accepte, se remplit au quai (acheter 200 kg = 330 CR) et paie : +84 CR
  net, instantanement (`play_14_contract_active`).
* Confus :
  * La barre d'aide du quai ne parlait que de `ENTREE VENDRE` : on ne savait pas qu'`B` achetait
    (corrige).
  * **Aucun retour d'action visible au quai** : les messages "ACHETE...", "DOUANES..." etaient
    ecrits sur deux lignes recouvertes par le bandeau d'actualites et la barre d'aide (corrige).
  * Le marche liste 63 ressources dans l'ordre des donnees (Fer, Nickel, ...), sans categorie ni tri,
    sans indication de ce qui est rentable ; des noms tronques ("ALLIAGE D'ALUMINIU",
    "MATERIAUX DE CONST").
  * `Echap` au quai **decolle** le vaisseau (message "DECOLLAGE") : touche dangereuse pour "revenir".
  * Offres de contrat "TRANSPORTER 0 KG 8 PELERINAGE +2 ES" : texte tronque.
* **Casse (corrige)** : a la premiere visite du quai, la douane saisissait 3 kg de medicaments
  *du scenario* (interdits par les Clans Errants) avec 150 CR d'amende sur 500, sans avertissement.
  Le tableau de bord (touche `\`) affichait pourtant "MARCHANDISES INTERDITES CHEZ CLANS ERRANTS :
  MEDICAMENTS" : l'information existait, pas au bon moment.
* Equilibrage : contrat ICI/ICI = arbitrage sans voyage (sain a petite echelle, 5 contrats max).
* **Minage et distances** (`play_16`, `play_17`, `play_18`) : `T` cible l'asteroide le plus proche,
  a **22,2 Gm** (0,15 UA). `G` y va en 3,9 jours en brachistochrone a 0,77 m/s2 : **4 700 kg de
  carburant sur 8 000** pour un aller simple (le plein vaut ~7 700 CR au quai, le joueur a 500 CR).
  Le laser mine ~525 kg en 20 s (mélange type C, ~170 CR de revente). Autrement dit : le premier
  minage "naturel" ruine le joueur ou le laisse sans carburant pour rentrer. Rien dans l'interface ne
  le dit avant d'appuyer sur `G` (corrige a moitie : le HUD affiche maintenant le reste du trajet
  et le delta-v ; voir P2 pour la suite).

### (c) Naufrage : reparer et partir de rien

Etapes : `scripts/play/start_wreck.txt` puis `d2.txt`, `q1.txt`.

* Marche : ouverture en vue de dessus dans la savane, aide `F1` tres complete (touches, clic droit,
  creuser, batir, drone), vue a la premiere personne (`F`) etonnamment belle (`play_54_rogue_fp`),
  retours d'action clairs ("HERBE SECHE CREUSE (1 TOURS) : 0.5 KG DE COMPOSES ORGANIQUES").
* **CASSE (corrige)** : le survivant mourait de suffocation en ~40 tours (air 0.47 bar toxique, aucune
  combinaison ; cerveau a 0 %). Le panneau d'equipe affichait "100 %" puis "96 %", "89 %"... et le
  message "L'air est irrespirable" n'apparait que tous les 10 tours. En essayant 5 graines (1, 2, 3, 11,
  24263) j'ai trouve que **la plupart des sites d'atterrissage sont mortels** (air toxique, vide,
  4 a 5 % de sante perdus par tour dans le pire cas, chaleur jusqu'a 411 K). Correctifs : le survivant
  sort avec une combinaison renforcee ; une combinaison etanche (defense >= 0,15) ne laisse passer que
  25 % de l'air toxique et du vide, comme le modele du corps le disait deja ; le site d'atterrissage
  evite les mondes toxiques et les fours/congelateurs quand le systeme offre mieux ; et le jeu
  **dit** a l'arrivee "ATTENTION : l'air est irrespirable ; environ 0.7 % de sante par tour. Explorez
  vite, ENTREE sur < ramene au vaisseau", puis "X est a bout : retournez au vaisseau" a 50 %. Resultat :
  sur les 5 graines il reste > 30 % de sante apres 30 tours ; sur la graine par defaut ~92 % apres 100 tours.
* Confus : on ne sait pas ou sont l'epave, les ruines et les campements ("CHERCHEZ PIECES DETACHEES ET
  CARBURANT"), pas de boussole ni de marqueur ; les touches `T`, `E`, `G` (tir, recharge, grenade)
  ne figurent pas dans le panneau COMMANDES ; des glyphes `G` (creatures) s'approchent des le tour 0
  sans indication de danger.
* Ennuyeux : attendre (Espace) n'a pas de sens ici ; il faut marcher, le jeu ne le suggere pas.
* Equilibrage : difficulte 5/5 annoncee ; avec la combinaison c'est jouable, sans elle c'etait un
  game over certain (mort avant d'avoir compris les controles).

### (d) A pied sur une planete et dans le vaisseau

Etapes : `scripts/play/m1.txt`, `q2.txt`.

* Marche : `F8` donne la surface d'un monde cible ("CORPS NON RELEVE : APPROCHEZ-VOUS POUR LE
  SCANNER (NIVEAU 2)" est un bon message), puis cartes par couche (biomes, altitude, vie...) ; la fiche
  de la planete est riche. `FIN` > "PARCOURIR LE VAISSEAU" ouvre un petit pont ou l'equipage se promene.
* Casse (corrige) : sur l'ecran Surface le tutoriel ecrasait le titre et les onglets.
* Confus : `E` sans case de terre ferme repond "PAS DE TERRE FERME ICI : CHOISISSEZ UNE CASE DE TERRE"
  **mais ce message n'etait affiche nulle part** sur cet ecran (corrige : meme probleme sur
  Inventaire, Journal, Technologies, Codex, Factions, Bourse, Tableau de bord).
* Non joue jusqu'au bout : ruines, combat a pied prolonge, construction (voir "non verifie").

### (e) Colonie

Etapes : `scripts/play/start_colon.txt`, `h1.txt`, `h2.txt`, `h3.txt`.

* Marche : `O` sur une planete a portee fonde une colonie ("COLONIE FONDEE : Torii Hakugawa",
  24 000 -> 4 000 CR : permis 20 000 CR). L'ecran de colonie est tres complet (energie, emplois,
  vivres, politiques, journal).
* Confus :
  * **Fonder sur un monde deja habite** (Kaae-Saison : 1,58 M d'habitants Krethak) est possible sans
    aucun avertissement.
  * Le vaisseau se pose seul apres la fondation et "bascule a l'atterrissage" (message du journal) ;
    alerte CRITIQUE ensuite, sans explication.
  * La zone **CONSTRUIRE** est vide tant qu'on n'a pas fait `TAB` trois fois (corrige : un indice est
    affiche), et le colon n'a que 2 kg de rations en stock : "VIVRES 0 %  EAU 0 %" des le premier jour.
  * Libelles de politiques tronques ("Maintien de l'ordr", "Cueillette et élev", "Quarantaine biolog").
* Equilibrage : "POP 5/60", 20 000 CR de permis : une colonie est un investissement lourd des le depart,
  sans indication de rentabilite.

### (f) Saut FTL, navigation auto, atterrissage

Etapes : `scripts/play/c1.txt`, `c2.txt`.

* Marche : `L` ouvre la carte galactique, `ENTREE` lance le saut : "MANOEUVRE : LE VAISSEAU
  S'ELOIGNE DES CORPS MASSIFS, LE SAUT PARTIRA TOUT SEUL (G ANNULE)" : tres bien. Arrivee au bout de
  ~20 h (x100 000), "IA : SAUT TERMINE : SYSTEME RUSA".
* Confus : la carte annonce "CARBURANT 136 KG, TRAJET 18 H" pour Rusa (2,9 AL), mais la sortie du
  puits de gravite a brule en plus **~900 kg** (11 % du reservoir) avant le saut : le cout total n'est
  pas affiche.
* Casse (P3, non corrige) : sur la carte, "AMAS NYMORUN" et "MILIEU INTERSTELLAIRE" s'ecrasent.
* Non joue : noeud de manoeuvre / plan de navigation (ecran `NAV AUTO`), atterrissage (voir "non verifie").

### (g) Equipage : medecine, cryo, recruter

* Marche : `K` puis `N` : candidats avec cout d'embauche (580 a 1035 CR) ; embauche = 630 CR, paie
  de l'equipage 353 CR/jour pour deux personnes. L'ecran Medecine (`M`) est tres detaille (organes,
  sang, douleur, sceau cryostase `C`).
* Confus : colonnes de traits tronquees ("Courageux, Nostalgique, Fru") ; avec 500 CR de depart on ne
  peut embaucher personne et on ne le comprend qu'en cliquant.
* Equilibrage : 227 CR/jour pour l'ingenieur de depart ; un Voyageur passif est a sec en ~2 jours
  ("CAISSE VIDE : LES SALAIRES ATTENDRONT").

### (h) Combat spatial et abordage

* Marche (avec `pirate` + `engage` du script) : cible hostile, "HOSTILE !", jauge de bouclier, faisceau
  rouge visible, tir manuel (pas de visee auto).
* Casse (corrige) : les batteries orbitales du secteur d'un joueur recherche tuent le Hors-la-loi en
  62 s apres le debut de partie sans qu'on les voie arriver (voir section 5).
* Non joue : abordage, combat a pied dans le vaisseau.

### (i) Tableau de bord, journal, codex, menu contextuel

* Marche : le **tableau de bord** (`\`, `play_46`) resume tout en 9 panneaux avec des alertes
  ("MARCHANDISES INTERDITES CHEZ CLANS ERRANTS"). Le **menu F10** (`play_32`) dit "VOUS ETES : EN VOL
  PRES DE ..." et propose des actions. Le **clic droit** (`play_49`) donne des actions avec la
  raison quand elles sont impossibles ("PAS UN MONDE", "DEJA RELEVE") : c'est le meilleur element de
  l'interface. Le **codex** (`play_47`) est vaste (90 categories) et lie ses entrees.
* Confus : le panneau `ESPRIT DE L'IA` du tableau de bord affiche "CALCUL 100.0 RALENTISSEMENT 0 %"
  sans explication.
* Casse (P3, non corrige) : sur le tableau de bord, `HISTORIQUE : PAS ENCORE DE MOIS CLOS` s'ecrase
  avec le titre du panneau voisin (detecte par `lint`).

### Autres constats

* **Recherche** (`F4`, `play_57`) : lancer une recherche donne "RECHERCHE LANCEE : SCIENCE DES
  MATERIAUX" mais "RESERVE 0 PTS, LABOS DE BORD 0.0 PTS/J" : sans module de labo la recherche ne
  progressera jamais, et rien ne le dit.
* **Sauvegarde / chargement** : `F5` "PARTIE SAUVEGARDEE", `F9` "PARTIE CHARGEE" : fiable.
* **Performance (headless)** : ~37 ms par image PNG complete en vol (rendu + encodage), ~10 ms sans
  encodage : pas de souci apparent en rendu logiciel.
* Les messages d'erreur sont en minuscules dans une interface en majuscules ("PILOTE AUTO REFUSE:
  gravite trop forte pour le pilote auto : approchez a la main").

## 4. Equilibrage ressenti (synthese)

* **Argent** : Voyageur 500 CR ; salaires 130 a 230 CR/jour par personne ; un plein de carburant ~7 700 CR ;
  minage ~8 CR/s de laser (~30 000 CR/h) *une fois sur place*. L'economie de depart est un mur
  pour le Voyageur (500 CR, aucun asteroide proche) et un jouet pour le Marchand (25 000 CR en 1 min).
* **Danger** : tres haut et tres brutal sur la graine par defaut (eruptions de 88 Gy a 0,14 UA d'une naine
  rouge, batteries, air toxique) ; il manque la phase "apprendre" avant que le monde ne soit mortel.
* **Vitesse de progression** : temps morts importants entre les points d'interet (3,9 jours de trajet
  pour miner, 20 h par saut) ; l'acceleration du temps (`PGUP`) est indispensable et enseignee,
  mais elle masque les alertes (corrige pour l'eruption).

## 5. Correctifs apportes dans cette session (avec tests)

| Probleme | Correctif | Test |
|---|---|---|
| Amende de 150 CR au premier quai pour la contrebande fournie par le scenario | la douane ne fait qu'**avertir** (saisie sans amende ni perte de reputation) pendant la 1ere heure de jeu (`CUSTOMS_GRACE_S`) ; message dedie | `economy::customs_only_warn_in_the_first_hour` ; `customs_seizes...` ajuste |
| Naufrage : mort par suffocation en ~40 tours, sans un mot | combinaison renforcee au depart ; combinaison etanche = 25 % des degats d'air/vide ; site moins toxique ; avertissement a l'arrivee et a 50 % de sante | `start::the_castaway_lives_long_enough_to_search_and_is_told_what_the_place_costs` (5 graines) |
| Hors-la-loi : batteries qui detruisent le vaisseau en 62 s, sans alerte | 10 min de grace (`BATTERY_GRACE_S`) + evenement `BatteryFire` : "ALERTE : LES BATTERIES ORBITALES TIRENT SUR VOTRE VAISSEAU - FUYEZ LA STATION !" | `arms::batteries_fire_on_a_hunted_ship_near_the_station_only` (etendu), `game::a_coming_flare_ends_the_time_warp...` |
| Eruption stellaire : alerte avalee par l'acceleration du temps, texte "ABRITEZ-VOUS" sans mode d'emploi | vitesse du temps limitee a x60 dans les 15 min precedant une eruption pour un vaisseau expose ; l'alerte repasse a x1 et dit "AMARREZ-VOUS (D) OU ACTIVEZ LE BOUCLIER" | `game::a_coming_flare_ends_the_time_warp_of_an_exposed_ship` |
| Tutoriel qui ecrase le titre de l'ecran, les boutons, les messages | une seule rangee en haut des ecrans de menu (defilement si trop long, bouton `[SUPPR ignorer]` fixe), rangee 6 en vol | `script::the_tutorial_line_never_lands_on_a_screen_title` |
| Tutoriel qui demande de pousser alors qu'on est deja au quai | l'amarrage saute les lecons de vol restantes | `game::docking_early_skips_the_flight_lessons` |
| Messages d'action invisibles au quai et sur 8 ecrans | message affiche sous la liste du marche ; ligne de message sur Inventaire, Journal, Technologies, Codex, Factions, Surface, Bourse, Tableau de bord | `script::the_result_of_an_action_is_readable_on_every_screen_that_has_actions` |
| Barre d'aide du quai sans `B` (acheter) | "B ACHETER 100 KG  S VENDRE 100 KG  ENTREE VENDRE TOUT..." | revue visuelle (`play_10`) |
| Jauge TEMP du HUD et texte "290 K 0.0/0.0 MW" superposes ; etiquette de la carte superposee a la jauge | decalages | `lint` (script) |
| Marche aux betes : titre et credits superposes | credits decales | `lint` |
| Pilote auto sans duree ni cout | HUD : "RESTE 3.9 J  DV 262 KM/S" | revue visuelle |
| Liste de construction de colonie invisible | indice "(TAB OU CLIC : LISTE...)" | revue visuelle |

Verification finale : `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings` et
`cargo test --workspace --no-fail-fast` (core 2, data 31, game 154, render 5, sim 741, world 73) sont
verts apres les correctifs. Rejoue apres correctifs : Hors-la-loi survit a 100 s puis recoit
"ALERTE : LES BATTERIES ORBITALES TIRENT..." (il doit fuir ; s'il reste immobile il est detruit
a ~660 s) ; le premier quai du Voyageur ne coute plus d'amende.

## 6. Ce qui reste, par priorite

### P1 (bloquant ou fatal pour un nouveau joueur)

1. **Eruption stellaire : dose 88 Gy a 0,14 UA d'une naine rouge, sans bouclier de depart.**
   `severity*2/d^2` ne tient pas compte de la luminosite de l'etoile ; le vaisseau de depart ne peut
   survivre que s'il est amarre (couverture 0,005). Proposition : plafonner la dose a la valeur la plus
   haute survivable avec le bouclier ou normaliser par `sqrt(L/Lsun)`, et donner un bouclier
   ou un abri (ombre d'une planete) au vaisseau de depart. Verifier aussi que l'ecran Equipage dit
   "MORT (irradiation)" avec la dose.
2. **Premier minage ruineux** : l'asteroide le plus proche est a 22 Gm et `G` brule 59 % du reservoir
   (brachistochrone a 0,77 m/s2). Propositions : (a) `G` propose un mode economique (accel. plus faible,
   ~10 fois moins de carburant), (b) avertir avant d'engager si le trajet depasse 20 % du reservoir ou
   si le retour devient impossible, (c) generer au moins une ceinture/asteroide a < 1 Gm de la station
   de depart.
3. **Hors-la-loi** : meme avec 10 min de grace, le scenario demarre a 2,9 km d'une station dont les
   batteries tuent en 1 min. Proposition : le faire demarrer a > 60 km (portee des batteries) ou en
   transit.

### P2 (gene)

* `Echap` au quai decolle : demander confirmation ou rediriger vers le menu `F10`.
* Marche : trier/filtrer (par rentabilite, par categories) ; noms tronques ("ALLIAGE D'ALUMINIU").
* Recherche lancee sans labo : avertir "AUCUN LABO : 0 PTS/J".
* Cout total d'un saut (sortie du puits comprise) sur la carte galactique.
* Fonder sur un monde habite : avertir (ou refuser) ; atterrissage automatique apres fondation :
  l'expliquer ; vaisseau qui bascule.
* Tutoriel lineaire : valider les lecons d'apres l'etat (cible = "Tab") plutot que d'apres la touche.
* Naufrage : indicateur de direction vers l'epave/ruines ; T/E/G dans le panneau.
* Colonie : stock vivres/eau initial ; message clair "RIEN DE CONSTRUCTIBLE : STOCK INSUFFISANT".
* Scenarios : equilibrer difficulte annoncee (Marchand 2/5 avec 25 000 CR vs Voyageur 2/5 avec 500).
* Barre d'aide `Pause` : deux curseurs `>` (REPRENDRE et REGLES DE PARTIE) sans explication.
* Messages d'erreur en minuscules dans une interface en majuscules.

### P3 (confort)

* Libelles tronques : traits de recrues, noms de politiques de colonie, offres de contrat de pelerins.
* Superpositions restantes (`lint`) : "AMAS NYMORUN / MILIEU INTERSTELLAIRE" (carte galactique),
  "HISTORIQUE : PAS ENCORE DE MOIS CLOS / PANNEAU" (tableau de bord).
* Traits de vitesse (streaks) sur tout l'ecran a 50 km/s qui couvrent le HUD ; envisager de les limiter
  a la zone centrale.
* Panneau COMMANDES : 60 lignes de touches (numpad, etc.) sans regroupement ; "WASD/QE:ROT..." du message
  de la touche `?` non localise en AZERTY (`game.rs`).
* Message "NOUVELLE PARTIE - GRAINE ..." reste affiche en haut tant qu'il n'est pas pousse hors de la
  pile.

## 7. Ce que je n'ai pas pu verifier

* **Rendu reel en fenetre** (taille de police a l'echelle, `scanlines`, `glow`, redimensionnement, DPI),
  latence du clavier et de la souris : tout est vu via le framebuffer logiciel.
* **Son** (`audio.rs`) : aucun peripherique, rien ecoute.
* **Performances en direct** : seulement des mesures de rendu a l'unite ; pas de fluidite sur une longue
  session ni a grande vitesse du temps dans une fenetre.
* Langue anglaise, palettes, options graphiques, clavier QWERTY/AZERTY a la volee (seul AZERTY du tutoriel
  verifie).
* Combat rapproche, abordage, ruines et grottes, construction d'une base a pied, epidemie de colonie,
  navigation par noeuds de manoeuvre, atterrissage manuel, casino/banque/bourse, flotte deleguee.
