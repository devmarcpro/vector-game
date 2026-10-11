//! The in-game encyclopedia: one entry per resource, module, hull, building, technology, recipe,
//! weapon, faction, anomaly and condition, generated from the real data (no hand-written numbers).
use vector_data::GameData;

pub const CATEGORIES: [&str; 34] = [
    "RESSOURCES",
    "MODULES",
    "ARMES",
    "COQUES",
    "BATIMENTS",
    "RECETTES",
    "TECHNOLOGIES",
    "FACTIONS",
    "ANOMALIES",
    "MALADIES",
    "INDUSTRIES",
    "BIOMES",
    "FLORE",
    "FAUNE",
    "EVENEMENTS",
    "GENES",
    "ORGANES",
    "IMPLANTS",
    "COMMUNICATIONS",
    "SITES",
    "CIVILISATIONS",
    "EQUIPEMENT",
    "CROYANCES",
    "ENTREPRISES",
    "CARTELS",
    "DIMENSIONS",
    "LIEUX A EXPLORER",
    "CREATURES",
    "MATERIAUX",
    "LOGICIELS",
    "TACHES DE L'IA",
    "AGES DE L'HISTOIRE",
    "CAUSES D'EFFONDREMENT",
    "ARTEFACTS",
];

/// Index in the source table of entry `k` of a category (modules and weapons share a table).
pub fn source_index(data: &GameData, cat: usize, k: usize) -> Option<usize> {
    entries(data, cat).get(k).copied()
}

/// Number of entries in a category.
pub fn count(data: &GameData, cat: usize) -> usize {
    entries(data, cat).len()
}

/// Sorted indices into the category's source table.
fn entries(data: &GameData, cat: usize) -> Vec<usize> {
    match cat {
        0 => (0..data.resources.len()).collect(),
        1 => (0..data.modules.len())
            .filter(|&i| data.modules[i].weapon.is_empty())
            .collect(),
        2 => (0..data.modules.len())
            .filter(|&i| !data.modules[i].weapon.is_empty())
            .collect(),
        3 => (0..data.hulls.len()).collect(),
        4 => (0..data.buildings.len()).collect(),
        5 => (0..data.recipes.len()).collect(),
        6 => (0..data.techs.len()).collect(),
        7 => (0..data.factions.len()).collect(),
        8 => (0..data.anomalies_def.len()).collect(),
        9 => (0..data.diseases.len()).collect(),
        10 => (0..data.industries.len()).collect(),
        11 => (0..data.biomes.len()).collect(),
        12 => (0..data.species.len())
            .filter(|&i| data.species[i].is_producer())
            .collect(),
        13 => (0..data.species.len())
            .filter(|&i| !data.species[i].is_producer())
            .collect(),
        14 => (0..data.world_events.len()).collect(),
        15 => (0..data.genes.len()).collect(),
        16 => (0..data.organs.len()).collect(),
        17 => (0..data.cybernetics.len()).collect(),
        18 => (0..vector_data::dialogue::ROLES.len()).collect(),
        19 => (0..data.sites.len()).collect(),
        20 => (0..data.civilisations.len()).collect(),
        21 => (0..data.gear.len()).collect(),
        22 => (0..data.religions.len()).collect(),
        23 => (0..data.companies.len()).collect(),
        24 => (0..data.cartels.len()).collect(),
        25 => (0..data.dimensions.len()).collect(),
        26 => (0..data.rogue_themes.len()).collect(),
        27 => (0..data.rogue_monsters.len()).collect(),
        28 => (0..data.materials.len()).collect(),
        29 => (0..data.mind.software.len()).collect(),
        30 => (0..data.mind.tasks.len()).collect(),
        31 => (0..data.history.ages.len()).collect(),
        32 => (0..data.history.causes.len()).collect(),
        33 => (0..data.history.artefacts.len()).collect(),
        _ => Vec::new(),
    }
}

/// Display names of the entries of a category.
pub fn names(data: &GameData, cat: usize) -> Vec<String> {
    entries(data, cat)
        .into_iter()
        .map(|i| match cat {
            0 => data.resources[i].name.clone(),
            1 | 2 => data.modules[i].name.clone(),
            3 => data.hulls[i].name.clone(),
            4 => data.buildings[i].name.clone(),
            5 => data.recipes[i].name.clone(),
            6 => data.techs[i].name.clone(),
            7 => data.factions[i].name.clone(),
            8 => data.anomalies_def[i].name.clone(),
            9 => data.diseases[i].name.clone(),
            10 => data.industries[i].name.clone(),
            11 => data.biomes[i].name.clone(),
            14 => data.world_events[i].name.clone(),
            15 => data.genes[i].name.clone(),
            16 => data.organs[i].name.clone(),
            17 => data.cybernetics[i].name.clone(),
            19 => data.sites[i].name.clone(),
            20 => data.civilisations[i].name.clone(),
            21 => data.gear[i].name.clone(),
            22 => data.religions[i].name.clone(),
            23 => data.companies[i].name.clone(),
            24 => data.cartels[i].name.clone(),
            25 => data.dimensions[i].name.clone(),
            26 => data.rogue_themes[i].name.clone(),
            27 => data.rogue_monsters[i].name.clone(),
            28 => data.materials[i].name.clone(),
            29 => data.mind.software[i].name.clone(),
            30 => data.mind.tasks[i].name.clone(),
            31 => data.history.ages[i].name.clone(),
            32 => data.history.causes[i].name.clone(),
            33 => data.history.artefacts[i].name.clone(),
            18 => format!(
                "Appeler : {}",
                vector_data::dialogue::ROLES[i].to_lowercase()
            ),
            _ => data.species[i].name.clone(),
        })
        .collect()
}

