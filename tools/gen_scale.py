#!/usr/bin/env python3
"""Content scale-up: writes data/events_extra.ron, data/recipes_extra.ron, data/text/extra.ron and
patches the recipe lists of data/buildings.ron (idempotent).  Run: python tools/gen_scale.py"""
import os
import re

D = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")


def q(s):
    return '"' + s.replace('"', "'") + '"'


def fl(x):
    return f"{float(x):.2f}".rstrip("0").rstrip(".") if abs(x) < 1000 else f"{float(x):.1f}"


# ------------------------------------------------------------------ events
EV = []


def ev(id, kind, weight, other, cond, effects, text):
    cs = ",".join(f'("{f}","{o}",{float(v)})' for f, o, v in cond)
    es = ",".join(f'("{f}",{float(v)})' for f, v in effects)
    EV.append(
        f' (id:"{id}", kind:"{kind}", weight:{weight}, needs_other:{str(other).lower()}, '
        f"cond:[{cs}], effects:[{es}], text:{q(text)}),"
    )


SKILLS = {
    "pilotage": ("un virage serre autour d'un debris", "{a} a fait un atterrissage parfait en simulation", "{a} perd le controle quelques secondes dans un simulateur de vol"),
    "ingenierie": ("une surtension dans le bus d'alimentation", "{a} optimise le routage de l'energie et gagne quelques kilowatts", "{a} court-circuite un panneau en testant une idee audacieuse"),
    "minage": ("une veine de minerai mal cartographiee", "{a} lit les echos sismiques comme un livre ouvert", "{a} coince un foret dans une faille instable"),
    "medecine": ("un diagnostic difficile", "{a} soigne un cas delicat avec une improvisation brillante", "{a} se trompe de dosage et doit tout reprendre"),
    "science": ("une mesure aberrante", "{a} repere une regularite dans les donnees du scanner", "{a} contamine un echantillon par negligence"),
    "combat": ("un exercice de tir", "{a} aligne une serie de tirs parfaits a l'entrainement", "{a} laisse partir un coup involontaire dans la salle d'armes"),
    "commerce": ("une negociation tendue", "{a} arrache un excellent prix a un marchand pressé", "{a} se fait rouler par un courtier peu scrupuleux"),
    "commandement": ("un moment de doute", "{a} rassemble l'equipage par un discours juste", "{a} donne un ordre confus qui fait perdre du temps"),
    "cuisine": ("des rations monotones", "{a} transforme les rations en un vrai repas", "{a} rate un plat et la coursive sent le brule pendant des heures"),
    "botanique": ("une serre capricieuse", "{a} obtient une recolte surprise dans les bacs de culture", "{a} noie un rang de plants par erreur"),
    "reparation": ("une fuite dans un conduit", "{a} bricole une rustine qui tient mieux que l'original", "{a} serre un raccord trop fort et le casse"),
    "navigation": ("un calcul de trajectoire", "{a} trouve un raccourci orbital qui economise du carburant", "{a} se trompe d'un chiffre dans un releve de position"),
}
for sk, (ctx, win, fail) in SKILLS.items():
    ev(f"{sk}_exploit", "work", 0.30, False, [(f"skill:{sk}", ">", 3.5), ("morale", ">", 0.3)], [(f"skill:*", 0.08), ("morale", 0.05)], f"Face a {ctx}, {win[0].lower()+win[1:] if False else win}.")
    ev(f"{sk}_ratage", "work", 0.30, False, [(f"skill:{sk}", "<", 3.0), ("fatigue", ">", 0.4)], [("stress", 0.08), ("morale", -0.04)], f"{fail}.")
    ev(f"{sk}_leçon".replace("ç", "c"), "social", 0.30, True, [(f"skill:{sk}", ">", 4.0), ("relation", ">", 0.0)], [("other.skill:*", 0.09), ("relation", 4.0)], f"{{a}} explique a {{b}} ses methodes ({sk}) pendant une pause.")

TRAITS = {
    "optimiste": ("morale", 0.05, "{a} reste confiant malgre la routine : tout ira bien."),
    "pessimiste": ("stress", 0.06, "{a} annonce que tout cela finira mal, comme d'habitude."),
    "colerique": ("stress", 0.08, "{a} claque une porte de soute apres une contrariete minuscule."),
    "calme": ("stress", -0.10, "{a} reste imperturbable pendant que l'alarme sonne et rassure les autres."),
    "sociable": ("social", -0.30, "{a} fait le tour des postes pour discuter avec tout le monde."),
    "solitaire": ("social", 0.15, "{a} s'isole dans un coin de la soute et refuse toute compagnie."),
    "courageux": ("loyalty", 0.04, "{a} se porte volontaire pour la sortie la plus risquee."),
    "peureux": ("stress", 0.10, "{a} refuse de s'approcher du sas apres un bruit suspect."),
    "perfectionniste": ("fatigue", 0.12, "{a} refait trois fois le meme travail pour qu'il soit parfait."),
    "paresseux": ("fatigue", -0.10, "{a} trouve un coin tranquille pour une sieste prolongee."),
    "travailleur": ("fatigue", 0.12, "{a} enchaine les heures supplementaires sans qu'on le lui demande."),
    "curieux": ("skill:*", 0.10, "{a} demonte un appareil pour comprendre comment il fonctionne."),
    "loyal": ("loyalty", 0.05, "{a} jure de rester a bord quoi qu'il arrive."),
    "opportuniste": ("credits", 120.0, "{a} revend discretement une pièce de rechange en surplus."),
    "gourmand": ("hunger", 0.15, "{a} est surpris en train de vider un placard de rations."),
    "frugal": ("hunger", -0.10, "{a} partage une partie de sa ration avec un camarade."),
    "insomniaque": ("fatigue", 0.15, "{a} arpente les coursives toute la nuit."),
    "costaud": ("health", 0.04, "{a} souleve seul un caisson qui en demandait deux."),
    "fragile": ("health", -0.08, "{a} attrape un coup de froid pres d'un sas mal isole."),
    "charismatique": ("morale", 0.06, "{a} electrise le carre par une histoire racontee a merveille."),
    "maladroit": ("health", -0.06, "{a} renverse un bac de visserie et glisse dessus."),
    "chanceux": ("credits", 200.0, "{a} trouve des credits oublies dans une poche de combinaison."),
    "malchanceux": ("morale", -0.06, "{a} perd son outil prefere dans une bouche d'evacuation."),
    "religieux": ("morale", 0.05, "{a} dit une priere silencieuse devant le hublot."),
    "rationnel": ("stress", -0.06, "{a} demontre calmement qu'il n'y avait pas de quoi s'inquieter."),
    "rancunier": ("relation", -8.0, "{a} ressasse une vieille dispute et refuse de saluer {b}."),
    "genereux": ("morale", 0.04, "{a} offre une part de son salaire pour aider un camarade."),
    "ambitieux": ("skill:*", 0.12, "{a} etudie le manuel de poste de son superieur."),
    "nostalgique": ("morale", -0.04, "{a} reecoute un vieil enregistrement de sa planete natale."),
    "aventureux": ("morale", 0.05, "{a} propose d'explorer l'epave voisine pour le plaisir."),
    "meticuleux": ("skill:*", 0.06, "{a} tient un carnet d'entretien d'une precision maniaque."),
    "bavard": ("social", -0.25, "{a} raconte la meme anecdote pour la quatrieme fois."),
}
for t, (field, val, text) in TRAITS.items():
    other = "{b}" in text
    cond = [(f"trait:{t}", ">", 0.5)]
    if other:
        cond.append(("relation", "<", 20.0))
    eff = [(field, val)]
    ev(f"trait_{t}", "trait", 0.35, other, cond, eff, text)

