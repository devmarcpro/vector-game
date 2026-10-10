# SIM_MODEL
Unites SI. Pas fixe 30 Hz (SIM_DT).

## Vol (M1)
- Masse = 20 000 kg a sec + carburant (<= 8 000 kg).
- Poussee principale 1.4e6 N (~50 m/s2 a vide), RCS 2.8e5 N par axe.
- Consommation : debit = F / (Isp * g0), Isp = 200 000 s (moteur a fusion avance ; delta-v ~660 km/s). Delta-v = Isp*g0*ln(m0/m1) (test unitaire).
- Rotation : vitesse angulaire max 1.5 rad/s, acceleration 2 rad/s2, integration par Rodrigues + re-orthonormalisation.
- Assistance : sans commande sur un axe, contre-poussee (limitee a la poussee dispo) vers vitesse nulle ; rotation amortie. OFF = newtonien pur.
- Collisions : spheres ; si vitesse normale > 5 m/s, degats = 0.5*m*v2 / 5e6 sur une coque de 1000 ; rebond 20 %.

## Monde (M2)
- Orbites kepleriennes sur rails (equation de Kepler par Newton), plan de reference XZ, positions/vitesses analytiques => acceleration du temps gratuite.
- Etoiles : classes O..M, masse tiree dans la plage de classe, L = f(M) par morceaux, R = M^0.8 (M<1) sinon M^0.57, T = 5772 (L/R^2)^0.25.
- Planetes : espacement geometrique x1.45-2.1, ligne des glaces 2.7 sqrt(L) UA, T_eq = 278.6 L^0.25 / sqrt(a) (1-albedo)^0.25, type deduit de T_eq et masse (12 types), lunes limitees a 0.4 rayon de Hill, stations en orbite circulaire.
- Gravite : somme GM/r^2 de tous les corps (stations negligeables), symplectique Euler ; pas adaptatif : min(0.02 sqrt(r^3/GM), 0.15 alt/v), borne [1/240 s, 120 s].
- Referentiel d'assistance : plus petit corps dont la sphere d'influence (60 R, station 20 km) contient le vaisseau ; hors zone l'assistance lineaire est inactive (rotation toujours amortie).
- Pilote auto : profil v = 0.85 sqrt(2 a d), a = min(0.5 a_max, dv^2/(16 d)) (consomme au plus la moitie du delta-v), attitude parfaite si dt > 0.1 s.

## Minage, soute, economie (M3)
- Asteroides en familles (5-8 groupes de 8 sur des orbites quasi identiques, ecart ~30 km) dans les ceintures ; classes C/S/M/I, composition en fractions massiques (data/worldgen/asteroids.ron).
- Laser : portee 2 km (depuis la surface), cone 3.4 deg + taille apparente, 25 kg/s ; la masse extraite est retiree de l'asteroide (rayon ~ cbrt de la masse restante) et le minerai entre dans la soute (scoop magnetique). Conservation de la masse testee.
- Soute : 30 t et 40 m3 (la masse ET le volume limitent ; volume = kg / densite).
- Marche : prix = valeur_base * modificateur_station * (stock_cible / stock)^0.35 borne [0.25, 4], spread 10 % (achat +5 %, vente -5 %), glissement de prix integre par morceaux ; le stock relaxe vers sa cible (tau 3 jours, rattrapage analytique).
- Amarrage < 500 m et < 8 m/s ; le vaisseau suit la station. Carburant, reparations (2 cr/pt) a la station.
- Sauvegarde : magic VECT + version u32 + postcard ; seed du systeme + deltas (temps, vaisseau, credits, masse minee, marches, cible).

