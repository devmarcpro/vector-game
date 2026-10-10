# PROMPT INITIAL — Construire VECTOR en autonomie totale

> À coller tel quel comme premier message d'une nouvelle session Claude Code ouverte dans `C:\Users\ciryl\jeu espace`.
> Lis aussi `DESIGN.md` (même dossier) : c'est le document d'architecture validé avec l'utilisateur. En cas de conflit, ce prompt prime.

---

## 0. Mission

Tu es l'unique développeur d'un jeu PC **solo**, en **Rust**, avec un **engine 100 % custom** (pas de moteur de jeu, pas de bibliothèque de rendu 3D haut niveau). Tu travailles **en autonomie totale** jusqu'à ce que le jeu soit **jouable de bout en bout et riche en contenu** (critères précis en section 12).

**Ne pose pas de questions à l'utilisateur** sauf blocage réel et irrémédiable (voir section 13). Prends les décisions toi-même, consigne-les dans `docs/DECISIONS.md`, et avance. L'utilisateur reviendra voir le résultat ; il veut un jeu qui marche, pas un dialogue.

Nom de travail : **VECTOR**. Tu peux le renommer si tu trouves mieux (note-le dans DECISIONS.md).

## 1. Vision du jeu

Un jeu d'**exploration et de gestion spatiale en sandbox total**, au look des premiers jeux 3D (Elite 1984, Starglider, Battlezone) :

