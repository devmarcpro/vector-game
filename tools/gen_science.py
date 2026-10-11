#!/usr/bin/env python3
"""Generates data/techs.ron, data/worldgen/anomalies.ron and data/diseases.ron.
Run after gen_content.py / gen_colony.py:  python tools/gen_science.py"""
import os, re

OUT = os.path.join(os.path.dirname(__file__), "..", "data")


def read(name):
    return open(os.path.join(OUT, name), encoding="utf-8").read()


# ---------------- technologies ----------------
FIELDS = {
    "propulsion": ["engine", "ftl", "rcs", "gyro"],
    "energie": ["reactor", "solar", "battery", "radiator"],
    "armement": ["weapon", "defense", "shield", "armor"],
    "structure": ["cargo", "tank", "dock", "hangar", "armory"],
    "sciences_vie": ["lab", "scanner", "medbay", "life_support", "habitat", "greenhouse", "workshop"],
    "industrie": ["mining"],
}
BASE_COST = {"propulsion": 500, "energie": 450, "armement": 400, "structure": 250, "sciences_vie": 300, "industrie": 350}

# theory techs: id, name, field, cost, requires
THEORY = [
    ("basics", "Fondamentaux", "sciences_vie", 0, []),
    ("plasma_physics", "Physique des plasmas", "energie", 600, ["basics"]),
    ("spatial_theory", "Theorie de l'espace-temps", "propulsion", 1500, ["plasma_physics"]),
    ("materials_science", "Science des materiaux", "structure", 500, ["basics"]),
    ("advanced_robotics", "Robotique avancee", "industrie", 800, ["materials_science"]),
    ("applied_ballistics", "Balistique appliquee", "armement", 450, ["basics"]),
    ("directed_energy", "Energie dirigee", "armement", 700, ["plasma_physics"]),
    ("xenology", "Xenologie", "sciences_vie", 900, ["basics"]),
    ("quantum_computing", "Calcul quantique", "sciences_vie", 1800, ["advanced_robotics"]),
    ("closed_ecology", "Ecologie en circuit ferme", "sciences_vie", 700, ["basics"]),
]
THEORY_FOR = {  # family prefix -> theory tech needed from tier 3
    "fusion": "plasma_physics", "torch": "plasma_physics", "ftl": "spatial_theory", "shield": "directed_energy",
    "pulselaser": "directed_energy", "particlebeam": "directed_energy", "massdriver": "applied_ballistics",
    "railgun": "applied_ballistics", "autocannon": "applied_ballistics", "missilerack": "applied_ballistics",
    "pointdefense": "applied_ballistics", "ecm": "quantum_computing", "scanner": "quantum_computing",
    "lab": "xenology", "habitat": "closed_ecology", "lifesup": "closed_ecology", "armor": "materials_science",
    "cargo": "materials_science", "tank": "materials_science", "laser": "advanced_robotics",
}

techs = {}  # id -> dict
order = []

def add(id, name, field, cost, requires, unlocks=()):
    assert id not in techs, id
    techs[id] = dict(id=id, name=name, field=field, cost=cost, requires=list(requires), unlocks=list(unlocks))
    order.append(id)

for tid, name, field, cost, req in THEORY:
    add(tid, name, field, cost, req)

# modules: group by family
mods = re.findall(r'\(id:"([a-z0-9_]+)", name:"([^"]+)", category:"([a-z_]+)"', read("modules.ron"))
families = {}
for mid, mname, cat in mods:
    m = re.match(r"(.*)_mk(\d+)$", mid)
    if not m:
        continue
    fam, tier = m.group(1), int(m.group(2))
    families.setdefault(fam, dict(cat=cat, tiers={}, label=re.sub(r" Mk\d+$", "", mname)))["tiers"][tier] = (mid, mname)

def field_of(cat):
    for f, cats in FIELDS.items():
        if cat in cats:
            return f
    return "industrie"

