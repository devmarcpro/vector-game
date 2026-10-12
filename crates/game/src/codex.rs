//! The in-game encyclopedia: one entry per resource, module, hull, building, technology, recipe,
//! weapon, faction, anomaly and condition, generated from the real data (no hand-written numbers).
use vector_data::GameData;

pub const CATEGORIES: [&str; 50] = [
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
    "LOIS",
    "PHENOMENES EXOTIQUES",
    "NOMS",
    "ELEMENTS",
    "COMPOSES",
    "REACTIONS",
    "AGENTS PATHOGENES",
    "MONNAIES",
    "SURPRISES",
    "XENOBIOLOGIE",
    "LOGICIELS",
    "TACHES DE L'IA",
    "AGES DE L'HISTOIRE",
    "CAUSES D'EFFONDREMENT",
    "ARTEFACTS",
    "PARTIES DU CORPS",
    "BLESSURES",
    "MEDICAMENTS",
    "SCENARIOS",
    "GEOLOGIE",
    "ASTRONOMIE",
];

/// The category of the reactions: the laboratory.
pub const LAB_CATEGORY: usize = 34;

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
        7 | 36 => (0..data.factions.len()).collect(),
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
        32 => (0..data.chemistry.elements.len()).collect(),
        33 => (0..data.chemistry.compounds.len()).collect(),
        34 => (0..data.reactions.len()).collect(),
        28 => (0..data.materials.len()).collect(),
        29 => (0..data.laws.len()).collect(),
        30 => (0..data.exotics.len()).collect(),
        31 => (0..data.cultures.len()).collect(),
        35 => (0..data.pathogens.len()).collect(),
        37 => (0..data.surprises.len()).collect(),
        38 => (0..data.chemistries.len()).collect(),
        39 => (0..data.mind.software.len()).collect(),
        40 => (0..data.mind.tasks.len()).collect(),
        41 => (0..data.history.ages.len()).collect(),
        42 => (0..data.history.causes.len()).collect(),
        43 => (0..data.history.artefacts.len()).collect(),
        44 => (0..data.body_parts.len()).collect(),
        45 => (0..data.wounds.len()).collect(),
        46 => (0..data.drugs.len()).collect(),
        47 => (0..data.scenarios.len()).collect(),
        48 => (0..data.geology.len()).collect(),
        49 => (0..data.cosmos.len()).collect(),
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
            32 => data.chemistry.elements[i].name.clone(),
            33 => data.chemistry.compounds[i].name.clone(),
            34 => data.reactions[i].name.clone(),
            28 => data.materials[i].name.clone(),
            29 => data.laws[i].name.clone(),
            30 => data.exotics[i].name.clone(),
            31 => format!("Noms : {}", data.cultures[i].name),
            35 => data.pathogens[i].name.clone(),
            36 => vector_sim::macroeco::currency_name(data, Some(i)),
            37 => data.surprises[i].name.clone(),
            38 => data.chemistries[i].name.clone(),
            39 => data.mind.software[i].name.clone(),
            40 => data.mind.tasks[i].name.clone(),
            41 => data.history.ages[i].name.clone(),
            42 => data.history.causes[i].name.clone(),
            43 => data.history.artefacts[i].name.clone(),
            44 => data.body_parts[i].name.clone(),
            45 => data.wounds[i].name.clone(),
            46 => data.drugs[i].name.clone(),
            47 => data.scenarios[i].name.clone(),
            48 => data.geology[i].name.clone(),
            49 => data.cosmos[i].name.clone(),
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

fn describe_target(data: &GameData, t: &vector_data::surprise::Target) -> String {
    use vector_data::surprise::Target;
    let fac = |id: &str| {
        data.factions
            .iter()
            .find(|f| f.id == id)
            .map_or(id.to_string(), |f| f.name.clone())
    };
    match t {
        Target::Faction(id) => fac(id),
        Target::Here => "le peuple du territoire".to_string(),
        Target::Ally => "un allie de ce peuple".to_string(),
        Target::Rival => "un ennemi de ce peuple".to_string(),
        Target::Cartel(id) => data
            .cartels
            .iter()
            .find(|c| c.id == *id)
            .map_or(id.clone(), |c| c.name.clone()),
        Target::Company(id) => data
            .companies
            .iter()
            .find(|c| c.id == *id)
            .map_or(id.clone(), |c| c.name.clone()),
        Target::Contact => "votre contact local".to_string(),
        Target::Civ => "les natifs du systeme".to_string(),
    }
}