DIS = [
    ("rhume", "{a} eternue sans arret et contamine le carre."),
    ("gastro", "{a} passe la nuit aux toilettes apres un repas douteux."),
    ("pneumonie", "{a} tousse de plus en plus fort ; l'infirmerie s'inquiete."),
    ("bronchite", "{a} s'essouffle au moindre effort dans la poussiere des soutes."),
    ("entorse", "{a} se tord la cheville en descendant une echelle."),
    ("commotion", "{a} se cogne la tete contre une cloison en gravite nulle."),
    ("brulure", "{a} se brule la main sur un conduit de refroidissement."),
    ("anxiete", "{a} fait une crise d'angoisse dans la coursive."),
    ("allergie", "{a} developpe une allergie a la poussiere des filtres."),
    ("insomnie_chronique", "{a} ne dort plus depuis des jours ; les cernes se creusent."),
    ("burn_out", "{a} s'effondre apres des semaines sans repos."),
    ("mal_de_l_espace", "{a} souffre du mal de l'espace apres un changement de reference."),
]
for d, text in DIS:
    ev(f"mal_{d}", "health", 0.25, False, [("health", "<", 0.97)], [(f"disease:{d}", 1.0)], text)

HAND = [
    ("blague_ratee", "social", 0.6, True, [("morale", ">", 0.3)], [("relation", 3.0), ("morale", 0.02)], "{a} fait une blague que {b} ne comprend qu'une heure plus tard."),
    ("partie_echecs", "social", 0.6, True, [("fatigue", "<", 0.7)], [("relation", 5.0), ("stress", -0.08)], "{a} et {b} disputent une interminable partie d'echecs."),
    ("film_commun", "social", 0.6, True, [("morale", ">", 0.2)], [("relation", 4.0), ("social", -0.2)], "{a} et {b} regardent un vieux film dans le carre."),
    ("cuisine_a_deux", "social", 0.5, True, [("hunger", "<", 0.7)], [("relation", 6.0), ("hunger", -0.1)], "{a} et {b} preparent un festin avec presque rien."),
    ("pari_stupide", "social", 0.4, True, [("morale", ">", 0.3)], [("relation", 4.0), ("credits", -20.0)], "{a} perd un pari stupide contre {b} et paie sa dette."),
    ("secret_confie", "social", 0.4, True, [("relation", ">", 30.0)], [("relation", 10.0), ("stress", -0.1)], "{a} confie un secret a {b}, qui promet de se taire."),
    ("sport_en_gravite", "morale", 0.5, False, [("health", ">", 0.6)], [("health", 0.03), ("fatigue", 0.08), ("morale", 0.04)], "{a} s'entraine dans le tambour rotatif jusqu'a l'epuisement."),
    ("lettre_a_la_famille", "morale", 0.5, False, [("morale", "<", 0.7)], [("morale", 0.05), ("social", -0.1)], "{a} enregistre un long message pour sa famille."),
    ("nuit_blanche_conversation", "social", 0.4, True, [("stress", ">", 0.2)], [("relation", 7.0), ("fatigue", 0.1)], "{a} et {b} refont le monde jusqu'au petit matin."),
    ("vol_de_ration", "conflict", 0.4, True, [("hunger", ">", 0.5), ("relation", "<", 40.0)], [("relation", -14.0), ("hunger", -0.15)], "{a} vole une ration a {b} dans le placard commun."),
    ("emprunt_non_rendu", "conflict", 0.4, True, [("relation", "<", 40.0)], [("relation", -8.0)], "{b} reclame un outil que {a} n'a jamais rendu."),
    ("bruit_de_nuit", "conflict", 0.5, True, [("fatigue", ">", 0.5)], [("relation", -9.0), ("other.fatigue", 0.1)], "{a} fait du bruit toute la nuit ; {b} ne dort pas."),
    ("accusation_de_sabotage", "conflict", 0.2, True, [("relation", "<", -10.0), ("stress", ">", 0.4)], [("relation", -20.0), ("other.stress", 0.15)], "{a} accuse {b} d'avoir sabote un module."),
    ("reconciliation", "social", 0.5, True, [("relation", "<", 0.0)], [("relation", 18.0), ("stress", -0.1), ("other.stress", -0.1)], "{a} et {b} enterrent la hache de guerre autour d'un cafe."),
    ("vol_de_credits", "crisis", 0.1, False, [("loyalty", "<", 0.3), ("morale", "<", 0.4)], [("credits", -150.0), ("loyalty", -0.1)], "{a} puise dans la caisse commune et le regrette aussitot."),
    ("greve_du_zele", "crisis", 0.2, False, [("loyalty", "<", 0.35)], [("morale", -0.1), ("fatigue", -0.2)], "{a} applique le reglement a la lettre et ralentit tout."),
    ("petition", "crisis", 0.2, True, [("loyalty", "<", 0.4), ("morale", "<", 0.45)], [("relation", 10.0), ("other.loyalty", -0.06)], "{a} fait signer a {b} une pétition contre l'IA de bord."),
    ("crise_de_nerfs", "crisis", 0.3, False, [("stress", ">", 0.85)], [("stress", -0.4), ("morale", -0.15), ("health", -0.03)], "{a} hurle sa colere dans le sas avant de s'effondrer."),
    ("volontaire_heroique", "good", 0.2, False, [("trait:courageux", ">", 0.5), ("health", ">", 0.5)], [("loyalty", 0.08), ("morale", 0.06), ("health", -0.05)], "{a} sort reparer une antenne en pleine tempete de poussiere."),
    ("belle_trouvaille_epave", "good", 0.08, False, [("morale", ">", 0.35)], [("credits", 220.0)], "{a} recupere des composants intacts dans une carcasse en derive."),
    ("remise_prime", "good", 0.1, False, [("loyalty", ">", 0.6)], [("credits", 80.0), ("morale", 0.08)], "{a} decouvre une ancienne prime de fidelite non reclamee."),
    ("signal_mysterieux", "good", 0.1, False, [("skill:science", ">", 3.0)], [("skill:*", 0.1), ("morale", 0.04)], "{a} capte un signal etrange et passe la nuit a le decoder."),
    ("lever_de_soleil", "morale", 0.5, False, [("morale", "<", 0.7)], [("morale", 0.05), ("stress", -0.05)], "{a} regarde l'etoile se lever derriere une planete et sourit."),
    ("minute_de_silence", "morale", 0.2, False, [("loyalty", ">", 0.4)], [("loyalty", 0.04), ("morale", -0.02)], "{a} observe une minute de silence pour les disparus de l'espace."),
    ("anniversaire_oublie", "morale", 0.4, True, [("morale", ">", 0.2)], [("morale", -0.03), ("other.morale", -0.01), ("relation", -4.0)], "{b} oublie l'anniversaire de {a}."),
    ("atelier_improvise", "work", 0.4, False, [("skill:ingenierie", ">", 2.0)], [("skill:*", 0.07), ("fatigue", 0.06)], "{a} monte un petit atelier d'usinage dans un coin de la soute."),
    ("inventaire_precis", "work", 0.4, False, [("trait:meticuleux", ">", 0.5)], [("skill:*", 0.04), ("morale", 0.02)], "{a} fait l'inventaire complet des pieces detachees."),
    ("formation_urgence", "work", 0.4, True, [("skill:commandement", ">", 3.0)], [("other.skill:*", 0.10), ("stress", 0.04)], "{a} organise un exercice d'urgence ; {b} apprend les bons gestes."),
    ("fausse_alerte", "work", 0.5, False, [("stress", ">", 0.3)], [("stress", 0.07), ("fatigue", 0.05)], "{a} declenche une fausse alerte de depressurisation."),
    ("capteur_defectueux", "work", 0.5, False, [("skill:reparation", ">", 1.0)], [("skill:*", 0.05), ("fatigue", 0.04)], "{a} passe la journee a traquer un capteur defectueux."),
    ("nuit_de_garde", "work", 0.5, False, [("fatigue", ">", 0.3)], [("fatigue", 0.15), ("loyalty", 0.02)], "{a} assure seul une longue veille pendant que les autres dorment."),
    ("recette_secrete", "good", 0.2, False, [("skill:cuisine", ">", 3.0)], [("morale", 0.08), ("hunger", -0.1)], "{a} sort une recette familiale qui fait pleurer tout le monde de joie."),
    ("potager_prospere", "good", 0.2, False, [("skill:botanique", ">", 3.0)], [("morale", 0.06), ("hunger", -0.08)], "{a} offre a l'equipage des tomates cultivees a bord."),
    ("mal_de_dos", "health", 0.4, False, [("fatigue", ">", 0.5)], [("health", -0.04), ("morale", -0.03)], "{a} se plaint d'un mal de dos tenace."),
    ("migraine", "health", 0.4, False, [("stress", ">", 0.4)], [("health", -0.03), ("fatigue", 0.08)], "{a} souffre d'une migraine epouvantable."),
    ("cauchemar", "health", 0.4, False, [("stress", ">", 0.5)], [("fatigue", 0.1), ("stress", 0.05)], "{a} se reveille en hurlant au milieu de la nuit."),
    ("bonne_nuit", "health", 0.5, False, [("fatigue", ">", 0.4)], [("fatigue", -0.3), ("morale", 0.03)], "{a} dort enfin douze heures d'affilee."),
    ("visite_medicale", "health", 0.4, False, [("health", "<", 0.9)], [("health", 0.08), ("stress", -0.04)], "{a} passe a l'infirmerie et repart rassure."),
    ("jeune_volontaire", "social", 0.3, True, [("relation", ">", 5.0)], [("other.skill:*", 0.06), ("relation", 3.0)], "{a} fait visiter la salle des machines a {b}."),
]
for h in HAND:
    ev(*h)


