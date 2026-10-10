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
