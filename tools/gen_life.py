#!/usr/bin/env python3
"""Generate data/species.ron: the flora, fauna and microbes of solid worlds (E2).

Run after gen_content/gen_colony/gen_science/gen_scale and before accents.py is not required:
names are written with their accents here. The data crate validates the result (habitats are
biomes, diets are species, and every non-producer has food in each of its habitats).
"""
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "data")

SEA = ["ocean", "ocean_profond"]
COLD = ["toundra", "calotte"]
GRASS = ["steppe", "prairie", "savane"]
WOOD = ["foret_temperee", "taiga"]
WET = ["jungle", "marais"]

# id, name, trophic, habitat, diet, body kg, growth/day, yields, domestic, danger, invasive, text
SPECIES = [
    # ---- producers: algae, plants, fungi, microbes
    ("plancton", "Plancton", "producer", SEA + ["mer_gelee"], [], 0.001, 0.30, [("organics", 0.2)], False, 0.0, 0.2, "Base de toute chaîne marine."),
    ("algues_marines", "Algues marines", "producer", ["ocean"], [], 0.5, 0.15, [("food", 0.6), ("fertilizer", 0.3)], True, 0.0, 0.3, "Prairies sous-marines récoltables."),
    ("varech", "Varech géant", "producer", ["ocean"], [], 30.0, 0.10, [("organics", 4.0), ("fertilizer", 1.0)], False, 0.0, 0.1, "Forêts flottantes de varech."),
    ("lichen_polaire", "Lichen polaire", "producer", COLD, [], 0.02, 0.05, [("organics", 0.1)], False, 0.0, 0.1, "Pousse sur la roche gelée."),
    ("mousse_des_toundras", "Mousse des toundras", "producer", ["toundra", "taiga"], [], 0.05, 0.07, [("medicine", 0.02), ("organics", 0.2)], False, 0.0, 0.1, "Tapis de mousse aux vertus antiseptiques."),
    ("herbe_rase", "Herbe rase", "producer", ["steppe", "prairie", "toundra", "montagne"], [], 0.05, 0.18, [("organics", 0.3)], False, 0.0, 0.5, "Fourrage des grandes plaines."),
    ("herbe_haute", "Herbe haute", "producer", ["savane", "prairie", "marais"], [], 0.2, 0.15, [("organics", 0.6)], False, 0.0, 0.5, "Herbes de plus de deux mètres."),
    ("ble_sauvage", "Blé sauvage", "producer", ["prairie", "steppe"], [], 0.1, 0.12, [("food", 0.8)], True, 0.0, 0.2, "Ancêtre sauvage des céréales."),
    ("tubercule", "Tubercule des steppes", "producer", ["steppe", "taiga", "prairie"], [], 0.8, 0.09, [("food", 1.5)], True, 0.0, 0.1, "Racine nourrissante."),
    ("conifere", "Conifère boréal", "producer", ["taiga"], [], 800.0, 0.04, [("organics", 120.0), ("construction_materials", 30.0)], False, 0.0, 0.3, "Forêts de conifères."),
    ("chene", "Chêne", "producer", ["foret_temperee"], [], 1500.0, 0.03, [("organics", 250.0), ("construction_materials", 60.0)], False, 0.0, 0.2, "Feuillu lent, bois noble."),
    ("bouleau", "Bouleau", "producer", ["foret_temperee", "taiga"], [], 300.0, 0.06, [("organics", 60.0)], False, 0.0, 0.5, "Pionnier des clairières."),
    ("fougere_geante", "Fougère géante", "producer", ["foret_temperee", "jungle", "marais"], [], 20.0, 0.12, [("medicine", 0.05), ("organics", 5.0)], False, 0.0, 0.4, "Fronde de trois mètres."),
    ("liane", "Liane étrangleuse", "producer", ["jungle"], [], 15.0, 0.20, [("textiles", 2.0), ("organics", 3.0)], False, 0.1, 0.8, "Étouffe les arbres qui la portent."),
    ("arbre_geant", "Arbre géant", "producer", ["jungle"], [], 5000.0, 0.03, [("organics", 900.0), ("construction_materials", 200.0)], False, 0.0, 0.2, "Cathédrale verte."),
    ("bambou", "Bambou", "producer", ["jungle", "marais"], [], 40.0, 0.25, [("organics", 20.0), ("construction_materials", 8.0)], True, 0.0, 0.7, "Pousse d'un mètre par jour."),
    ("roseau", "Roseau", "producer", ["marais"], [], 0.5, 0.22, [("textiles", 0.3), ("food", 0.1)], False, 0.0, 0.4, "Roselières des marais."),
    ("cactus", "Cactus à eau", "producer", ["desert", "savane"], [], 20.0, 0.03, [("food", 2.0), ("medicine", 0.05)], False, 0.1, 0.1, "Réserve d'eau et de pulpe."),
    ("arbuste_epineux", "Arbuste épineux", "producer", ["desert", "savane", "steppe"], [], 6.0, 0.05, [("organics", 2.0)], False, 0.1, 0.4, "Survit à la sécheresse."),
    ("fleur_medicinale", "Fleur médicinale", "producer", ["prairie", "foret_temperee", "montagne"], [], 0.05, 0.10, [("medicine", 0.08)], True, 0.0, 0.1, "Base de remèdes courants."),
    ("cotonnier", "Cotonnier", "producer", ["savane", "prairie"], [], 2.0, 0.10, [("textiles", 1.2)], True, 0.0, 0.2, "Fibre textile."),
    ("champignon", "Champignons", "producer", ["foret_temperee", "taiga", "jungle", "marais"], [], 0.3, 0.20, [("food", 0.3), ("medicine", 0.03)], True, 0.1, 0.5, "Décomposeurs du sous-bois."),
    ("microbes_thermophiles", "Microbes thermophiles", "producer", ["plaine_brulante"], [], 0.0001, 0.20, [("organics", 0.05)], False, 0.0, 0.0, "Vie de roche chauffée à blanc."),
    ("microbes_glaciaires", "Microbes glaciaires", "producer", ["calotte", "mer_gelee"], [], 0.0001, 0.05, [("organics", 0.03)], False, 0.0, 0.0, "Vie dans la glace."),
    ("tapis_bacterien", "Tapis bactérien", "producer", ["mer_bouillante", "marais"], [], 0.01, 0.25, [("organics", 0.1)], False, 0.0, 0.3, "Colonies en eaux chaudes."),
    # ---- herbivores
    ("krill", "Krill", "herbivore", SEA + ["mer_gelee"], ["plancton"], 0.001, 0.12, [("food", 0.002)], False, 0.0, 0.2, "Nuées de crustacés."),
    ("poisson_argente", "Poisson argenté", "herbivore", ["ocean"], ["plancton", "algues_marines"], 0.8, 0.06, [("food", 0.6)], True, 0.0, 0.2, "Bancs de poissons de pêche."),
    ("raie_geante", "Raie géante", "omnivore", ["ocean_profond", "ocean"], ["plancton", "krill"], 200.0, 0.02, [("food", 90.0)], False, 0.1, 0.0, "Filtreur des abysses."),
    ("morse_des_glaces", "Morse des glaces", "omnivore", ["mer_gelee", "calotte"], ["krill", "microbes_glaciaires"], 700.0, 0.015, [("food", 300.0), ("textiles", 20.0)], False, 0.2, 0.0, "Colonies sur la banquise."),
    ("lagomorphe", "Lagomorphe des steppes", "herbivore", ["steppe", "prairie", "toundra"], ["herbe_rase", "tubercule"], 2.0, 0.07, [("food", 1.2), ("textiles", 0.1)], True, 0.0, 0.8, "Se reproduit très vite."),
    ("bovide", "Bovidé des plaines", "herbivore", ["prairie", "savane", "steppe"], ["herbe_rase", "herbe_haute"], 450.0, 0.02, [("food", 220.0), ("textiles", 15.0)], True, 0.2, 0.1, "Grands troupeaux."),
    ("cerf", "Cerf des forêts", "herbivore", ["foret_temperee", "taiga"], ["mousse_des_toundras", "bouleau", "herbe_rase"], 150.0, 0.03, [("food", 80.0), ("textiles", 6.0)], False, 0.1, 0.1, "Gibier de forêt."),
    ("elan_des_neiges", "Élan des neiges", "herbivore", ["taiga", "toundra"], ["mousse_des_toundras", "lichen_polaire", "bouleau"], 400.0, 0.02, [("food", 200.0), ("textiles", 10.0)], False, 0.3, 0.0, "Grand cervidé boréal."),
    ("mouton_laineux", "Mouton laineux", "herbivore", ["steppe", "montagne", "prairie"], ["herbe_rase"], 70.0, 0.04, [("food", 35.0), ("textiles", 4.0)], True, 0.0, 0.2, "Laine épaisse."),
    ("chevre_de_montagne", "Chèvre de montagne", "herbivore", ["montagne"], ["fleur_medicinale", "arbuste_epineux", "lichen_polaire"], 60.0, 0.03, [("food", 30.0)], True, 0.1, 0.1, "Bondit sur les à-pics."),
    ("gazelle", "Gazelle des dunes", "herbivore", ["desert", "savane"], ["arbuste_epineux", "herbe_haute", "cactus"], 40.0, 0.05, [("food", 20.0)], False, 0.0, 0.1, "Rapide, farouche."),
    ("rongeur_des_marais", "Rongeur des marais", "herbivore", ["marais"], ["roseau", "herbe_haute"], 8.0, 0.09, [("food", 4.0)], False, 0.0, 0.7, "Prolifère dans les roselières."),
    ("tortue_geante", "Tortue géante", "herbivore", ["marais", "jungle"], ["roseau", "fougere_geante", "champignon"], 300.0, 0.01, [("food", 110.0)], False, 0.0, 0.0, "Vit des siècles."),
    ("singe_arboricole", "Singe arboricole", "herbivore", ["jungle", "foret_temperee"], ["liane", "arbre_geant", "champignon"], 15.0, 0.04, [("food", 6.0)], False, 0.1, 0.2, "Troupes bruyantes."),
    ("pollinisateur", "Insecte pollinisateur", "herbivore", ["prairie", "jungle", "foret_temperee", "savane"], ["fleur_medicinale", "herbe_haute", "liane"], 0.001, 0.20, [("food", 0.001)], True, 0.0, 0.1, "Sans lui, les cultures donnent moins."),
    ("criquet", "Criquet migrateur", "herbivore", ["savane", "desert", "steppe"], ["herbe_haute", "arbuste_epineux", "herbe_rase"], 0.002, 0.25, [("food", 0.002)], False, 0.2, 1.0, "En nuées, ravage les cultures."),
    ("escargot_geant", "Escargot géant", "herbivore", ["jungle", "marais"], ["champignon", "fougere_geante"], 3.0, 0.05, [("food", 1.5)], True, 0.0, 0.5, "Délicat à cuisiner."),
    ("manchot", "Manchot des glaces", "carnivore", ["mer_gelee"], ["krill"], 30.0, 0.02, [("food", 12.0)], False, 0.0, 0.0, "Colonies serrées."),
    ("ver_des_roches", "Ver des roches", "herbivore", ["plaine_brulante"], ["microbes_thermophiles"], 1.0, 0.04, [("organics", 0.2)], False, 0.1, 0.0, "Broute les microbes sur la roche."),
    # ---- carnivores and omnivores
    ("requin_des_abysses", "Requin des abysses", "carnivore", ["ocean_profond", "ocean"], ["poisson_argente", "raie_geante", "krill"], 600.0, 0.01, [("food", 250.0)], False, 0.8, 0.0, "Prédateur au sommet de la chaîne marine."),
    ("cetace_chasseur", "Cétacé chasseur", "carnivore", ["ocean", "mer_gelee"], ["poisson_argente", "morse_des_glaces", "manchot"], 4000.0, 0.006, [("food", 1500.0)], False, 0.7, 0.0, "Chasse en groupe."),
    ("ours_des_glaces", "Ours des glaces", "carnivore", ["calotte", "toundra", "mer_gelee"], ["morse_des_glaces", "manchot", "lagomorphe"], 500.0, 0.008, [("food", 200.0), ("textiles", 30.0)], False, 0.9, 0.0, "Seigneur de la banquise."),
    ("loup_des_steppes", "Loup des steppes", "carnivore", ["steppe", "taiga", "toundra", "prairie"], ["lagomorphe", "cerf", "elan_des_neiges", "mouton_laineux"], 45.0, 0.015, [("food", 18.0), ("textiles", 3.0)], True, 0.6, 0.1, "Chasse en meute ; domesticable."),
    ("felin_des_savanes", "Félin des savanes", "carnivore", ["savane"], ["gazelle", "bovide"], 150.0, 0.01, [("food", 60.0), ("textiles", 8.0)], False, 0.8, 0.0, "Fauve des hautes herbes."),
    ("felin_des_jungles", "Félin des jungles", "carnivore", ["jungle"], ["singe_arboricole", "tortue_geante", "escargot_geant"], 120.0, 0.01, [("food", 50.0), ("textiles", 6.0)], False, 0.9, 0.0, "Embusqué dans la canopée."),
    ("ours_brun", "Ours brun", "omnivore", ["foret_temperee", "taiga", "montagne"], ["cerf", "tubercule", "champignon", "lagomorphe", "chevre_de_montagne"], 350.0, 0.008, [("food", 150.0), ("textiles", 20.0)], False, 0.8, 0.0, "Omnivore opportuniste."),
    ("lezard_des_dunes", "Lézard des dunes", "carnivore", ["desert"], ["criquet", "gazelle", "rongeur_des_marais"], 12.0, 0.02, [("food", 5.0)], False, 0.3, 0.1, "Guette immobile sous le soleil."),
    ("ver_des_sables", "Ver des sables", "carnivore", ["desert"], ["gazelle", "lezard_des_dunes"], 4000.0, 0.003, [("food", 1000.0)], False, 1.0, 0.0, "Rare, gigantesque, mortel."),
    ("saurien_des_marais", "Saurien des marais", "carnivore", ["marais"], ["rongeur_des_marais", "poisson_argente", "tortue_geante"], 400.0, 0.008, [("food", 150.0), ("textiles", 25.0)], False, 0.9, 0.0, "Embusqué sous l'eau trouble."),
    ("rapace", "Rapace des montagnes", "carnivore", ["montagne", "steppe", "prairie"], ["lagomorphe", "chevre_de_montagne", "rongeur_des_marais"], 6.0, 0.02, [("food", 2.0)], True, 0.3, 0.0, "Dressable pour la chasse."),
    ("charognard", "Charognard des plaines", "omnivore", ["savane", "desert", "steppe", "prairie"], ["gazelle", "bovide", "lagomorphe", "criquet", "arbuste_epineux"], 50.0, 0.025, [("food", 15.0)], False, 0.4, 0.2, "Nettoie ce que les fauves laissent."),
    ("araignee_geante", "Araignée géante", "carnivore", ["jungle", "foret_temperee"], ["singe_arboricole", "pollinisateur", "escargot_geant"], 25.0, 0.03, [("medicine", 0.2)], False, 0.8, 0.1, "Venin utilisé en pharmacie."),
    ("predateur_des_roches", "Prédateur des roches", "carnivore", ["plaine_brulante"], ["ver_des_roches"], 15.0, 0.01, [("organics", 1.0)], False, 0.7, 0.0, "Vie de roche, lente et blindée."),
]