ev("conversation_technique", "work", 0.4, True, [("skill:ingenierie", ">", 2.5), ("relation", ">", 0.0)], [("skill:*", 0.05), ("other.skill:*", 0.05)], "{a} et {b} comparent leurs bricolages et en tirent des idees neuves.")
ev("regard_vers_les_etoiles", "morale", 0.5, True, [("morale", ">", 0.3), ("relation", ">", 15.0)], [("morale", 0.04), ("other.morale", 0.04), ("relation", 4.0)], "{a} et {b} contemplent ensemble la voie lactee depuis le poste d'observation.")
ev("repas_de_fete", "good", 0.3, False, [("morale", ">", 0.5), ("hunger", "<", 0.4)], [("morale", 0.07), ("loyalty", 0.03)], "{a} dresse la table pour un repas de fete improvise.")
ev("histoire_de_fantomes", "social", 0.4, True, [("trait:bavard", ">", 0.5)], [("relation", 3.0), ("other.stress", 0.05)], "{a} raconte une histoire de vaisseau fantome a {b}, qui regrette de l'avoir ecoutee.")
ev("pannes_en_cascade", "work", 0.3, False, [("skill:reparation", ">", 2.0), ("stress", ">", 0.2)], [("skill:*", 0.08), ("fatigue", 0.12)], "{a} enchaine les reparations pendant que tout semble tomber en panne.")
ev("curiosite_scientifique", "work", 0.3, False, [("skill:science", ">", 2.0)], [("skill:*", 0.07), ("morale", 0.03)], "{a} monte une petite experience sur les echantillons recoltes.")

