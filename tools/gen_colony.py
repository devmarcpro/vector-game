#!/usr/bin/env python3
"""Generates data/recipes.ron, data/buildings.ron, data/colony_policies.ron and appends the
intermediate goods to data/resources.ron (idempotent).  Run: python tools/gen_colony.py"""
import os, re

OUT = os.path.join(os.path.dirname(__file__), "..", "data")

# ---- intermediate goods: id, name, category, density kg/m3, value cr/kg ----
NEW_RESOURCES = [
    ("steel", "Acier", "alloy", 7850, 1.2),
    ("aluminium_alloy", "Alliage d'aluminium", "alloy", 2800, 2.5),
    ("titanium_alloy", "Alliage de titane", "alloy", 4500, 9.0),
    ("copper_wire", "Fil de cuivre", "component", 8900, 2.5),
    ("circuits", "Circuits", "component", 1800, 40.0),
    ("electronics", "Electronique", "component", 1500, 60.0),
    ("polymers", "Polymeres", "chemical", 1100, 2.0),
    ("plastics", "Plastiques", "chemical", 1050, 1.5),
    ("glass", "Verre", "material", 2500, 0.6),
    ("ceramics", "Ceramiques", "material", 3000, 1.2),
    ("construction_materials", "Materiaux de construction", "material", 1800, 1.0),
    ("solar_cells", "Cellules solaires", "component", 2300, 30.0),
    ("batteries", "Batteries", "component", 2200, 20.0),
    ("fertilizer", "Engrais", "chemical", 1100, 0.8),
    ("machinery", "Machines-outils", "component", 2500, 25.0),
    ("tools", "Outillage", "component", 7000, 15.0),
    ("hull_plates", "Plaques de coque", "component", 5000, 8.0),
    ("engine_parts", "Pieces de moteur", "component", 4000, 35.0),
    ("reactor_parts", "Pieces de reacteur", "component", 6000, 60.0),
    ("life_support_parts", "Pieces de support de vie", "component", 1500, 45.0),
    ("textiles", "Textiles", "consumer", 300, 5.0),
    ("consumer_goods", "Biens de consommation", "consumer", 600, 6.0),
    ("luxury_goods", "Produits de luxe", "consumer", 500, 40.0),
    ("robotics", "Robotique", "component", 2000, 120.0),
    ("nanomaterials", "Nanomateriaux", "material", 1800, 200.0),
    ("fuel_rods", "Barres de combustible", "nuclear", 10000, 150.0),
    ("colonists", "Colons (hibernation)", "people", 400, 8.0),
    ("research_data", "Donnees de recherche", "data", 100, 50.0),
    ("weapon_parts", "Pieces d'armement", "component", 5000, 50.0),
    ("ammo_slugs", "Munitions cinetiques", "ammo", 7800, 6.0),
    ("missiles", "Missiles", "ammo", 1500, 120.0),
    ("mines", "Mines", "ammo", 2500, 90.0),
    ("artifacts", "Artefacts anciens", "artifact", 3000, 900.0),
]

def sync_resources():
    p = os.path.join(OUT, "resources.ron")
    s = open(p, encoding="utf-8").read()
    add = ""
    for rid, name, cat, dens, val in NEW_RESOURCES:
        if f'id:"{rid}"' in s:
            continue
        add += f' (id:"{rid}", name:"{name}", category:"{cat}", density:{float(dens)!r}, value:{float(val)!r}),\n'
    if add:
        s = s.rstrip()
        assert s.endswith("]")
        s = s[:-1].rstrip() + "\n" + add + "]\n"
        open(p, "w", encoding="utf-8").write(s)

# ---- recipes: id, name, kind, inputs, outputs, hours per batch, energy kWh per batch ----
# kind "extract": outputs depend on the site's deposits (kg_out is the ore mass per batch).
# kind "data": information output, exempt from mass conservation.
R = []
def recipe(id, name, kind, ins, outs, hours=24.0, kwh=0.0):
    R.append((id, name, kind, ins, outs, hours, kwh))

