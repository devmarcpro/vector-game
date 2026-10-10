#!/usr/bin/env python3
"""Generates data/modules.ron and data/hulls.ron (tiered families keep the data consistent).
Run: python tools/gen_content.py   -- the .ron files are the source of truth at runtime."""
import os

OUT = os.path.join(os.path.dirname(__file__), "..", "data")
modules = []


def mod(id, name, category, slot, size, mass, hp, cost, **kw):
    modules.append(dict(id=id, name=name, category=category, slot=slot, size=size, mass=mass, hp=hp, cost=cost, **kw))


def family(prefix, label, category, slot, tiers, base):
    """tiers: list of dict overrides per Mk level (size = Mk)."""
    for k, t in enumerate(tiers, start=1):
        d = dict(base)
        d.update(t)
        mod(f"{prefix}_mk{k}", f"{label} Mk{k}", category, slot, d.pop("size", k), d.pop("mass"), d.pop("hp"), d.pop("cost"), **d)


# Engines: thrust (N), isp (s), power at full thrust (kW, negative), heat via consumption.
family("torch", "Torche a fusion", "engine", "Core", [
    dict(mass=2200, hp=300, cost=40000, thrust=1.0e6, isp=150000, power=-4000, priority=1),
    dict(mass=4000, hp=450, cost=90000, thrust=1.4e6, isp=200000, power=-6000, priority=1),
    dict(mass=7500, hp=700, cost=220000, thrust=2.2e6, isp=260000, power=-9000, priority=1),
    dict(mass=14000, hp=1100, cost=600000, thrust=3.5e6, isp=340000, power=-14000, priority=1),
], {})
family("chem", "Moteur chimique", "engine", "Core", [
    dict(mass=1500, hp=250, cost=9000, thrust=1.2e6, isp=4500, power=-50, priority=1),
    dict(mass=2800, hp=380, cost=21000, thrust=2.4e6, isp=4800, power=-80, priority=1),
], {})
# Reactors: power (kW produced), heat = waste ratio handled in sim (50 % for fusion).
family("fusion", "Reacteur a fusion", "reactor", "Core", [
    dict(mass=1500, hp=250, cost=60000, power=8000, priority=0),
    dict(mass=2500, hp=350, cost=120000, power=15000, priority=0),
    dict(mass=4500, hp=550, cost=320000, power=30000, priority=0),
    dict(mass=8000, hp=900, cost=900000, power=60000, priority=0),
], {})
family("fission", "Reacteur a fission", "reactor", "Core", [
    dict(mass=3500, hp=300, cost=25000, power=2000, priority=0),
    dict(mass=6000, hp=450, cost=60000, power=6000, priority=0),
], {})
family("rtg", "Generateur RTG", "reactor", "Internal", [
    dict(mass=150, hp=80, cost=18000, power=50, priority=0),
    dict(mass=400, hp=120, cost=45000, power=200, priority=0),
], {})
family("solar", "Panneaux solaires", "solar", "External", [
    dict(mass=300, hp=60, cost=8000, power=300, priority=0),
    dict(mass=700, hp=90, cost=19000, power=800, priority=0),
    dict(mass=1500, hp=130, cost=45000, power=2000, priority=0),
], {})
family("battery", "Batterie", "battery", "Internal", [
    dict(mass=300, hp=80, cost=5000, battery=60),
    dict(mass=700, hp=120, cost=12000, battery=200),
    dict(mass=1600, hp=180, cost=32000, battery=600),
    dict(mass=3500, hp=260, cost=90000, battery=1800),
], {})
family("radiator", "Radiateur", "radiator", "External", [
    dict(mass=250, hp=60, cost=4000, radiate=5000),
    dict(mass=550, hp=90, cost=9000, radiate=10000),
    dict(mass=1100, hp=130, cost=21000, radiate=20000),
    dict(mass=2200, hp=190, cost=50000, radiate=40000),
], {})
family("tank", "Reservoir", "tank", "Internal", [
    dict(mass=400, hp=100, cost=4000, fuel=4000),
    dict(mass=800, hp=150, cost=8000, fuel=8000),
    dict(mass=1600, hp=220, cost=17000, fuel=16000),
    dict(mass=3200, hp=320, cost=36000, fuel=32000),
], {})
family("cargo", "Soute", "cargo", "Internal", [
    dict(mass=1500, hp=120, cost=6000, cargo_kg=15000, cargo_m3=20),
    dict(mass=3000, hp=180, cost=13000, cargo_kg=30000, cargo_m3=40),
    dict(mass=6000, hp=260, cost=28000, cargo_kg=60000, cargo_m3=80),
    dict(mass=12000, hp=380, cost=60000, cargo_kg=120000, cargo_m3=160),
], {})
family("laser", "Laser de minage", "mining", "Hardpoint", [
    dict(mass=350, hp=70, cost=12000, mining_rate=12, mining_range=1500, power=-1500, priority=2),
    dict(mass=700, hp=110, cost=26000, mining_rate=25, mining_range=2000, power=-3000, priority=2),
    dict(mass=1400, hp=170, cost=60000, mining_rate=50, mining_range=2500, power=-6000, priority=2),
    dict(mass=2800, hp=260, cost=140000, mining_rate=100, mining_range=3000, power=-12000, priority=2),
], {})
family("lifesup", "Support de vie", "life_support", "Internal", [
    dict(mass=300, hp=80, cost=7000, life_support=4, power=-40, priority=0),
    dict(mass=550, hp=110, cost=15000, life_support=10, power=-90, priority=0),
    dict(mass=1100, hp=160, cost=34000, life_support=24, power=-200, priority=0),
], {})
family("habitat", "Module d'habitation", "habitat", "Internal", [
    dict(mass=1200, hp=120, cost=9000, crew_cap=2, power=-20, priority=1),
    dict(mass=2400, hp=180, cost=20000, crew_cap=5, power=-40, priority=1),
    dict(mass=4800, hp=260, cost=44000, crew_cap=12, power=-80, priority=1),
], {})
family("medbay", "Infirmerie", "medbay", "Internal", [
    dict(mass=600, hp=90, cost=15000, medical=1, power=-150, priority=2),
    dict(mass=1300, hp=140, cost=38000, medical=3, power=-300, priority=2),
], {})
family("lab", "Laboratoire", "lab", "Internal", [
    dict(mass=900, hp=90, cost=22000, science=1, power=-250, priority=3),
    dict(mass=1900, hp=140, cost=55000, science=3, power=-600, priority=3),
    dict(mass=3800, hp=210, cost=130000, science=8, power=-1400, priority=3),
], {})
family("workshop", "Atelier", "workshop", "Internal", [
    dict(mass=1100, hp=110, cost=20000, repair=1, power=-300, priority=2),
    dict(mass=2400, hp=170, cost=52000, repair=3, power=-700, priority=2),
], {})
family("greenhouse", "Serre", "greenhouse", "Internal", [
    dict(mass=1000, hp=70, cost=14000, food=2, power=-200, priority=2),
    dict(mass=2200, hp=110, cost=34000, food=6, power=-450, priority=2),
], {})
family("scanner", "Scanner", "scanner", "External", [
    dict(mass=200, hp=50, cost=9000, scan_range=2.0e5, power=-100, priority=2),
    dict(mass=450, hp=80, cost=26000, scan_range=2.0e7, power=-300, priority=2),
    dict(mass=900, hp=120, cost=80000, scan_range=2.0e9, power=-900, priority=2),
], {})
family("armor", "Blindage", "armor", "External", [
    dict(mass=900, hp=500, cost=7000, armor=1),
    dict(mass=1900, hp=1100, cost=18000, armor=2),
    dict(mass=4000, hp=2400, cost=46000, armor=4),
], {})
family("gyro", "Gyroscopes", "gyro", "Core", [
    dict(mass=250, hp=60, cost=6000, agility=0.2, power=-60, priority=1),
    dict(mass=600, hp=90, cost=15000, agility=0.45, power=-140, priority=1),
], {})
family("rcs", "Propulseurs RCS", "rcs", "External", [
    dict(mass=200, hp=50, cost=4000, rcs=1.0e5, power=-30, priority=1),
    dict(mass=450, hp=80, cost=10000, rcs=2.5e5, power=-60, priority=1),
], {})
family("shield", "Generateur de bouclier", "shield", "Core", [
    dict(mass=800, hp=100, cost=40000, shield=100, power=-1200, priority=2),
    dict(mass=1800, hp=160, cost=100000, shield=300, power=-3000, priority=2),
    dict(mass=4000, hp=240, cost=260000, shield=800, power=-7000, priority=2),
    dict(mass=8500, hp=360, cost=700000, shield=2000, power=-15000, priority=2),
], {})
family("dock", "Pince d'amarrage", "dock", "External", [
    dict(mass=400, hp=90, cost=8000, dock=1),
], {})
family("hangar", "Hangar", "hangar", "Internal", [
    dict(mass=2500, hp=150, cost=30000, hangar=1, power=-100, priority=3),
    dict(mass=6000, hp=250, cost=80000, hangar=3, power=-250, priority=3),
], {})
family("armory", "Armurerie", "armory", "Internal", [
    dict(mass=800, hp=120, cost=16000, armory=1, power=-50, priority=3),
], {})