with open(os.path.join(D, "events_extra.ron"), "w", encoding="utf8") as f:
    f.write("// GENERATED by tools/gen_scale.py - more emergent crew events (same format as events.ron).\n[\n")
    f.write("\n".join(EV))
    f.write("\n]\n")
print("events", len(EV))

# ------------------------------------------------------------------ recipes
R = []


def rec(id, name, kind, ins, outs, hours, kwh):
    fi = ", ".join(f'("{r}",{float(k)})' for r, k in ins)
    fo = ", ".join(f'("{r}",{float(k)})' for r, k in outs)
    R.append(
        f' (id:"{id}", name:{q(name)}, kind:"{kind}", inputs:[{fi}], outputs:[{fo}], hours:{float(hours)}, energy_kwh:{float(kwh)}),'
    )


# building id -> recipe ids added to it
BMAP = {}


def add(b, *ids):
    BMAP.setdefault(b, []).extend(ids)


# Metallurgy
M = [
    ("refine_nickel_steel", "Acier au nickel", [("iron", 850), ("nickel", 120)], [("steel", 940)], 24, 3200),
    ("refine_chrome_steel", "Acier inoxydable", [("iron", 800), ("chromium", 150), ("nickel", 40)], [("steel", 960)], 24, 3600),
    ("alloy_tungsten", "Alliage de tungstene", [("tungsten", 40), ("nickel", 80), ("iron", 580)], [("steel", 700)], 24, 3000),
    ("alloy_magnesium", "Alliage de magnesium", [("magnesium", 800), ("aluminium", 100), ("zinc", 30)], [("aluminium_alloy", 900)], 24, 2200),
    ("alloy_cobalt", "Superalliage de cobalt", [("cobalt", 500), ("chromium", 200), ("tungsten", 60)], [("titanium_alloy", 740)], 24, 5200),
    ("galvanize_steel", "Galvanisation", [("steel", 500), ("zinc", 40)], [("steel", 530)], 12, 700),
    ("cast_hull_plates_light", "Plaques de coque legeres", [("aluminium_alloy", 700), ("ceramics", 100)], [("hull_plates", 780)], 24, 3000),
    ("sinter_ceramic_armor", "Blindage ceramique", [("ceramics", 600), ("titanium_alloy", 250)], [("hull_plates", 820)], 24, 4200),
    ("refine_copper", "Raffinage du cuivre", [("copper", 1000)], [("copper_wire", 980)], 12, 900),
    ("refine_silicon", "Silicium pur", [("silica", 1000), ("carbon", 100)], [("silicon", 480)], 24, 5200),
    ("refine_graphite", "Graphite", [("carbon", 500)], [("graphite", 480)], 12, 900),
    ("synth_diamond", "Diamant synthetique", [("graphite", 400), ("nickel", 20)], [("diamond", 380)], 48, 9000),
    ("smelt_gold", "Affinage de l'or", [("gold", 400)], [("gold", 392)], 12, 500),
    ("smelt_platinum", "Affinage du platine", [("platinum", 300), ("palladium", 40)], [("platinum", 330)], 12, 800),
    ("recycle_circuits", "Recyclage de circuits", [("circuits", 100)], [("silicon", 40), ("copper", 20), ("gold", 1)], 12, 300),
    ("recycle_batteries", "Recyclage de batteries", [("batteries", 100)], [("lithium", 20), ("cobalt", 10), ("aluminium", 30)], 12, 400),
    ("recycle_hull_plates", "Recyclage de plaques", [("hull_plates", 500)], [("titanium_alloy", 250), ("steel", 150)], 12, 800),
    ("recycle_electronics", "Recyclage d'electronique", [("electronics", 100)], [("circuits", 40), ("polymers", 20)], 12, 300),
]
for id, n, i, o, h, e in M:
    rec(id, n, "process", i, o, h, e)
add("smelter", "refine_nickel_steel", "refine_chrome_steel", "alloy_tungsten", "alloy_magnesium", "alloy_cobalt", "galvanize_steel", "refine_copper")
add("component_factory", "cast_hull_plates_light", "sinter_ceramic_armor")
add("glassworks", "refine_silicon")
add("chemical_plant", "refine_graphite", "synth_diamond")
add("recycler", "recycle_circuits", "recycle_batteries", "recycle_hull_plates", "recycle_electronics", "smelt_gold", "smelt_platinum")

# Chemistry and fuel
C = [
    ("make_methane", "Methanisation", [("hydrogen", 100), ("carbon", 300)], [("methane", 380)], 24, 600),
    ("make_ammonia", "Synthese de l'ammoniac", [("nitrogen", 800), ("hydrogen", 170)], [("ammonia", 960)], 24, 2400),
    ("refine_helium3", "Raffinage d'helium 3", [("hydrogen", 1500), ("helium", 600)], [("helium3", 1.5), ("helium", 590), ("hydrogen", 1400)], 24, 4000),
    ("distill_deuterium", "Distillation du deuterium", [("water", 3000)], [("deuterium", 1.3), ("hydrogen", 330)], 24, 900),
    ("crack_co2", "Craquage du CO2", [("co2_ice", 1000)], [("oxygen", 700), ("carbon", 270)], 12, 1800),
    ("sabatier", "Reaction de Sabatier", [("co2_ice", 400), ("hydrogen", 40)], [("methane", 150), ("water", 250)], 12, 900),
    ("make_pharma", "Chimie pharmaceutique", [("organics", 300), ("salts", 50), ("water", 100)], [("medicine", 160)], 24, 1500),
    ("make_synthetic_food", "Alimentation synthetique", [("organics", 400), ("water", 200), ("nitrogen", 20)], [("food", 450)], 24, 1600),
    ("brew_biofuel", "Biocarburant", [("organics", 400), ("water", 100)], [("fuel", 480)], 24, 300),
    ("make_fuel_rods_2", "Barres de combustible", [("uranium", 100), ("steel", 60), ("ceramics", 30)], [("fuel_rods", 180)], 48, 3000),
    ("enrich_thorium", "Cycle du thorium", [("thorium", 200), ("steel", 40)], [("fuel_rods", 220)], 48, 2800),
    ("process_salts", "Traitement des sels", [("salts", 800)], [("magnesium", 120), ("sulfur", 60), ("phosphorus", 20)], 24, 1700),
    ("refine_sulfur", "Soufre industriel", [("regolith", 2000)], [("sulfur", 80), ("silica", 600)], 24, 500),
    ("make_graphite_fiber", "Fibres de carbone", [("carbon", 300), ("polymers", 100)], [("polymers", 380)], 24, 2500),
    ("make_nanomaterials_2", "Nanomateriaux", [("carbon", 200), ("silicon", 60), ("rare_earths", 20)], [("nanomaterials", 270)], 96, 8000),
    ("extract_rare_earths", "Separation des terres rares", [("regolith", 4000)], [("rare_earths", 120), ("iron", 1500), ("silica", 1200)], 24, 6000),
]
for id, n, i, o, h, e in C:
    rec(id, n, "process", i, o, h, e)