recipe("mine_ore", "Extraction de minerai", "extract", [], [("ore", 3000.0)], 24, 4000)
recipe("extract_volatiles", "Extraction de volatils", "extract", [], [("volatiles", 3000.0)], 24, 3000)
recipe("scoop_gas", "Ecopage de gaz", "extract", [], [("gas", 4000.0)], 24, 5000)
recipe("excavate_regolith", "Terrassement de regolithe", "extract", [], [("regolith", 6000.0)], 24, 1500)
recipe("smelt_steel", "Acierie", "process", [("iron", 1000), ("carbon", 80)], [("steel", 1000)], 24, 3000)
recipe("alloy_aluminium", "Alliage d'aluminium", "process", [("aluminium", 900), ("magnesium", 50), ("copper", 40)], [("aluminium_alloy", 950)], 24, 2500)
recipe("alloy_titanium", "Alliage de titane", "process", [("titanium", 900), ("aluminium", 60), ("manganese", 20)], [("titanium_alloy", 950)], 24, 5000)
recipe("draw_wire", "Tréfilage du cuivre", "process", [("copper", 500)], [("copper_wire", 490)], 12, 600)
recipe("melt_ice", "Fonte de la glace", "process", [("water_ice", 1000)], [("water", 1000)], 12, 100)
recipe("make_glass", "Verrerie", "process", [("silica", 1000)], [("glass", 950)], 24, 2000)
recipe("fire_ceramics", "Ceramiques", "process", [("silica", 600), ("aluminium", 100)], [("ceramics", 650)], 24, 1800)
recipe("make_concrete", "Materiaux de construction", "process", [("regolith", 1500), ("silica", 500), ("water", 100)], [("construction_materials", 1900)], 24, 800)
recipe("make_plastics", "Petrochimie : plastiques", "process", [("methane", 400), ("carbon", 100)], [("plastics", 450)], 24, 1500)
recipe("make_polymers", "Polymeres", "process", [("ammonia", 300), ("carbon", 150), ("hydrogen", 20)], [("polymers", 420)], 24, 1800)
recipe("make_fertilizer", "Engrais", "process", [("ammonia", 300), ("phosphorus", 50)], [("fertilizer", 320)], 24, 600)
recipe("make_fuel", "Carburant de fusion", "process", [("hydrogen", 596), ("deuterium", 4)], [("fuel", 600)], 24, 1500)
recipe("electrolysis", "Electrolyse de l'eau", "process", [("water", 900)], [("oxygen", 800), ("hydrogen", 100)], 12, 3000)
recipe("fab_circuits", "Circuits imprimes", "process", [("silicon", 80), ("copper_wire", 50), ("plastics", 40), ("gold", 0.5)], [("circuits", 150)], 12, 1200)
recipe("assemble_electronics", "Electronique", "process", [("circuits", 100), ("polymers", 40), ("aluminium_alloy", 40)], [("electronics", 170)], 12, 900)
recipe("make_solar_cells", "Cellules solaires", "process", [("silicon", 120), ("glass", 60), ("copper_wire", 15)], [("solar_cells", 190)], 24, 2500)
recipe("make_batteries", "Batteries", "process", [("lithium", 40), ("cobalt", 20), ("aluminium", 40), ("polymers", 20)], [("batteries", 110)], 24, 1200)
recipe("build_machinery", "Machines-outils", "process", [("steel", 500), ("electronics", 60)], [("machinery", 540)], 24, 1500)
recipe("forge_tools", "Outillage", "process", [("steel", 200), ("carbon", 10)], [("tools", 200)], 12, 500)
recipe("press_hull_plates", "Plaques de coque", "process", [("titanium_alloy", 600), ("steel", 300)], [("hull_plates", 880)], 24, 4000)
recipe("build_engine_parts", "Pieces de moteur", "process", [("titanium_alloy", 300), ("machinery", 200), ("electronics", 50)], [("engine_parts", 520)], 24, 3000)
recipe("build_reactor_parts", "Pieces de reacteur", "process", [("tungsten", 100), ("steel", 300), ("electronics", 80), ("ceramics", 100)], [("reactor_parts", 560)], 24, 3500)
recipe("build_life_support", "Pieces de support de vie", "process", [("machinery", 100), ("polymers", 60), ("electronics", 60), ("glass", 30)], [("life_support_parts", 230)], 24, 1800)
recipe("grow_hydroponics", "Hydroponie", "grow", [("water", 800), ("fertilizer", 30), ("co2_ice", 50)], [("food", 300)], 24, 1500)
recipe("grow_algae", "Cuves d'algues", "grow", [("water", 500), ("ammonia", 20), ("co2_ice", 40)], [("food", 200)], 24, 900)
recipe("weave_textiles", "Textiles", "process", [("polymers", 200)], [("textiles", 190)], 12, 400)
recipe("make_consumer_goods", "Biens de consommation", "process", [("plastics", 100), ("textiles", 50), ("electronics", 20)], [("consumer_goods", 160)], 12, 500)
recipe("make_luxury_goods", "Produits de luxe", "process", [("gold", 2), ("electronics", 30), ("textiles", 30), ("diamond", 1)], [("luxury_goods", 60)], 24, 800)
recipe("build_robotics", "Robotique", "process", [("machinery", 200), ("electronics", 150), ("batteries", 50)], [("robotics", 400)], 24, 2500)
recipe("make_nanomaterials", "Nanomateriaux", "process", [("graphite", 100), ("carbon", 100), ("polymers", 50)], [("nanomaterials", 240)], 72, 6000)
recipe("make_fuel_rods", "Barres de combustible", "process", [("uranium", 20), ("steel", 150), ("ceramics", 60)], [("fuel_rods", 220)], 60, 2500)
recipe("make_medicine", "Pharmacie", "process", [("organics", 100), ("fertilizer", 10), ("water", 100)], [("medicine", 55)], 24, 800)
recipe("make_spare_parts", "Pieces detachees", "process", [("steel", 200), ("machinery", 40), ("polymers", 40)], [("spare_parts", 270)], 12, 600)
recipe("make_weapon_parts", "Pieces d'armement", "process", [("steel", 300), ("titanium_alloy", 100), ("electronics", 60)], [("weapon_parts", 450)], 24, 2500)
recipe("make_slugs", "Munitions cinetiques", "process", [("steel", 120), ("tungsten", 5)], [("ammo_slugs", 100)], 12, 400)
recipe("make_missiles", "Missiles", "process", [("weapon_parts", 60), ("electronics", 20), ("fuel", 20)], [("missiles", 95)], 24, 1500)
recipe("make_mines", "Mines", "process", [("weapon_parts", 40), ("electronics", 10), ("steel", 30)], [("mines", 75)], 24, 900)
recipe("do_research", "Recherche appliquee", "data", [("electronics", 20)], [("research_data", 25)], 24, 3000)
recipe("recycle_scrap", "Recyclage", "process", [("spare_parts", 100)], [("steel", 70), ("polymers", 20)], 12, 300)

