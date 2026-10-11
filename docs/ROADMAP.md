# ROADMAP (validée par l'utilisateur le 2026-10-11)

État de départ : M0-M10 terminés (voir PROGRESS.md, ACCEPTANCE.md). 147 tests verts, 100 ans stables.

## Étape 0 — Consolidation (avant les extensions, ou en parallèle des petites étapes)
1. Jouabilité réelle : parcours complet par script (miner, vendre, acheter un module, fonder une colonie, sauter) ; 3-4 scripts de régression (combat, colonie, FTL, rechargement).
2. Équilibrage : plafonner nanomatériaux, barres de combustible, diamant ; revoir le recyclage ; `balance_check` sous 10 avertissements.
3. Performance : comprendre le ralentissement du vol libre sur très longue durée (`sim_bench drift`).
4. Interface : accents dans la police 5x7 (au minimum é è à ç ê ô), retirer la normalisation ASCII des générateurs, captures supplémentaires.
5. Finition : message clair ou migration pour les sauvegardes (format v11), régénérer `dist/VECTOR.zip`, mettre à jour ACCEPTANCE.md.

## Extensions validées (ordre conseillé, chacune découpée en étapes testables)
### E1 — Planètes détaillées
Carte de surface par planète, générée à la demande depuis la graine (bruit + plaques simplifiées) : continents, océans, relief ; biomes (température, humidité, altitude) ; climats par latitude et saisons ; météo et catastrophes (tempêtes, séismes, éruptions) ; villes issues des colonies placées sur la carte (côte, rivière, ressources). Rendu : globe en fil de fer avec courbes de niveau et villes, écran de surface en grille. Sauvegarde : seulement des deltas.
### E2 — Faune et flore
Espèces générées par biome (producteurs, herbivores, prédateurs), chaîne trophique simulée par populations (pas d'individus), usages (nourriture, médicaments, matériaux, chasse, domestication), espèces envahissantes, quarantaine biologique.
### E3 — Biologie : organes, cybernétique, mutations
Organes (cœur, poumons, foie, reins, cerveau, yeux, membres) avec santé propre ; la santé globale devient dérivée. Maladies, radiations, blessures et manque d'air touchent des organes précis ; greffes, clonage, prothèses. Cybernétique : implants par emplacement, coût énergétique, rejet, piratage, maintenance. Mutations : mutagènes (radiation, anomalies, virus), traits hérités/acquis, instabilité. Étend `crew.rs` et le système de traits.
### E4 — Dialogues et civilisations
Arbres de dialogue data-driven (conditions : réputation, traits, compétences, rumeurs ; effets), mémoire des relations par personnage. Civilisations indigènes : culture, langue (noms générés), technologie, diplomatie, premier contact, non-ingérence. Histoire procédurale (guerres, empires, ruines, artefacts cohérents avec la graine).
### E5 — Vue roguelike à la Dwarf Fortress (exploration à pied)
Vue en quadrillage de caractères (déjà compatible avec la grille texte 8x10 du moteur) pour : intérieur du vaisseau (plan généré depuis les modules installés), stations, villes, ruines, épaves, grottes et surfaces planétaires. Tuiles par `.ron` (sol, mur, porte, sas, machine, végétation...), cartes locales générées par hachage (coordonnée + graine), déplacement au tour par tour ou en temps réel pausable, équipe contrôlable, objets et inventaire, lignes de vue, atmosphère/pression/température par tuile, combat à pied, interactions avec les PNJ et les dialogues (E4), fouille et archéologie. Les organes (E3) y deviennent visibles (blessures localisées).
### E6 — Autres idées retenues
Épidémies à l'échelle station/planète (propagation par le trafic PNJ, quarantaines, vaccins) ; écologie et terraformation à long terme ; religions et idéologies internes aux colonies (schismes, révolutions) ; marché noir plus profond (biologie, cybernétique, organes, mercenaires) ; piratage et guerre électronique ; journal de bord de l'IA généré depuis les événements réels.

## Principes à conserver
Données `.ron` validées au chargement ; génération déterministe par hachage ; sim/rendu/UI séparés ; chaque règle testée ; tout nouveau système a une entrée de codex générée depuis les données ; pas de dépendances nouvelles sans raison ; saves = graine + deltas.

## Idées en attente (demandées par l'utilisateur)
- Rendus wireframe pour les objets (modules, ressources, cargaisons, équipements) : une icône ou un modèle en fil de fer par objet dans l'inventaire, le codex, le chantier naval et les boutiques (demandé le 2026-10-11, à planifier après E1 ; à relier à la vue roguelike E5 pour les objets au sol).
- Couleurs par type de corps et barre des commandes permanente : faits (voir PROGRESS).
- E5, à ajouter (demandé le 2026-10-11) : véhicules d'exploration terrestre (rover, quad, aéroglisseur, drone, navette de surface : capacité, autonomie, blindage, énergie, soute, modules), préparation d'expédition (choix de l'équipage et du véhicule avant de descendre, équipement et provisions emportés, rôles à bord), retour et récupération des trouvailles. À relier à l'équipage (compétences, santé par organe d'E3) et aux cartes de surface d'E1 (biomes, météo, catastrophes).

- Bourse (en cours, 2026-10-11) : événements du monde et courbes de prix. Idées à étudier ensuite : contrats à terme et options, information retardée par la distance, arbitrage assisté par la flotte, actions de colonies et de factions, spéculation et réactions des PNJ, rumeurs vraies ou fausses, assurance et couverture, matières stratégiques contrôlées par un cartel, appels d'offres.
- Généalogie (demandée le 2026-10-11) : chaque entité vivante a des parents, des ancêtres, des frères et sœurs, des cousins. Équipage, colons et PNJ d'abord (arbre généré de façon déterministe à partir de la graine et de l'identité, avec liens affectifs et héritage des traits, à relier à E3 hérédité et mutations, E4 mémoire des relations et histoire, E5 rencontres de parents), puis espèces domestiquées. Écran d'arbre généalogique, parentés visibles dans les relations et les événements d'équipage.
- Génétique (demandée le 2026-10-11, avec la généalogie) : génome par individu (gènes à allèles dominants et récessifs, traits physiques, prédispositions aux maladies, aptitudes), transmission aux enfants par recombinaison et mutations, ressemblance familiale visible (traits, compétences de départ), consanguinité et maladies héréditaires ; s'appuie sur la généalogie et sur E3 (organes, mutations, clonage, greffes) ; vaut aussi pour les espèces d'E2 (sélection des espèces domestiquées, élevage).
- Commerce d'organes (demandé le 2026-10-11) : tout se vend et tout s'achète. Marché noir (E3, E6) des organes, greffons, prothèses, clones et cybernétique ; offre et demande liées aux maladies, aux guerres et aux catastrophes (même mécanique d'événements que la bourse), prix selon la compatibilité génétique (voir généalogie et génétique), risques de rejet, contrebande et douanes, dilemmes moraux et effets sur le moral de l'équipage et la réputation de l'IA. Les prix passent par les événements du monde de data/world_events.ron.

## Pilier : mœurs, culture et moralité (demandé le 2026-10-11)
Chaque faction, espèce, empire, ville, race a ses propres mœurs, culture et moralité, et cela influence tout le reste, le commerce compris. Première couche faite (8 axes, profils de faction, étiquettes morales des marchandises, demande et interdits dans les marchés et la douane). Suite : cultures des colonies et des villes qui dérivent selon leurs événements et leurs politiques ; espèces et civilisations indigènes avec leurs valeurs (E4) ; la réputation de l'IA pesée selon les valeurs de chaque interlocuteur ; lois locales (organes, clonage, cybernétique, esclavage, drogues) ; religions et idéologies internes (E6).