add("chemical_plant", "make_methane", "make_ammonia", "make_pharma", "make_synthetic_food", "brew_biofuel", "process_salts", "refine_sulfur", "make_graphite_fiber", "make_nanomaterials_2", "sabatier")
add("water_plant", "distill_deuterium", "sabatier")
add("gas_scoop", "refine_helium3", "crack_co2")
add("component_factory", "make_fuel_rods_2", "enrich_thorium")
add("deep_drill", "extract_rare_earths")

# Manufacturing
F = [
    ("make_robotics", "Robotique", [("electronics", 100), ("machinery", 80), ("batteries", 20)], [("robotics", 190)], 24, 2400),
    ("make_missiles_2", "Missiles", [("engine_parts", 40), ("electronics", 20), ("steel", 40)], [("missiles", 100)], 12, 900),
    ("make_mines_2", "Mines spatiales", [("steel", 60), ("electronics", 20), ("weapon_parts", 20)], [("mines", 95)], 12, 700),
    ("make_ammo", "Munitions cinetiques", [("steel", 500), ("copper", 40)], [("ammo_slugs", 520)], 12, 600),
    ("make_weapon_parts_2", "Pieces d'armement", [("steel", 300), ("titanium_alloy", 100), ("electronics", 30)], [("weapon_parts", 410)], 24, 2200),
    ("make_spare_parts_2", "Pieces detachees", [("steel", 200), ("plastics", 40), ("copper_wire", 20)], [("spare_parts", 250)], 12, 600),
    ("weave_synthetic", "Textiles synthetiques", [("polymers", 200), ("organics", 40)], [("textiles", 230)], 12, 500),
    ("assemble_luxury", "Produits de luxe", [("gold", 5), ("textiles", 60), ("glass", 40), ("electronics", 20)], [("luxury_goods", 120)], 24, 1200),
    ("assemble_consumer", "Biens de consommation", [("plastics", 150), ("electronics", 20), ("textiles", 30)], [("consumer_goods", 195)], 12, 600),
    ("make_tools_ti", "Outils en titane", [("titanium_alloy", 150), ("tools", 20)], [("tools", 165)], 12, 500),
    ("make_batteries_adv", "Batteries avancees", [("lithium", 50), ("cobalt", 25), ("nanomaterials", 5), ("polymers", 20)], [("batteries", 95)], 24, 1600),
    ("make_solar_adv", "Cellules solaires avancees", [("silicon", 120), ("nanomaterials", 8), ("glass", 40)], [("solar_cells", 160)], 24, 2800),
    ("make_circuits_adv", "Circuits avances", [("silicon", 90), ("gold", 1), ("nanomaterials", 4), ("copper_wire", 30)], [("circuits", 120)], 24, 2200),
    ("build_reactor_parts_adv", "Pieces de reacteur avancees", [("tungsten", 60), ("ceramics", 100), ("nanomaterials", 10), ("electronics", 60)], [("reactor_parts", 220)], 24, 3800),
    ("build_engine_adv", "Pieces de moteur avancees", [("tungsten", 80), ("titanium_alloy", 200), ("robotics", 20)], [("engine_parts", 290)], 24, 3600),
    ("press_hull_adv", "Coque composite", [("titanium_alloy", 400), ("carbon", 100), ("nanomaterials", 10)], [("hull_plates", 500)], 24, 4800),
    ("build_life_support_adv", "Support de vie renforce", [("robotics", 20), ("polymers", 60), ("electronics", 50)], [("life_support_parts", 120)], 24, 2000),
]
for id, n, i, o, h, e in F:
    rec(id, n, "process", i, o, h, e)
add("component_factory", "make_robotics", "make_missiles_2", "make_mines_2", "make_ammo", "make_weapon_parts_2", "make_spare_parts_2", "make_tools_ti", "build_reactor_parts_adv", "build_engine_adv", "press_hull_adv", "build_life_support_adv")
add("consumer_factory", "weave_synthetic", "assemble_luxury", "assemble_consumer")
add("electronics_fab", "make_batteries_adv", "make_solar_adv", "make_circuits_adv")

# Agriculture and science
G = [
    ("grow_wheat", "Culture de cereales", "grow", [("water", 900), ("fertilizer", 40), ("co2_ice", 40)], [("food", 380)], 48, 2200),
    ("grow_vegetables", "Maraichage sous serre", "grow", [("water", 600), ("fertilizer", 25), ("co2_ice", 30)], [("food", 260)], 24, 1200),
    ("grow_fruit_trees", "Verger sous dome", "grow", [("water", 700), ("fertilizer", 30), ("co2_ice", 30)], [("food", 280), ("organics", 40)], 48, 1800),
    ("grow_biomass", "Biomasse", "grow", [("water", 600), ("ammonia", 30), ("co2_ice", 50)], [("organics", 300)], 24, 1400),
    ("grow_fungi", "Culture de champignons", "grow", [("organics", 150), ("water", 100)], [("food", 220)], 24, 400),
    ("grow_cotton", "Fibres vegetales", "grow", [("water", 700), ("fertilizer", 30), ("co2_ice", 20)], [("textiles", 200)], 48, 1300),
    ("grow_medicinal", "Plantes medicinales", "grow", [("water", 400), ("fertilizer", 20), ("organics", 40)], [("medicine", 60)], 48, 1500),
    ("research_materials", "Recherche en materiaux", "data", [("electronics", 15), ("nanomaterials", 1)], [("research_data", 22)], 24, 3500),
    ("research_biology", "Recherche en biologie", "data", [("medicine", 10), ("organics", 10)], [("research_data", 18)], 24, 2500),
    ("research_astro", "Recherche en astrophysique", "data", [("electronics", 12)], [("research_data", 15)], 24, 2200),
    ("research_robotics", "Recherche en robotique", "data", [("robotics", 8), ("electronics", 10)], [("research_data", 24)], 24, 3200),
    ("research_energy", "Recherche energetique", "data", [("fuel_rods", 4), ("electronics", 10)], [("research_data", 26)], 24, 4200),
]
for id, n, k, i, o, h, e in G:
    rec(id, n, k, i, o, h, e)