def emit_recipes():
    lines = ["// GENERATED by tools/gen_colony.py. kind: extract | process | grow | data.",
             "// Mass is conserved (outputs <= inputs) for process/grow; extraction takes from site deposits.", "["]
    for id, name, kind, ins, outs, hours, kwh in R:
        i = ", ".join(f'("{a}",{float(b)!r})' for a, b in ins)
        o = ", ".join(f'("{a}",{float(b)!r})' for a, b in outs)
        lines.append(f' (id:"{id}", name:"{name}", kind:"{kind}", inputs:[{i}], outputs:[{o}], hours:{float(hours)!r}, energy_kwh:{float(kwh)!r}),')
    lines.append("]")
    open(os.path.join(OUT, "recipes.ron"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

# ---- buildings ----
B = []
def bld(id, name, cat, cost, days, power, workers, housing=0, storage=0, upkeep=10.0, recipes=(), throughput=1.0, sites=("any",), **kw):
    B.append(dict(id=id, name=name, category=cat, cost=cost, days=days, power=power, workers=workers, housing=housing,
                  storage=storage, upkeep=upkeep, recipes=list(recipes), throughput=throughput, sites=list(sites), **kw))

CM = "construction_materials"
bld("habitat_dome", "Dome d'habitation", "housing", [(CM, 1500), ("glass", 300), ("life_support_parts", 40)], 8, -60, 4, housing=60, upkeep=15, sites=("rocky", "icy", "asteroid"))
bld("residential_block", "Immeuble residentiel", "housing", [(CM, 2500), ("glass", 200), ("steel", 300)], 12, -120, 6, housing=250, upkeep=30, sites=("habitable",))
bld("arcology", "Arcologie", "housing", [(CM, 12000), ("steel", 3000), ("life_support_parts", 200), ("electronics", 200)], 40, -900, 40, housing=1500, upkeep=200, sites=("rocky", "habitable"))
bld("solar_farm", "Centrale solaire", "power", [(CM, 600), ("solar_cells", 120)], 6, 2500, 2, upkeep=8, sites=("rocky", "icy", "asteroid"))
bld("fission_plant", "Centrale a fission", "power", [(CM, 1500), ("reactor_parts", 80), ("steel", 400)], 14, 9000, 8, upkeep=60, sites=("rocky", "icy", "asteroid"), burns=[("fuel_rods", 4.0)])
bld("fusion_plant", "Centrale a fusion", "power", [(CM, 3000), ("reactor_parts", 200), ("steel", 800), ("electronics", 120)], 24, 40000, 14, upkeep=150, sites=("rocky", "icy", "asteroid", "gas_giant"), burns=[("deuterium", 6.0)])
bld("geothermal_plant", "Centrale geothermique", "power", [(CM, 1800), ("steel", 500), ("machinery", 60)], 14, 6000, 6, upkeep=40, sites=("rocky",))
bld("surface_mine", "Mine de surface", "extraction", [(CM, 1200), ("machinery", 80), ("steel", 300)], 10, -4000, 18, storage=20000, upkeep=40, recipes=["mine_ore"], throughput=1.0, sites=("rocky", "asteroid"))
bld("deep_drill", "Forage profond", "extraction", [(CM, 2600), ("machinery", 160), ("steel", 700), ("robotics", 20)], 20, -9000, 30, storage=40000, upkeep=110, recipes=["mine_ore"], throughput=2.6, sites=("rocky",))
bld("ice_extractor", "Extracteur de glace", "extraction", [(CM, 1000), ("machinery", 60), ("steel", 250)], 9, -3000, 12, storage=20000, upkeep=30, recipes=["extract_volatiles"], sites=("icy", "asteroid"))
bld("gas_scoop", "Ecope orbitale", "extraction", [(CM, 2200), ("machinery", 120), ("steel", 600), ("hull_plates", 100)], 18, -6000, 16, storage=30000, upkeep=80, recipes=["scoop_gas"], sites=("gas_giant",))
bld("regolith_plant", "Usine de regolithe", "extraction", [(CM, 600), ("machinery", 40)], 5, -1500, 6, upkeep=20, recipes=["excavate_regolith"], sites=("rocky", "icy", "asteroid"))
bld("water_plant", "Usine de traitement d'eau", "industry", [(CM, 700), ("machinery", 50), ("polymers", 40)], 6, -1200, 6, upkeep=20, recipes=["melt_ice", "electrolysis"], throughput=2.0, sites=("rocky", "icy", "asteroid"))
bld("hydroponics", "Serres hydroponiques", "agriculture", [(CM, 800), ("glass", 400), ("machinery", 30)], 7, -1500, 10, upkeep=25, recipes=["grow_hydroponics", "grow_algae"], throughput=1.0, sites=("rocky", "icy", "asteroid"))
bld("algae_vats", "Cuves d'algues", "agriculture", [(CM, 700), ("polymers", 150), ("machinery", 30)], 6, -900, 6, upkeep=18, recipes=["grow_algae"], throughput=1.6, sites=("rocky", "icy", "asteroid"))
bld("smelter", "Fonderie", "industry", [(CM, 1600), ("machinery", 120), ("ceramics", 100)], 12, -3000, 14, upkeep=45, recipes=["smelt_steel", "alloy_aluminium", "alloy_titanium", "draw_wire"], throughput=1.0, sites=("rocky", "icy", "asteroid"))
bld("glassworks", "Verrerie-ceramique", "industry", [(CM, 900), ("machinery", 60)], 8, -1800, 8, upkeep=25, recipes=["make_glass", "fire_ceramics", "make_concrete"], sites=("rocky", "icy", "asteroid"))
bld("chemical_plant", "Usine chimique", "industry", [(CM, 1800), ("machinery", 140), ("steel", 300)], 14, -3500, 16, upkeep=55, recipes=["make_plastics", "make_polymers", "make_fertilizer", "make_fuel", "electrolysis", "make_medicine"], sites=("rocky", "icy", "asteroid"))
bld("component_factory", "Usine de composants", "industry", [(CM, 2200), ("machinery", 200), ("electronics", 60)], 16, -4500, 24, upkeep=75, recipes=["build_machinery", "forge_tools", "make_spare_parts", "press_hull_plates", "build_engine_parts", "build_reactor_parts", "build_life_support", "make_weapon_parts", "make_slugs", "make_missiles", "make_mines"], sites=("rocky", "icy", "asteroid"))
bld("electronics_fab", "Fabrique d'electronique", "industry", [(CM, 2000), ("machinery", 180), ("robotics", 20)], 16, -5000, 24, upkeep=90, recipes=["fab_circuits", "assemble_electronics", "make_solar_cells", "make_batteries", "build_robotics", "make_nanomaterials", "make_fuel_rods"], sites=("rocky", "icy", "asteroid"))
bld("consumer_factory", "Usine de biens de consommation", "industry", [(CM, 1400), ("machinery", 100)], 10, -1800, 14, upkeep=35, recipes=["weave_textiles", "make_consumer_goods", "make_luxury_goods"], sites=("rocky", "icy", "asteroid"))
bld("recycler", "Centre de recyclage", "industry", [(CM, 700), ("machinery", 50)], 6, -800, 6, upkeep=15, recipes=["recycle_scrap"], sites=("rocky", "icy", "asteroid"))
bld("research_lab", "Laboratoire de recherche", "science", [(CM, 1500), ("electronics", 150), ("machinery", 60)], 14, -3000, 20, upkeep=70, recipes=["do_research"], sites=("rocky", "icy", "asteroid"), research=1.0)
bld("medical_center", "Centre medical", "service", [(CM, 1000), ("electronics", 60), ("glass", 100)], 10, -400, 12, upkeep=40, sites=("rocky", "icy", "asteroid"), medical=1.0)
bld("school", "Ecole technique", "service", [(CM, 800), ("glass", 80)], 8, -200, 10, upkeep=25, sites=("rocky", "icy", "asteroid"), education=1.0)
bld("recreation_center", "Centre de loisirs", "service", [(CM, 900), ("glass", 150), ("textiles", 40)], 8, -300, 8, upkeep=30, sites=("rocky", "icy", "asteroid"), recreation=1.0)
bld("police_station", "Poste de police", "service", [(CM, 600), ("electronics", 30)], 6, -150, 8, upkeep=30, sites=("rocky", "icy", "asteroid"), security=1.0)
bld("life_support_plant", "Centrale de support de vie", "service", [(CM, 1200), ("life_support_parts", 60), ("machinery", 40)], 9, -700, 8, upkeep=35, sites=("rocky", "icy", "asteroid"), recycling=1.0)
bld("warehouse", "Entrepot", "storage", [(CM, 700), ("steel", 150)], 5, -50, 3, storage=250000, upkeep=12, sites=("rocky", "icy", "asteroid"))
bld("spaceport", "Astroport", "spaceport", [(CM, 1800), ("steel", 500), ("electronics", 40)], 12, -800, 10, storage=60000, upkeep=60, sites=("rocky", "icy", "asteroid"), port=1.0)
bld("terraformer", "Terraformeur atmospherique", "terraform", [(CM, 8000), ("reactor_parts", 150), ("machinery", 300), ("nanomaterials", 40)], 60, -20000, 40, upkeep=300, sites=("rocky",), terraform=0.004)
bld("defense_turret", "Tourelle de defense", "defense", [(CM, 700), ("weapon_parts", 60), ("electronics", 30)], 8, -500, 6, upkeep=30, sites=("rocky", "icy", "asteroid"), defense=1.0)

def lst(pairs):
    return "[" + ", ".join(f'("{a}",{float(b)!r})' for a, b in pairs) + "]"

def emit_buildings():
    lines = ["// GENERATED by tools/gen_colony.py. power in kW (+produced, -consumed); sites: any-of tags the body must have.", "["]
    for b in B:
        extra = ""
        for k in ("medical", "education", "recreation", "security", "recycling", "research", "port", "terraform", "defense"):
            if k in b:
                extra += f", {k}:{float(b[k])!r}"
        if "burns" in b:
            extra += f", burns:{lst(b['burns'])}"
        sites = "[" + ", ".join(f'"{x}"' for x in b["sites"]) + "]"
        recipes = "[" + ", ".join(f'"{x}"' for x in b["recipes"]) + "]"
        lines.append(
            f' (id:"{b["id"]}", name:"{b["name"]}", category:"{b["category"]}", cost:{lst(b["cost"])}, days:{float(b["days"])!r}, power:{float(b["power"])!r}, '
            f'workers:{float(b["workers"])!r}, housing:{float(b["housing"])!r}, storage:{float(b["storage"])!r}, upkeep:{float(b["upkeep"])!r}, '
            f'recipes:{recipes}, throughput:{float(b["throughput"])!r}, sites:{sites}{extra}),')
    lines.append("]")
    open(os.path.join(OUT, "buildings.ron"), "w", encoding="utf-8").write("\n".join(lines) + "\n")

POLICIES = """// Colony policies: each offers levels with effects (tax: credits/person/day; productivity and happiness multipliers).
[
 (id:"tax", name:"Fiscalite", levels:[("Aucune",0.0,0.00,0.03),("Legere",2.0,0.0,0.0),("Normale",5.0,0.0,-0.04),("Lourde",9.0,0.0,-0.10)], default_level:2),
 (id:"work", name:"Temps de travail", levels:[("Reduit",0.0,-0.10,0.05),("Normal",0.0,0.0,0.0),("Heures supp.",0.0,0.20,-0.08)], default_level:1),
 (id:"rations", name:"Rationnement", levels:[("Genereux",0.0,0.0,0.05),("Normal",0.0,0.0,0.0),("Strict",0.0,0.0,-0.06)], default_level:1),
 (id:"immigration", name:"Immigration", levels:[("Fermee",0.0,0.0,0.0),("Selective",0.0,0.0,0.0),("Ouverte",0.0,0.0,-0.02)], default_level:1),
 (id:"security", name:"Maintien de l'ordre", levels:[("Libre",0.0,0.0,0.04),("Normal",0.0,0.0,0.0),("Loi martiale",0.0,0.05,-0.15)], default_level:1),
 (id:"gather", name:"Cueillette et élevage", levels:[("Aucune",0.0,0.0,0.0),("Mesurée",0.0,0.0,0.01),("Intensive",0.0,0.0,0.0),("Surexploitation",0.0,0.0,-0.02)], default_level:0),
 (id:"quarantine", name:"Quarantaine biologique", levels:[("Aucune",0.0,0.0,0.0),("Contrôles",0.0,0.0,-0.01),("Stricte",0.0,0.0,-0.03),("Totale",0.0,-0.05,-0.05)], default_level:1),
]
"""

def validate():
    res = {r[0] for r in NEW_RESOURCES}
    base = set(re.findall(r'id:"([a-z0-9_]+)"', open(os.path.join(OUT, "resources.ron"), encoding="utf-8").read()))
    allres = base | res
    for id, name, kind, ins, outs, hours, kwh in R:
        for a, b in ins:
            assert a in allres, (id, a)
        if kind in ("process", "grow"):
            for a, b in outs:
                assert a in allres, (id, a)
            assert sum(b for _, b in outs) <= sum(b for _, b in ins) + 1e-9, f"mass not conserved in {id}"
    rids = {r[0] for r in R}
    for b in B:
        for r in b["recipes"]:
            assert r in rids, (b["id"], r)
        for a, _ in b["cost"]:
            assert a in allres, (b["id"], a)

if __name__ == "__main__":
    sync_resources()
    validate()
    emit_recipes()
    emit_buildings()
    open(os.path.join(OUT, "colony_policies.ron"), "w", encoding="utf-8").write(POLICIES)
    print(len(R), "recipes,", len(B), "buildings")