for fam, info in sorted(families.items()):
    cat = info["cat"]
    gate_from = 2 if cat in ("weapon", "defense", "shield") or fam == "ftl" else 3
    tiers = sorted(info["tiers"])
    for t in tiers:
        if t < gate_from:
            continue
        mid, mname = info["tiers"][t]
        field = field_of(cat)
        req = []
        prev = f"{fam}_mk{t - 1}"
        if prev in techs:
            req.append(prev)
        if fam in THEORY_FOR and t >= max(gate_from, 3 if gate_from == 3 else 2):
            req.append(THEORY_FOR[fam])
        if not req:
            req.append("basics")
        cost = int(BASE_COST[field] * (t - 1) ** 1.6 * (1.0 if gate_from == 3 else 1.3))
        add(mid, mname, field, max(cost, 150), sorted(set(req)), [("module", mid)])

# hulls
hulls = re.findall(r'\(id:"([a-z0-9_]+)", name:"([^"]+)", class:"([a-z]+)", mass:[0-9.]+, hp:[0-9.]+, cost:([0-9.]+)', read("hulls.ron"))
FREE_HULLS = {"shuttle", "skiff", "prospector", "courier", "fighter"}
HULL_REQ = {"driller": "materials_science", "hauler": "materials_science", "explorer": "xenology", "survey": "xenology",
            "corvette": "applied_ballistics", "tender": "hull_hauler", "freighter": "hull_hauler", "frigate": "hull_corvette",
            "colony": "hull_freighter", "stationkit": "hull_colony"}
for hid, hname, cls, cost in hulls:
    if hid in FREE_HULLS:
        continue
    add(f"hull_{hid}", f"Coque : {hname}", "structure", int(float(cost) / 700), [HULL_REQ.get(hid, "basics")], [("hull", hid)])

# buildings
GATED_BUILDINGS = {
    "arcology": ("structure", 1200, ["materials_science"]),
    "fusion_plant": ("energie", 1400, ["plasma_physics"]),
    "fission_plant": ("energie", 500, ["basics"]),
    "geothermal_plant": ("energie", 450, ["basics"]),
    "deep_drill": ("industrie", 800, ["advanced_robotics"]),
    "gas_scoop": ("industrie", 700, ["basics"]),
    "electronics_fab": ("industrie", 900, ["advanced_robotics"]),
    "terraformer": ("sciences_vie", 3000, ["closed_ecology", "advanced_robotics"]),
    "defense_turret": ("armement", 400, ["applied_ballistics"]),
    "component_factory": ("industrie", 600, ["materials_science"]),
}
for bid, (field, cost, req) in GATED_BUILDINGS.items():
    add(f"bld_{bid}", f"Batiment : {bid.replace('_', ' ')}", field, cost, req, [("building", bid)])

# a few extra capability techs (no unlocks yet; they feed gameplay modifiers later)
EXTRA = [
    ("efficient_mining", "Minage efficace", "industrie", 500, ["basics"]),
    ("deep_space_navigation", "Navigation hauturiere", "propulsion", 800, ["spatial_theory"]),
    ("field_medicine", "Medecine de terrain", "sciences_vie", 400, ["basics"]),
    ("trade_networks", "Reseaux commerciaux", "structure", 450, ["basics"]),
    ("hardened_electronics", "Electronique durcie", "energie", 700, ["quantum_computing"]),
    ("autonomous_fleets", "Flottes autonomes", "structure", 1100, ["advanced_robotics"]),
]
for tid, name, field, cost, req in EXTRA:
    add(tid, name, field, cost, req)

def fmt_list(items):
    return "[" + ", ".join(f'"{x}"' for x in items) + "]"

lines = ["// GENERATED by tools/gen_science.py. start:true techs are known from the beginning.", "["]
for tid in order:
    t = techs[tid]
    unlocks = "[" + ", ".join(f'("{k}","{v}")' for k, v in t["unlocks"]) + "]"
    start = "true" if tid == "basics" else "false"
    lines.append(f' (id:"{tid}", name:"{t["name"]}", field:"{t["field"]}", cost:{float(t["cost"])!r}, requires:{fmt_list(t["requires"])}, unlocks:{unlocks}, start:{start}),')