add("hydroponics", "grow_wheat", "grow_vegetables", "grow_fruit_trees", "grow_fungi", "grow_cotton", "grow_medicinal")
add("algae_vats", "grow_biomass")
add("research_lab", "research_materials", "research_biology", "research_astro", "research_robotics", "research_energy")

with open(os.path.join(D, "recipes_extra.ron"), "w", encoding="utf8") as f:
    f.write("// GENERATED by tools/gen_scale.py - more production recipes (same format as recipes.ron).\n[\n")
    f.write("\n".join(R))
    f.write("\n]\n")
print("recipes", len(R))

# patch buildings.ron recipe lists
p = os.path.join(D, "buildings.ron")
s = open(p, encoding="utf8").read()
for b, ids in BMAP.items():
    m = re.search(r'\(id:"%s",.*?recipes:\[(.*?)\]' % re.escape(b), s)
    assert m, b
    cur = [x.strip().strip('"') for x in m.group(1).split(",") if x.strip()]
    for i in ids:
        if i not in cur:
            cur.append(i)
    new = ", ".join(f'"{x}"' for x in cur)
    s = s[: m.start(1)] + new + s[m.end(1) :]
open(p, "w", encoding="utf8").write(s)
print("buildings patched", len(BMAP))

# ------------------------------------------------------------------ text templates
T = {
    "hire": ["{a} signe son contrat et rejoint l'equipage comme {role}.", "Un nouveau {role} : {a} prend ses quartiers.", "{a} serre la main de chacun : le poste de {role} est pourvu.", "L'equipage accueille {a}, tout juste embauche comme {role}."],
    "dismiss": ["{a} fait ses adieux et part sans se retourner.", "{a} range ses affaires ; le poste est libre.", "On souhaite bonne chance a {a}, qui quitte le bord."],
    "death": ["{a} ne reviendra pas. On range ses affaires en silence.", "Le bord pleure {a}.", "{a} est mort ; l'IA de bord observe une minute de silence.", "Une ceremonie sobre est tenue pour {a}."],
    "sick": ["{a} est cloue au lit par {disease}.", "{disease} frappe {a} de plein fouet.", "L'infirmerie accueille {a} : {disease}."],
    "healed": ["{a} reprend le service, gueri de {disease}.", "Fin de {disease} pour {a} : tout le monde est soulage.", "{a} est enfin debarrasse de {disease}."],
    "hungry": ["Les estomacs gargouillent : plus rien dans les placards.", "Famine a bord : il faut trouver de la nourriture vite."],
    "thirsty": ["Les gourdes sont vides ; la soif devient un probleme.", "Plus une goutte d'eau potable."],
    "air": ["Les alarmes d'atmosphere hurlent : l'air se raréfie.".replace("é", "e"), "Le taux de CO2 grimpe dangereusement."],
    "unpaid": ["La paie n'est pas tombee ; les visages se ferment.", "Caisse vide : les salaires attendront.", "Les salaires sont en retard ; l'ambiance se tend."],
    "paid": ["{amount} credits de salaires viennent d'etre distribues.", "Jour de paie : {amount} credits.", "L'equipage touche {amount} credits."],
    "repair": ["{a} resserre quelques boulons et le module repart.", "{a} remet un systeme d'aplomb avec trois bouts de ficelle.", "Reparation de fortune par {a} ; ca tiendra."],
    "quit": ["{a} s'en va pour de bon.", "{a} dépose sa démission auprès de l'IA de bord."],
    "dock_greet": ["Station : bienvenue, vaisseau. Amarrage autorise.", "Controle : sas {n} ouvert, bon sejour.", "Vous etes amarre. Les services de la station sont a votre disposition.", "Controle : verrouillage des pinces confirmé, IA de bord.", "Station : tarifs de port affiches. Faites bon commerce.", "Equipe de quai : on s'occupe de tout, capitaine."],
    "undock": ["Controle : liberation des pinces, bon vol.", "Station : depart autorise, soyez prudent dehors.", "Les pinces se retractent ; le vide vous attend.", "Controle : trajectoire de sortie degagee."],
    "customs_clear": ["Douane : rien a signaler. Circulez.", "Inspecteur : papiers en regle, bonne journee.", "La douane ne trouve rien de suspect."],
    "customs_fine": ["Douane : marchandise interdite saisie, amende de {amount} credits.", "Controle fructueux : {amount} credits d'amende.", "L'inspecteur sourit et vous tend une amende de {amount} credits."],
    "hail_trader": ["Marchand : salut, vaisseau, un peu de commerce ?", "Cargo en approche : on peut s'arranger sur les prix.", "Marchand : bonne route, qu'il vous soit profitable.", "Un cargo vous salue d'un clignotement de feux."],
    "hail_pirate": ["Pirate : lachez la cargaison et personne ne souffrira !", "Inconnu : coupez vos moteurs ou nous ouvrons le feu.", "Pirate : un beau vaisseau... on va le prendre.", "Menace : votre soute nous intéresse."],
    "hail_patrol": ["Patrouille : identifiez-vous et restez a distance.", "Patrouille : zone surveillee, circulez dans le calme.", "La patrouille balaie votre vaisseau avec ses scanners."],
    "hail_miner": ["Mineur : belle journee pour casser du caillou !", "Mineur : prenez garde, nos foreuses balancent des debris.", "Un mineur vous fait signe depuis son astéroide.".replace("é", "e")],
    "jump_charge": ["Les bobines du saut se chargent en gemissant.", "Le saut approche : tout le monde s'attache.", "Le moteur de saut monte en puissance."],
    "jump_arrive": ["Sortie d'hyperespace : les etoiles se recomposent.", "Saut reussi : une nouvelle etoile vous accueille.", "Arrivee : tous les systemes sont nominaux.", "Les instruments se recalibrent apres le saut."],
    "scan_found": ["Le scanner signale un contact interessant.", "Echo inhabituel sur les capteurs.", "Nouvelle signature detectee."],
    "mining_done": ["Le chargement du minerai est termine.", "Le laser de minage s'eteint : soute pleine.", "Minage termine : bonne prise."],
    "low_fuel": ["Alerte : reserve de carburant critique.", "Les jauges de carburant plongent dans le rouge.", "Il faudra bientot trouver du carburant."],
    "low_power": ["Delestage : plusieurs systemes coupes pour tenir le coup.", "La reserve d'energie est presque a sec.", "Le reacteur peine a alimenter le vaisseau."],
    "overheat": ["Temperature critique : les radiateurs saturent.", "La coque chauffe : coupez des systemes !", "Alerte thermique : le vaisseau surchauffe."],
    "hull_hit": ["Impact sur la coque !", "Choc violent : la coque encaisse.", "Un projectile frappe le blindage.", "La structure craque sous l'impact."],
    "shield_down": ["Boucliers a zero !", "Les boucliers cedent.", "Plus de bouclier : la coque est exposee."],
    "victory": ["Cible detruite.", "L'adversaire explose en silence.", "Menace neutralisee.", "Le combat est termine, vous etes toujours la."],
    "flare": ["Eruption stellaire : tempete de particules !", "Eruption : les capteurs saturent.", "Une eruption solaire balaie le systeme."],
    "meteoroid": ["Micrometeoroides : la coque crepite.", "Impact de micrometeoroide sur la coque.", "Une pluie de poussiere frappe le vaisseau."],
    "research_done": ["Recherche terminee : une nouvelle technologie est disponible.", "Les laboratoires ont abouti : decouverte !", "Eurêka : nouvelle technologie debloquee.".replace("Eurêka", "Eureka")],
    "colony_founded": ["Premiere pierre posee : la colonie est fondee.", "Une nouvelle colonie voit le jour.", "Les premiers colons plantent le drapeau."],
    "colony_growth": ["La colonie prospere et gagne des habitants.", "Nouvelle naissance a la colonie.", "La population de la colonie augmente."],
    "colony_trouble": ["La colonie manque de nourriture.", "Troubles dans la colonie : les colons s'agitent.", "La colonie manque d'energie."],
    "contract_done": ["Contrat accompli : vos credits arrivent.", "Livraison validee : la paie est versee.", "Mission terminee, client satisfait."],
    "contract_failed": ["Contrat echoue : le client est furieux.", "Delai depasse : penalite appliquee.", "Vous n'avez pas honore le contrat."],
    "news_war": ["Les tensions montent entre deux factions voisines.", "Une escarmouche eclate a la frontiere.", "Les chantiers navals tournent a plein regime.", "Les marches de l'armement s'envolent."],
    "news_trade": ["Un convoi de ravitaillement arrive de loin.", "Penurie signalee dans plusieurs stations.", "Nouvelle route commerciale ouverte par un consortium.", "Les prix du carburant s'effondrent."],
    "news_science": ["Une equipe annonce une decouverte majeure.", "Un observatoire detecte une anomalie lointaine.", "Un laboratoire publie des resultats inattendus.", "Un vaisseau d'exploration manque a l'appel."],
    "news_culture": ["Un festival anime la station principale.", "Un celebre musicien se produit a bord d'un paquebot.", "La mode des vieux jeux d'arcade refait surface.", "Un concours de cuisine en gravite nulle fait sensation."],
    "flavor_deep": ["Le silence de l'espace profond vous enveloppe.", "Loin de toute lumiere, on entend son propre coeur.", "Seule l'etoile lointaine tient compagnie au vaisseau.", "Rien alentour, a des annees-lumiere a la ronde."],
    "flavor_star": ["La chaleur de l'etoile fait chanter les radiateurs.", "L'etoile emplit les hublots d'une lumiere crue.", "On devine des protuberances sur le disque stellaire."],
    "flavor_planet": ["Les nuages defilent lentement sous la coque.", "La planete occupe tout le champ de vision.", "Le terminateur trace une ligne nette sur le sol."],
    "flavor_belt": ["Des cailloux scintillent a perte de vue.", "La ceinture bruisse de milliers de reflets.", "Chaque rocher cache peut-etre un filon."],
    "flavor_station": ["Les lumieres de la station percent l'obscurite.", "On entend le ronron des echangeurs de chaleur.", "La station tourne lentement sur son axe."],
    "tip_trade": ["Astuce : achetez pres des industries, vendez loin.", "Astuce : l'ecart d'achat et de vente est d'environ 10 %.", "Astuce : la contrebande rapporte, mais la douane veille."],
    "tip_flight": ["Astuce : F active le pilotage assiste, retire-le pour piloter libre.", "Astuce : visez le prograde pour freiner efficacement.", "Astuce : le pilote auto s'arrete quand le carburant manque."],
    "tip_ship": ["Astuce : les radiateurs evacuent la chaleur des reacteurs et des armes.", "Astuce : un module detruit peut etre repare a quai.", "Astuce : equilibrez production et consommation d'energie."],
    "tip_crew": ["Astuce : un equipage heureux travaille mieux.", "Astuce : un medecin a bord evite bien des drames.", "Astuce : la cuisine et la botanique allongent les provisions."],
    "tip_empire": ["Astuce : une colonie a besoin de nourriture, d'eau et d'energie.", "Astuce : les routes de flotte automatisent votre commerce.", "Astuce : recherchez des technologies pour debloquer de nouveaux modules."],
}