def ron_value(v):
    if isinstance(v, str):
        return '"%s"' % v.replace('"', "'")
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, float):
        return repr(v)
    if isinstance(v, int):
        return repr(float(v)) if False else str(v)
    return str(v)


FLOAT_FIELDS = {"mass", "hp", "cost", "power", "thrust", "isp", "fuel", "cargo_kg", "cargo_m3", "battery", "radiate",
                "mining_rate", "mining_range", "life_support", "crew_cap", "medical", "science", "repair", "food",
                "scan_range", "armor", "agility", "rcs", "shield", "dock", "hangar", "armory"}


def emit_module(m):
    parts = []
    for k, v in m.items():
        if k in FLOAT_FIELDS:
            parts.append(f"{k}:{float(v)!r}")
        elif k == "size" or k == "priority":
            parts.append(f"{k}:{v}")
        elif k == "slot":
            parts.append(f"{k}:{v}")
        else:
            parts.append(f'{k}:"{v}"')
    return " (" + ", ".join(parts) + "),"


with open(os.path.join(OUT, "modules.ron"), "w", encoding="utf-8") as f:
    f.write("// GENERATED by tools/gen_content.py -- edit the generator or this file freely.\n[\n")
    for m in modules:
        f.write(emit_module(m) + "\n")
    f.write("]\n")