## Vaisseau modulaire (M4)
- Un vaisseau = coque (data/hulls.ron : masse, PV, RCS, vitesse/acceleration angulaires, emplacements typés Noyau/Interne/Externe/Armement de taille 1-4) + modules (data/modules.ron, 70 modules générés par tools/gen_content.py en familles Mk1-Mk4).
- Statistiques dérivées (ShipStats) : masse, poussée, Isp (pondérée par la poussée), capacité de carburant, soute (masse ET volume), batterie, radiateurs, mining_rate... Elles dépendent de la santé de chaque module.
- Bus d'énergie (par pas) : offre = réacteurs (x santé) + solaire (x flux L/d2) ; demande = modules consommateurs x charge (moteur = manette, laser = tir, gyro/RCS = usage). Déficit -> batterie ; batterie vide -> délestage par priorité (3 puis 2 puis 1 ; priorité 0 = support de vie, jamais délesté tant qu'il reste de l'énergie).
- Chaleur : chaleur = 0.6 x sortie réacteur + toute la puissance consommée ; rayonnement = P_rated x (T^4-T0^4)/(Tref^4-T0^4), Tref 350 K, T0 150 K ; capacité thermique 0.5 kJ/kg/K ; intégration en sous-pas de 5 s. Au-dessus de 650 K les modules se dégradent ; au-dessus de 900 K les réacteurs sont coupés (SCRAM).
- Dégâts : un impact (> 5 m/s) retire des PV de coque et abîme un module (et un voisin à 20 %) ; la santé réduit les performances, 0 = module perdu.
- Chantier naval (amarré) : acheter/vendre des modules (revente 50 %), installer/retirer (stock), changer de coque (reprise 50 %, soute vide requise). Réparation : 2 cr/PV de coque + 30 % du prix par point de santé de module.
- Assistance linéaire désactivée sous pilote auto et au-dessus de 500 m/s relatifs ; le pilote auto compense la gravité (feed-forward) et arrive à < 100 m de l'altitude d'arrêt avec < 15 m/s.