T.update({
    "flavor_barren": ["Un monde de roche nue, creusé de cratères.".replace("é","e"), "Aucune atmosphere : la surface garde la memoire de chaque impact.", "Le regolithe gris s'etend sans fin sous les projecteurs."],
    "flavor_dwarf": ["Une petite boule glacee, gravite derisoire.", "Le monde nain est a peine plus qu'un gros caillou rond."],
    "flavor_desert": ["Des dunes ocre defilent sous les capteurs.", "Un ciel poussiereux et un sol craquele : le desert regne.", "Les vents de sable dessinent des spirales a l'horizon."],
    "flavor_terran": ["Mers, nuages et continents : un monde vivant.", "Les lumieres d'une atmosphere respirable font oublier le vide.", "Le vert des forets contraste avec le noir de l'espace."],
    "flavor_ocean": ["Un ocean mondial scintille jusqu'a l'horizon.", "Pas la moindre terre emergee, seulement des vagues sans fin.", "Des tempetes tourbillonnent sur une mer immense."],
    "flavor_tundra": ["Un monde froid, balaye de vents glaciaux.", "Les calottes blanches rongent les continents.", "Le gel recouvre presque tout, sauf quelques vallees."],
    "flavor_ice": ["Une banquise sans fin reflechit l'etoile.", "Des geysers de vapeur fendent la glace dans un nuage d'argent.", "Un monde de glace, silencieux et brillant."],
    "flavor_lava": ["Des fleuves de lave eclairent la face nocturne.", "La croute fume et se fissure : la surface est un enfer.", "Le sol rougeoie sous un ciel de cendres."],
    "flavor_venusian": ["Une epaisse couche de nuages acides masque le sol.", "La pression et la chaleur ecraseraient n'importe quelle sonde.", "Des eclairs zebrent une atmosphere etouffante."],
    "flavor_gasgiant": ["Des bandes de nuages colorees s'etirent sur des milliers de kilometres.", "Une tempete grande comme un monde tourne lentement.", "La geante gazeuse emplit l'ecran de ses volutes."],
    "flavor_icegiant": ["Un bleu profond et froid enveloppe la geante de glace.", "Des vents supersoniques cinglent les hautes couches de methane.", "La geante de glace brille d'un bleu electrique."],
    "flavor_hotjupiter": ["Une geante brulante colle a son etoile.", "L'atmosphere s'evapore dans un sillage lumineux.", "Un Jupiter chaud : la nuit y est aussi chaude que le jour."],
    "flavor_class_o": ["Une geante bleue brule d'un eclat insoutenable.", "Les capteurs saturent sous le flot d'ultraviolets."],
    "flavor_class_b": ["Une etoile bleu-blanc inonde le systeme de lumiere.", "Le vent stellaire est violent a cette distance."],
    "flavor_class_a": ["Une etoile blanche brille d'un feu pur.", "Un disque blanc-bleu domine le ciel."],
    "flavor_class_f": ["Une etoile jaune-blanc, chaude et vive.", "Une lumiere claire baigne le systeme."],
    "flavor_class_g": ["Une etoile jaune, proche de celle des anciens.", "Une douce lumiere doree eclaire les planetes."],
    "flavor_class_k": ["Une naine orange eclaire le systeme d'une lueur chaleureuse.", "Une etoile ambre, stable et paisible."],
    "flavor_class_m": ["Une naine rouge faible et eruptive.", "Le systeme baigne dans une lueur sombre et rougeatre."],
    "flavor_class_d": ["Une naine blanche, vestige d'une etoile morte, brille faiblement.", "Le coeur nu d'une ancienne etoile eclaire un systeme silencieux."],
    "tip_map": ["Astuce : M ouvre la carte du systeme, la molette zoome.", "Astuce : F1 affiche l'aide contextuelle.", "Astuce : F5 ouvre la carte de la galaxie ; la lettre indique la classe de l'etoile."],
    "tip_combat": ["Astuce : coupez la poussee pour viser, puis relancez.", "Astuce : les boucliers se regenerent, la coque non.", "Astuce : les missiles peuvent etre interceptes par la defense ponctuelle."],
    "tip_mining": ["Astuce : visez un asteroide M pour le fer et le nickel.", "Astuce : le minage consomme de l'energie ; surveillez la batterie.", "Astuce : les asteroides C contiennent de l'eau et des volatils."],
    "tip_colony": ["Astuce : fondez une colonie sur un monde rocheux avec de l'eau.", "Astuce : une colonie produit mieux avec des logements et de l'energie.", "Astuce : les politiques de colonie changent la croissance et la production."],
    "tip_tech": ["Astuce : F4 ouvre l'arbre technologique.", "Astuce : la recherche vient des colonies, des laboratoires et des anomalies.", "Astuce : certaines coques et modules sont verrouilles par la recherche."],
    "tip_rules": ["Astuce : les regles de partie se changent depuis le menu principal ou la pause.", "Astuce : le mode creatif active la console avec la touche ~.", "Astuce : une partie modifiee est signalee en haut a droite."],
    "news_faction": ["Un traite de commerce est signe entre deux factions.", "Un gouverneur est destitue apres un scandale.", "Une faction renforce ses douanes aux frontieres.", "Un embargo frappe un produit strategique."],
    "news_danger": ["Des pirates ont ete vus pres d'une ceinture d'asteroides.", "Une eruption stellaire perturbe les communications.", "Une epave derivante signalee sur la route marchande.", "Les patrouilles doublent leurs rondes."],
    "news_economy": ["La demande en electronique s'envole.", "Le prix de l'eau chute a la suite d'un convoi de glace.", "Les mineurs font grise mine : le minerai de fer se vend mal.", "Une station recherche des livreurs de nourriture."],
    "station_busy": ["Le trafic est dense : attendez votre tour d'amarrage.", "Plusieurs navires attendent leur creneau.", "Les quais sont bondes aujourd'hui."],
    "station_quiet": ["La station est calme : les quais sont deserts.", "Peu de trafic : amarrage immediat.", "Un silence inhabituel regne sur les quais."],
})