lines.append("]")
open(os.path.join(OUT, "techs.ron"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

# ---------------- anomalies ----------------
# id, name, effect radius (m), hazard kind, strength, research (min,max), credits (min,max), loot [(res,min,max)], text
ANOMALIES = [
    ("derelict_ship", "Epave derivante", 3000, "none", 0.0, (20, 120), (0, 1500), [("spare_parts", 200, 900), ("electronics", 20, 160), ("fuel", 100, 600)], "Un vaisseau mort depuis des decennies ; ses soutes sont intactes."),
    ("ancient_station", "Station ancienne", 8000, "none", 0.0, (200, 900), (0, 4000), [("artifacts", 5, 40), ("circuits", 50, 300), ("nanomaterials", 10, 60)], "Une station aux lignes inconnues, silencieuse et froide."),
    ("alien_beacon", "Balise inconnue", 5000, "none", 0.0, (150, 600), (0, 0), [("artifacts", 2, 15)], "Une balise emet un signal regulier, nul ne sait pour qui."),
    ("artifact_field", "Champ d'artefacts", 6000, "none", 0.0, (100, 500), (0, 2500), [("artifacts", 10, 80)], "Des fragments d'origine inconnue flottent en nuage."),
    ("crystal_formation", "Formation cristalline", 7000, "none", 0.0, (80, 300), (0, 500), [("diamond", 40, 300), ("rare_earths", 100, 700)], "Un cristal geant, sommet de pureté, dérive entre les orbites."),
    ("gravity_well", "Puits gravitationnel", 200000, "gravity", 0.8, (60, 250), (0, 0), [], "Un point de masse invisible courbe les trajectoires."),
    ("radiation_belt", "Ceinture de radiations", 150000, "radiation", 0.6, (60, 220), (0, 0), [("uranium", 20, 120)], "Des particules piegees saturent le champ de mesure."),
    ("ion_storm", "Tempete ionique", 250000, "ion", 0.5, (50, 200), (0, 0), [], "Un front de plasma charge sature les capteurs et les circuits."),
    ("nebula", "Nebuleuse", 600000, "sensor", 0.8, (40, 180), (0, 0), [("hydrogen", 200, 900), ("helium3", 5, 40)], "Un nuage de gaz et de poussiere aveugle les scanners."),
    ("comet", "Comete", 30000, "none", 0.0, (30, 150), (0, 0), [("water_ice", 500, 3000), ("ammonia", 100, 600)], "Une comete active, queue deployee dans le vent stellaire."),
    ("magnetic_anomaly", "Anomalie magnetique", 120000, "magnetic", 0.5, (70, 260), (0, 0), [("cobalt", 100, 600)], "Le compas s'affole : un champ magnetique intense perturbe les gyroscopes."),
    ("spatial_rift", "Faille spatiale", 80000, "rift", 0.4, (300, 1200), (0, 0), [("artifacts", 3, 25)], "L'espace-temps semble dechire ; les instruments divergent."),
]
lines = ["// GENERATED by tools/gen_science.py. hazard: none | radiation | gravity | ion | sensor | magnetic | rift.", "["]
for a in ANOMALIES:
    loot = "[" + ", ".join(f'("{r}",{float(lo)!r},{float(hi)!r})' for r, lo, hi in a[7]) + "]"
    lines.append(f' (id:"{a[0]}", name:"{a[1]}", radius:{float(a[2])!r}, hazard:"{a[3]}", strength:{float(a[4])!r}, research:({float(a[5][0])!r},{float(a[5][1])!r}), credits:({float(a[6][0])!r},{float(a[6][1])!r}), loot:{loot}, text:"{a[8]}"),')
lines.append("]")
os.makedirs(os.path.join(OUT, "worldgen"), exist_ok=True)
open(os.path.join(OUT, "worldgen", "anomalies.ron"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

# ---------------- diseases and injuries ----------------
# id, name, severity/day, duration days, contagion/day, lethality, cure resource ("" = rest only)
DISEASES = [
    ("grippe", "Grippe spatiale", 0.04, 6, 0.10, 0.0, "medicine"),
    ("rhume", "Rhume des soutes", 0.01, 4, 0.12, 0.0, ""),
    ("intoxication", "Intoxication alimentaire", 0.08, 2, 0.0, 0.0, "medicine"),
    ("gastro", "Gastro-enterite", 0.05, 3, 0.15, 0.0, "medicine"),
    ("pneumonie", "Pneumonie", 0.09, 10, 0.03, 0.06, "medicine"),
    ("bronchite", "Bronchite chronique", 0.02, 20, 0.0, 0.0, "medicine"),
    ("fievre_bleue", "Fievre bleue", 0.10, 8, 0.25, 0.05, "medicine"),
    ("variole_spatiale", "Variole spatiale", 0.12, 14, 0.20, 0.12, "medicine"),
    ("cholera", "Cholera des citernes", 0.14, 6, 0.10, 0.10, "medicine"),
    ("typhoide", "Typhoide", 0.08, 12, 0.06, 0.04, "medicine"),
    ("hepatite", "Hepatite", 0.03, 30, 0.02, 0.01, "medicine"),
    ("tuberculose", "Tuberculose", 0.03, 45, 0.04, 0.03, "medicine"),
    ("septicemie", "Septicemie", 0.15, 5, 0.0, 0.15, "medicine"),
    ("meningite", "Meningite", 0.13, 7, 0.05, 0.12, "medicine"),
    ("radiation", "Mal des rayons", 0.07, 12, 0.0, 0.02, "medicine"),
    ("brulure_rayons", "Brulures de rayonnement", 0.05, 15, 0.0, 0.0, "medicine"),
    ("atrophie", "Atrophie musculaire", 0.01, 40, 0.0, 0.0, ""),
    ("perte_osseuse", "Perte de densite osseuse", 0.01, 60, 0.0, 0.0, ""),
    ("mal_de_l_espace", "Mal de l'espace", 0.02, 3, 0.0, 0.0, ""),
    ("decompression", "Accident de decompression", 0.12, 4, 0.0, 0.05, "medicine"),
    ("hypoxie", "Hypoxie", 0.15, 2, 0.0, 0.08, ""),
    ("intoxication_co2", "Intoxication au CO2", 0.10, 3, 0.0, 0.05, ""),
    ("hypothermie", "Hypothermie", 0.09, 3, 0.0, 0.04, ""),
    ("coup_de_chaleur", "Coup de chaleur", 0.08, 3, 0.0, 0.03, ""),
    ("fracture", "Fracture", 0.02, 25, 0.0, 0.0, ""),
    ("entorse", "Entorse", 0.01, 10, 0.0, 0.0, ""),
    ("commotion", "Commotion cerebrale", 0.04, 7, 0.0, 0.01, ""),
    ("brulure", "Brulure", 0.05, 12, 0.0, 0.0, "medicine"),
    ("blessure_arme", "Blessure par arme", 0.08, 14, 0.0, 0.05, "medicine"),
    ("infection_blessure", "Infection de blessure", 0.07, 9, 0.0, 0.06, "medicine"),
    ("depression", "Depression", 0.01, 60, 0.0, 0.0, ""),
    ("anxiete", "Anxiete d'isolement", 0.005, 40, 0.01, 0.0, ""),
    ("paranoia_isolement", "Paranoia d'isolement", 0.01, 50, 0.02, 0.0, ""),
    ("burn_out", "Burn-out", 0.01, 30, 0.0, 0.0, ""),
    ("insomnie_chronique", "Insomnie chronique", 0.005, 40, 0.0, 0.0, ""),
    ("addiction", "Addiction", 0.01, 80, 0.0, 0.01, ""),
    ("allergie", "Allergie severe", 0.04, 2, 0.0, 0.01, "medicine"),
    ("cancer_precoce", "Cancer precoce", 0.02, 120, 0.0, 0.08, "medicine"),
    ("spores_fongiques", "Mycose pulmonaire", 0.06, 14, 0.08, 0.03, "medicine"),
    ("peste_des_cales", "Peste des cales", 0.13, 8, 0.10, 0.14, "medicine"),
    ("virus_chimere", "Fievre chimere", 0.11, 7, 0.30, 0.09, "medicine"),
    ("fievre_vitreuse", "Fievre vitreuse", 0.10, 12, 0.12, 0.07, ""),
]
lines = ["// GENERATED by tools/gen_science.py. Diseases, injuries and conditions.", "["]
for i, n, sev, dur, con, leth, cure in DISEASES:
    lines.append(f' (id:"{i}", name:"{n}", severity:{sev!r}, duration_days:{float(dur)!r}, contagion:{con!r}, lethality:{leth!r}, cure:"{cure}"),')
lines.append("]")
open(os.path.join(OUT, "diseases.ron"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

print(len(order), "techs,", len(ANOMALIES), "anomaly types,", len(DISEASES), "conditions")