- **Graphismes** : 3D en **lignes** (fil de fer, wireframe), monochrome ou 2-3 couleurs, façon écran vectoriel/phosphore. Aucun sprite, aucune texture, aucun modèle 3D importé. Tous les modèles sont **générés procéduralement** en code ou décrits en données (listes de sommets/arêtes).
- **Beaucoup de texte et d'ASCII** : interface en grille de caractères monospace (comme un terminal), panneaux, tableaux, jauges ASCII (`[#####-----]`), cartes ASCII, journal de bord, descriptions générées.
- **Monde infini généré procéduralement** à partir d'une graine : galaxie → secteurs → systèmes stellaires → corps (étoiles, planètes, lunes, ceintures d'astéroïdes, stations, épaves, anomalies).
- **Sandbox sans limite artificielle** : pas de scénario imposé, pas de niveau max, pas de zones verrouillées. Le joueur choisit son style : mineur, marchand, explorateur, pirate, bâtisseur de colonies, chef de flotte, ou contemplatif. Les « missions » sont des contrats **émergents** issus de la simulation (offres/demandes des marchés, besoins des colonies, menaces), jamais une campagne scriptée.
- **Simulation profonde** : le plus de réalisme possible, ajouté **par couches** (voir 6). On préfère peu de règles simples qui interagissent à beaucoup de cas particuliers.
- **Solo uniquement.** Pas de réseau.
- **Temps hybride** : pilotage **en temps réel** dans le cockpit 3D ; **cartes et menus de gestion** en pause active ou en accélération du temps (x1 jusqu'à x1000+).
- **Très optimisé** : doit tourner à 60+ FPS sur un PC modeste et simuler des milliers d'entités sans à-coups.

## 2. Contraintes techniques non négociables

1. **Langage : Rust** (édition 2021 ou 2024), toolchain `stable`. Workspace Cargo multi-crates.
2. **Engine custom** : tu écris toi-même le rasteriseur de lignes (CPU), la projection, le clipping, le rendu de texte bitmap, la boucle de jeu, l'ECS, la génération procédurale, la sauvegarde. Dépendances externes **autorisées et minimales** :
   - fenêtre/entrées : `winit` ;
   - présentation d'un framebuffer CPU : `softbuffer` (ou `pixels`) ;
   - sérialisation : `serde` + `ron` (données) + `postcard`/`bincode` (sauvegardes) ;
   - parallélisme : `rayon` (optionnel, seulement si mesuré utile) ;
   - tests/bench : `criterion` (optionnel), `proptest` (optionnel) ;
   - audio : `cpal` (+ synthèse maison, voir 9) — optionnel mais souhaité.
   - **Interdits** : Bevy, Unity-like, ggez, macroquad, SDL pour le rendu, tout moteur ECS tiers (écris le tien), toute lib de rendu 3D, tout asset binaire importé.
3. **Plateforme cible : Windows 11** (le dev se fait sous PowerShell/Git Bash). Garde le code portable (pas d'API Win32 directe) mais ne teste que Windows.
4. **Déterminisme** : la simulation est à **pas fixe** (par défaut 30 Hz sim ; le rendu tourne indépendamment). Même graine + mêmes commandes = même résultat. Pas de `HashMap` à itération non déterministe dans la sim (utilise des structures ordonnées ou des IDs triés) ; pas de `SystemTime`/`rand::thread_rng` dans la sim. Tout le hasard vient d'un RNG maison seedé (ex. SplitMix64/PCG) et de `hash(seed, coord)`.
5. **Séparation stricte sim / rendu / UI** : la sim ne connaît ni la fenêtre ni le framebuffer. Les entrées du joueur deviennent des **`Command`** appliquées à la sim. Les PNJ utilisent les mêmes commandes.
6. **Zéro allocation dans la boucle chaude** quand c'est raisonnablement possible (réutilise des `Vec`, arènes, pools). ECS en **SoA** (tableaux contigus par composant), IDs générationnels.
7. **Tout le contenu est de la donnée** (fichiers `.ron` dans `data/`) : ressources, matériaux, modules de vaisseau, vaisseaux, recettes, bâtiments de colonie, factions, noms, événements, tables de génération. Ajouter du contenu ne doit pas exiger de modifier le code Rust (sauf nouveau *type* de comportement).
8. **Aucun `unsafe`** sauf justification mesurée et commentée.
9. **Qualité** : `cargo fmt`, `cargo clippy -- -D warnings`, `cargo test` verts à chaque commit. Pas de `unwrap()` dans le code de production hors invariants prouvés ; erreurs typées (`thiserror` autorisé).

## 3. Mise en place de l'environnement (première chose à faire)

L'état de départ connu : **Rust n'est pas installé**, aucun linker MSVC détecté, le dossier est vide (hors `DESIGN.md`) et **n'est pas un dépôt git**.

1. `git init` dans le dossier ; crée un `.gitignore` (`target/`, `saves/`, `*.log`, captures).
2. Installe la toolchain. Essaie dans l'ordre :
   - `winget install --id Rustlang.Rustup -e` puis `rustup default stable` ;
   - pour le linker : `winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"` (lourd, plusieurs Go, prends ton temps) ;
   - **repli sans MSVC** : `rustup toolchain install stable-x86_64-pc-windows-gnu` + `rustup default stable-x86_64-pc-windows-gnu` (nécessite un MinGW : `winget install --id MSYS2.MSYS2` ou `BrechtSanders.WinLibs.POSIX.UCRT`). Choisis ce qui marche le plus vite.
   - Si une installation demande une élévation/approbation que tu n'as pas, **n'abandonne pas** : essaie le repli, sinon explique précisément à l'utilisateur la commande à lancer (voir 13) et continue en préparant tout le code qui n'a pas besoin de compiler.
3. Vérifie : `cargo --version`, puis un `cargo new` jetable qui affiche « hello » pour prouver que la chaîne complète (compilation + liaison) fonctionne.
4. Redémarre le shell/PATH si besoin (`$env:USERPROFILE\.cargo\bin`).

## 4. Organisation du dépôt

```
jeu espace/
  Cargo.toml                (workspace)
  DESIGN.md                 (existant)
  PROMPT_AUTONOME.md        (ce fichier)
  docs/
    PROGRESS.md             (journal d'avancement — tu le tiens à jour en continu)
    DECISIONS.md            (décisions d'architecture et pourquoi)
    CONTENT.md              (inventaire du contenu : ce qui existe, ce qui manque)
    CONTROLS.md             (touches et UI, à jour)
    SIM_MODEL.md            (formules et unités de la simulation)
  data/                     (contenu .ron, voir 7)
  crates/
    core/    (ECS, maths vecteurs/matrices/quaternions, RNG/hash, IDs, utilitaires)
    data/    (schémas serde + chargement + validation des .ron)
    world/   (génération procédurale, orbites, noms, cartes)
    sim/     (tous les systèmes de simulation, commandes, événements)
    render/  (framebuffer, lignes, projection, clipping, police bitmap, effets)
    ui/      (grille de texte, widgets ASCII, écrans)
    audio/   (synthèse procédurale, optionnel)
    game/    (binaire : fenêtre, boucle, entrées, sauvegardes, assemblage)
    tools/   (binaires utilitaires : worldgen_dump, sim_bench, render_snapshot, balance_check)
  saves/                    (ignoré par git)
```

## 5. Architecture technique détaillée

### 5.1 Boucle principale
Pas fixe pour la sim avec accumulateur (cap du nombre de pas par frame pour éviter la spirale de la mort). Le rendu interpole entre les deux derniers états pour la fluidité. L'accélération du temps (x1…x1000) = plus de pas sim par frame **et/ou** pas de temps plus grand pour les systèmes « macro » (voir LOD). Pause = zéro pas.

### 5.2 ECS maison
- `Entity { index: u32, generation: u32 }`.
- Stockage de composants par type en SoA (`Vec<T>` dense + table sparse/slot map). Requêtes simples par itération sur 1 à 3 composants ; pas besoin de macros complexes mais l'API doit rester ergonomique.
- Systèmes = fonctions `fn(&mut World, &SimContext)` exécutées dans un ordre fixe et documenté ; parallélisme `rayon` uniquement pour les systèmes sans dépendance mutable croisée, après mesure.
- Ressources globales (horloge, RNG, marchés, tables de données) dans un conteneur typé.

### 5.3 Niveaux de détail (LOD) de simulation — **crucial pour tenir « tout le réel »**
- **LOD 0 (local)** : le système stellaire où se trouve le joueur + ses propres vaisseaux/colonies actives. Simulation fine : physique, chaque module, chaque membre d'équipage, atmosphère/énergie par compartiment, chaque cargaison.
- **LOD 1 (proche)** : systèmes voisins ou possédés : agrégats (population, production, stocks, flottes comme unités) mis à jour à pas plus large (ex. 1 pas/min de temps de jeu).
- **LOD 2 (lointain)** : le reste de la galaxie n'existe **pas** en mémoire : il est **reconstruit à la demande** depuis `hash(seed, coord)` et un petit « delta » persistant. Les marchés/factions lointains évoluent par équations macro, avec rattrapage (« catch-up ») analytique quand le joueur arrive.
- Promotion/dégradation : quand le joueur entre dans un système, ses entités passent en LOD 0 ; en partant, elles sont **agrégées** de façon réversible et déterministe.

### 5.4 Univers infini et précision numérique
- Position = `(sector: IVec3 (i64), local: DVec3 (f64))` ; renormalisation du `local` dans le secteur. Dans un système stellaire, repère local centré sur l'étoile ou sur le joueur (« floating origin ») pour que le rendu en `f32` n'ait jamais de jitter.
- Hiérarchie : **galaxie** (spirale/bras, densité d'étoiles) → **secteurs** (cubes, ex. ~10 al) → **systèmes** (0..N étoiles, planètes, lunes, ceintures) → **corps**.
- Orbites **képlériennes** (sur rails) pour les corps naturels : position = fonction analytique du temps (aucune intégration → accélération du temps gratuite). Vaisseaux : dynamique newtonienne avec poussée, plus **trajectoires prédictives** et transferts calculés (Hohmann simplifié/lambert approché) pour le « warp » interne au système.
- Voyage interstellaire : choisis un modèle simple et jouable (ex. **moteur FTL à saut avec charge/chauffe/carburant**, temps de trajet proportionnel à la distance, événements possibles pendant le saut). Documente-le dans `SIM_MODEL.md`.
- Persistance : on ne sauvegarde **que** la graine + les **deltas** (corps modifiés, stocks, entités du joueur, état des factions/marchés touchés, horloge). Fichier binaire versionné (`postcard`), avec migration simple si le format change.

### 5.5 Rendu (crate `render`)
- Framebuffer CPU RGBA. Lignes : Bresenham ou Wu (anti-aliasing optionnel), épaisseur 1-2 px, additif pour l'effet « phosphore » ; option de **lueur** (blur très léger, désactivable) et scanlines.
- Pipeline : sommets modèle → monde → vue → projection perspective → **clipping** de segments contre le frustum (au minimum plan proche + écran) → écran. Culling d'objets entiers (sphère englobante) avant tout.
- **Suppression des lignes cachées** simple pour les gros corps (planètes/astéroïdes sphériques : dessiner uniquement le contour/limbe + parallèles/méridiens visibles) ; les vaisseaux restent en fil de fer complet (esthétique Elite). Optionnel : arêtes cachées pour les polyèdres convexes via test de face avant.
- Planètes : sphères en fil de fer (latitudes/longitudes) avec un motif ASCII/lignes dérivé du type de planète (bandes pour géantes gazeuses, calottes polaires, contours de « continents » en polylignes générées par bruit). Étoiles : cercle + rayons/couronne. Anneaux, nébuleuses en points/traits épars.
- **Police bitmap monospace** embarquée dans le code (8x8 ou 8x16, jeu ASCII + quelques symboles de boîtes `─│┌┐└┘├┤┬┴┼` et blocs `░▒▓█`). Rendu du texte en grille ; l'UI est une **grille de cellules** (char + couleur) que le renderer rastérise.
- Budget : rendu complet d'une frame (cockpit chargé + HUD) **< 4 ms** à 1280x720 sur CPU moyen. Mesure-le avec un benchmark.
- Résolution interne configurable (ex. 960x540 mise à l'échelle) pour garder des perfs et un look rétro net.

### 5.6 UI (crate `ui`) — le cœur de l'expérience
Écrans (tous navigables **clavier d'abord**, souris en bonus) :
1. **Cockpit** : vue 3D wireframe plein cadre + HUD ASCII (vitesse, cap, cible, jauges énergie/bouclier/coque/carburant, radar 3D à la Elite en ellipse, messages, alertes).
2. **Carte système** : vue de dessus/inclinée ASCII+lignes des orbites, sélection de corps, planification de trajet.
3. **Carte galactique** : secteurs, étoiles, routes, filtres (faction, ressources connues, danger), recherche.
4. **Vaisseau** : schéma ASCII des compartiments/modules, état de chaque module, équipage par poste, réparations, énergie (bus, priorités), éditeur/chantier naval.
5. **Équipage** : liste des membres, compétences, santé, moral, besoins, affectations, relations, historique.
6. **Cargaison/Inventaire** : soutes, masses/volumes, transferts, tri.
7. **Marché / Station** : prix, offre/demande, contrats, chantier, services (réparation, carburant, recrutement).
8. **Colonies/Empire** : liste des colonies, bâtiments, production/consommation, population, files de construction, logistique, politiques.
9. **Production/Recettes** : arbre de fabrication consultable, planificateur.
10. **Journal de bord / Encyclopédie** : logs d'événements, découvertes, « codex » généré qui décrit chaque ressource/module/planète rencontrés avec données réelles.
11. **Options / Règles de partie / Sauvegardes** : difficulté, **réglages sandbox** (voir 6.10), affichage (résolution, CRT, couleurs), touches.
12. **Console de debug** (touche `~`) : commandes `spawn`, `tp`, `time`, `seed`, `give`, `reveal`, `inspect`, etc. Indispensable pour tes tests et pour le joueur sandbox.

Chaque écran doit avoir une aide contextuelle (`?`).

## 6. Systèmes de simulation (par couches — implémente dans cet ordre, chaque couche reste jouable)

Pour chaque système : définir ses unités (SI), ses formules dans `docs/SIM_MODEL.md`, ses données dans `data/`, ses tests unitaires, et son affichage UI.

### Couche A — Fondations de vol
- Vaisseau = corps rigide (masse variable selon cargaison/carburant, tenseur d'inertie simplifié, moment angulaire). Contrôles : tangage/lacet/roulis, poussée avant/arrière/latérale (RCS), frein/assistance de vol (flight assist on/off), propulsion principale avec consommation de carburant (équation de Tsiolkovski).
- Caméra cockpit avec légère inertie ; vues : cockpit, extérieure, carte.
- Collisions simples (sphères englobantes puis test segment/polyèdre pour les impacts importants), dégâts selon l'énergie d'impact.
- Accélération du temps avec « autopilote » (voyage jusqu'à une cible via transfert calculé).

### Couche B — Univers
- Génération galaxie/secteur/système/corps déterministe, avec classes réalistes : étoiles (O, B, A, F, G, K, M, naines blanches, géantes, étoiles à neutrons, trous noirs rares, binaires), zones habitables, planètes rocheuses/océaniques/désertiques/glacées/lava/géantes gazeuses/géantes de glace, lunes, ceintures (astéroïdes de types C/S/M, glace), anneaux, comètes, stations/épaves/anomalies.
- Propriétés physiques cohérentes (masse, rayon, gravité de surface, température d'équilibre dérivée de la luminosité et du demi-grand axe, pression atmosphérique, composition de l'atmosphère, période orbitale/rotation, inclinaison, excentricité).
- Noms procéduraux (syllabes par culture/faction) + désignations catalogue (`HD 40492-b`).
- Scanners : découverte progressive (il faut **scanner** pour connaître composition/ressources ; niveaux de détail de connaissance par corps, persistés).

### Couche C — Ressources, minage, logistique
- Table périodique **simplifiée mais réelle** : éléments/minerais/volatils avec propriétés (densité, valeur, usages). Au minimum ~40 ressources : fer, nickel, cuivre, aluminium, titane, silicium, lithium, uranium, thorium, or, platine, terres rares, carbone, eau (glace), azote, oxygène, hydrogène, deutérium, hélium-3, méthane, ammoniac, soufre, silice, regolith, etc. + composés/produits intermédiaires.
- Astéroïdes : types (C carbonés, S silicatés, M métalliques, glacés), composition par gisements, taille/masse → **le minage réduit réellement la masse**, produit des débris/minerai ramassables. Minage de base **volontairement simple** : cibler → laser/foreuse à portée → minerai éjecté → collecte (grappin/soute magnétique) → soute (masse+volume). Variantes : forage de surface sur planètes/lunes par module dédié, collecte de gaz (écopage d'atmosphère de géantes).
- Raffinage et fabrication par **recettes** (entrées → sorties, temps, énergie) exécutées par des modules/installations. Arbre de production ≥ 100 recettes à terme.
- Logistique : soutes, conteneurs, transfert vaisseau↔vaisseau/station/colonie, convoyeurs automatiques (routes commerciales assignables à des vaisseaux IA).

### Couche D — Vaisseau, modules, énergie
- Vaisseau modulaire : coque (classes de tailles), emplacements typés (moteur, réacteur, bouclier, arme, soute, laboratoire, atelier, habitat, médic, serre, scanner, armurerie, hangar, tourelle, minage, quai…). Modules avec masse, énergie, chaleur, santé, entretien, besoin d'équipage.
- **Énergie** : production (réacteurs fission/fusion, panneaux solaires dépendants de la distance à l'étoile, RTG), batteries, bus de distribution avec priorités ; surcharge → coupures en cascade.
- **Chaleur** : dissipation par radiateurs, surchauffe, effets.
- **Support de vie** : atmosphère par compartiment (O2/CO2/pression), eau, nourriture ; fuites/brèches, incendies simples.
- **Dégâts** localisés par compartiment/module, réparation par équipage + matériaux.
- Éditeur de vaisseau en ASCII/wireframe (placer des modules sur une grille, calculs de masse/centre de masse/consommation en direct) et **chantier naval** produisant des vaisseaux à partir de plans (sauvegardables).

### Couche E — Équipage et habitants
- Chaque personne : identité générée, âge, santé (blessures, maladies, radiation, fatigue), **compétences** (pilotage, ingénierie, médecine, minage, science, combat, commerce, leadership…), **besoins** (nourriture, eau, sommeil, social, sens), **moral**, traits de personnalité, relations (amitiés/conflits), salaire/loyauté.
- Postes et quarts de travail, affectation manuelle ou automatique ; IA de tâches (réparer, soigner, cuisiner, miner, piloter) avec priorités réglables.
- Événements humains émergents (conflits, romances, mutinerie, maladie, deuil, célébrations) rendus par un **journal textuel** généré.
- Recrutement dans les stations/colonies, formation, vieillissement, naissance (colonies), mort.

### Couche F — Économie
- **Marchés locaux** par station/colonie : offre/demande par ressource, prix dynamiques (élasticité, stocks, production locale, taxes), spreads, frais. Pas de prix scriptés.
- **PNJ commerçants/mineurs/pirates/patrouilles** simulés (LOD adapté) qui font bouger l'économie, avec les mêmes règles que le joueur.
- Monnaie unique + éventuellement monnaies de faction ; banque simple (prêts avec intérêts).
- **Contrats émergents** : un marché en pénurie génère une offre de livraison ; une colonie sous attaque génère un contrat de défense ; etc.
- Réputation par faction, législation (contrebande), douanes/inspections.

### Couche G — Colonies et empire
- Coloniser un corps : choix d'un site, transport de modules/colons/matériaux, **terraformation** lente optionnelle pour très long terme.
- Bâtiments (data) : habitats, mines, raffineries, usines, serres/agri, centrales, spatioports, laboratoires, défenses, marché, etc. Consommations/productions, maintenance, entretien.
- Population agrégée par groupes (âge/compétence) en LOD 1, **individus détaillés** quand le joueur y est (LOD 0).
- Croissance, satisfaction, productivité, pénuries, émeutes, migration. Politiques (impôts, rationnement, priorités de production).
- Réseaux logistiques entre colonies (routes automatiques, entrepôts, convois).
- Gestion de **flottes** (formations, ordres, ravitaillement, maintenance) déléguables à l'IA.

### Couche H — Monde vivant, danger, combat
- Factions (4 à 8 avec cultures, idéologies, économies, relations diplomatiques évolutives : paix/commerce/guerre/embargo), territoires, flottes, stations.
- Menaces : pirates, créatures spatiales ou anomalies (optionnel), météoroïdes, éruptions stellaires, radiations, pannes, épidémies.
- Combat simple mais lisible : armes à énergie dirigée, projectiles balistiques (avec vraie balistique), missiles, mines, brouillage ; boucliers/armure/dégâts par module ; ciblage de sous-systèmes ; IA tactique basique (poursuite, orbite, fuite, soutien).
- Abordage et capture/épaves/récupération (salvage).

### Couche I — Science, connaissances, réalisme additionnel (apports progressifs, « pas mal de contenu » exige d'en livrer plusieurs)
Au minimum, livre : **recherche technologique** (arbre en données, débloque modules/recettes), **exploration scientifique** (échantillons, relevés, anomalies, codex), **climat/atmosphère simples** pour les planètes colonisées, **radiations/vents stellaires**, **médecine** (maladies, traitements), **chimie de production** (conservation de la masse). Si le temps le permet : **géologie**, **biologie extraterrestre simple** (écosystèmes générés, ressources biologiques), **astronomie** (éclipses, alignements affichés), **météorologie** locale.

### 6.10 Réglages sandbox (« règles de partie »)
Le joueur peut, au démarrage ou en cours de partie, activer/désactiver/moduler : consommation de carburant, besoins de l'équipage, mort permanente, dégâts, coût des constructions, vitesse de recherche, agressivité des IA, fréquence des événements, fog of war (carte révélée ou non), ressources infinies, invulnérabilité, mode créatif (spawn/téléportation/édition du monde), vitesse de temps max. Un **profil** par partie, modifiable à tout moment (avec note « partie modifiée »).

## 7. Données (`data/*.ron`)

Définis des schémas serde versionnés et un **validateur** (`tools/data_check`) qui échoue sur : références cassées, doublons d'ID, valeurs hors bornes, recettes impossibles (ressource inconnue), cycles de production inutiles. Fichiers prévus :
- `resources.ron`, `materials.ron`, `recipes.ron`
- `modules.ron`, `hulls.ron`, `ships_templates.ron`, `weapons.ron`
- `buildings.ron`, `colony_policies.ron`
- `factions.ron`, `cultures_names.ron`
- `tech_tree.ron`
- `events.ron` (tables d'événements pondérées avec conditions/effets déclaratifs)
- `skills.ron`, `traits.ron`, `diseases.ron`
- `worldgen/*.ron` (classes stellaires, types de planètes, ceintures, anomalies)
- `text/*.ron` (modèles de phrases pour journaux, descriptions, codex, avec variables)

Les valeurs doivent être **réalistes et équilibrées** ; documente les sources/ordres de grandeur dans `docs/SIM_MODEL.md`.

## 8. Génération de texte
Les descriptions (planètes, stations, personnages, événements, journal) sont assemblées par **grammaires/gabarits** pilotés par les données réelles de l'objet (ex. température, composition, histoire). Évite le texte générique identique ; vise variété et cohérence. Prévoir une centaine de gabarits dès le départ, extensible.

## 9. Audio (optionnel mais souhaité en fin de parcours)
Synthèse **procédurale** uniquement (pas de fichiers) : bruit de moteur selon la poussée, bips d'interface, alarmes, hum du réacteur, laser de minage, impacts, ambiances. Désactivable. Ne bloque jamais la jouabilité.

## 10. Méthode de travail autonome

### 10.1 Boucle de travail
Pour chaque jalon (section 11) : (1) planifie en sous-tâches, (2) implémente, (3) **teste réellement** (voir 10.2), (4) corrige, (5) `cargo fmt`/`clippy`/`test`, (6) commit git avec un message clair, (7) mets à jour `docs/PROGRESS.md` (date, ce qui marche, ce qui reste, bugs connus), (8) passe au suivant. **Ne t'arrête pas** tant que les critères de la section 12 ne sont pas remplis.

### 10.2 Comment vérifier sans « voir » l'écran
Tu n'as pas toujours d'œil sur la fenêtre. Construis dès M0 :
- Un **mode headless** de rendu : `render_snapshot` qui rend une scène donnée dans un framebuffer et l'écrit en **PNG** (via un petit encodeur maison ou le crate `png`/`image` limité à cet outil). Ouvre/inspecte l'image avec l'outil de lecture d'images pour vérifier visuellement lignes, HUD, texte.
- Un **mode scénario scripté** : le jeu accepte `--script fichier.ron` qui injecte des commandes/touches et fait des captures à des instants donnés, puis quitte. Sert aux tests d'intégration visuels et aux régressions.
- **Tests unitaires** de tout ce qui est calculable (orbites, conservation de masse/énergie, génération déterministe : *même graine → mêmes hash*, sérialisation aller-retour, économie stable sur 10 000 pas, pas de NaN/Inf).
- **Tests de propriétés / long run** : un binaire `sim_bench` qui fait tourner 1 an, 10 ans, 100 ans de jeu accéléré et vérifie : pas de panique, pas de valeurs aberrantes, économie qui ne s'effondre pas ni n'explose, population plausible, temps CPU par pas sous budget.
- **Équilibrage automatique** : `balance_check` simule des stratégies types (mineur seul, commerçant, colonisateur) sur N heures de jeu simulées et rapporte revenus/h, risques, goulots ; ajuste les données pour qu'aucune stratégie ne soit absurdement dominante ni impossible.
- Quand possible, **lance vraiment le jeu** (fenêtre) via le terminal/`game-automation` si ce skill est dispo, capture l'écran, vérifie. Mais le headless est la base.

### 10.3 Performance
- Benchmarks suivis dans `docs/PROGRESS.md` : temps de rendu par frame, temps de pas sim (LOD 0 avec 5 000 entités, LOD 1 avec 1 000 systèmes), temps de génération d'un système (< 1 ms visé), temps de sauvegarde/chargement.
- Profile avant d'optimiser. Cibles : **60 FPS** stables, pas sim 30 Hz en < 8 ms au pire cas courant, génération d'un secteur à la volée sans saccade (génère en tâche de fond ou sur plusieurs frames), RAM < 1 Go.

### 10.4 Hygiène
- Commits fréquents et atomiques (au moins un par sous-fonctionnalité). Branche unique `main` suffit.
- Journal `docs/PROGRESS.md` toujours à jour : si ta session est interrompue/contextuellement tronquée, la suivante doit pouvoir reprendre **uniquement** avec ce fichier + `git log`.
- Garde le code lisible, modules petits, noms en anglais dans le code, **textes joueur en français** (par défaut ; centralise les chaînes pour une future traduction).
- Ne laisse pas de `TODO` critique non suivi : tout TODO va dans `docs/PROGRESS.md`.

## 11. Jalons (ordre à suivre)

Chaque jalon doit se terminer par un état **jouable** (lançable) et commité.

**M0 — Fondations (jouable : démo technique)**
Toolchain OK, workspace, boucle à pas fixe, fenêtre, framebuffer, rasteriseur de lignes avec clipping, police bitmap, grille de texte, cube wireframe qui tourne + HUD ASCII, `render_snapshot` PNG, CI locale (fmt/clippy/test). Benchmarks de rendu.

**M1 — Vol et cockpit**
ECS, maths, caméra, vaisseau pilotable (poussée, rotation, flight assist), cockpit + HUD + radar, étoiles de fond, un corps céleste simple, collisions basiques, console debug. Contrôles documentés dans `CONTROLS.md`.

**M2 — Un système solaire complet**
Génération déterministe d'un système (étoile, planètes, lunes, ceinture), orbites képlériennes, rendu des corps en wireframe, carte système, sélection/ciblage, autopilote + accélération du temps, floating origin.

**M3 — Minage, soute, station, sauvegarde**
Astéroïdes avec composition, laser de minage, collecte, soute (masse/volume), station avec marché minimal (vente), crédit/monnaie, **sauvegarde/chargement** complets (graine + deltas), menu principal, écran inventaire.

**M4 — Vaisseau modulaire et énergie**
Modules (data), énergie/chaleur, dégâts par module, éditeur de vaisseau, chantier naval, ravitaillement, support de vie de base.

**M5 — Équipage**
Personnages, compétences, besoins, postes, IA de tâches, journal, recrutement, événements humains simples.

**M6 — Galaxie infinie**
Génération galaxie/secteurs, carte galactique, FTL, scanners/découverte progressive, LOD 1/2, rattrapage de simulation lointaine, performance de génération à la volée.

**M7 — Économie vivante**
Marchés dynamiques, PNJ commerçants/mineurs/pirates/patrouilles, contrats émergents, factions et réputation, contrebande, banque.

**M8 — Colonies et empire**
Colonisation, bâtiments, production/recettes complètes, population, politiques, logistique et routes automatiques, gestion de flottes déléguée.

**M9 — Combat, menaces, science**
Armes variées, IA tactique, boucliers/armure, abordage/épaves, recherche technologique, anomalies, médecine/maladies, événements environnementaux (éruptions, radiations).

**M10 — Contenu et finition**
Multiplier le contenu (voir 12), équilibrage automatique, textes variés, audio procédural, options graphiques (CRT, couleurs), tutoriel intégré, écrans d'aide, polissage UI, optimisations, correction de bugs, documentation finale, packaging (`cargo build --release` + dossier `dist/` avec `data/`).

## 12. Définition de « terminé » (critères d'acceptation)

Le jeu est considéré **jouable avec beaucoup de contenu** quand **tout** ceci est vrai :

**Jouabilité**
- [ ] Lancement sans erreur depuis `dist/` ; menu principal ; nouvelle partie avec graine saisissable/aléatoire et réglages sandbox ; sauvegarde/chargement multiples fiables.
- [ ] Un joueur peut, **sans aide extérieure**, pendant plusieurs heures : miner, vendre, acheter un meilleur vaisseau, recruter un équipage, explorer plusieurs systèmes et secteurs, fonder une colonie, y construire une chaîne de production, assigner des routes commerciales automatiques, déléguer une flotte, combattre des pirates, faire de la recherche.
- [ ] Aucun écran sans issue ; navigation clavier complète ; aide contextuelle présente ; tutoriel court de prise en main (pilotage + minage + vente + gestion).
- [ ] 60 FPS stables, sauvegarde < 1 s, génération à la volée sans saccade perceptible.

**Contenu minimal (inventaire à tenir dans `CONTENT.md`, avec compte réel)**
- ≥ 40 ressources, ≥ 100 recettes, ≥ 60 modules de vaisseau, ≥ 15 coques/classes de vaisseaux (du petit mineur au cargo/vaisseau de guerre/station), ≥ 25 types de bâtiments de colonie, ≥ 20 armes/défenses, ≥ 60 technologies, ≥ 6 factions avec cultures et noms propres, ≥ 40 compétences/traits combinés, ≥ 30 maladies/blessures/états, ≥ 150 événements émergents, ≥ 300 gabarits de texte, ≥ 12 types de planètes et ≥ 8 classes d'étoiles, ≥ 10 types d'anomalies/points d'intérêt.
- Chaque ressource/module/bâtiment/technologie a une entrée **codex** générée avec ses données réelles.

**Profondeur**
- [ ] Les systèmes des couches A à I existent et **interagissent** (ex. une pénurie d'eau sur une colonie → baisse du moral → baisse de productivité → hausse des prix → contrats de livraison générés → des PNJ y répondent).
- [ ] Les réglages sandbox fonctionnent réellement (chaque option a un effet testé).
- [ ] Le long run (100 ans de jeu accéléré) ne plante pas et reste stable.

**Qualité**
- [ ] `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test` entièrement verts ; `data_check` vert.
- [ ] Documentation à jour : `PROGRESS.md`, `DECISIONS.md`, `CONTENT.md`, `CONTROLS.md`, `SIM_MODEL.md`, plus un `README.md` (installation, lancement, contrôles, structure, comment ajouter du contenu en données).

Si tu atteins ces critères et qu'il te reste de la marge, **continue à enrichir** (profondeur, contenu, polish) plutôt que de t'arrêter : ajoute de nouvelles couches de réalisme, plus d'événements, plus de textes, meilleure IA, meilleur équilibrage.

## 13. Quand (et seulement quand) t'arrêter ou demander

Ne demande **rien** pour les choix de design, de nommage, d'équilibrage, de bibliothèques secondaires : décide et consigne. Interromps-toi uniquement si :
- Une installation indispensable (Rust/linker) est **impossible** sans action de l'utilisateur (droits admin refusés, antivirus bloquant…) après avoir essayé les replis ; dans ce cas, écris dans `docs/PROGRESS.md` et dans ta réponse **la commande exacte** à exécuter et ce qui s'est passé, puis continue tout ce qui ne dépend pas de la compilation.
- Une action serait **destructrice hors du dossier du projet** ou nécessiterait des identifiants/paiements.
- Tu as atteint les critères de la section 12 **et** passé du temps raisonnable sur l'enrichissement : fais alors un rapport final (ce qui existe, comment lancer, contenu en chiffres, limites connues, idées suivantes).

## 14. Pièges connus et conseils

- **Portée** : la profondeur « tout le réel » est une direction, pas un livrable M0. Livre en **tranches verticales jouables**, puis approfondis. Évite les systèmes à moitié connectés : un système n'est « fini » que s'il a données + sim + UI + tests + codex.
- **Déterminisme** : attention aux itérations de `HashMap`, aux flottants dans des chemins comparés à l'égalité, à l'ordre des systèmes, au parallélisme non déterministe. Tests de replay (même séquence de commandes → même hash d'état).
- **Précision** : toujours `f64` en sim ; `f32` seulement au dernier moment pour le rendu, après soustraction de l'origine flottante.
- **Explosion combinatoire des données** : génère-les en partie par programme/tables paramétriques (ex. variantes de modules par tier/matériau) plutôt qu'à la main, tout en gardant des identités distinctes.
- **UI texte** : privilégie la lisibilité (alignement, tableaux, couleurs limitées), cohérence des raccourcis, retours d'information immédiats (messages, sons, flash de lignes).
- **Équilibrage** : fais-le par simulation automatique, pas à l'intuition seule.
- **Sauvegardes** : versionne le format dès le début ; teste le chargement d'anciennes sauvegardes quand le format évolue.
- **Windows** : attention aux chemins avec espaces (`jeu espace`) — cite toujours les chemins ; préfère les chemins relatifs dans le code et utilise `std::path`.
- Quand tu doutes entre « plus de fonctionnalités » et « plus de solidité », choisis la solidité jusqu'à ce que le jalon soit jouable, puis enrichis.

## 15. Premier geste attendu

1. Lis `DESIGN.md`.
2. Crée `docs/PROGRESS.md` avec le plan des jalons M0→M10 et coche au fur et à mesure.
3. Installe la toolchain (section 3) et valide un « hello ».
4. Démarre M0.

Bon courage — construis quelque chose de grand et de cohérent.
