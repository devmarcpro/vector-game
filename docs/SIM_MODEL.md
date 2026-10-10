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
