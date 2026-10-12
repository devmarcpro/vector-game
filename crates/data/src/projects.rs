//! Terraforming and megastructure projects (see `data/projects.ron`): staged works of a colony.
//! A project is built stage after stage; each stage streams its materials out of the colony's
//! stores while a crew works and power is drawn, so a stage that lacks anything simply waits.
use crate::{DataError, GameData, SITE_TAGS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// What a project does when finished (the `kind` field of the data).
pub const PROJECT_KINDS: [&str; 11] = [
    "mirror", "soot", "comet", "seeding", "ring", "elevator", "dyson", "driller", "factory",
    "station", "shipyard",
];

/// Gases a comet can bring (ids of the gas table of the simulation).
pub const VOLATILE_GASES: [&str; 6] = ["H2O", "CO2", "NH3", "CH4", "N2", "H2"];

/// How a stage's materials scale with the world it is built on.
pub const STAGE_SCALES: [&str; 3] = ["", "taper", "radius"];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StageDef {
    pub name: String,
    /// Materials streamed out of the stores over the stage (resource id, kg).
    pub cost: Vec<(String, f64)>,
    pub days: f64,
    /// People working on the stage.
    pub crew: f64,
    /// kW drawn while it runs.
    pub power: f64,
    /// "" (none), "taper" (times the cable taper ratio of an elevator) or "radius" (times the
    /// radius of the world relative to the Earth: a ring is as long as the world is wide).
    pub scale: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectDef {
    pub id: String,
    pub name: String,
    pub kind: String,
    /// "terraformation" or "megastructure".
    pub category: String,
    pub text: String,
    pub sites: Vec<String>,
    pub stages: Vec<StageDef>,
    /// Can be ordered several times over (units): swarms, mirrors, comets...
    pub repeat: bool,
    pub max_units: u32,
    /// Kind-specific size of one unit: m2 of mirror, kg of soot or comet, kg of founding
    /// biomass, m2 of collectors, specific strength (J/kg) of the cable of an elevator, ...
    pub param: f64,
    /// Credits per day and unit.
    pub upkeep: f64,
    /// Share of the health lost per year and unit when nobody mends it.
    pub decay: f64,
    pub housing: f64,
    pub storage: f64,
    pub research: f64,
    pub port: f64,
    pub defense: f64,
    /// Cargo an elevator lifts, tonnes per day.
    pub payload: f64,
    /// Productivity gained per unit (capped in the colony).
    pub prod: f64,
    /// kg of resources a unit consumes per day to run (factories).
    pub inputs: Vec<(String, f64)>,
    /// kg of resources a unit brings in per day (drillers, factories).
    pub yields: Vec<(String, f64)>,
    /// What a comet is made of: (gas id, share of the mass).
    pub volatile: Vec<(String, f64)>,
}

impl ProjectDef {
    /// Materials of a whole project (all stages, scale 1), kg per resource id.
    pub fn total_cost(&self) -> Vec<(String, f64)> {
        let mut out: Vec<(String, f64)> = Vec::new();
        for s in &self.stages {
            for (r, kg) in &s.cost {
                match out.iter_mut().find(|(id, _)| id == r) {
                    Some((_, k)) => *k += kg,
                    None => out.push((r.clone(), *kg)),
                }
            }
        }
        out
    }

    pub fn total_days(&self) -> f64 {
        self.stages.iter().map(|s| s.days).sum()
    }
}

impl GameData {
    pub fn project_index(&self, id: &str) -> Option<usize> {
        self.projects.iter().position(|p| p.id == id)
    }

    /// Fails on duplicate ids, unknown kinds, sites, resources and gases, and on nonsense numbers.
    pub fn validate_projects(&self) -> Result<(), DataError> {
        let bad = |s: String| Err(DataError::Invalid(s));
        let mut ids = BTreeSet::new();
        for p in &self.projects {
            if !ids.insert(p.id.as_str()) {
                return bad(format!("projet en double: {}", p.id));
            }
            if !PROJECT_KINDS.contains(&p.kind.as_str()) {
                return bad(format!("projet {}: genre inconnu {}", p.id, p.kind));
            }
            if p.category != "terraformation" && p.category != "megastructure" {
                return bad(format!("projet {}: categorie invalide", p.id));
            }
            if p.stages.is_empty() || p.sites.is_empty() || p.param <= 0.0 || p.text.is_empty() {
                return bad(format!("projet invalide: {}", p.id));
            }
            if p.sites.iter().any(|t| !SITE_TAGS.contains(&t.as_str())) {
                return bad(format!("projet {}: site inconnu", p.id));
            }
            if p.repeat && p.max_units == 0 {
                return bad(format!("projet {}: max_units nul", p.id));
            }
            for s in &p.stages {
                if s.days <= 0.0
                    || s.crew <= 0.0
                    || s.power < 0.0
                    || s.cost.is_empty()
                    || !STAGE_SCALES.contains(&s.scale.as_str())
                {
                    return bad(format!("projet {}: etape invalide {}", p.id, s.name));
                }
                for (r, kg) in &s.cost {
                    if self.resource_index(r).is_none() || *kg <= 0.0 {
                        return bad(format!("projet {}: ressource invalide {r}", p.id));
                    }
                }
            }
            for (r, kg) in p.yields.iter().chain(&p.inputs) {
                if self.resource_index(r).is_none() || *kg <= 0.0 {
                    return bad(format!("projet {}: rendement invalide {r}", p.id));
                }
            }
            // A factory turns matter into other matter: it cannot make more than it eats.
            let (eaten, made): (f64, f64) = (
                p.inputs.iter().map(|(_, k)| k).sum(),
                p.yields.iter().map(|(_, k)| k).sum(),
            );
            if p.kind == "factory" && (p.inputs.is_empty() || made > eaten + 1e-9) {
                return bad(format!("projet {}: la masse n'est pas conservee", p.id));
            }
            for (g, f) in &p.volatile {
                if !VOLATILE_GASES.contains(&g.as_str()) || *f <= 0.0 {
                    return bad(format!("projet {}: gaz volatil invalide {g}", p.id));
                }
            }
            let share: f64 = p.volatile.iter().map(|(_, f)| f).sum();
            if p.kind == "comet" && (share - 1.0).abs() > 1e-6 {
                return bad(format!("projet {}: la comete doit totaliser 100 %", p.id));
            }
            if p.kind == "driller" && p.yields.is_empty() {
                return bad(format!("projet {}: une foreuse doit rapporter", p.id));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> GameData {
        GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
            .expect("data")
    }

    #[test]
    fn the_projects_of_the_game_are_valid_and_cover_every_kind() {
        let d = data();
        assert!(d.projects.len() >= 12);
        for k in PROJECT_KINDS {
            assert!(
                d.projects.iter().any(|p| p.kind == k),
                "no project of kind {k}"
            );
        }
        for p in &d.projects {
            // Every project is unlocked by exactly one technology.
            assert!(d.unlocking_tech("project", &p.id).is_some(), "{}", p.id);
            assert!(p.total_days() > 0.0 && !p.total_cost().is_empty());
        }
    }

    #[test]
    fn nonsense_projects_are_refused_at_loading() {
        let bad = |f: &dyn Fn(&mut GameData)| {
            let mut d = data();
            f(&mut d);
            assert!(d.validate().is_err());
        };
        bad(&|d| d.projects[0].kind = "teleporter".into());
        bad(&|d| d.projects[0].stages[0].cost[0].0 = "unobtainium".into());
        bad(&|d| d.projects[0].stages[0].days = 0.0);
        bad(&|d| d.projects[0].stages[0].crew = 0.0);
        bad(&|d| d.projects[0].stages[0].scale = "weird".into());
        bad(&|d| d.projects[0].sites = vec!["moon_base".into()]);
        bad(&|d| d.projects[0].stages.clear());
        let dup = d_clone_first();
        bad(&move |d| d.projects.push(dup.clone()));
        // A comet is made of 100 % of something, and a factory cannot create matter.
        let comet = data().project_index("comete_eau").expect("comet");
        bad(&move |d| d.projects[comet].volatile = vec![("H2O".into(), 0.5)]);
        let comet = data().project_index("comete_eau").expect("comet");
        bad(&move |d| d.projects[comet].volatile = vec![("XX".into(), 1.0)]);
        let fac = data().project_index("usine_orbitale").expect("factory");
        bad(&move |d| d.projects[fac].yields[0].1 = 1.0e9);
        // A project nobody can unlock is an error too.
        bad(&|d| d.techs[0].unlocks.push(("project".into(), "nope".into())));
    }

    fn d_clone_first() -> ProjectDef {
        data().projects[0].clone()
    }
}
