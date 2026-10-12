# CONTROLES (touches physiques, positions QWERTY ; sur AZERTY : ZQSD = WASD)
| Touche | Action |
|---|---|
| W / S | Tangage (piquer / cabrer) |
| A / D | Lacet |
| Q / E | Roulis |
| Espace / X | Poussee avant (moteur principal) / recul (RCS) |
| Fleches | RCS lateral (gauche/droite) et vertical (haut/bas) |
| F | Assistance de vol ON/OFF (amortit rotation et derive) |
| Maj gauche (maintenir) | Boost : poussee x2.5, rotation x1.4, carburant x3 sur la poussee principale |
| C (maintenir, carte fermee) | Frein : annule la vitesse relative a la cible (sinon au corps de reference, sinon au repere de l'etoile), partout et a toute vitesse, par RCS |
| F7 | Visee souris ON/OFF : le mouvement relatif de la souris commande tangage et lacet (500 px/s = pleine deflexion), s'ajoute au clavier ; curseur capture en vol seulement |
| Suppr | Vue externe / cockpit : camera a la troisieme personne qui orbite autour du vaisseau (coque et modules reels, eclaires par l'etoile ; flammes proportionnelles a la poussee, radiateurs rouges a chaud, bouclier, degats, train d'atterrissage a l'approche du sol, ombre et poussiere). En vue externe : glisser la souris = orbite, molette (ou +/-) = zoom, pave numerique 4/6/8/2 = orbite, 5 = camera suivant le vaisseau / libre, 0 = recentrer. Aussi dans le menu PILOTAGE (VUE EXTERNE) et par les mots cliquables en bas a gauche |
| Tab | Cible suivante (la plus proche d'abord) |
| R | Cibler le vaisseau PNJ le plus proche (cycle) |
| T / Y | Cible : asteroide / station le(la) plus proche |
| V (maintenir) | Laser de minage (cible asteroide a < 2 km, visee dans le cone) |
| D | Amarrer (station < 500 m, vitesse relative < 8 m/s) |
| I | Soute / inventaire (J = jeter la ressource, F = verser le carburant de la soute dans le reservoir) |
| K | Equipage (Gauche/Droite poste, T quart, X débarquer à quai, J journal) |
| J | Journal de bord (Tab : nouvelles du monde, puis ARCHIVES : histoire du secteur et artefacts ; D dater, C collectionneur, M musée, R restituer, T fouille prudente) |
| L | Carte galactique (Haut/Bas étoile, +/- zoom, Entrée = saut FTL) ; Retour arrière annule une charge de saut |
| F2 | Empire (colonies) ; Entrée = détail ; F3 = Flotte |
| O | Fonder une colonie sur le corps ciblé (kit en soute) |
| F4 | Arbre technologique (Entrée lance une recherche) ; E = panneau ESPRIT (budget de calcul de l'IA, logiciels, autres IA : Gauche/Droite budget, Entrée installer, C/A/L contacter/allier/libérer, S ancre) |
| Ctrl / Entrée (maintenir) | Tir principal (faisceaux, canons) |
| B (maintenir) | Missiles et mines |
| Z | Payer le tribut demandé par des pirates |
| N | Récupérer l'épave la plus proche |
| U | Aborder la cible neutralisée |
| R | Cibler le vaisseau PNJ le plus proche |
| H | Ecran vaisseau (emplacements, santé, énergie ; amarré : Entrée installer, X retirer, Y chantier naval) |
| F5 / F9 | Sauvegarde / chargement rapide (saves/slot1.sav) |
| Echap | Menu pause |
| G | Pilote auto vers la cible (brachistochrone limitee par le delta-v) |
| M | Carte systeme ; +/- zoom ; C centre etoile/vaisseau |
| P | Pause |
| PgUp / PgDn | Temps x0.25 .. x100000 (pas adaptatifs ; poussee manuelle coupee au-dela de x2) |
| ~ (Backquote) | Console de debug : help, list, tp, tpbody, stop, speed, fuel, repair, assist, time |
| F1 | Aide contextuelle de l'ecran courant (re-appuyer pour fermer) |
| F6 | Codex (Gauche/Droite categorie, Haut/Bas entree ; categorie REACTIONS = laboratoire : L ou Entree lance un lot de 100 kg de reactifs tires de la soute) |
| F10 | Ignorer le tutoriel |
| F6 | Codex (Gauche/Droite categorie, Haut/Bas entree) |
| F10 | Menu principal : les huit espaces qui regroupent tous les ecrans (voir « Le menu principal ») ; re-appuyer ferme |
| Suppr | Ignorer le tutoriel |

Options de ligne de commande : `--snapshot fichier.png [scene]` (rendu headless), `--bench`, `--script fichier.txt` (jeu scripte sans fenetre, voir `crates/game/src/script.rs`).

## Station
Haut/Bas : ressource ; Entree : vendre tout ; S : vendre 100 kg ; B : acheter 100 kg ; F : plein de carburant ; R : reparer ; U : decoller.
## Menu principal
Haut/Bas + Entree ; taper une graine (nombre ou texte) avant Nouvelle partie.

Station : N = recruter (Entrée embauche au poste de sa meilleure compétence), K = équipage.
Station : T = bourse (Haut/Bas ressource, PgUp/PgDn x10, Tab ou Gauche/Droite période 30/90/180 jours, T ou Échap retour).
Station : C = contrats (Entrée accepter/livrer, X abandonner), E = banque (Haut emprunter 1000, Bas rembourser 1000, Entrée tout rembourser, H hypothéquer le vaisseau / lever l'hypothèque, C acheter 1000 CR de monnaie locale, V la revendre, B blanchir l'argent sale ; le tableau ECONOMIE LOCALE et les rapports mensuels sont sur le même écran).

Ecran colonie : Tab zone (bâtiments / stocks / politiques / construction), Haut/Bas, Espace on/off, R recette, X démolir, D déposer / W retirer (stocks, amarré), Gauche/Droite politique, Entrée construire, T récupérer la caisse, U décoller. F = schéma des chaînes de production, H = suivi sanitaire (épidémies de la colonie, secteur, équipage, journal) ; les mêmes boutons sont cliquables en haut à droite.
Ecran flotte : Tab liste, N nouvelle route (Gauche/Droite valeur, Tab champ, Entrée), A affecter, U retirer, X supprimer/vendre, B acheter (à quai).

## Menus
Menu de demarrage : Partie rapide (voyageur standard, graine tapee ou par defaut) / Nouvelle partie (le flux en sept etapes ci-dessous) / Continuer / Regles de partie / Options / Codex-Aide / Quitter. Menu pause : Reprendre / Sauvegarder / Charger / Regles / Options / Codex-Aide / Menu principal / Quitter.

## Nouvelle partie (sept etapes)
Menu de demarrage > Nouvelle partie. Entree passe a l'etape suivante, Echap revient a la precedente (la premiere revient au menu de demarrage), un clic sur le nom d'une etape (en haut) y va directement, clic droit = Echap, les boutons « < PRECEDENT » et « SUIVANT > » sont du texte cliquable.
1. **Explications** : ce qu'est le jeu (bac a sable sans objectif impose, vous etes l'IA du vaisseau, l'equipage vit sa vie) et les touches a connaitre.
2. **Scenario** (Haut/Bas) : Voyageur, Marchand, Explorateur, Hors-la-loi, Colon, Naufrage. Chacun fixe vraiment les credits, la soute, le vaisseau, l'equipage a depenser, les technologies connues, les relations avec les factions (et une prime pour le hors-la-loi). Naufrage : on commence a pied sur un monde nu, vaisseau hors d'etat de voler (voir « Naufrage »).
3. **Vaisseau** (Haut/Bas) : les coques proposees par le scenario, modules montes, apercu fil de fer qui tourne, poussee, energie, reservoir, soute, logements, saut FTL, scanners.
4. **Equipage** : douze candidats tires de la graine ; Haut/Bas choisit, Espace (ou clic) prend ou laisse, Gauche/Droite change le poste de la personne prise ; budget de points du scenario (1 point par personne, 2 pour un specialiste, 3 pour un expert), au moins une personne.
5. **Galaxie** : taper une graine (nombre ou mot), Tab en tire une au hasard, Suppr revient a celle par defaut ; l'ecran donne le systeme de depart (classe de l'etoile, planetes, lunes, asteroides, stations, territoire).
6. **Options** (Haut/Bas, Gauche/Droite) : langue (FRANCAIS), clavier (AZERTY par defaut / QWERTY), tutoriel guide, difficulte (DOUX / STANDARD / RUDE = degats, agressivite des IA, evenements, carburant) et, sur la derniere ligne, les regles avancees. La difficulte indicative du depart (1 a 5) suit le scenario et les regles.
7. **Recapitulatif** : Entree lance la partie.

## Le menu principal (F10, huit espaces)
F10 ou un clic sur « MENU (F10) » (en haut a droite de tous les ecrans) ouvre le menu ; il gele le vol. En haut, l'**orientation** : ou vous etes (en vol pres de..., amarre a..., pose sur..., a pied : lieu) et ce que vous pouvez faire maintenant (consignes selon l'etat : quai, vol, vaisseau au sol, a pied). Dessous, huit espaces (1 a 8, Haut/Bas, clic) :
- 1 PILOTAGE : cockpit / decoller, radio (Entree), portail et temps (Accueil), pause et sauvegarde.
- 2 NAVIGATION : carte du systeme (M), galaxie et saut FTL (L), surface d'un monde (F8).
- 3 VAISSEAU : modules et energie (H), soute (I), chantier naval (Y a quai).
- 4 EQUIPAGE : equipage (K), arbre familial, recrutement (N a quai).
- 5 COMMERCE / STATION : marche, bourse (T), courtage (D), contrats (C), banque (E), elevage (L), casino (A) ; a quai seulement.
- 6 EXPLORATION A PIED : explorer a pied (Fin), expedition sur un monde (F8 puis E).
- 7 COLONIES : empire (F2), colonie, flotte (F3), factions (X), fonder une colonie (O).
- 8 CODEX / JOURNAL : journal (J), codex (F6), technologies (F4), prototypes d'interface (F12).
Un espace ouvre l'ecran existant par la meme fonction que sa touche : aucun ecran n'est duplique. Une ligne grisee dit pourquoi elle est indisponible (amarrage requis, decollage requis, a pied).
**Echap remonte d'un niveau, partout** : un ecran ouvert depuis le menu revient a la page de son espace ; la page revient a la liste des espaces ; la liste ferme le menu, qui rend l'ecran d'ou il a ete ouvert (cockpit, quai ou marche a pied). Le clic droit fait de meme.
A pied (equipe sur le terrain), seuls les ecrans d'information s'ouvrent (vaisseau, soute, equipage, journal, codex, technologies) et Echap revient a la marche.

## Naufrage (scenario)
Le vaisseau est un epave : moteur a 0 %, reservoirs vides, aucun credit, un ingenieur seul, a pied sur un monde choisi pour son air. Les epaves, campements, ruines et grottes de la surface (touche Fin / carte de surface) donnent des pieces detachees et du carburant ; le butin va en soute au retour (Entree sur < ou R) ; l'ingenieur de quart repare avec les pieces detachees (ses reparations ordinaires, le module le plus abime d'abord) ; **I puis F** verse le carburant de la soute dans le reservoir ; le menu (F10) dit moteur, energie, pieces et carburant. Le vaisseau peut decoller (Espace) quand la poussee depasse son poids sur ce monde. Rien n'est impose : on peut aussi rester vivre sur place.
Regles : Haut/Bas, Gauche/Droite modifier, R valeurs par defaut ; depuis la pause elles s'appliquent immediatement.
Options : lignes de balayage, halo, palette (couleur, vert, ambre, blanc, bleu), volume, tutoriel, barre des commandes ; sauvegardees dans saves/settings.cfg.


## Barre des commandes
La dernière ligne de l'écran affiche en permanence les touches utiles ici et maintenant (vol, carte, saut en charge, pilote automatique, station, module, colonie, menus...). Elle suit l'écran et son sous-état ; Options > Barre des commandes la coupe. Le bandeau du tutoriel passe alors en haut des écrans hors vol.

## Couleurs
Une fois un corps relevé (niveau 2), sa couleur donne son type : étoiles par classe (O bleu à M rouge), planètes (aride gris, désert sable, terrestre vert, océan bleu, toundra turquoise, glace blanc-bleu, lave rouge-orangé, vénusienne ocre, géante gazeuse orange, géante de glace bleue, Jupiter chaud magenta), astéroïdes par type (C brun, S sable, M métal, I glace), stations cyan. Non relevé : vert neutre. La cible est entourée de crochets ambre ; atmosphères et géantes ont un halo, certaines géantes des anneaux.

## Souris
Tout se fait aussi à la souris (hors visée F7, où la souris pilote le vaisseau).
- **Clic gauche** : une ligne de menu ou de liste la sélectionne ; un bouton (barre des commandes du bas, onglets, flèches < > autour d'une valeur) fait ce que fait sa touche ; en vol, un objet du cockpit ou de la carte système devient la cible ; sur la carte de surface le curseur va sous le clic ; dans l'arbre des technologies une case est sélectionnée.
- **Double clic** : station, soute, vaisseau, chantier naval, recrutement = Entrée sur la ligne (vendre, installer, acheter, embaucher) ; équipage = arbre généalogique ; technologie = lancer la recherche ; objet en vol = pilote automatique ; étoile = saut.
- **Clic droit** : retour (Échap) sur tous les écrans sauf en vol et au menu principal.
- **Molette** : fait défiler la sélection de l'écran (Haut/Bas) ; en vol, change l'accélération du temps ; sur la carte système, zoome.
- **Barre des commandes** : chaque commande (« TOUCHE ce qu'elle fait ») est un bouton, sauf les touches maintenues en vol (poussée, boost, frein, lasers).
- Un arbre généalogique : un parent à bord est un bouton qui ouvre son propre arbre.

- **X** (en vol) : écran des factions, de leurs moeurs et de leurs lois ; Gauche/Droite ou clic pour choisir, X ou Echap pour revenir. Situation juridique du joueur dans le territoire choisi : **P** payer les amendes (de partout), **B** pot-de-vin (coût et chance affichés ; peut échouer), **L** avocat (amendes et notoriété divisées par deux) ; ces actions sont aussi des boutons cliquables.

- **Codex** : sous chaque entrée, « VOIR AUSSI » liste les entrées citées ; clic ou touches 1 à 9 pour les suivre, Retour arrière pour revenir.

- **Equipage, O** : liste des organes de la personne choisie ; Haut/Bas choisit l'organe, Entree greffe (ou prothèse s'il n'y a pas de greffon en soute), P prothèse, H prélève, O ou Echap revient à la liste. Il faut la pièce en soute et un hôpital (à quai, 1500 CR) ou un médecin et une infirmerie à bord.


- **Equipage, M** : onglet MEDECINE de la personne choisie (silhouette colorée par partie, signes vitaux, blessures, médicaments dans le sang, handicaps). Haut/Bas change de personne, Tab choisit la plaie, Gauche/Droite le médicament, Entrée le donne (dose prélevée en soute), A premiers soins (pansement ou attelle, qualité selon le soignant), S opère la plaie choisie (infirmerie avec un médecin, ou hôpital à quai : 600 CR), R choc électrique sur un inconscient, C cryostase (infirmerie alimentée ou hôpital à quai), M ou Echap revient à la liste. Les mots en ambre sont aussi cliquables.
- **Equipage, O, puis Gauche/Droite** choisit un implant, **I** le pose, **U** le retire (la pièce doit être en soute ; 3000 CR à quai).

- **Liste des organes, M** : injecter une dose de mutagène (0.1 kg) à la personne choisie.

- **Entrée** (en vol, vaisseau ciblé) : appeler ; 1-6 ou clic pour répondre, Haut/Bas + Entrée, Echap coupe.

## Clavier
Par défaut AZERTY : tangage Z/S, lacet Q/D, roulis A/E. Options > Clavier bascule en QWERTY (W/S, A/D, Q/E). Les autres commandes se nomment par la lettre imprimée sur la touche, quelle que soit la disposition. Le tribut des pirates se paie avec la touche Z en QWERTY, par la radio (Entrée) en AZERTY. Options > Langue : FRANCAIS (les traductions viendront plus tard).

## Expédition (surface, touche E)
Écran de préparation : Haut/Bas, Espace coche un équipier ou le rover, Entrée part. Ensuite c'est l'exploration commune (voir « Exploration à pied ») : flèches, pavé numérique ou Z/Q/S/D pour avancer (foncer sur un animal ou un monstre l'attaque), les plantes et le butin se ramassent en marchant, Entrée sur un site (ruine, grotte, épave…) y descend et sur < remonte, V lance un drone, R rappelle l'équipe depuis la surface, Tab change de chef, clic = marcher jusqu'à la case.

- **Globe de surface** : glisser avec le bouton gauche pour le faire tourner.
- **Carte de surface (F8)** : Tab change de couche (biomes, altitude, température, humidité, météo, vie, nuages). Sur la couche biomes, `~` marque un lac ou un fleuve et `=` l'eau prise par la glace (selon la saison) ; le panneau de droite dit la mer, le lac ou le fleuve sous le curseur, son débit et s'il est gelé.
- **Couche GEOLOGIE** (écran de surface F8, Tab jusqu'à la dernière couche ou clic sur son onglet) : plaques teintées, frontières convergentes (^), divergentes (=) et transformantes (/), volcans actifs (A) et éteints (a), gisements (M veine, H sulfures, K kimberlite, E évaporites, P pétrole, G pegmatite, I glace) ; le panneau de droite donne plaque, roche, frontière, chaleur du sol et gisements à moins de 6 cases du curseur. Fonder une colonie à moins de 12 cases d'un gisement en met le minerai dans son sol.
- **Carte galactique (L)** : les cercles colorés sont les nébuleuses, amas et régions de formation d'étoiles à portée (rayon réel), le panneau donne le stade de l'étoile choisie, les effets du milieu où l'on est et la distance au trou noir central. Codex (F6) : catégories GEOLOGIE et ASTRONOMIE.

- **Menu de pilotage** (cockpit, à droite) : cliquer un mot ouvre l'écran ou exécute la commande ; les mots « (MAINT.) » agissent tant que le bouton de la souris reste enfoncé. F11 plie ou déplie le menu (Options > Menu de pilotage pour le retirer).

- **Maj + Tab** : cible la planète ou lune suivante. **Pilote auto (G)** : transfert assisté vers la cible, le vaisseau reste garé à l'arrivée (une poussée le libère) ; refusé pour les géantes gazeuses (approche à la main). **Galaxie (L) puis Entrée** : le vaisseau s'éloigne seul des corps massifs puis saute.

- **Entrée sur une planète ciblée** (Maj+Tab, assez près) : parler à son peuple s'il en a un (voir la ligne CIVILISATION de l'écran de surface F8).

- **Bourse** : B / N contrat à terme à la hausse / à la baisse sur la ressource choisie (100 kg, 30 j, marge 25 %), C clôt un contrat.

- **Equipage, E** : liste de l'équipement (flèches pour choisir, Entrée équipe depuis la soute, U retire, E ou Echap revient). Clic sur une ligne ARME / ARMURE / OUTIL.

- **Bourse, E** : bascule entre marchandises et entreprises ; sur les entreprises, B achète 10 actions et N en vend 10 (il faut être à quai).

- Station : G achete 50 kg, J vend 50 kg de la ressource choisie au marche noir (seulement pour les biens interdits du territoire ; prime ~x1,9 a l'achat, x1,35 a la vente, 12 % de descente de police : amende 600 CR, saisie, reputation -0,25).

- Station : L elevage (B acheter, P reproduire avec le meilleur partenaire, H abattre, Gauche/Droite espece), A casino (Gauche/Droite jeu, Haut/Bas mise, +/- numero, Entree jouer).

- Station : D courtage (Gauche/Droite ou clic sur le nom de l'onglet : rumeurs, arbitrage, cartels, appels d'offres, assurance, titres, ventes, lignes, economie ; Haut/Bas choix, PageHaut/PageBas par 10 ; +/- mise ; Entree agit : assurer la soute, acheter 1000 CR d'obligations, miser sur le premier lot, financer le premier convoi ; chaque texte vert d'un onglet est un bouton cliquable).
  - Assurance : [ASSURER LA SOUTE], [ASSURER SOUTE ET COQUE] (prime selon le risque reel et le dossier), [DECLARER LA SOUTE PERDUE] (controle : fraude = police annulee et amende).
  - Titres : [ACHETER 1000/5000 CR] d'obligations, [VENDRE] ; fonds actions et matieres [ACHETER 1000 CR] / [VENDRE] ; options d'achat et de vente sur le bien choisi (fleches).
  - Ventes : [MISER] sur un lot d'enchere (+/- pour la mise), [EXPERTISE] et [ACHETER] aux puces, [VENDRE TOUT AUX PUCES], [CAMPAGNE DE PUBLICITE 2000 CR] pour le bien choisi.
  - Lignes : fleches pour choisir une ligne du systeme, [FINANCER 1000 CR] sur un convoi encore a quai.
  - Economie : clic sur une juridiction pour son tableau et ses courbes (24 mois).
- Contrats (C) : les croisieres et pelerinages ont plusieurs escales ; Entree a chaque escale (dans l'ordre), puis a l'arrivee.

- Accueil (vol ou quai) : portails, dimensions et temps (Entree portail, A ancre, R remonter, X lever, +/- jours, S stase).

- Ecran Portail/Temps (Accueil), colonne HAUTE SCIENCE-FICTION : N personne suivante, K cloner (1 kg de nanites + 80000 CR). Le HUD de vol affiche en plus l'antimatiere (piege), la bulle Alcubierre et les phenomenes exotiques proches.
- Ecran Portail/Temps (Accueil) : 1/2/3 echo, B briser une boucle, O avant-poste, Gauche/Droite + J relais.

- Fin (vol ou quai) : explorer a pied (fleches / pave numerique, Entree escalier, Espace attendre, Tab chef, clic = marcher).
- Exploration à pied (un seul système pour les mondes, stations, épaves, ruines, grottes, portails, dimensions) : touche Fin en vol ou à quai, ou E sur la carte de surface (F8). Mondes : « Atterrir » ; la surface a ses sites qui mènent aux étages des lieux.
- Pilotage : Inser (ou « ORBITE CIRCULAIRE » du menu) met le vaisseau sur une orbite circulaire autour du monde le plus proche (dans 12 rayons), payee en carburant. Un contact a moins de 25 m/s avec un monde solide est un atterrissage (le vaisseau reste pose jusqu'a la poussee ; Fin pour explorer a pied) ; plus vite, c'est un impact. L'air des mondes freine les plongees.
- Exploration a pied : F bascule la vue a la premiere personne (prototype a la Daggerfall : memes etages, memes tours ; gauche/droite tournent d'un huitieme de tour, haut/bas avancent et reculent ; creatures, gens, arbres, plantes, bornes et escaliers sont des panneaux faits d'ASCII).
- Matiere (exploration à pied) : X creuse ou casse la case devant (outil et dureté = temps ; donne les ressources de la matière selon la physique du monde), B construit un mur de pierre taillée, N pose un sol de tôle (ou un pont sur l'eau) avec ce que porte l'équipe ; les cases bâties sont à vous. La glace fait déraper, soufre et lave blessent, le froid mord sans vêtements chauds.

- Panneau de physique (pilotage, ecran assez haut) : ORBITE (periapsis/apoapsis sont des altitudes, periode, vitesses circulaire et de liberation, excentricite), HOHMANN vers la cible (dv, duree, attente de la fenetre), flux solaire, retard de la lumiere de l'etoile, charge en g, dose (mGy/h et cumulee) et signature infrarouge. Aucune touche nouvelle.

## Relations et surprises
Ecran Factions (X) : Tab ouvre le registre des relations (Haut/Bas choisir, PgUp/PgDn sauter, Tab retour a l'anneau, X/Echap quitter). Journal (J) : quand une surprise attend un choix, elle s'affiche en bas ; touches 1 a 3 pour repondre.

- **Couche BIOSPHERE** (écran de surface F8, Tab jusqu'à la dernière couche) : la carte montre la densité de l'espèce choisie ; V / Maj+V changent d'espèce, un clic sur une espèce de la liste « ICI » l'affiche, Entrée ouvre et ferme la fiche de sa variante locale (générée depuis les données et l'état du monde), F ouvre le codex.

- **S** (sur l'ecran des factions, X) : page SOCIETE de la faction choisie (Gauche/Droite) : loi locale et situation juridique, langue (mots, ere, ce que l'equipage et l'IA comprennent, effet sur les prix), culture du lieu (annee locale, saison, cuisine, fete), equipage et competence linguistique, ecole (PgUp/PgDn choisir l'etude, **E** inscrire l'equipier choisi avec Haut/Bas), arts (**A** art suivant, **C** composer, **V** vendre la derniere oeuvre a la galerie, **D** la donner au musee), sports (**J** sport suivant, **P** parier 100 CR sur le favori, a quai). Tous ces choix sont aussi des boutons cliquables ; S ou X/Echap pour revenir.
## Concevoir son vaisseau (chantier naval, touche C)
Barre d'onglets en haut du chantier : COQUES, CONCEPTION, EQUIPEMENT, PLANS ET COMMANDES (clic) ; Tab passe a l'onglet suivant, Echap revient au chantier.
- CONCEPTION : clic sur une piece de la liste (ou PgUp/PgDn), clic sur la grille pour la poser (outils POSER, RETIRER, CHOISIR ; fleches = curseur, Entree pose, X retire) ; couche (etage) Debut/Fin ou boutons - et + ; M ou bouton SYMETRIE pour poser aussi le miroir (babord/tribord) ; N nomme le plan. Glisser la souris sur l'apercu le fait tourner. Les pieces cerclees de rouge ne sont pas reliees au noyau.
- EQUIPEMENT : clic sur une case de la grille, puis sur un emplacement, puis sur un module (double clic ou Entree installe, X retire) ; Espace change de zone (grille, emplacements, modules), fleches pour naviguer.
- PLANS ET COMMANDES : colonnes MES PLANS (clic choisit, double clic modifie ; boutons NOUVEAU, COPIER, SUPPRIMER, NOM, MODIFIER, EXPORTER ; fichiers de `saves/plans` a importer d'un clic), PLANS EN VENTE (double clic ou Entree achete), CONSTRUIRE (boutons - + equipes, COMMANDER LA CONSTRUCTION ; O au clavier ; PgUp/PgDn equipes) et MA FLOTTE (EMBARQUER, DEMONTER, REFONDRE AVEC LE PLAN CHOISI ; au clavier Entree, D, R). Gauche/Droite change de colonne, F nouveau plan, C copier, X supprimer, E exporter, I importer.