PRODUCER_TAG = "*"


def ron_str(s):
    return '"' + s.replace('"', "'") + '"'


def fl(x):
    return repr(float(x))


def main():
    ids = set()
    lines = [
        "// GENERATED by tools/gen_life.py -- flora, fauna and microbes of solid worlds (E2).",
        "// trophic: producer | herbivore | carnivore | omnivore. habitat: biome ids. diet: species ids.",
        "// body: kg per individual; growth: intrinsic rate per day; yields: kg of a resource per",
        "// individual harvested or hunted; invasive: how aggressively it spreads when introduced.",
        "[",
    ]
    for (sid, name, trophic, habitat, diet, body, growth, yields, domestic, danger, invasive, text) in SPECIES:
        assert sid not in ids, sid
        ids.add(sid)
        y = ", ".join(f'("{r}",{fl(k)})' for r, k in yields)
        h = ", ".join(f'"{b}"' for b in habitat)
        d = ", ".join(f'"{x}"' for x in diet)
        lines.append(
            f' (id:"{sid}", name:{ron_str(name)}, trophic:"{trophic}", habitat:[{h}], diet:[{d}], '
            f"body:{fl(body)}, growth:{fl(growth)}, yields:[{y}], domestic:{str(domestic).lower()}, "
            f"danger:{fl(danger)}, invasive:{fl(invasive)}, text:{ron_str(text)}),"
        )
    lines.append("]")
    with open(os.path.join(OUT, "species.ron"), "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines) + "\n")
    print(f"{len(SPECIES)} species")


if __name__ == "__main__":
    main()