fn describe_cond(data: &GameData, c: &vector_data::surprise::Cond) -> String {
    use vector_data::surprise::{Cmp, Cond};
    let cmp = |c: &Cmp| {
        if *c == Cmp::Gt {
            "au-dessus de"
        } else {
            "en dessous de"
        }
    };
    let cartel = |id: &str| {
        data.cartels
            .iter()
            .find(|k| k.id == id)
            .map_or(id.to_string(), |k| k.name.clone())
    };
    match c {
        Cond::WorldEvent(id) => format!(
            "l'evenement \"{}\" touche la region",
            data.world_events
                .iter()
                .find(|w| w.id == *id)
                .map_or(id.as_str(), |w| w.name.as_str())
        ),
        Cond::AtWar => "le secteur est en guerre".to_string(),
        Cond::Unrest(v) => format!(
            "troubles apres un coup ou un meurtre (au moins {:.0} %)",
            v * 100.0
        ),
        Cond::CartelHere(id) => format!("le cartel {} tient les prix", cartel(id)),
        Cond::Incident => "une entreprise du secteur subit ou profite d'un incident".to_string(),
        Cond::Epidemic(d) => format!("une de vos colonies a moins de {d:.0} al a une epidemie"),
        Cond::ColonyNear(d) => format!("une de vos colonies est a moins de {d:.0} al"),
        Cond::ColonyUnrest(v) => format!(
            "vos colonies proches grondent (au moins {:.0} %)",
            v * 100.0
        ),
        Cond::Territory(id) => format!(
            "vous etes chez {}",
            data.factions
                .iter()
                .find(|f| f.id == *id)
                .map_or(id.as_str(), |f| f.name.as_str())
        ),
        Cond::Claimed => "vous etes sur un territoire".to_string(),
        Cond::Civ => "une civilisation native vit dans le systeme".to_string(),
        Cond::Anomaly => "une anomalie flotte dans le systeme".to_string(),
        Cond::AnomalyOf(id) => format!(
            "une anomalie \"{}\" flotte dans le systeme",
            data.anomaly_def(id)
                .map_or(id.as_str(), |a| a.name.as_str())
        ),
        Cond::Docked => "vous etes amarre".to_string(),
        Cond::InFlight => "vous etes en vol".to_string(),
        Cond::Credits(c, v) => format!("credits {} {v:.0}", cmp(c)),
        Cond::Contracts(n) => format!("au moins {n} contrat(s) actif(s)"),
        Cond::Rel(t, c, v) => format!(
            "relation avec {} {} {v:+.2}",
            describe_target(data, t),
            cmp(c)
        ),
        Cond::Morale(c, v) => format!("moral moyen de l'equipage {} {:.0} %", cmp(c), v * 100.0),
        Cond::Crew(n) => format!("au moins {n} personnes a bord"),
        Cond::Hold(r, kg) => format!(
            "au moins {kg:.0} kg de {} en soute",
            res_name(data, r).unwrap_or_else(|| r.clone())
        ),
        Cond::Seat(id) => format!("vous avez un siege chez {}", cartel(id)),
        Cond::Wanted => "vous etes recherche ici".to_string(),
        Cond::InDebt => "la banque vous reclame de l'argent".to_string(),
        Cond::Shares => "vous detenez des actions".to_string(),
        Cond::Herd => "vous gardez des betes a bord".to_string(),
    }
}