## Equipage (M5)
- Chaque personne : identité générée (culture -> syllabes), âge, 12 compétences (0-10), 1-3 traits (32 traits en données, effets: morale_base, social_need, temper, stress_resist, work_speed, health_base, appetite, sleep_need, luck), besoins (fatigue, faim, soif, social, stress ; 0 = satisfait, 1 = critique), morale, loyauté, santé, maladie en cours, relations (-100..100), poste, quart (3 x 8 h), salaire.
- Couche équipage au pas de 10 min (1 h si le retard dépasse 2 jours, 1 jour au-delà de 60 jours ; rattrapage borné).
- Consommation : nourriture 0.6 kg/j et eau 3 kg/j par personne depuis la soute (l'eau est recyclée à 90 % si le support de vie est alimenté). Serres : kg de nourriture/jour ajoutés à la soute. Air : remonte si le support de vie alimenté couvre l'effectif, sinon chute de 20 %/h ; sous 40 % la santé baisse.
- Santé : faim > 0.9, soif > 0.8, air bas et maladie la réduisent ; régénération lente si tout va bien (bonus médecin de garde et infirmerie). Maladies (data/diseases.ron) : sévérité/jour, durée, contagion, létalité ; traitement par médicament consommé si un médecin est de garde.
- Moral : cible = 0.75 + traits - faim/soif - fatigue - social - stress - entassement (> couchettes) - impayés + travail. Loyauté dérive avec le moral ; salaires impayés = -8 % loyauté, -10 % moral par jour.
- Postes (data/roles.ron) : efficacité = (0.6 + 0.1 x compétence) x vitesse de travail x santé x (1 - 0.4 fatigue) de la meilleure personne de garde ; personne = 0.7. Effets : mineur -> débit du laser, pilote -> accélération angulaire (x0.8..1.3), ingénieur/mécanicien -> réparation des modules (1 % de santé/h x (0.5 + compétence/5) x (1 + atelier), 1 kg de pièces détachées par 1 %).
- Evénements émergents (data/events.ron, 30 aujourd'hui) : conditions et effets déclaratifs sur la personne (et un partenaire tiré au hasard), journal en français. Probabilité de base 1.4 %/h par personne (x température). Les morts affectent le moral des proches.
- Recrutement : 6 candidats par station, renouvelés tous les 30 jours, coût = 5 jours de salaire, plafonné par les couchettes (modules d'habitation). Paie quotidienne.

## Galaxie, FTL, découverte (M6)
- Galaxie : secteurs cubiques de 10 al ; densité moyenne d'étoiles par secteur = disque exponentiel (échelle 9000 al, hauteur 300 al, 4 par secteur au rayon solaire 26 000 al du centre) x 2 bras spiraux logarithmiques + bulbe ; tirage de Poisson par secteur depuis hash(graine, coord) ; galaxie vide au-delà de 60 000 al. Rien n'est stocké : un secteur se régénère à l'identique. Le secteur d'origine contient toujours au moins 3 étoiles.
- Une étoile = (secteur, indice) + graine de système ; `peek_star` donne classe/nom/luminosité sans générer le système (mêmes tirages d'en-tête que `generate_system`, vérifié par test). Génération complète d'un système : < 1 ms en pratique.
- FTL : module Propulseur FTL (Mk1-Mk4 : 4/8/15/30 al). Séquence : charge 20 s (consomme la puissance du module, délestable) -> transit figé 6 h de jeu par al (temps accéléré automatiquement x10 000) -> arrivée aux confins du système (2 x ligne des glaces, min 3e11 m), vitesse nulle. Coût propulseur : 1.5e-3 x masse x distance (kg/al). Refusé : amarré, hors portée, dans un champ de gravité (corps de référence), carburant insuffisant, propulseur absent/hors service. Risque d'incident (dégâts de module) : 1 % x distance x (1.6 - santé du propulseur).
- Persistance (LOD 2) : quitter un système en sauvegarde le delta (masse minée, marchés, niveaux de scan) ; revenir le restaure. Les orbites sont analytiques et les marchés se relaxent analytiquement, donc le « rattrapage » d'un système lointain est exact et gratuit.
- Découverte progressive : niveaux 0 inconnu, 1 détecté, 2 relevé (nom, type, rayon, gravité, température), 3 scanné (composition, atmosphère). Portée de détection = (3e7 m + portée du scanner alimenté) x sqrt(rayon/6000 km) borné [0.02, 5]. Niveau 2 : < 0.3 x portée pendant 10 s ; niveau 3 : < 0.08 x portée pendant 40 s (accéléré par un scientifique). Étoile et grands mondes sont détectés dès l'arrivée. Les corps non détectés sont invisibles et non ciblables.
- Carte galactique : seules les étoiles à moins de 15 al d'un système visité sont connues (brouillard de guerre).

## Economie vivante (M7)
- Factions (data/factions.ron, 8) : culture, idéologie, taxe sur les ventes, contrebande (ressources interdites), agressivité, alliés/ennemis. Territoires : cellules de 60 al avec un point germe par cellule (jitter hashé), propriétaire = faction du germe le plus proche (ou espace non revendiqué : 2 « cases » sur n+2). Test : contigus (> 90 % des paires à 1 al), variés (>= 5 factions sur un échantillon).
- Industries (data/industries.ron, 8) : chaque station en a une ; produit = stock d'équilibre x3 (prix x0.68), consommé = x0.15 (prix x1.94), le reste x1. Prix = valeur x modificateur de station x (stock de référence / stock)^0.35 borné [0.25, 4] ; écart de 10 % achat/vente. Les marchés relaxent vers l'équilibre (tau 3 j, rattrapage analytique) : les écarts de prix entre industries créent des routes commerciales rentables (test : fer au mine vs raffinerie < 0.5, nourriture mine vs ferme > 2).
- Réputation (-100..100 par faction) : prix du joueur x(1 + 0.002 rep) ; taxe prélevée sur les ventes ; +1 pour 20 000 cr échangés ; réputation <= -60 = amarrage refusé. Douane à l'amarrage et patrouilles (6 h de délai) : probabilité de fouille 0.4 + 0.5 x agressivité ; contrebande saisie + amende 2x la valeur + -8 réputation ; soute propre : +0.3.
- Contrats émergents (renouvelés chaque semaine de jeu) : pénuries de la station (stock < 50 % de référence, reward = 1.35 x prix x quantité) et exportations vers des stations voisines (<= 9 al) : quantité d'un produit de l'industrie locale, récompense = 1.8 x valeur + 800 cr/al. Max 5 actifs ; livraison = marchandise vers le marché de destination, +3 réputation ; expiration -2 ; abandon -1.
- Banque : prêt plafonné à 10 000 + 200 x Σ réputations positives ; intérêts 0.2 %/jour composés ; encours > 120 % du plafond pendant 30 jours = saisie de 50 % du prêt sur les crédits et -10 réputation.
- Trafic PNJ (LOD 0, déterministe par système/mois) : traders (chargent le surplus >130 % de référence, livrent à une autre station, ou partent hors système 3-10 j et reviennent avec ce qui manque), mineurs (astéroïde -> station), patrouilles (orbite 8 km, scannent à < 20 km), pirates (embuscade à un astéroïde, alerte si cargaison > 2000 cr à < 40 km). Mouvement analytique (interpolation lissée), donc sans coût à haute accélération. Les PNJ lisent et écrivent les mêmes marchés que le joueur ; un an de trafic garde les stocks bornés (test).
- Les systèmes génèrent 0 (40 %), 1 (35 %), 2 (18 %) ou 3 (7 %) stations.

## Colonies, production, flotte (M8)
- Fonder une colonie : vaisseau à < 2 rayons (min 20 000 m) d'une planète/lune/astéroïde, < 250 m/s relatifs ; kit en soute : 3 000 kg de matériaux de construction, 400 kg de colons (5 personnes), 300 kg de vivres, 600 kg d'eau ; permis 20 000 cr. Départ : dôme d'habitation, centrale solaire, astroport.
- Corps -> étiquettes de site (rocky, icy, asteroid, habitable, gas_giant) ; les bâtiments exigent l'une d'elles. Gisements : compositions d'astéroïdes (data), roches silicatées/ferreuses, glaces, géantes (gaz : H2, He, D, He-3), modulées par corps (x0.5..1.5).
- Production (data/recettes) : `extract` (extraction proportionnelle aux gisements : minerai, volatils, gaz, régolithe), `process`/`grow` (entrées -> sorties, conservation de la masse imposée par le validateur), `data` (recherche). Un bâtiment exécute la recette choisie à `throughput` lots/jour x productivité, limité par les stocks d'entrée et la place restante. Productivité = effectifs (travailleurs 55 % de la pop / emplois) x énergie (offre/demande) x (0.35 + 0.9 moral) x politiques, /2 en émeute, x0.3 si la caisse est négative.
- Besoins : 0.6 kg de nourriture, 3 kg d'eau (80-95 % recyclés ; centrale de support de vie), 0.04 kg de biens de consommation par personne/jour. Moral : cible = 0.10 + 0.30 nourriture + 0.20 eau + 0.08 biens + 0.12 énergie + services (médical, loisirs, éducation, sécurité) + politiques - surpopulation. Population : naissances (si logements libres), mortalité (base + famine/soif/soins), immigration (politique), départs si moral < 0.25. Agitation si moral < 0.25 ; > 0.7 : émeutes (dégâts, productivité /2).
- Caisse : impôts (0/2/5/9 cr par habitant et par jour x moral) - entretien des bâtiments. Terraformage lent (centrale dédiée). Recherche accumulée (laboratoires) pour l'arbre technologique (M9).
- Pas de simulation : 6 h, puis 1 jour (> 5 j de retard), puis 5 jours (> 200 j) ; toutes les colonies avancent, où que soit le joueur (LOD 1 agrégé).
- Flotte : cargos achetés 70 % du prix de la coque, capacité = masse x 2 (cargo/colonial/support), x1.2 (mineur), x0.5 sinon ; entretien 0.04 %/jour. Routes entre colonies et stations (y compris d'autres systèmes visités) : chargement 6 h (achat au marché ou prélèvement en colonie), trajet à 120 km/s dans le système ou 6 h/al + 12 h par saut, déchargement 4 h (vente au marché moins taxe de faction, ou stock de la colonie), retour à vide. Les marchés distants sont estimés à leur équilibre (identique au marché réel d'une station jamais touchée, testé).
- Astroports : le vaisseau s'amarre à < 100 km de la surface (< 40 m/s) ; transfert de soute <-> stocks, caisse, construction depuis l'écran colonie.