/// Entries of other categories (or the same one) whose name appears in the text of entry
/// (`cat`, `k`): the cross-links of the codex, as (category, index, name).
pub fn links(data: &GameData, cat: usize, k: usize) -> Vec<(usize, usize, String)> {
    let lines = entry(data, cat, k);
    let text = lines
        .iter()
        .skip(1)
        .map(|l| l.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ");
    let mut out: Vec<(usize, usize, String)> = Vec::new();
    for c in 0..CATEGORIES.len() {
        for (j, name) in names(data, c).into_iter().enumerate() {
            if (c, j) == (cat, k) || name.chars().count() < 4 {
                continue;
            }
            let lower = name.to_lowercase();
            // Whole-word match, so "fer" does not light up inside "carburant de fer".
            let found = text.match_indices(&lower).any(|(p, m)| {
                let before = text[..p].chars().next_back();
                let after = text[p + m.len()..].chars().next();
                !before.is_some_and(char::is_alphanumeric)
                    && !after.is_some_and(char::is_alphanumeric)
            });
            if found && !out.iter().any(|(oc, _, on)| *oc == c && *on == name) {
                out.push((c, j, name));
            }
        }
    }
    out.truncate(14);
    out
}

fn names_of(
    data: &GameData,
    ids: &[String],
    lookup: fn(&GameData, &str) -> Option<String>,
) -> String {
    ids.iter()
        .map(|i| lookup(data, i).unwrap_or_else(|| i.clone()))
        .collect::<Vec<_>>()
        .join(", ")
}

fn res_name(data: &GameData, id: &str) -> Option<String> {
    data.resource_index(id)
        .map(|i| data.resources[i].name.clone())
}

fn fmt_list(data: &GameData, items: &[(String, f64)]) -> String {
    items
        .iter()
        .map(|(r, kg)| {
            format!(
                "{kg:.0} kg {}",
                res_name(data, r).unwrap_or_else(|| r.clone())
            )
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

/// Lines describing entry `k` (position within `names(data, cat)`).
pub fn entry(data: &GameData, cat: usize, k: usize) -> Vec<String> {
    let idx = entries(data, cat);
    let Some(&i) = idx.get(k) else {
        return vec!["(entree inconnue)".into()];
    };
    let mut out = Vec::new();
    match cat {
        0 => {
            let r = &data.resources[i];
            out.push(format!("{}  [{}]", r.name, r.category));
            out.push(format!("Masse volumique : {:.0} kg/m3", r.density));
            out.push(format!("Valeur de base : {:.2} cr/kg", r.value));
            let from: Vec<String> = data
                .asteroids
                .iter()
                .filter_map(|a| {
                    a.composition
                        .iter()
                        .find(|(id, _)| *id == r.id)
                        .map(|(_, f)| format!("asteroides {} ({:.1} %)", a.id, f * 100.0))
                })
                .collect();
            if !from.is_empty() {
                out.push(format!("Se mine sur : {}", from.join(", ")));
            }
            let made: Vec<String> = data
                .recipes
                .iter()
                .filter(|x| x.outputs.iter().any(|(o, _)| *o == r.id))
                .map(|x| x.name.clone())
                .collect();
            if !made.is_empty() {
                out.push(format!("Produit par : {}", made.join(", ")));
            }
            let used: Vec<String> = data
                .recipes
                .iter()
                .filter(|x| x.inputs.iter().any(|(o, _)| *o == r.id))
                .map(|x| x.name.clone())
                .collect();
            if !used.is_empty() {
                out.push(format!("Utilise dans : {}", used.join(", ")));
            }
            let makers: Vec<String> = data
                .industries
                .iter()
                .filter(|x| x.produces.contains(&r.id))
                .map(|x| x.name.clone())
                .collect();
            if !makers.is_empty() {
                out.push(format!("Surproduit par : {}", makers.join(", ")));
            }
            let wanting: Vec<String> = data
                .industries
                .iter()
                .filter(|x| x.consumes.contains(&r.id))
                .map(|x| x.name.clone())
                .collect();
            if !wanting.is_empty() {
                out.push(format!("Tres demande par : {}", wanting.join(", ")));
            }
            let banned: Vec<String> = data
                .factions
                .iter()
                .filter(|f| f.contraband.contains(&r.id))
                .map(|f| f.name.clone())
                .collect();
            if !banned.is_empty() {
                out.push(format!("Contrebande chez : {}", banned.join(", ")));
            }
        }
        1 | 2 => {
            let m = &data.modules[i];
            out.push(format!("{}  [{}]", m.name, m.category));
            out.push(format!(
                "Emplacement {} taille {}   masse {:.0} kg   PV {:.0}   prix {:.0} cr",
                m.slot.label(),
                m.size,
                m.mass,
                m.hp,
                m.cost
            ));
            let mut stat = |label: &str, v: f64, unit: &str| {
                if v != 0.0 {
                    out.push(format!("{label} : {v:.1} {unit}"));
                }
            };
            stat("Puissance", m.power / 1000.0, "MW (+ produit / - consomme)");
            stat("Poussee", m.thrust / 1000.0, "kN");
            stat("Isp", m.isp, "s");
            stat("Carburant", m.fuel, "kg");
            stat("Soute", m.cargo_kg, "kg");
            stat("Volume de soute", m.cargo_m3, "m3");
            stat("Batterie", m.battery, "MJ");
            stat("Rayonnement thermique", m.radiate / 1000.0, "MW");
            stat("Debit de minage", m.mining_rate, "kg/s");
            stat("Portee de minage", m.mining_range, "m");
            stat("Support de vie", m.life_support, "personnes");
            stat("Couchettes", m.crew_cap, "");
            stat("Portee du scanner", m.scan_range, "m");
            stat("Portee de saut", m.jump_range, "al");
            stat("Bouclier", m.shield, "MJ");
            stat("Blindage", m.armor, "");
            stat("Agilite", m.agility, "");
            stat(
                "Degats",
                m.damage,
                if m.weapon == "beam" {
                    "par seconde"
                } else {
                    "par coup"
                },
            );
            stat("Cadence", m.rate, "coups/s");
            stat("Vitesse du projectile", m.speed, "m/s");
            stat("Portee", m.range, "m");
            if !m.ammo.is_empty() {
                out.push(format!(
                    "Munitions : {} ({:.2} kg par coup)",
                    res_name(data, &m.ammo).unwrap_or_else(|| m.ammo.clone()),
                    m.ammo_use
                ));
            }
            if let Some(t) = data.unlocking_tech("module", &m.id) {
                out.push(format!(
                    "Debloque par : {} ({:.0} pts)",
                    data.techs[t].name, data.techs[t].cost
                ));
            } else {
                out.push("Disponible des le depart.".into());
            }
        }
        3 => {
            let h = &data.hulls[i];
            out.push(format!("{}  [{}]", h.name, h.class));
            out.push(format!(
                "Masse a vide {:.0} kg   PV {:.0}   prix {:.0} cr",
                h.mass, h.hp, h.cost
            ));
            out.push(format!(
                "RCS {:.0} kN   rotation max {:.1} rad/s   acceleration angulaire {:.1} rad/s2",
                h.rcs / 1000.0,
                h.max_rate,
                h.ang_accel
            ));
            let slots: Vec<String> = h
                .slots
                .iter()
                .map(|(t, s)| format!("{} {}", t.label(), s))
                .collect();
            out.push(format!(
                "Emplacements ({}) : {}",
                h.slots.len(),
                slots.join(", ")
            ));
            if let Some(t) = data.unlocking_tech("hull", &h.id) {
                out.push(format!("Debloque par : {}", data.techs[t].name));
            }
        }
        4 => {
            let b = &data.buildings[i];
            out.push(format!("{}  [{}]", b.name, b.category));
            out.push(format!(
                "Construction : {} en {:.0} jours",
                fmt_list(data, &b.cost),
                b.days
            ));
            out.push(format!(
                "Energie {:.1} MW   personnel {:.0}   entretien {:.0} cr/j",
                b.power / 1000.0,
                b.workers,
                b.upkeep
            ));
            if b.housing > 0.0 {
                out.push(format!("Logements : {:.0}", b.housing));
            }
            if b.storage > 0.0 {
                out.push(format!("Stockage : {:.0} kg", b.storage));
            }
            if !b.recipes.is_empty() {
                out.push(format!("Recettes : {}", b.recipes.join(", ")));
                out.push(format!("Cadence : {:.1} lots/jour", b.throughput));
            }
            if !b.burns.is_empty() {
                out.push(format!("Brule par jour : {}", fmt_list(data, &b.burns)));
            }
            out.push(format!("Sites : {}", b.sites.join(", ")));
            if let Some(t) = data.unlocking_tech("building", &b.id) {
                out.push(format!("Debloque par : {}", data.techs[t].name));
            }
        }
        5 => {
            let r = &data.recipes[i];
            out.push(format!("{}  [{}]", r.name, r.kind));
            out.push(format!(
                "Entrees : {}",
                if r.inputs.is_empty() {
                    "gisements du site".to_string()
                } else {
                    fmt_list(data, &r.inputs)
                }
            ));
            let outs: Vec<(String, f64)> = r.outputs.clone();
            out.push(format!("Sorties : {}", fmt_list(data, &outs)));
            out.push(format!(
                "Duree {:.0} h   energie {:.0} kWh",
                r.hours, r.energy_kwh
            ));
            let by: Vec<String> = data
                .buildings
                .iter()
                .filter(|b| b.recipes.contains(&r.id))
                .map(|b| b.name.clone())
                .collect();
            out.push(format!(
                "Realisee par : {}",
                if by.is_empty() {
                    "aucun batiment".into()
                } else {
                    by.join(", ")
                }
            ));
        }
        6 => {
            let t = &data.techs[i];
            out.push(format!("{}  [{}]", t.name, t.field));
            out.push(format!("Cout : {:.0} points de recherche", t.cost));
            out.push(format!(
                "Prerequis : {}",
                if t.requires.is_empty() {
                    "aucun".to_string()
                } else {
                    names_of(data, &t.requires, |d, id| {
                        d.tech_index(id).map(|i| d.techs[i].name.clone())
                    })
                }
            ));
            for (k, id) in &t.unlocks {
                out.push(format!("Debloque ({k}) : {id}"));
            }
        }
        7 => {
            let f = &data.factions[i];
            out.push(format!("{}  [{}]", f.name, f.ideology));
            out.push(format!(
                "Taxe sur les ventes : {:.0} %   agressivite {:.0} %",
                f.tax * 100.0,
                f.aggression * 100.0
            ));
            out.push(format!("Culture : {}", f.culture));
            let values = data.ethos.profile(Some(f.id.as_str()));
            out.push(format!(
                "Moeurs : {}",
                data.ethos.traits(&values, 4).join(", ")
            ));
            let banned: Vec<String> = (0..data.resources.len())
                .filter(|&r| data.ethos.banned_by(&values, &data.resources, r).is_some())
                .map(|r| data.resources[r].name.clone())
                .collect();
            if !banned.is_empty() {
                out.push(format!("Interdit par ses moeurs : {}", banned.join(", ")));
            }
            let wants: Vec<String> = (0..data.resources.len())
                .map(|r| (r, data.ethos.demand_factor(&values, &data.resources, r)))
                .filter(|&(_, d)| d > 1.12)
                .map(|(r, d)| format!("{} {:+.0} %", data.resources[r].name, (d - 1.0) * 100.0))
                .collect();
            if !wants.is_empty() {
                out.push(format!("Demande forte : {}", wants.join(", ")));
            }
            out.push(format!(
                "Allies : {}",
                if f.allies.is_empty() {
                    "aucun".to_string()
                } else {
                    names_of(data, &f.allies, |d, id| {
                        d.faction_index(id).map(|i| d.factions[i].name.clone())
                    })
                }
            ));
            out.push(format!(
                "Ennemis : {}",
                if f.enemies.is_empty() {
                    "aucun".to_string()
                } else {
                    names_of(data, &f.enemies, |d, id| {
                        d.faction_index(id).map(|i| d.factions[i].name.clone())
                    })
                }
            ));
            out.push(format!(
                "Contrebande : {}",
                if f.contraband.is_empty() {
                    "aucune".to_string()
                } else {
                    names_of(data, &f.contraband, res_name)
                }
            ));
        }
        8 => {
            let a = &data.anomalies_def[i];
            out.push(a.name.clone());
            out.push(a.text.clone());
            out.push(format!(
                "Zone d'effet : {:.0} km   danger : {} (force {:.1})",
                a.radius / 1000.0,
                a.hazard,
                a.strength
            ));
            out.push(format!(
                "Recompenses : recherche {:.0}-{:.0}, credits {:.0}-{:.0}",
                a.research.0, a.research.1, a.credits.0, a.credits.1
            ));
            for (r, lo, hi) in &a.loot {
                out.push(format!(
                    "Butin possible : {lo:.0}-{hi:.0} kg {}",
                    res_name(data, r).unwrap_or_else(|| r.clone())
                ));
            }
        }
        9 => {
            let d = &data.diseases[i];
            out.push(d.name.clone());
            out.push(format!(
                "Gravite : {:.1} % de sante par jour sans soins",
                d.severity * 100.0
            ));
            out.push(format!("Duree : {:.0} jours   contagion {:.0} %/jour   letalite {:.0} %/jour a sante faible", d.duration_days, d.contagion * 100.0, d.lethality * 100.0));
            out.push(format!(
                "Traitement : {}",
                if d.cure.is_empty() {
                    "repos seulement".to_string()
                } else {
                    res_name(data, &d.cure).unwrap_or_else(|| d.cure.clone())
                }
            ));
        }
        28 => {
            let m = &data.materials[i];
            out.push(format!(
                "{} ({})",
                m.name,
                m.glyphs.chars().take(1).collect::<String>()
            ));
            out.push(m.text.clone());
            out.push(format!(
                "{}   durete {:.0} %   temps de marche x{:.2}   aspect en vue a la premiere personne : {}",
                match m.kind.as_str() {
                    "wall" => "Mur ou obstacle",
                    "liquid" => "Liquide",
                    _ => "Sol",
                },
                m.hardness * 100.0,
                m.slow,
                m.pattern
            ));
            let at: Vec<String> = m
                .ground_of
                .iter()
                .filter_map(|(b, _)| data.biomes.iter().find(|x| &x.id == b))
                .map(|b| b.name.clone())
                .collect();
            if !at.is_empty() {
                out.push(format!("Se trouve sur : {}", at.join(", ")));
            }
            let places: Vec<&str> = data
                .rogue_themes
                .iter()
                .filter(|t| t.floor_mat == m.id || t.wall_mat == m.id)
                .map(|t| t.name.as_str())
                .collect();
            if !places.is_empty() {
                out.push(format!("Construit : {}", places.join(", ")));
            }
        }
        27 => {
            let m = &data.rogue_monsters[i];
            out.push(format!("{} ({})", m.name, m.glyph));
            out.push(m.text.clone());
            out.push(format!(
                "Points de vie {:.0}   attaque {:.2}   defense {:.0} %   vue {} cases   apparait des l'etage {}",
                m.hp,
                m.attack,
                m.defense * 100.0,
                m.sight,
                m.min_depth + 1
            ));
            if let Some(n) = res_name(data, &m.loot.0) {
                out.push(format!("Butin : {n}"));
            }
            let places: Vec<&str> = data
                .rogue_themes
                .iter()
                .filter(|t| t.monsters.iter().any(|x| x.0 == m.id))
                .map(|t| t.name.as_str())
                .collect();
            out.push(format!("Se rencontre : {}", places.join(", ")));
        }
        26 => {
            let t = &data.rogue_themes[i];
            out.push(t.name.clone());
            out.push(t.text.clone());
            out.push(format!(
                "{} etages de {}x{} cases ; {} a {} salles par etage",
                t.floors, t.size.0, t.size.1, t.rooms.0, t.rooms.1
            ));
            if t.hazard > 0.0 {
                out.push(format!("Danger : {}", t.hazard_name));
            }
            let monsters: Vec<String> = t
                .monsters
                .iter()
                .filter_map(|(m, _)| {
                    data.rogue_monster_index(m)
                        .map(|k| data.rogue_monsters[k].name.clone())
                })
                .collect();
            out.push(format!("Creatures : {}", monsters.join(", ")));
            let loot: Vec<String> = t
                .loot
                .iter()
                .filter_map(|(r, ..)| res_name(data, r))
                .collect();
            out.push(format!("Butin : {}", loot.join(", ")));
            out.push(
                "On y va depuis le menu de vol (EXPLORER A PIED) ou avec la touche Fin."
                    .to_string(),
            );
        }
        25 => {
            let d = &data.dimensions[i];
            out.push(d.name.clone());
            out.push(d.text.clone());
            out.push(format!(
                "Minerai x{:.1}   dangers x{:.1}   etude x{:.1}   instabilite de l'equipage +{:.0} % par jour   temps dehors x{:.1}",
                d.ore_scale,
                d.hazard_scale,
                d.research_scale,
                d.instability * 100.0,
                d.time_ratio
            ));
            out.push(
                "On y entre par un portail (anomalie rare) ; un portail y ramene toujours."
                    .to_string(),
            );
        }
        24 => {
            let c = &data.cartels[i];
            out.push(c.name.clone());
            out.push(c.text.clone());
            let hq = data
                .factions
                .iter()
                .find(|f| f.id == c.hq)
                .map_or(c.hq.clone(), |f| f.name.clone());
            out.push(format!(
                "Siege : {hq}   majoration des prix : {:.0} % quand il est actif dans une region",
                c.markup * 100.0
            ));
            let goods: Vec<String> = c.goods.iter().filter_map(|r| res_name(data, r)).collect();
            out.push(format!("Biens controles : {}", goods.join(", ")));
            out.push(
                "Un siege s'achete au courtage (D a quai) ; il rapporte chaque mois, au risque d'une enquete antitrust."
                    .to_string(),
            );
        }
        23 => {
            let c = &data.companies[i];
            out.push(c.name.clone());
            out.push(c.text.clone());
            let hq = data
                .factions
                .iter()
                .find(|f| f.id == c.hq)
                .map_or(c.hq.clone(), |f| f.name.clone());
            out.push(format!(
                "Siege : {hq}   cours de reference {:.0} cr   dividende {:.1} % par an   agitation {:.0} %",
                c.base,
                c.yield_year * 100.0,
                c.vol * 100.0
            ));
            let sells: Vec<String> = c
                .goods
                .iter()
                .filter_map(|(r, w)| res_name(data, r).map(|n| format!("{n} {:.0} %", w * 100.0)))
                .collect();
            out.push(format!("Son cours suit les prix de : {}", sells.join(", ")));
            out.push(
                "Actions : bourse (T a quai), touche E, B achete 10 actions, N en vend 10."
                    .to_string(),
            );
        }
        22 => {
            let r = &data.religions[i];
            out.push(r.name.clone());
            out.push(r.text.clone());
            out.push(format!(
                "Se developpe chez les peuples dont la foi (axe des moeurs) va de {:+.1} a {:+.1}.",
                r.foi_min, r.foi_max
            ));
            out.push(format!(
                "Effets sur la colonie : moral {:+.0} %, recherche {:+.0} %.",
                r.morale * 100.0,
                r.research * 100.0
            ));
        }
        21 => {
            let g = &data.gear[i];
            let slot = match g.slot.as_str() {
                "weapon" => "arme",
                "armor" => "armure",
                _ => "outil",
            };
            out.push(format!("{}  ({slot})", g.name));
            out.push(g.text.clone());
            if g.attack > 0.0 {
                out.push(format!("Attaque : +{:.1} de degats par coup", g.attack));
            }
            if g.defense > 0.0 {
                out.push(format!(
                    "Defense : {:.0} % des degats subis arretes",
                    g.defense * 100.0
                ));
            }
            if g.gather > 0.0 {
                out.push(format!(
                    "Recolte : +{:.0} % de ce que donnent plantes et sites",
                    g.gather * 100.0
                ));
            }
            out.push(format!(
                "Valeur : {:.0} cr (se vend et s'achete comme marchandise, equipement : gear_{})",
                g.value, g.id
            ));
            out.push("Equiper : ecran equipage, touche E (il faut l'objet en soute). Sert pendant les expeditions a pied.".to_string());
        }
        20 => {
            let c = &data.civilisations[i];
            out.push(format!("{}  (technologie {}/4)", c.name, c.tech));
            out.push(c.plan.clone());
            let mores: Vec<String> = data
                .ethos
                .axes
                .iter()
                .zip(&c.profile)
                .filter(|(_, v)| v.abs() >= 0.4)
                .map(|(a, v)| format!("{} ({:+.1})", if *v > 0.0 { &a.high } else { &a.low }, v))
                .collect();
            out.push(format!("Moeurs marquees : {}", mores.join(", ")));
            out.push(format!(
                "Offre : {}",
                c.offers
                    .iter()
                    .filter_map(|r| res_name(data, r))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push(format!(
                "Demande : {}",
                c.wants
                    .iter()
                    .filter_map(|r| res_name(data, r))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            out.push("Environ un monde habitable sur trois en abrite une. Entree en vol, planete ciblee (Maj+Tab) : leur parler ; leur ton suit leurs moeurs.".to_string());
        }
        19 => {
            let s = &data.sites[i];
            out.push(format!("{}  (symbole {})", s.name, s.glyph));
            out.push(s.text.clone());
            let loot: Vec<String> = s
                .loot
                .iter()
                .map(|(r, kg)| {
                    format!(
                        "{} {kg:.0} kg",
                        res_name(data, r).unwrap_or_else(|| r.clone())
                    )
                })
                .collect();
            if !loot.is_empty() {
                out.push(format!("A y trouver : {}", loot.join(", ")));
            }
            out.push(format!(
                "Frequence relative {:.1}   risque d'embuscade ou d'eboulement : {:.0} %{}",
                s.weight,
                s.danger * 100.0,
                if s.heal {
                    "   soigne toute l'equipe"
                } else {
                    ""
                }
            ));
            out.push("Se fouille une fois, avec G a cote, pendant une expedition a pied (E sur la surface).".to_string());
        }
        29 => {
            let sw = &data.mind.software[i];
            let task = data
                .mind
                .tasks
                .iter()
                .find(|t| t.id == sw.task)
                .map_or("?", |t| t.name.as_str());
            out.push(format!("{}  [tache : {task}]", sw.name));
            out.push(sw.text.clone());
            out.push(format!(
                "Bonus +{:.0} % a la tache (+50 % de ce bonus par mise a jour, 3 versions), memoire {} (+1 par mise a jour), {:.0} cr (x numero de version).",
                sw.bonus * 100.0,
                sw.memory,
                sw.price
            ));
            out.push("Se gere dans le panneau ESPRIT : F4 puis E.".to_string());
        }
        30 => {
            let t = &data.mind.tasks[i];
            out.push(format!(
                "{}  [tache de l'IA {}/{}]",
                t.name,
                i + 1,
                data.mind.tasks.len()
            ));
            out.push(t.text.clone());
            out.push(
                "Le budget de calcul se partage par poids de 0 a 10 : a parts egales sur un vaisseau nu, le facteur est x1,00 (de x0,50 sans rien a x1,50 en pleine puissance). Chaque laboratoire ajoute de la puissance et de la memoire ; la chaleur, un bus electrique affame et les programmes malveillants la reduisent.".to_string(),
            );
        }
        31 => {
            let a = &data.history.ages[i];
            out.push(format!("{}  [age de 400 ans]", a.name));
            out.push(a.text.clone());
            let next: Vec<String> = data
                .history
                .ages
                .iter()
                .enumerate()
                .filter(|(j, _)| vector_sim::history::can_follow(i, *j))
                .map(|(_, n)| n.name.clone())
                .collect();
            out.push(format!("Peut etre suivi de : {}.", next.join(", ")));
            out.push("Chaque secteur a 8 ages (3200 ans) calcules depuis la graine ; les annees recentes sont celles de la politique. Voir ARCHIVES (J puis TAB).".to_string());
        }
        32 => {
            let c = &data.history.causes[i];
            out.push(format!("{}  [poids {:.1}]", c.name, c.weight));
            out.push(c.text.clone());
            out.push("Une ruine tombee pendant cet effondrement le dit dans ses textes une fois traduits.".to_string());
        }
        33 => {
            let a = &data.history.artefacts[i];
            out.push(format!("{}  [valeur de base {:.0} cr]", a.name, a.value));
            out.push(a.text.clone());
            out.push(format!(
                "Traduction : {:.0} jours de calcul au rythme 1 ; la valeur croit avec l'age ; un faux est fabrique 150 a 400 ans avant aujourd'hui et se trahit a la datation si le budget d'analyse est assez grand.",
                a.difficulty
            ));
            out.push("Se vend a un collectionneur (80 %, sans question), se verse a un musee (50 %, documente seulement) ou se restitue aux heritiers.".to_string());
        }
        18 => {
            let role = vector_data::dialogue::ROLES[i];
            out.push(format!(
                "Appeler un vaisseau {} (Entree en vol, vaisseau cible)",
                role.to_lowercase()
            ));
            out.push("Le ton de l'interlocuteur suit les moeurs de sa faction : formel (autorite), cordial (ouverture), mefiant (xenophobie), neutre.".to_string());
            if let Some(start) = data
                .dialogues
                .start_for(role)
                .and_then(|s| data.dialogues.node(s))
            {
                out.push(format!(
                    "{} options au premier echange, {} noeuds de conversation en tout.",
                    start.options.len(),
                    data.dialogues.nodes.len()
                ));
                for o in &start.options {
                    let cond = if o.cond.is_empty() {
                        String::new()
                    } else {
                        format!("  [condition : {}]", o.cond)
                    };
                    out.push(format!("Option : {}{cond}", o.label));
                }
            }
        }
        17 => {
            let c = &data.cybernetics[i];
            out.push(format!("{}  (implant, emplacement {})", c.name, c.slot));
            for (k, w) in &c.effects {
                let what = match k.strip_prefix("skill:") {
                    Some(s) => format!("competence {s}"),
                    None => k.clone(),
                };
                out.push(format!("Effet : {what} {w:+.2}"));
            }
            out.push(format!(
                "Besoins : {:.2} kW du vaisseau, {:.3} kg de pieces detachees par jour. Sans courant ou sans pieces, il s'arrete.",
                c.power, c.upkeep
            ));
            out.push(format!(
                "Contrainte constante sur le porteur : {:.0} %   valeur : {:.0} cr",
                c.stress * 100.0,
                c.value
            ));
            out.push(if c.hackable {
                "Piratable : un intrus peut le prendre pour quelques jours.".to_string()
            } else {
                "Non piratable (aucune interface sans fil).".to_string()
            });
        }
        16 => {
            let o = &data.organs[i];
            out.push(format!(
                "{}  ({})",
                o.name,
                if o.vital {
                    "organe vital"
                } else {
                    "organe non vital"
                }
            ));
            out.push(format!(
                "Part dans l'etat general : {:.1}   guerison naturelle : {:.1} % par jour   valeur : {:.0} cr",
                o.weight,
                o.heal * 100.0,
                o.value
            ));
            if o.vital {
                out.push(
                    "A zero, la personne meurt. Un organe a zero ne guerit plus seul.".to_string(),
                );
            }
            for (k, w) in &o.effects {
                out.push(format!("Quand il est detruit : {k} {w:+.2}"));
            }
            let sick: Vec<String> = o
                .afflicted_by
                .iter()
                .filter_map(|d| data.disease_index(d).map(|k| data.diseases[k].name.clone()))
                .collect();
            if !sick.is_empty() {
                out.push(format!("Attaque par : {}", sick.join(", ")));
            }
        }
        15 => {
            let g = &data.genes[i];
            out.push(format!(
                "{}  ({})",
                g.name,
                if g.is_predisposition() {
                    "predisposition"
                } else {
                    "gene additif"
                }
            ));
            if g.is_predisposition() {
                let d = data
                    .disease_index(&g.disease)
                    .map_or(g.disease.clone(), |k| data.diseases[k].name.clone());
                out.push(format!("Maladie liee : {d}"));
                out.push("L'allele 0 est nefaste : un exemplaire (porteur) multiplie le risque par 1.5, deux (atteint) par 4. Frequence de l'allele nefaste : environ 10 % dans la population fondatrice.".to_string());
            } else {
                for (k, w) in &g.effects {
                    let what = match k.strip_prefix("skill:") {
                        Some(s) => format!("competence {s}"),
                        None => k.clone(),
                    };
                    out.push(format!(
                        "Effet : {what}, de {:+.2} a {:+.2} selon les alleles",
                        -w, w
                    ));
                }
                out.push("Deux alleles de 0 a 3 (un de chaque parent) ; l'effet suit leur somme. Les enfants en recoivent un de chaque parent, avec 0.5 % de mutation.".to_string());
            }
        }
        14 => {
            let e = &data.world_events[i];
            out.push(e.name.clone());
            out.push(e.text.clone());
            out.push(format!(
                "Portee : {}   duree {:.0} a {:.0} jours   frequence relative {:.1}",
                if e.scope == "global" {
                    "toute la galaxie"
                } else {
                    "une region de 60 al"
                },
                e.days.0,
                e.days.1,
                e.weight
            ));
            for (k, m) in &e.effects {
                let what = match k.strip_prefix("cat:") {
                    Some(c) => format!("toutes les ressources de categorie {c}"),
                    None => res_name(data, k).unwrap_or_else(|| k.clone()),
                };
                out.push(format!(
                    "{what} : {:+.0} % au plus fort de l'evenement",
                    (m - 1.0) * 100.0
                ));
            }
        }
        12 | 13 => {
            let s = &data.species[i];
            let biome_names = |ids: &[String]| {
                ids.iter()
                    .map(|b| {
                        data.biome_index(b)
                            .map_or_else(|| b.clone(), |k| data.biomes[k].name.clone())
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            let kind = match s.trophic.as_str() {
                "producer" => "producteur",
                "herbivore" => "herbivore",
                "carnivore" => "carnivore",
                _ => "omnivore",
            };
            out.push(format!("{}  ({kind})", s.name));
            out.push(s.text.clone());
            out.push(format!("Habitats : {}", biome_names(&s.habitat)));
            if !s.diet.is_empty() {
                let prey: Vec<String> = s
                    .diet
                    .iter()
                    .filter_map(|d| data.species_index(d).map(|k| data.species[k].name.clone()))
                    .collect();
                out.push(format!("Se nourrit de : {}", prey.join(", ")));
            }
            out.push(format!(
                "Masse {:.3} kg   croissance {:.1} %/jour   danger {:.0} %   envahissante {:.0} %",
                s.body,
                s.growth * 100.0,
                s.danger * 100.0,
                s.invasive * 100.0
            ));
            if !s.yields.is_empty() {
                out.push(format!("Rend par individu : {}", fmt_list(data, &s.yields)));
            }
            if s.domestic {
                out.push("Domesticable : oui (voir les colonies)".to_string());
            }
        }
        11 => {
            let b = &data.biomes[i];
            let range = |r: (f64, f64), unit: &str| {
                if r.1 > 1.0e8 {
                    format!("plus de {:.0} {unit}", r.0)
                } else {
                    format!("{:.0} a {:.0} {unit}", r.0, r.1)
                }
            };
            out.push(format!("{}  [{}]", b.name, b.glyph));
            out.push(b.text.clone());
            out.push(format!("Temperature annuelle : {}", range(b.t, "K")));
            out.push(format!(
                "Humidite {:.0} a {:.0} %   altitude relative {:+.2} a {:+.2}   pression {}",
                b.hum.0 * 100.0,
                b.hum.1.min(1.0) * 100.0,
                b.elev.0.max(-1.0),
                b.elev.1.min(1.0),
                if b.p.1 > 1.0e8 {
                    format!("plus de {} bar", b.p.0)
                } else {
                    format!("{} a {} bar", b.p.0, b.p.1)
                }
            ));
            out.push(format!(
                "Attrait pour une colonie : {:.0} %   fertilite : {:.0} %",
                b.habit * 100.0,
                b.fertility * 100.0
            ));
        }
        _ => {
            let n = &data.industries[i];
            out.push(n.name.clone());
            out.push(format!(
                "Surproduit : {}",
                names_of(data, &n.produces, res_name)
            ));
            out.push(format!(
                "Tres demande : {}",
                names_of(data, &n.consumes, res_name)
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data")
    }

    #[test]
    fn every_entry_of_every_category_has_real_data() {
        let d = data();
        let mut total = 0;
        for (cat, cat_name) in CATEGORIES.iter().enumerate() {
            let n = count(&d, cat);
            assert!(n > 0, "{cat_name}");
            assert_eq!(names(&d, cat).len(), n);
            for k in 0..n {
                let e = entry(&d, cat, k);
                assert!(e.len() >= 2, "{} #{k} is too thin: {e:?}", CATEGORIES[cat]);
                assert!(
                    e.iter().any(|l| l.chars().any(|c| c.is_ascii_digit()))
                        || cat == 8
                        || cat == 10
                        || cat == 7,
                    "{} #{k} has no number: {e:?}",
                    CATEGORIES[cat]
                );
                total += 1;
            }
        }
        assert!(total > 350, "{total}");
    }

    #[test]
    fn resource_entries_cross_reference_the_rest_of_the_game() {
        let d = data();
        let iron = entry(&d, 0, d.resource_index("iron").expect("iron"));
        let text = iron.join("\n");
        assert!(text.contains("7870"), "real density");
        assert!(
            text.contains("asteroides M") && text.contains("Utilise dans"),
            "{text}"
        );
        let uranium = entry(&d, 0, d.resource_index("uranium").expect("u")).join("\n");
        assert!(uranium.contains("Contrebande chez"), "{uranium}");
        let tech = entry(&d, 6, d.tech_index("fusion_mk3").expect("tech")).join("\n");
        assert!(
            tech.contains("fusion_mk3") && tech.contains("Physique des plasmas"),
            "{tech}"
        );
    }
}