fn describe_effect(data: &GameData, e: &vector_data::surprise::Effect) -> String {
    use vector_data::surprise::Effect;
    match e {
        Effect::Credits(v) => format!("{v:+.0} CR"),
        Effect::Rel(t, d) => format!("relation avec {} {d:+.2}", describe_target(data, t)),
        Effect::Bond(a, b, w, days) => format!(
            "{} entre {} et {} pendant {days:.0} jours",
            if *w > 0.0 { "alliance" } else { "rivalite" },
            describe_target(data, a),
            describe_target(data, b)
        ),
        Effect::Shock(k, m, days) => format!(
            "prix de {} x{m:.2} dans la region pendant {days:.0} jours",
            match k.strip_prefix("cat:") {
                Some(c) => format!("la categorie {c}"),
                None => res_name(data, k).unwrap_or_else(|| k.clone()),
            }
        ),
        Effect::Cargo(r, kg) => format!(
            "{kg:+.0} kg de {} en soute",
            res_name(data, r).unwrap_or_else(|| r.clone())
        ),
        Effect::Morale(d) => format!("moral de l'equipage {:+.0} %", d * 100.0),
        Effect::Contract(r, kg, m) => format!(
            "un contrat de {kg:.0} kg de {} a {m:.1} fois le cours est propose a la station",
            res_name(data, r).unwrap_or_else(|| r.clone())
        ),
        Effect::Research(v) => format!("{v:.0} points de recherche"),
        Effect::ColonyHappiness(d) => format!("bonheur des colonies proches {:+.0} %", d * 100.0),
    }
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
            if let (Some(c), Some(parts)) = (
                data.compound_of_resource(&r.id),
                data.resource_elements(&r.id),
            ) {
                out.push(format!(
                    "Chimie : {} ({})   {}",
                    data.chemistry.compounds[c].name,
                    data.chemistry.compounds[c].formula,
                    parts
                        .iter()
                        .map(|(s, f)| format!("{s} {:.1} %", f * 100.0))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
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
            if m.proj_mass > 0.0 {
                stat("Masse du projectile", m.proj_mass, "kg");
                stat(
                    "Energie cinetique",
                    vector_sim::arms::kinetic_energy_j(m.proj_mass, m.speed) / 1e6,
                    "MJ",
                );
            }
            stat(
                "Dispersion",
                m.spread,
                "mrad (le tir s'ecarte avec la distance)",
            );
            stat("Longueur d'onde", m.wavelength, "nm");
            stat(
                "Ouverture du faisceau",
                m.aperture,
                "m (diverge avec la distance)",
            );
            stat(
                "Chaleur au tir",
                m.flash,
                "kW (vu par les capteurs passifs)",
            );
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
            match b.source.as_str() {
                "solar" => {
                    out.push("Source : solaire (flux de l'etoile, attenue par l'atmosphere)".into())
                }
                "wind" => {
                    out.push("Source : eolienne (pression atmospherique, rafales variables)".into())
                }
                "geothermal" => out.push("Source : chaleur interne de la planete".into()),
                _ => {}
            }
            if b.battery > 0.0 {
                out.push(format!(
                    "Batterie : {:.0} kWh (rendement {:.0} %)",
                    b.battery,
                    vector_sim::colony::BATTERY_EFFICIENCY * 100.0
                ));
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
            if let Some(k) = data.pathogen_of_disease(i) {
                out.push(format!(
                    "Cette maladie est celle d'une epidemie : voir l'agent {}",
                    data.pathogens[k].name
                ));
            }
        }
        35 => {
            let p = &data.pathogens[i];
            out.push(format!(
                "{} ({})",
                p.name,
                match p.origin.as_str() {
                    "lab" => "souche de laboratoire",
                    "alien" => "agent extraterrestre",
                    _ => "agent naturel",
                }
            ));
            out.push(p.text.clone());
            out.push(format!(
                "R0 {:.1} (personnes infectees par un malade, sans mesures)   incubation {:.0} j   malade {:.0} j   letalite {:.1} %",
                p.r0,
                p.incubation_days,
                p.infectious_days,
                p.fatality * 100.0
            ));
            out.push(format!(
                "Immunite apres guerison : {}   vaccin : {}",
                if p.immunity_days > 0.0 {
                    format!("{:.0} jours", p.immunity_days)
                } else {
                    "a vie".to_string()
                },
                if p.vaccine_eff > 0.0 {
                    format!("efficace a {:.0} %", p.vaccine_eff * 100.0)
                } else {
                    "aucun connu".to_string()
                }
            ));
            out.push(format!(
                "Se declare surtout : {}",
                match p.trigger.as_str() {
                    "crowd" => "dans les colonies surpeuplees",
                    "hunger" => "quand les rations manquent",
                    "water" => "quand l'eau est rare ou souillee",
                    "wild" => "au contact de la faune et de la flore sauvages",
                    "lab" => "dans les colonies qui font de la recherche",
                    "alien" => "au contact de mondes inconnus",
                    _ => "partout",
                }
            ));
            if let Some(d) = data.disease_index(&p.disease) {
                out.push(format!("Symptomes : {}", data.diseases[d].name));
            }
            out.push(format!(
                "Effets : {:.0} % de production perdue quand 10 % des gens sont malades",
                p.prod_hit * 100.0
            ));
            for (r, delta) in &p.prices {
                out.push(format!(
                    "Prix : {} {:+.0} % quand 10 % des gens sont malades",
                    res_name(data, r).unwrap_or_else(|| r.clone()),
                    delta * 100.0
                ));
            }
            out.push("Mesures : quarantaine (politique de colonie), medicaments, centre medical, infirmerie du bord, vaccins (Culture de vaccins)".to_string());
        }
        29 => {
            let l = &data.laws[i];
            out.push(format!("{}  [gravite {}]", l.name, l.severity));
            out.push(l.text.clone());
            out.push(format!("Amende : {:.0} CR", l.fine));
            let values = |f: &vector_data::Faction| data.ethos.profile(Some(f.id.as_str()));
            let places: Vec<String> = data
                .factions
                .iter()
                .filter(|f| l.in_force(&data.ethos, &values(f)))
                .map(|f| f.name.clone())
                .collect();
            out.push(if places.len() == data.factions.len() {
                "En vigueur : partout".to_string()
            } else {
                format!("En vigueur chez : {}", places.join(", "))
            });
            if l.severity >= 2 {
                out.push("Crime grave : suivi chez les allies (extradition)".to_string());
            }
        }
        31 => {
            use vector_world::names::{self, Spec};
            let c = &data.cultures[i];
            let id = c.id.as_str();
            let n = |f: &dyn Fn(u64) -> String| (0..4u64).map(f).collect::<Vec<_>>().join(", ");
            out.push(format!(
                "{} : les noms de personnes et de lieux de cette culture suivent ses propres syllabes.",
                c.name
            ));
            out.push(format!(
                "{} combinaisons de prenoms et de noms de famille.",
                names::combinations(id)
            ));
            out.push(format!(
                "Prenoms d'hommes : {}",
                n(&|h| names::given_name(id, false, h * 5 + 1))
            ));
            out.push(format!(
                "Prenoms de femmes : {}",
                n(&|h| names::given_name(id, true, h * 5 + 2))
            ));
            out.push(format!(
                "Noms de famille : {}",
                n(&|h| names::surname(id, h * 5 + 3))
            ));
            out.push(format!(
                "Surnoms : {}",
                n(&|h| names::nickname(id, h % 2 == 1, h * 5 + 4))
            ));
            out.push(format!(
                "Titres : {}",
                n(&|h| names::title(id, h % 2 == 1, h * 5 + 5))
            ));
            for (label, kind) in [
                ("Villes", "city"),
                ("Colonies", "colony"),
                ("Stations", "station"),
                ("Fleuves", "river"),
                ("Montagnes", "mountain"),
                ("Regions", "region"),
                ("Entreprises", "company"),
                ("Vaisseaux", "ship_civil_f"),
            ] {
                out.push(format!(
                    "{label} : {}",
                    n(&|h| Spec::new(kind)
                        .culture(Some(id))
                        .host("Karon")
                        .name(h * 7 + 11))
                ));
            }
            out.push(
                "Les mondes morts portent un nom de catalogue (etoile + chiffre romain), les mondes vivants un nom evocateur, les geantes un nom de titan."
                    .to_string(),
            );
        }
        32 => {
            let e = &data.chemistry.elements[i];
            out.push(format!(
                "{} ({})  numero atomique {}",
                e.name, e.symbol, e.z
            ));
            out.push(e.text.clone());
            out.push(format!(
                "Masse molaire : {:.3} g/mol   electronegativite : {:.2}",
                e.mass, e.en
            ));
            out.push(format!(
                "Fusion : {:.0} K   ebullition : {:.0} K",
                e.melt, e.boil
            ));
            out.push(format!(
                "Abondance solaire : 10^{:.1} atomes pour 10^12 d'hydrogene",
                e.abund
            ));
            let found: Vec<String> = data
                .chemistry
                .compounds
                .iter()
                .enumerate()
                .filter(|(c, _)| data.compound_atoms(*c).iter().any(|(s, _)| *s == e.symbol))
                .map(|(_, c)| c.name.clone())
                .collect();
            if !found.is_empty() {
                out.push(format!("Entre dans : {}", found.join(", ")));
            }
        }
        33 => {
            let c = &data.chemistry.compounds[i];
            out.push(format!("{}  [{}]", c.name, c.class));
            out.push(c.text.clone());
            out.push(format!(
                "Formule : {}   masse molaire : {:.2} g/mol",
                c.formula,
                data.molar_mass(i)
            ));
            out.push(format!(
                "Fusion : {:.0} K   ebullition : {:.0} K   a 288 K : {}",
                c.melt,
                c.boil,
                c.state(288.0)
            ));
            out.push(format!("Enthalpie de formation : {:.1} kJ/mol", c.hf));
            let mut risks = Vec::new();
            if c.limit > 0.0 {
                risks.push(format!(
                    "toxique au-dela de {:.4} % de l'air",
                    c.limit * 100.0
                ));
            }
            if c.corrosive > 0.0 {
                risks.push(format!("corrosif (indice {:.2})", c.corrosive));
            }
            if c.ignite > 0.0 {
                risks.push(format!(
                    "inflammable : auto-inflammation {:.0} K, brule des {:.1} % dans l'air",
                    c.ignite,
                    c.lel * 100.0
                ));
            }
            if c.oxidizer {
                risks.push("comburant : attise les feux".to_string());
            }
            if c.class == "explosif" {
                risks.push("explosif".to_string());
            }
            if !risks.is_empty() {
                out.push(format!("Dangers : {}", risks.join(" ; ")));
            }
            let res: Vec<String> = c
                .resources
                .iter()
                .filter_map(|r| res_name(data, r))
                .collect();
            if !res.is_empty() {
                out.push(format!("Ressource : {}", res.join(", ")));
            }
            let ins: Vec<String> = data
                .reactions
                .iter()
                .filter(|r| {
                    r.reactants
                        .iter()
                        .chain(&r.products)
                        .any(|(x, _)| *x == c.id)
                        || r.catalyst == c.id
                })
                .map(|r| r.name.clone())
                .collect();
            if !ins.is_empty() {
                out.push(format!("Reactions : {}", ins.join(", ")));
            }
        }
        34 => {
            let r = &data.reactions[i];
            let side = |l: &[(String, u32)]| -> String {
                l.iter()
                    .map(|(c, n)| {
                        let f = data
                            .compound_index(c)
                            .map_or(c.clone(), |x| data.chemistry.compounds[x].formula.clone());
                        if *n > 1 {
                            format!("{n} {f}")
                        } else {
                            f
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" + ")
            };
            out.push(format!("{}  [{}]", r.name, r.kind));
            out.push(r.text.clone());
            out.push(format!("{}  ->  {}", side(&r.reactants), side(&r.products)));
            let dh = data.reaction_enthalpy(r);
            out.push(format!(
                "Enthalpie : {dh:+.1} kJ/mol ({})   energie : {:+.2} MJ/kg de reactifs",
                if dh < 0.0 {
                    "DEGAGE de la chaleur"
                } else {
                    "ABSORBE de la chaleur"
                },
                vector_sim::chemistry::energy_density(data, r)
            ));
            let mut cond = format!(
                "Activation : {:.0} K   pression minimale : {:.1} bar",
                r.t_act, r.p_min
            );
            if let Some(c) = data.compound_index(&r.catalyst) {
                cond.push_str(&format!(
                    "   catalyseur : {}",
                    data.chemistry.compounds[c].name
                ));
            }
            out.push(cond);
            if let Some(isp) = vector_sim::chemistry::specific_impulse(data, r) {
                out.push(format!(
                    "Comme ergol : impulsion specifique {isp:.0} s (vitesse d'ejection {:.0} m/s)",
                    isp * 9.80665
                ));
            }
            let held = |c: &str| {
                data.compound_index(c).is_some_and(|x| {
                    data.chemistry.compounds[x]
                        .resources
                        .iter()
                        .any(|r| data.resource_index(r).is_some())
                })
            };
            let reach = r.t_act <= vector_sim::chemistry::LAB_T_MAX
                && r.p_min <= vector_sim::chemistry::LAB_P_MAX;
            let runnable = reach && r.reactants.iter().chain(&r.products).all(|(c, _)| held(c));
            out.push(if runnable {
                "Laboratoire du bord : REALISABLE (touche L ou clic sur LANCER UN LOT : 100 kg de reactifs tires de la soute, energie prise sur la batterie)".to_string()
            } else {
                "Laboratoire du bord : hors de portee du reacteur (2400 K / 300 bar max) ou substance sans ressource".to_string()
            });
        }
        38 => {
            let c = &data.chemistries[i];
            out.push(format!("{}  (biochimie)", c.name));
            out.push(c.text.clone());
            out.push(format!(
                "Squelette : {}   solvant : {}   energie : {}",
                c.backbone, c.solvent, c.energy
            ));
            out.push(format!(
                "Temperature viable : {:.0} a {:.0} K",
                c.t.0, c.t.1
            ));
            let names: Vec<String> = c
                .species
                .iter()
                .filter_map(|s| data.species_index(s).map(|k| data.species[k].name.clone()))
                .collect();
            out.push(format!("Especes : {}", names.join(", ")));
        }
        47 => {
            let s = &data.scenarios[i];
            out.push(s.name.clone());
            out.push(s.text.clone());
            out.push(format!(
                "Difficulte indicative : {}/5   credits de depart : {:.0}   {}",
                s.difficulty,
                s.credits,
                if s.wreck {
                    "depart au sol, a pied, sur un vaisseau hors d'etat de voler"
                } else {
                    "depart en vol, pres d'une station"
                }
            ));
            let ships: Vec<String> = s
                .ships
                .iter()
                .filter_map(|(h, _)| data.hull_index(h).map(|k| data.hulls[k].name.clone()))
                .collect();
            out.push(format!("Vaisseaux au choix : {}", ships.join(", ")));
            let cargo: Vec<(String, f64)> = s.cargo.clone();
            out.push(format!("Soute : {}", fmt_list(data, &cargo)));
            let roles: Vec<String> = s
                .crew
                .iter()
                .filter_map(|r| {
                    data.roles
                        .iter()
                        .find(|ro| ro.id == *r)
                        .map(|ro| ro.name.clone())
                })
                .collect();
            out.push(format!(
                "Equipage : {} ({} points a depenser)",
                roles.join(", "),
                s.crew_points
            ));
            if !s.techs.is_empty() {
                out.push(format!(
                    "Technologies connues : {}",
                    names_of(data, &s.techs, |d, t| d
                        .tech_index(t)
                        .map(|k| d.techs[k].name.clone()))
                ));
            }
            if s.wanted_days > 0.0 {
                out.push(format!(
                    "Recherche par le territoire de depart pendant {:.0} jours.",
                    s.wanted_days
                ));
            }
            if s.wreck {
                out.push(
                    "Pour repartir : pieces detachees et carburant (epaves, campements, ruines), puis l'ingenieur repare ; I puis F verse le carburant de la soute."
                        .to_string(),
                );
            }
            out.push("Se choisit au menu principal : Nouvelle partie.".to_string());
        }
        48 => out = crate::codex_sky::geology_entry(data, i),
        49 => out = crate::codex_sky::cosmos_entry(data, i),
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
            if !m.yields.is_empty() {
                let list: Vec<String> = m
                    .yields
                    .iter()
                    .filter_map(|(res, kg, rarity)| {
                        res_name(data, res)
                            .map(|n| format!("{n} {kg:.1} kg ({:.0} %)", rarity * 100.0))
                    })
                    .collect();
                out.push(format!(
                    "Creuser (X) donne : {} - selon la physique du monde (densite, activite, froid)",
                    list.join(", ")
                ));
            }
            if let Some(u) = data.material_index(&m.under) {
                out.push(format!("Creuse, laisse a nu : {}", data.materials[u].name));
            }
            if m.harm > 0.0 {
                out.push(format!(
                    "Dangereux : {:.1} % de sante par tour (un liquide brule a cote)",
                    m.harm * 100.0
                ));
            }
            if m.slick > 0.0 {
                out.push(format!(
                    "Glissant : {:.0} % de deraper d'une case",
                    m.slick * 100.0
                ));
            }
            if m.fertility > 0.0 {
                out.push(format!(
                    "Fertilite : {:.0} % (les plantes y poussent)",
                    m.fertility * 100.0
                ));
            }
            if !m.cost.is_empty() {
                let list: Vec<String> = m
                    .cost
                    .iter()
                    .filter_map(|(res, kg)| {
                        res_name(data, res).map(|n| format!("{kg:.0} kg de {n}"))
                    })
                    .collect();
                out.push(format!(
                    "Se bati (B mur, N sol) avec : {} - la case est a vous",
                    list.join(", ")
                ));
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
        30 => {
            let e = &data.exotics[i];
            out.push(e.name.clone());
            out.push(e.text.clone());
            if e.kind == "fermi" {
                out.push(format!(
                    "Une seule reponse est vraie par galaxie (graine) ; {} indices des anciens la revelent.",
                    vector_sim::exotic::CLUES_NEEDED
                ));
            } else {
                if e.kardashev > 0.0 {
                    out.push(format!(
                        "Echelle de Kardashev : K{:.1} (puissance {:.1e} W)",
                        e.kardashev,
                        vector_sim::exotic_field::power_of_kardashev(e.kardashev)
                    ));
                }
                let radius = vector_sim::exotic_field::field_radius(&e.id);
                if radius > 0.0 {
                    out.push(format!(
                        "Effet continu dans {:.0} 000 km : attraction, degats, paradoxe ou stase selon le phenomene.",
                        radius / 1e6
                    ));
                }
                out.push(format!(
                    "Genre : {}   danger {:.0} %   etude {:.0} a {:.0} points de recherche",
                    e.kind,
                    e.danger * 100.0,
                    e.research.0,
                    e.research.1
                ));
                if !e.loot.is_empty() {
                    let l: Vec<(String, f64)> = e
                        .loot
                        .iter()
                        .map(|(r, lo, hi)| (r.clone(), (lo + hi) / 2.0))
                        .collect();
                    out.push(format!("Butin moyen : {}", fmt_list(data, &l)));
                }
                out.push(
                    "On les trouve en explorant des anomalies ; les paradoxes de temps augmentent gains et dangers."
                        .to_string(),
                );
            }
        }
        37 => {
            let s = &data.surprises[i];
            out.push(s.name.clone());
            out.push(s.text.clone());
            out.push(format!(
                "Nouvelles : {}   frequence relative {:.1}   ne revient pas avant {:.0} jours",
                s.tag, s.weight, s.cooldown
            ));
            out.push(
                "Arrive seulement ici et maintenant, quand tout ceci est vrai a la fois :"
                    .to_string(),
            );
            for c in &s.when {
                out.push(format!("  - {}", describe_cond(data, c)));
            }
            for e in &s.effects {
                out.push(format!("Effet : {}", describe_effect(data, e)));
            }
            for (k, c) in s.choices.iter().enumerate() {
                let fx: Vec<String> = c.effects.iter().map(|e| describe_effect(data, e)).collect();
                out.push(format!(
                    "Choix {} ({:.0} jours pour repondre) : {} -> {}",
                    k + 1,
                    s.window,
                    c.label,
                    if fx.is_empty() {
                        "aucun effet".to_string()
                    } else {
                        fx.join(" ; ")
                    }
                ));
            }
            out.push(
                "Tire au sort chaque jour (graine, secteur, jour) : rare, local, jamais ecrit a l'avance."
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
        36 => {
            let f = &data.factions[i];
            out.push(vector_sim::macroeco::currency_name(data, Some(i)));
            out.push(format!(
                "Monnaie de {} : sa valeur en credits flotte avec l'inflation, les guerres, les troubles et les krachs de ses secteurs.",
                f.name
            ));
            out.push(format!(
                "Taxe sur les ventes {:.0} %   agressivite {:.0} %   change {:.1} % a l'achat comme a la vente",
                f.tax * 100.0,
                f.aggression * 100.0,
                vector_sim::macroeco::FX_FEE * 100.0
            ));
            out.push(
                "Change et comptes : banque (E a quai) : C achete 1000 CR de monnaie locale, V la vend."
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
            if g.warmth > 0.0 {
                out.push(format!(
                    "Chaleur : +{:.0} % de protection contre le froid (sous 250 K)",
                    g.warmth * 100.0
                ));
            }
            if g.noise > 0.0 {
                out.push(format!(
                    "Bruit : reveille les creatures endormies jusqu'a {:.0} cases",
                    g.noise
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
        39 => {
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
        40 => {
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
        41 => {
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
        42 => {
            let c = &data.history.causes[i];
            out.push(format!("{}  [poids {:.1}]", c.name, c.weight));
            out.push(c.text.clone());
            out.push("Une ruine tombee pendant cet effondrement le dit dans ses textes une fois traduits.".to_string());
        }
        43 => {
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
        44 => {
            let p = &data.body_parts[i];
            out.push(format!(
                "{}  ({})",
                p.name,
                if p.limb { "extremite" } else { "tronc et tete" }
            ));
            out.push(format!(
                "Part des coups recus : {:.0} %",
                p.hit / data.body_parts.iter().map(|q| q.hit).sum::<f64>() * 100.0
            ));
            let organs: Vec<String> = p
                .organs
                .iter()
                .filter_map(|o| data.organ_index(o).map(|k| data.organs[k].name.clone()))
                .collect();
            if !organs.is_empty() {
                out.push(format!("Abrite : {}", organs.join(", ")));
            }
            for (k, w) in &p.effects {
                out.push(format!("Handicap total : {k} {w:+.2}"));
            }
            let wounds: Vec<String> = data
                .wounds
                .iter()
                .filter(|w| w.parts.is_empty() || w.parts.contains(&p.id))
                .map(|w| w.name.clone())
                .collect();
            out.push(format!("Blessures possibles : {}", wounds.join(", ")));
        }
        45 => {
            let w = &data.wounds[i];
            out.push(format!(
                "{}  ({})",
                w.name,
                if w.internal { "interne" } else { "externe" }
            ));
            out.push(format!(
                "Saignement : {:.1} % du sang par heure au plus grave   douleur : {:.0} %   guerison : {:.0} jours",
                w.bleed * 100.0,
                w.pain * 100.0,
                w.heal_days
            ));
            out.push(format!("Cause : {}", w.cause));
            if w.infect > 0.0 {
                out.push(format!(
                    "Risque d'infection : {:.0} % par jour sans pansement",
                    w.infect * 100.0
                ));
            }
            if w.internal {
                out.push(
                    "Invisible sans medecin competent (3) ou infirmerie ; seule la chirurgie la ferme."
                        .to_string(),
                );
            } else if w.dress {
                out.push(
                    "Demande un pansement ou une attelle : sans soin elle guerit lentement et laisse une sequelle."
                        .to_string(),
                );
            }
            let places: Vec<String> = data
                .body_parts
                .iter()
                .filter(|p| w.parts.is_empty() || w.parts.contains(&p.id))
                .map(|p| p.name.clone())
                .collect();
            out.push(format!("Peut toucher : {}", places.join(", ")));
        }
        46 => {
            let d = &data.drugs[i];
            out.push(format!("{}  ({})", d.name, d.text));
            out.push(format!(
                "Dose : {:.2} kg de {}   demi-vie : {:.0} h",
                d.dose,
                res_name(data, &d.resource).unwrap_or_else(|| d.resource.clone()),
                d.half_life_h
            ));
            for (k, v) in &d.effects {
                let what = match k.as_str() {
                    "pain" => "soulage la douleur",
                    "clot" => "arrete le saignement",
                    "infection" => "combat l'infection",
                    "stress" => "calme (negatif : angoisse)",
                    "fatigue" => "tient eveille (negatif : endort)",
                    "rad" => "protege du rayonnement",
                    "heal" => "accelere la guerison",
                    "blood_now" => "restitue du sang",
                    "resp" => "deprime la respiration",
                    _ => "abime le foie par jour",
                };
                out.push(format!("Effet : {what} ({v:.2})"));
            }
            out.push(format!(
                "Tolerance : +{:.0} % par dose   dependance : +{:.0} % par dose",
                d.tolerance * 100.0,
                d.dependence * 100.0
            ));
            for (o, f) in &d.interactions {
                if let Some(k) = data.drug_index(o) {
                    out.push(format!(
                        "Interaction dangereuse avec {} (+{:.1} de pression de surdose)",
                        data.drugs[k].name, f
                    ));
                }
            }
            out.push(format!(
                "Surdose au-dessus d'un niveau de {:.1} (foie faible ou melange : plus tot).",
                vector_sim::body::OVERDOSE
            ));
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
            out.extend(biology_lines(data, i));
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

/// Biology of a species, from its mass and its place in the web: metabolism, life cycle,
/// behaviour and, for the life that is not carbon-water, its biochemistry.
fn biology_lines(data: &GameData, i: usize) -> Vec<String> {
    use vector_sim::biosphere::{behavior_of, life_of};
    let s = &data.species[i];
    let (l, b) = (life_of(s), behavior_of(data, s));
    let mut out = Vec::new();
    if let Some(c) = data.chemistry_of(i) {
        let c = &data.chemistries[c];
        out.push(format!(
            "Biochimie : {} (solvant {}, energie : {}), viable de {:.0} a {:.0} K",
            c.name, c.solvent, c.energy, c.t.0, c.t.1
        ));
    }
    out.push(format!(
        "Metabolisme {:.2} W (masse^0,75)   vie {:.1} ans   maturite {:.2} an   portee {}   reproduction {}",
        l.watts,
        l.lifespan_days / 365.25,
        l.maturity_days / 365.25,
        l.litter,
        if l.sexual { "sexuee" } else { "clonale" }
    ));
    let mut ways: Vec<String> = Vec::new();
    if b.pack {
        ways.push(format!("meute de {}", b.herd));
    } else if b.herd > 1 {
        ways.push(format!("troupeau de {}", b.herd));
    }
    if b.territory_km2 > 0.0 {
        ways.push(format!("territoire {:.1} km2", b.territory_km2));
    }
    if b.nocturnal {
        ways.push("nocturne".into());
    }
    if b.hibernates {
        ways.push(format!(
            "hiberne sous {:.0} K",
            vector_sim::biosphere::HIBERNATION_K
        ));
    }
    if !ways.is_empty() {
        out.push(format!("Comportement : {}", ways.join(", ")));
    }
    out
}

/// The card of a local variant of a species, generated from the data and the state of the world
/// (how long it has been isolated, what its place selected): the species' own entry follows.
pub fn variant_card(
    data: &GameData,
    v: &vector_sim::biosphere::Variant,
    world: &str,
) -> Vec<String> {
    let s = &data.species[v.species];
    let mut out = vec![
        format!(
            "{}  [{}]",
            v.name(data),
            if v.speciated() {
                "variante distincte"
            } else {
                "population locale"
            }
        ),
        format!(
            "Population de {} sur {} (biome : {}), isolee depuis {:.0} generations ; divergence {:.0} % du seuil de speciation.",
            s.name,
            world,
            data.biomes[v.biome].name,
            v.generations,
            v.divergence * 100.0
        ),
        format!(
            "Masse x{:.2} ({:.3} kg)   pelage {} {:.0} %   durcissement aux radiations {:.0} %",
            v.mass_factor(),
            s.body * v.mass_factor(),
            if v.tint >= 0.0 { "pale" } else { "sombre" },
            v.tint.abs() * 100.0,
            v.resist * 100.0
        ),
    ];
    let cat = if s.is_producer() { 12 } else { 13 };
    if let Some(k) = entries(data, cat).iter().position(|&x| x == v.species) {
        out.extend(entry(data, cat, k));
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

    /// A character the font draws: a glyph of its own, or a French accent over a letter.
    fn drawable(c: char) -> bool {
        c == '0'
            || vector_render::font::decompose(c).is_some()
            || vector_render::font::glyph(c) != vector_render::font::glyph('\u{1}')
    }

    #[test]
    fn generated_names_only_use_glyphs_of_the_font() {
        use vector_world::names::{self, Spec, KINDS};
        let d = data();
        for c in &d.cultures {
            for k in KINDS {
                for h in 0..300u64 {
                    let n = Spec::new(k)
                        .culture(Some(&c.id))
                        .host("Karon")
                        .label("Epave")
                        .name(h);
                    assert!(n.chars().all(drawable), "{k} ({}): {n:?}", c.id);
                }
            }
            for h in 0..300u64 {
                for s in [
                    names::given_name(&c.id, h % 2 == 0, h),
                    names::surname(&c.id, h),
                    names::nickname(&c.id, h % 2 == 0, h),
                    names::title(&c.id, h % 2 == 0, h),
                ] {
                    assert!(s.to_uppercase().chars().all(drawable), "{s:?}");
                }
            }
        }
        for k in 0..count(&d, 31) {
            for l in entry(&d, 31, k) {
                assert!(l.chars().all(drawable), "{l:?}");
            }
        }
    }

    #[test]
    fn every_culture_of_the_data_has_a_name_grammar() {
        let d = data();
        for c in &d.cultures {
            assert!(
                vector_world::names::book().culture(&c.id).is_some(),
                "culture {} absente de names.ron",
                c.id
            );
        }
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
    fn species_cards_carry_biology_and_xenobiology_has_its_own_category() {
        let d = data();
        let wolf = d.species_index("loup_des_steppes").expect("wolf");
        let k = entries(&d, 13)
            .iter()
            .position(|&x| x == wolf)
            .expect("fauna");
        let card = entry(&d, 13, k).join("\n");
        assert!(
            card.contains("Metabolisme") && card.contains("Comportement"),
            "{card}"
        );
        let worm = d.species_index("serpent_de_scorie").expect("xeno");
        let k = entries(&d, 13)
            .iter()
            .position(|&x| x == worm)
            .expect("fauna");
        assert!(entry(&d, 13, k)
            .join("\n")
            .contains("Silicones thermophiles"));
        assert_eq!(count(&d, 38), d.chemistries.len());
        let chem = entry(&d, 38, 0).join("\n");
        assert!(
            chem.contains("Serpent de scorie") && chem.contains("K"),
            "{chem}"
        );
        // A variant card is generated from the state of the world and embeds the species' entry.
        let steppe = d.biome_index("steppe").expect("biome");
        let v = vector_sim::biosphere::variant_after(&d, 5, 0.0, wolf, steppe, 1.0e5);
        let card = variant_card(&d, &v, "Kepler").join("\n");
        assert!(
            card.contains("Kepler") && card.contains("generations") && card.contains("Metabolisme")
        );
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

    #[test]
    fn material_entries_list_what_they_give_and_do() {
        let d = data();
        let text = |id: &str| {
            entry(&d, 28, d.material_index(id).expect(id)).join(
                "
",
            )
        };
        let roche = text("roche");
        assert!(
            roche.contains("Creuser (X) donne")
                && roche.contains("Fer")
                && roche.contains("laisse a nu"),
            "{roche}"
        );
        assert!(text("glace").contains("Glissant"));
        assert!(text("soufre").contains("Dangereux"));
        assert!(text("humus").contains("Fertilite"));
        assert!(text("pierre_taillee").contains("Se bati"));
        assert!(text("lave").contains("Dangereux"));
        let suit = entry(&d, 21, d.gear_index("exosquelette").unwrap()).join(
            "
",
        );
        assert!(suit.contains("Chaleur"), "{suit}");
    }
}