T.update({
    "tip_misc": ["Astuce : sauvegardez avant un saut long.", "Astuce : le temps accelere s'arrete automatiquement en cas de danger.", "Astuce : la touche H ouvre l'ecran du vaisseau a quai.", "Astuce : un pilote auto bien regle economise du carburant.", "Astuce : verifiez le fret avant de franchir une frontiere.", "Astuce : les contrats emergent des penuries reelles des stations.", "Astuce : l'assurance reconstruit votre vaisseau, mais pas votre cargaison."],
    "flavor_misc": ["Un cliquetis irregulier court dans une cloison ; probablement rien.", "L'eclairage de la coursive vacille un instant.", "Quelqu'un fredonne dans la salle des machines.", "L'odeur de cafe brule flotte dans le carre.", "Les pompes de refroidissement grondent doucement.", "Le journal de bord s'allonge de page en page."],
    "news_misc": ["Une comete passe pres de la ceinture exterieure.", "Un vieux croiseur est mis a la ferraille.", "Des chercheurs annoncent un nouveau type d'alliage.", "Une epidemie de grippe frappe une station voisine.", "Un pilote debutant bat le record de traversee.", "Le prix de l'helium 3 fait la une des marches."],
})
n = 0
with open(os.path.join(D, "text", "extra.ron"), "w", encoding="utf8") as f:
    f.write("// GENERATED by tools/gen_scale.py - extra text templates (merged into text/crew.ron keys).\n[\n")
    for k, v in T.items():
        f.write(f' ("{k}", [' + ", ".join(q(x) for x in v) + "]),\n")
        n += len(v)
    f.write("]\n")
print("text templates", n)