# Hulls: slots are (type, size). class gives role. rcs N, max angular rate, angular accel.
hulls = [
    ("shuttle", "Navette Colibri", "navette", 3500, 400, 25000, 6.0e4, 2.2, 3.0, [("Core", 1), ("Core", 1), ("Internal", 1), ("Internal", 1), ("External", 1)]),
    ("skiff", "Esquif Sparrow", "leger", 4800, 600, 48000, 1.0e5, 2.0, 2.6, [("Core", 2), ("Core", 1), ("Internal", 2), ("Internal", 1), ("External", 1), ("Hardpoint", 1)]),
    ("prospector", "Prospecteur Atlas", "mineur", 6000, 1000, 90000, 2.8e5, 1.5, 2.0, [("Core", 2), ("Core", 2), ("Core", 1), ("Internal", 2), ("Internal", 2), ("Internal", 1), ("Internal", 1), ("Internal", 1), ("External", 2), ("External", 1), ("Hardpoint", 2)]),
    ("driller", "Foreur Titan", "mineur", 11000, 1800, 210000, 4.5e5, 1.2, 1.5, [("Core", 3), ("Core", 2), ("Core", 2), ("Internal", 3), ("Internal", 3), ("Internal", 2), ("Internal", 1), ("External", 3), ("External", 2), ("Hardpoint", 3), ("Hardpoint", 2)]),
    ("courier", "Courrier Zephyr", "rapide", 5200, 700, 140000, 3.0e5, 2.4, 3.2, [("Core", 3), ("Core", 2), ("Internal", 2), ("Internal", 1), ("External", 2), ("Hardpoint", 1)]),
    ("hauler", "Cargo Mule", "cargo", 9500, 1500, 160000, 3.5e5, 1.1, 1.3, [("Core", 2), ("Core", 2), ("Internal", 3), ("Internal", 3), ("Internal", 3), ("Internal", 2), ("External", 2), ("External", 2), ("Hardpoint", 1)]),
    ("freighter", "Cargo Leviathan", "cargo", 26000, 4200, 520000, 8.0e5, 0.8, 0.9, [("Core", 4), ("Core", 3), ("Core", 2), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("Internal", 3), ("Internal", 3), ("External", 3), ("External", 3), ("Hardpoint", 2)]),
    ("explorer", "Explorateur Magellan", "explorateur", 8800, 1300, 260000, 3.2e5, 1.6, 2.1, [("Core", 3), ("Core", 3), ("Core", 2), ("Internal", 3), ("Internal", 3), ("Internal", 2), ("Internal", 2), ("External", 3), ("External", 2), ("Hardpoint", 1)]),
    ("fighter", "Chasseur Vespa", "combat", 4200, 900, 180000, 3.6e5, 2.6, 3.6, [("Core", 2), ("Core", 2), ("Core", 1), ("Internal", 1), ("External", 2), ("Hardpoint", 2), ("Hardpoint", 2)]),
    ("corvette", "Corvette Aegis", "combat", 14000, 3200, 640000, 5.5e5, 1.3, 1.8, [("Core", 3), ("Core", 3), ("Core", 2), ("Internal", 3), ("Internal", 2), ("Internal", 2), ("External", 3), ("External", 2), ("Hardpoint", 3), ("Hardpoint", 2), ("Hardpoint", 2)]),
    ("frigate", "Fregate Orion", "combat", 38000, 8500, 1900000, 1.0e6, 0.8, 1.0, [("Core", 4), ("Core", 4), ("Core", 3), ("Core", 2), ("Internal", 4), ("Internal", 4), ("Internal", 3), ("Internal", 3), ("Internal", 2), ("External", 4), ("External", 3), ("Hardpoint", 4), ("Hardpoint", 3), ("Hardpoint", 3)]),
    ("survey", "Releveur Kepler", "science", 7000, 900, 210000, 2.4e5, 1.7, 2.2, [("Core", 2), ("Core", 2), ("Internal", 3), ("Internal", 3), ("Internal", 2), ("External", 3), ("External", 3), ("Hardpoint", 1)]),
    ("tender", "Ravitailleur Atlas-R", "support", 12500, 2000, 380000, 4.0e5, 1.0, 1.4, [("Core", 3), ("Core", 2), ("Internal", 4), ("Internal", 4), ("Internal", 3), ("Internal", 2), ("External", 3), ("External", 2), ("Hardpoint", 2)]),
    ("colony", "Vaisseau colonial Exode", "colonial", 52000, 11000, 2600000, 1.2e6, 0.6, 0.7, [("Core", 4), ("Core", 4), ("Core", 3), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("Internal", 3), ("Internal", 3), ("External", 4), ("External", 3), ("Hardpoint", 2)]),
    ("stationkit", "Noyau de station", "station", 90000, 20000, 4800000, 2.0e6, 0.3, 0.3, [("Core", 4), ("Core", 4), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("Internal", 4), ("External", 4), ("External", 4), ("Hardpoint", 4), ("Hardpoint", 4)]),
]
with open(os.path.join(OUT, "hulls.ron"), "w", encoding="utf-8") as f:
    f.write("// GENERATED by tools/gen_content.py\n[\n")
    for (id, name, cls, mass, hp, cost, rcs, rate, accel, slots) in hulls:
        sl = ", ".join(f"({t},{s})" for t, s in slots)
        f.write(f' (id:"{id}", name:"{name}", class:"{cls}", mass:{float(mass)!r}, hp:{float(hp)!r}, cost:{float(cost)!r}, rcs:{rcs!r}, max_rate:{rate!r}, ang_accel:{accel!r}, slots:[{sl}]),\n')
    f.write("]\n")

print(len(modules), "modules,", len(hulls), "hulls")
