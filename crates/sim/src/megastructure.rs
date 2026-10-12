//! Projects of a colony: terraforming works and megastructures (see `data/projects.ron`).
//!
//! A project goes through its stages in order. A stage streams its materials out of the colony
//! stores as it advances (a stage that lacks one simply waits), needs a crew (which comes out of
//! the colony's workers, so a great work slows the factories) and draws power; a crowded colony
//! with a weak grid builds slowly. A finished unit does what its kind does, in physical terms:
//! mirrors add to the light a world receives, comets bring their mass, an elevator is a cable
//! whose thickness comes from the world's gravity and spin, a swarm of collectors yields the
//! luminosity of its star times the fraction of the sky it covers, and so on. Finished works wear
//! out, cost upkeep, and attract pirates and war.
use crate::colony::Colony;
use crate::terraform::{WorldSnap, SOLAR_CONSTANT};
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_data::projects::ProjectDef;
use vector_data::GameData;

const DAY: f64 = 86_400.0;
const YEAR: f64 = 31_557_600.0;
const AU: f64 = 1.495_978_707e11;
const R_EARTH: f64 = 6.371e6;
const L_SUN: f64 = 3.828e26;
/// Share of the light a collector turns into a beam to the colony.
pub const COLLECTOR_EFFICIENCY: f64 = 0.3;
/// Safety factor on the stress of an elevator cable.
pub const CABLE_SAFETY: f64 = 2.0;
/// Thickest base a cable may have relative to its top: beyond, it cannot be spun.
pub const MAX_TAPER: f64 = 1.0e5;
/// What a tonne lifted by a rocket costs more than one lifted by a cable, credits.
pub const LAUNCH_SAVING: f64 = 1500.0;
/// Most megastructures a colony can hold at once.
pub const MAX_PROJECTS: usize = 24;
/// Cap on the productivity that works can add.
const PROD_CAP: f64 = 0.6;

/// A project of a colony.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProjectInst {
    /// Index into `GameData::projects`.
    pub def: usize,
    /// Species seeded (index into `GameData::species`) for a seeding, else 0.
    pub arg: usize,
    /// Units ordered and units finished.
    pub target: u32,
    pub units: u32,
    pub stage: usize,
    /// Progress of the current stage, 0..1.
    pub progress: f64,
    /// Cable taper ratio of an elevator (cost of the "taper" stages), 1 otherwise.
    pub taper: f64,
    /// State of the finished units, 0..1.
    pub health: f64,
    pub enabled: bool,
    /// What held the work up the last tick: 0 nothing, 1 materials, 2 power, 3 crew.
    pub wait: u8,
    /// The resource missing when `wait` is 1 (index), else 0.
    pub short: u16,
}

impl ProjectInst {
    pub fn active(&self) -> bool {
        self.enabled && self.units < self.target
    }

    /// Whether finished units work now.
    pub fn working(&self) -> bool {
        self.units > 0 && self.enabled && self.health > 0.2
    }
}

/// What the works of a colony give and ask this tick.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Effects {
    pub housing: f64,
    pub storage: f64,
    pub research: f64,
    pub port: f64,
    pub defense: f64,
    /// Beamed power from collectors, kW.
    pub power: f64,
    /// Crew and power the stages under way ask for (people, kW).
    pub crew: f64,
    pub demand: f64,
    pub prod: f64,
    /// Credits a day that elevators save on launches.
    pub income: f64,
    /// Credits a day of upkeep.
    pub upkeep: f64,
}

/// What an elevator on a world looks like.
#[derive(Clone, Debug, PartialEq)]
pub struct ElevatorDesign {
    /// Radius of the geostationary orbit, m.
    pub r_geo: f64,
    /// Length of the cable from the ground to the station, m.
    pub length: f64,
    /// Ratio of the section at the top of the taper (the station) to the base: the factor the
    /// thickness of the cable grows by, so the factor its mass is multiplied by.
    pub taper: f64,
}

/// The cable of an elevator from the surface to the synchronous orbit. The tension of a
/// cable falls from the station down; for a constant stress s the section follows
/// A(r) ~ exp(rho * (Phi(r) - Phi(R)) / s) where Phi is the potential in the frame that turns
/// with the world (gravity plus the centrifugal term). With the specific strength s / rho in
/// J/kg this gives the taper ratio, which is what makes the cable's mass.
pub fn elevator_design(snap: &WorldSnap, specific_strength: f64) -> Result<ElevatorDesign, String> {
    if !snap.rocky {
        return Err("pas de surface solide".into());
    }
    let r_geo = snap
        .geostationary()
        .ok_or("pas d'orbite synchrone stable (monde verrouille ou rotation trop lente)")?;
    let gm = snap.gm();
    let w = std::f64::consts::TAU / snap.spin;
    let phi = |r: f64| -gm / r - 0.5 * w * w * r * r;
    let dphi = phi(r_geo) - phi(snap.radius);
    let taper = (CABLE_SAFETY * dphi / specific_strength).exp();
    if !(taper.is_finite() && taper <= MAX_TAPER) {
        return Err(format!(
            "le cable ne tient pas : section {taper:.1e} fois plus epaisse a la base"
        ));
    }
    Ok(ElevatorDesign {
        r_geo,
        length: r_geo - snap.radius,
        taper,
    })
}

/// Radius of the orbit of a swarm of collectors around the star of a world, m: inside the
/// orbit of the world, never closer than a twentieth of an AU.
pub fn swarm_radius(snap: &WorldSnap) -> f64 {
    (0.5 * snap.star_dist).max(0.05 * AU)
}

/// Power of `units` collectors of `area` m^2 each, W: the luminosity of the star times the
/// share of the sphere at the swarm's radius that they cover, times the efficiency.
pub fn swarm_power(snap: &WorldSnap, units: f64, area: f64) -> f64 {
    let r = swarm_radius(snap);
    let share = (units * area / (4.0 * std::f64::consts::PI * r * r)).min(1.0);
    snap.luminosity * L_SUN * share * COLLECTOR_EFFICIENCY
}

/// Kardashev's scale from the power a civilisation commands (Sagan's interpolation: 0 at
/// a megawatt, 1 at ten quadrillion watts, 2 at the luminosity of a star, 3 at a galaxy's).
pub fn kardashev(power_w: f64) -> f64 {
    ((power_w.max(1.0)).log10() - 6.0).max(0.0) / 10.0
}

impl ProjectInst {
    pub fn new(def: usize, arg: usize, units: u32, taper: f64) -> ProjectInst {
        ProjectInst {
            def,
            arg,
            target: units,
            units: 0,
            stage: 0,
            progress: 0.0,
            taper,
            health: 1.0,
            enabled: true,
            wait: 0,
            short: 0,
        }
    }
}

/// How many times a stage's listed materials this project needs on this world.
fn stage_scale(snap: Option<&WorldSnap>, p: &ProjectInst, scale: &str) -> f64 {
    match scale {
        "taper" => p.taper.max(1.0),
        "radius" => snap.map_or(1.0, |s| s.radius / R_EARTH).max(0.05),
        _ => 1.0,
    }
}

impl Colony {
    /// Finished and under-way works as the colony feels them this tick.
    pub fn mega_effects(&self, data: &GameData) -> Effects {
        let mut e = Effects::default();
        let snap = self.terra.as_ref().map(|t| &t.snap);
        let mut prod = 0.0;
        for p in &self.projects {
            let d = &data.projects[p.def];
            if p.active() {
                if let Some(s) = d.stages.get(p.stage) {
                    e.crew += s.crew;
                    e.demand += s.power;
                }
            }
            if !p.working() {
                continue;
            }
            let k = f64::from(p.units) * p.health;
            e.housing += d.housing * k;
            e.storage += d.storage * k;
            e.research += d.research * k;
            e.port += d.port * k;
            e.defense += d.defense * k;
            prod += d.prod * k;
            e.upkeep += d.upkeep * f64::from(p.units);
            match (d.kind.as_str(), snap) {
                ("dyson", Some(s)) => e.power += swarm_power(s, k, d.param) / 1000.0,
                ("elevator", _) => {
                    let traffic = self.pop / (self.pop + 300.0);
                    e.income += d.payload * LAUNCH_SAVING * traffic * p.health;
                }
                _ => {}
            }
        }
        e.prod = prod.min(PROD_CAP);
        e
    }

    /// Open-air housing a terraformed world gives: the share of the outdoors fit for people,
    /// squared, times the land of a world of this size that people can use.
    pub fn open_air_housing(&self) -> f64 {
        match &self.terra {
            Some(t) if t.snap.rocky && t.version > 0 => {
                let land = t.snap.area() * (1.0 - t.sea_fraction()) * 1.0e-7;
                t.habitability().powi(2) * land.min(2.0e6)
            }
            _ => 0.0,
        }
    }

    /// Power sent by the collectors, W (before any use): for the empire's Kardashev figure.
    pub fn beamed_power_w(&self, data: &GameData) -> f64 {
        self.mega_effects(data).power * 1000.0
    }

    /// Advance the works by `days`: progress, materials, wear, upkeep, production of drillers
    /// and factories, and the terraforming itself. `power_sat` and `staffing` (0..1) come from
    /// the colony's grid and workforce this tick; `proc_w` is the power the process plants have.
    pub(crate) fn mega_tick(
        &mut self,
        data: &GameData,
        days: f64,
        power_sat: f64,
        staffing: f64,
        proc_w: f64,
        now: f64,
    ) {
        let mut notes: Vec<String> = Vec::new();
        let rate = power_sat.min(staffing);
        let snap = self.terra.as_ref().map(|t| t.snap.clone());
        let mut finished: Vec<usize> = Vec::new();
        for pi in 0..self.projects.len() {
            let def = &data.projects[self.projects[pi].def];
            let (active, stage, wait_before) = {
                let p = &self.projects[pi];
                (p.active(), p.stage, p.wait)
            };
            let _ = wait_before;
            if !active {
                continue;
            }
            let Some(st) = def.stages.get(stage) else {
                continue;
            };
            let scale = stage_scale(snap.as_ref(), &self.projects[pi], &st.scale);
            let mut dp = days / st.days * rate;
            let mut wait = 0u8;
            let mut short = 0u16;
            if power_sat < 0.999 {
                wait = 2;
            }
            if staffing < power_sat && staffing < 0.999 {
                wait = 3;
            }
            // Materials flow with the progress: the stage goes as far as the stores allow.
            for (r, kg) in &st.cost {
                let Some(i) = data.resource_index(r) else {
                    continue;
                };
                let need = kg * scale * dp;
                if need > 1e-9 && self.storage[i] < need {
                    dp *= (self.storage[i] / need).max(0.0);
                    wait = 1;
                    short = i as u16;
                }
            }
            let p = &mut self.projects[pi];
            p.wait = wait;
            p.short = short;
            let dp = dp.min(1.0 - p.progress);
            if dp <= 0.0 {
                continue;
            }
            for (r, kg) in &st.cost {
                if let Some(i) = data.resource_index(r) {
                    self.storage[i] = (self.storage[i] - kg * scale * dp).max(0.0);
                }
            }
            let p = &mut self.projects[pi];
            p.progress += dp;
            if p.progress >= 1.0 - 1e-9 {
                p.progress = 0.0;
                p.stage += 1;
                if p.stage >= def.stages.len() {
                    p.stage = 0;
                    p.units += 1;
                    if p.units == 1 {
                        p.health = 1.0;
                    }
                    finished.push(pi);
                }
            }
        }
        for pi in finished {
            let (d, arg) = (&data.projects[self.projects[pi].def], self.projects[pi].arg);
            self.unit_done(data, d, arg, &mut notes);
        }
        // Finished works: wear, upkeep, drillers and factories at work.
        let mut cost = 0.0;
        let paid = self.treasury >= 0.0;
        for pi in 0..self.projects.len() {
            let def = &data.projects[self.projects[pi].def];
            let (units, working) = (self.projects[pi].units, self.projects[pi].working());
            if units == 0 {
                continue;
            }
            cost += def.upkeep * f64::from(units) * days;
            // Wear against repairs: unpaid upkeep or an idle crew lets the structure decay.
            let wear = def.decay * f64::from(units).sqrt() * days / 365.25;
            let p = &mut self.projects[pi];
            if paid && staffing > 0.3 {
                p.health = (p.health - 0.1 * wear + 0.002 * days).clamp(0.0, 1.0);
            } else {
                p.health = (p.health - wear).max(0.0);
            }
            if !working {
                continue;
            }
            let k = f64::from(units) * p.health;
            let run = (days * rate).min(days);
            // Inputs and outputs flow while stores and room allow.
            let mut share = run * k;
            for (r, kg) in &def.inputs {
                if let Some(i) = data.resource_index(r) {
                    share = share.min(self.storage[i] / kg);
                }
            }
            if !def.yields.is_empty() {
                let out: f64 = def.yields.iter().map(|(_, kg)| kg).sum();
                let eaten: f64 = def.inputs.iter().map(|(_, kg)| kg).sum();
                let net = out - eaten;
                if net > 0.0 {
                    share = share.min(self.free_space() / net);
                }
                if def.kind == "driller" {
                    // A drill needs a belt to eat.
                    let belt = snap.as_ref().is_none_or(|s| s.belt);
                    if !belt {
                        share = 0.0;
                    }
                }
                if share > 1e-9 {
                    for (r, kg) in &def.inputs {
                        if let Some(i) = data.resource_index(r) {
                            self.storage[i] -= kg * share;
                        }
                    }
                    for (r, kg) in &def.yields {
                        if let Some(i) = data.resource_index(r) {
                            self.storage[i] += kg * share;
                        }
                    }
                }
            }
        }
        self.treasury -= cost;
        // The world itself.
        let mut tnotes = Vec::new();
        if let Some(t) = &mut self.terra {
            t.step(data, days * DAY, proc_w);
            tnotes.append(&mut t.notes);
            self.terraform = t.progress();
        }
        notes.extend(tnotes);
        for n in notes {
            self.log_line(now, n);
        }
    }

    /// A unit has been finished: do what its kind does.
    fn unit_done(&mut self, data: &GameData, d: &ProjectDef, arg: usize, notes: &mut Vec<String>) {
        notes.push(format!("Projet : une unité de {} est achevée.", d.name));
        let Some(t) = self.terra.as_mut() else {
            return;
        };
        match d.kind.as_str() {
            "mirror" => t.mirror += d.param,
            "soot" => t.soot += d.param,
            "comet" => {
                let parts: Vec<(String, f64)> = d
                    .volatile
                    .iter()
                    .map(|(g, f)| (g.clone(), f * d.param))
                    .collect();
                t.deliver(&parts);
                notes.push(format!(
                    "Impact : {:.1e} kg de matière frappent {} (la chaleur d'impact se dissipe sur des années).",
                    d.param, self.body_name
                ));
            }
            "seeding" if arg < data.species.len() => {
                t.seed(data, arg, d.param);
                self.introduced.push(arg);
            }
            _ => {}
        }
    }
}

impl Sim {
    /// Start (or add units to) project `def` in colony `ci`. `arg` is the species of a seeding.
    /// Can colony `ci` take up project `def` (with `arg`, the species of a seeding)? On success
    /// gives the taper ratio of the cable of an elevator (1 for anything else).
    pub fn project_check(&self, ci: usize, def: usize, arg: usize) -> Result<f64, String> {
        let d = self.data.projects.get(def).ok_or("projet inconnu")?;
        if !self.is_unlocked("project", &d.id) {
            return Err("technologie non recherchee (F4, ou regle: technologies)".into());
        }
        let c = self.colonies.get(ci).ok_or("colonie inconnue")?;
        if !d.sites.iter().any(|s| c.tags.contains(s)) {
            return Err("site inadapte a ce projet".into());
        }
        let terra = c.terra.as_ref();
        let needs_climate = matches!(d.kind.as_str(), "mirror" | "soot" | "comet" | "seeding");
        if needs_climate && !terra.is_some_and(|t| t.snap.rocky) {
            return Err("ce monde n'a ni climat ni atmosphere a transformer".into());
        }
        let mut taper = 1.0;
        match d.kind.as_str() {
            "elevator" => {
                let t = terra.ok_or("pas de surface solide")?;
                taper = elevator_design(&t.snap, d.param)?.taper;
            }
            "driller" => {
                if !terra.is_some_and(|t| t.snap.belt) {
                    return Err("aucune ceinture d'asteroides dans ce systeme".into());
                }
            }
            "seeding" => {
                let sp = self.data.species.get(arg).ok_or("espece inconnue")?;
                if !sp.is_producer() {
                    return Err("seuls les producteurs primaires peuvent etre semes".into());
                }
            }
            _ => {}
        }
        Ok(taper)
    }

    pub fn colony_project_start(
        &mut self,
        ci: usize,
        def: usize,
        units: u32,
        arg: usize,
    ) -> Result<(), String> {
        let taper = self.project_check(ci, def, arg)?;
        let data = std::sync::Arc::clone(&self.data);
        let d = &data.projects[def];
        let units = units.max(1);
        let c = &mut self.colonies[ci];
        if let Some(p) = c
            .projects
            .iter_mut()
            .find(|p| p.def == def && (p.arg == arg || d.kind != "seeding"))
        {
            if !d.repeat {
                return Err("deja commande".into());
            }
            if p.target + units > d.max_units {
                return Err("nombre maximal d'unites atteint".into());
            }
            p.target += units;
        } else {
            if c.projects.len() >= MAX_PROJECTS {
                return Err("trop de projets".into());
            }
            c.projects
                .push(ProjectInst::new(def, arg, units.min(d.max_units), taper));
        }
        let t = self.time;
        let c = &mut self.colonies[ci];
        c.log_line(
            t,
            format!("Projet lancé : {} ({} unité(s)).", d.name, units),
        );
        Ok(())
    }

    /// Stop project `pi` of colony `ci`: unfinished units are abandoned (the materials already
    /// used are lost); finished ones stay, and so does what they did to the world.
    pub fn colony_project_cancel(&mut self, ci: usize, pi: usize) -> Result<(), &'static str> {
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        let p = c.projects.get_mut(pi).ok_or("projet inconnu")?;
        if p.units == 0 {
            c.projects.remove(pi);
        } else {
            p.target = p.units;
            p.stage = 0;
            p.progress = 0.0;
        }
        Ok(())
    }

    pub fn colony_project_toggle(&mut self, ci: usize, pi: usize) -> Result<(), &'static str> {
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        let p = c.projects.get_mut(pi).ok_or("projet inconnu")?;
        p.enabled = !p.enabled;
        Ok(())
    }

    /// Pirates and war against the works of colony `ci` over the days (t0, t1]: a raid is more
    /// likely the more there is to take, in a sector at war, and when the colony is restless;
    /// turrets and stations turn it away. Raids hurt the structures, they never remove the unit
    /// at once: a wrecked work (health 0) is lost.
    pub(crate) fn apply_mega_threats(&mut self, ci: usize, t0: f64, t1: f64) {
        let data = std::sync::Arc::clone(&self.data);
        let (d0, d1) = ((t0 / DAY).floor() as i64, (t1 / DAY).floor() as i64);
        if d1 <= d0 {
            return;
        }
        let c = &self.colonies[ci];
        let exposure: f64 = c
            .projects
            .iter()
            .filter(|p| p.units > 0 && data.projects[p.def].category == "megastructure")
            .map(|p| 1.0 + 0.5 * f64::from(p.units).ln_1p())
            .sum();
        if exposure <= 0.0 {
            return;
        }
        let at_war = c.star == self.here
            && crate::politics::turmoil(&data, &self.galaxy, self.galaxy.seed, self.here_pos, t1)
                .at_war;
        let guns: f64 = c
            .buildings
            .iter()
            .filter(|b| b.built() && b.enabled && b.health > 0.2)
            .map(|b| data.buildings[b.def].defense * b.health)
            .sum::<f64>()
            + c.mega_effects(&data).defense;
        let (seed, id, unrest) = (c.system_seed, c.id, c.unrest);
        for day in (d0 + 1).max(d1 - 59)..=d1 {
            let roll = |salt: i64| {
                (vector_core::hash_coord(seed ^ 0x4D45, i64::from(id) * 8 + salt, 0, day) >> 11)
                    as f64
                    / (1u64 << 53) as f64
            };
            let chance = 0.002 * exposure * (1.0 + 2.0 * unrest) * if at_war { 3.0 } else { 1.0 }
                / (1.0 + guns);
            if roll(1) >= chance {
                continue;
            }
            let c = &mut self.colonies[ci];
            let live: Vec<usize> = (0..c.projects.len())
                .filter(|&i| {
                    c.projects[i].units > 0
                        && data.projects[c.projects[i].def].category == "megastructure"
                })
                .collect();
            if live.is_empty() {
                continue;
            }
            let pi = live[(roll(2) * live.len() as f64) as usize % live.len()];
            let hit = 0.1 + 0.3 * roll(3);
            let p = &mut c.projects[pi];
            p.health = (p.health - hit).max(0.0);
            let name = data.projects[p.def].name.clone();
            let who = if at_war {
                "Bombardement"
            } else {
                "Raid de pirates"
            };
            let lost = p.health <= 0.0;
            if lost {
                p.units = p.units.saturating_sub(1);
                p.target = p.target.max(p.units);
                p.health = if p.units > 0 { 0.5 } else { 1.0 };
            }
            let text = if lost {
                format!("{who} : une unité de {name} est détruite.")
            } else {
                format!(
                    "{who} sur {name} : structure endommagée ({:.0} %).",
                    p.health * 100.0
                )
            };
            c.log_line(day as f64 * DAY, text);
        }
    }

    /// The empire's power, W: what its colonies generate and what its collectors beam.
    pub fn empire_power_w(&self) -> f64 {
        self.colonies
            .iter()
            .map(|c| c.stats.power_supply.max(0.0) * 1000.0)
            .sum()
    }

    /// Where the player's civilisation stands on the Kardashev scale.
    pub fn empire_kardashev(&self) -> f64 {
        kardashev(self.empire_power_w())
    }

    /// Species that a seeding in the colony just introduced, to bring into the food web of its
    /// territory (see `ecology`).
    pub(crate) fn apply_introductions(&mut self, ci: usize) {
        let list = std::mem::take(&mut self.colonies[ci].introduced);
        if list.is_empty() {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let Some(map) = self
            .colony_world(ci)
            .map(|(m, c)| (m.biome_at(c.0, c.1), m))
        else {
            return;
        };
        let biome = map.0;
        if let Some(eco) = self.colony_ecology(ci) {
            for sp in list {
                if data.species[sp].habitat.contains(&data.biomes[biome].id) {
                    eco.introduce(sp, 0, 1.0e3);
                }
            }
        }
    }
}

/// Sunlight on a square metre at a distance (m) from a star of luminosity `l` (solar units).
pub fn irradiance(l: f64, distance: f64) -> f64 {
    SOLAR_CONSTANT * l * (AU / distance).powi(2)
}

/// Years a stage lasts at full speed (for the display).
pub fn project_years(d: &ProjectDef) -> f64 {
    d.total_days() * DAY / YEAR
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::tests::founded;
    use crate::colony::BuildingInst;
    use crate::terraform::tests::{any_producer, data, earthlike};
    use std::sync::Arc;

    fn fill(s: &mut Sim, ci: usize, items: &[(&str, f64)]) {
        let d = Arc::clone(&s.data);
        for (id, kg) in items {
            s.colonies[ci].storage[d.resource_index(id).expect("res")] += kg;
        }
    }

    fn stock(s: &Sim, ci: usize, id: &str) -> f64 {
        s.colonies[ci].storage[s.data.resource_index(id).expect("res")]
    }

    /// A colony with a big crew, plenty of power and every technology.
    fn busy() -> (Sim, usize) {
        let (mut s, ci) = founded();
        s.researched.iter_mut().for_each(|r| *r = true);
        let d = Arc::clone(&s.data);
        let plant = d.building_index("geothermal_plant").expect("plant");
        let dome = d.building_index("habitat_dome").expect("dome");
        let c = &mut s.colonies[ci];
        for _ in 0..40 {
            c.buildings.push(BuildingInst {
                def: plant,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        for _ in 0..60 {
            c.buildings.push(BuildingInst {
                def: dome,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        let warehouse = d.building_index("warehouse").expect("warehouse");
        for _ in 0..300 {
            c.buildings.push(BuildingInst {
                def: warehouse,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        // A town this size has its metro and its spare parts (see `colony_works`: commuting, wear).
        let metro = d.building_index("metro_line").expect("metro");
        let sewage = d.building_index("sewage_plant").expect("sewage");
        for _ in 0..2 {
            c.buildings.push(BuildingInst {
                def: sewage,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        for _ in 0..3 {
            c.buildings.push(BuildingInst {
                def: metro,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        c.pop = 3000.0;
        c.happiness = 0.9;
        fill(
            &mut s,
            ci,
            &[
                ("food", 3.0e7),
                ("water", 3.0e7),
                ("consumer_goods", 3.0e6),
                ("spare_parts", 2.0e6),
                ("construction_materials", 2.0e6),
            ],
        );
        (s, ci)
    }

    /// Food spoils (see `colony_works`): the granary of a long test is refilled as it rots.
    fn feed(s: &mut Sim) {
        for c in &mut s.colonies {
            if let Some(f) = s.data.resource_index("food") {
                c.storage[f] = c.storage[f].max(2.0e7);
            }
        }
    }

    fn advance(s: &mut Sim, days: f64) {
        s.time += days * 86_400.0;
        s.update_colonies_pub();
    }

    /// The snapshot of an Earth: mass, radius, day.
    fn earth(d: &GameData) -> WorldSnap {
        let mut snap = earthlike(d).0.snap;
        snap.mass = 5.972e24;
        snap.radius = 6.371e6;
        snap.gravity = 9.81;
        snap.spin = 86_164.0;
        snap.locked = false;
        snap.hill = 0.0;
        snap.rocky = true;
        snap
    }

    #[test]
    fn an_elevator_cable_comes_from_gravity_spin_and_the_strength_of_the_material() {
        let d = data();
        let e = earth(&d);
        let r = e.geostationary().expect("orbit");
        assert!(
            (r / 4.2164e7 - 1.0).abs() < 0.005,
            "geostationary radius {r}"
        );
        let nano = elevator_design(&e, 4.0e7).expect("nanotubes hold on the Earth");
        assert!(
            nano.taper > 8.0 && nano.taper < 16.0,
            "taper {}",
            nano.taper
        );
        assert!((nano.length - (r - e.radius)).abs() < 1.0);
        assert!(
            elevator_design(&e, 2.5e6).is_err(),
            "a polymer cable cannot hold on the Earth"
        );
        // Stronger material, thinner cable.
        let better = elevator_design(&e, 8.0e7).expect("design");
        assert!(better.taper < nano.taper);
        // A heavier world pulls harder: the same cable is thicker or impossible.
        let mut heavy = e.clone();
        heavy.mass *= 4.0;
        heavy.gravity *= 4.0;
        let h = elevator_design(&heavy, 4.0e7);
        assert!(h.as_ref().map_or(true, |x| x.taper > nano.taper));
        // A small world takes the weak cable: the polymer one works on a Mars.
        let mut mars = e.clone();
        mars.mass = 6.42e23;
        mars.radius = 3.39e6;
        mars.gravity = 3.7;
        mars.spin = 88_642.0;
        let weak = elevator_design(&mars, 2.5e6).expect("a polymer cable holds on Mars");
        assert!(weak.taper > 1.0 && weak.taper < MAX_TAPER);
        // A world locked to its star has no synchronous orbit.
        let mut locked = e.clone();
        locked.locked = true;
        assert!(elevator_design(&locked, 4.0e7).is_err());
        // Spinning very slowly pushes the orbit out of the sphere the world rules.
        let mut slow = e.clone();
        slow.spin = 3.0e7;
        slow.hill = 1.0e9;
        assert!(slow.geostationary().is_none());
    }

    #[test]
    fn a_swarm_yields_the_luminosity_times_the_sky_it_covers() {
        let d = data();
        let e = earth(&d);
        let mut sun = e.clone();
        sun.luminosity = 1.0;
        sun.star_dist = 1.0e11;
        let p1 = swarm_power(&sun, 1000.0, 1.0e8);
        assert!(p1 > 0.0);
        assert!(
            (swarm_power(&sun, 2000.0, 1.0e8) / p1 - 2.0).abs() < 1e-9,
            "linear in area"
        );
        let mut bright = sun.clone();
        bright.luminosity = 3.0;
        assert!((swarm_power(&bright, 1000.0, 1.0e8) / p1 - 3.0).abs() < 1e-9);
        // The formula itself: L * share * efficiency.
        let r = swarm_radius(&sun);
        let share = 1000.0 * 1.0e8 / (4.0 * std::f64::consts::PI * r * r);
        assert!((p1 / (3.828e26 * share * COLLECTOR_EFFICIENCY) - 1.0).abs() < 1e-9);
        // It never captures more than the whole star.
        assert!(swarm_power(&sun, 1.0e30, 1.0e8) <= 3.828e26 * COLLECTOR_EFFICIENCY * 1.0000001);
        // Kardashev (Sagan): 0 at a megawatt, 1 at 1e16 W, the Earth today near 0.73.
        assert_eq!(kardashev(1.0e6), 0.0);
        assert!((kardashev(1.0e16) - 1.0).abs() < 1e-9);
        assert!((kardashev(2.0e13) - 0.73).abs() < 0.01);
        assert!(kardashev(1.0e12) < kardashev(1.0e13));
    }

    #[test]
    fn a_work_needs_its_technology_its_site_and_a_world_it_can_stand_on() {
        let (mut s, ci) = founded();
        let d = Arc::clone(&s.data);
        let mirror = d.project_index("miroirs_orbitaux").expect("def");
        s.rules.tech_locks = true;
        s.researched.iter_mut().for_each(|r| *r = false);
        let err = s.colony_project_start(ci, mirror, 1, 0).unwrap_err();
        assert!(err.contains("technologie"), "{err}");
        s.researched.iter_mut().for_each(|r| *r = true);
        assert!(s.colony_project_start(ci, mirror, 1, 0).is_ok());
        assert!(
            s.colony_project_start(ci, mirror, 1, 0).is_ok(),
            "repeatable"
        );
        assert_eq!(s.colonies[ci].projects[0].target, 2);
        // A ring is built once.
        let ring = d.project_index("anneau_orbital").expect("def");
        assert!(s.colony_project_start(ci, ring, 1, 0).is_ok());
        assert!(s.colony_project_start(ci, ring, 1, 0).is_err());
        // A seeding takes a producer, not a herbivore.
        let seed = d.project_index("ensemencement").expect("def");
        let beast = (0..d.species.len())
            .find(|&i| !d.species[i].is_producer())
            .expect("consumer");
        assert!(s.colony_project_start(ci, seed, 1, beast).is_err());
        assert!(s
            .colony_project_start(ci, seed, 1, any_producer(&d))
            .is_ok());
        // The elevator check agrees with the physics of the world.
        for id in ["ascenseur_kevlar", "ascenseur_nanotubes"] {
            let def = d.project_index(id).expect("def");
            let t = s.colonies[ci].terra.as_ref().expect("terra");
            let physics = elevator_design(&t.snap, d.projects[def].param);
            assert_eq!(s.project_check(ci, def, 0).is_ok(), physics.is_ok(), "{id}");
        }
    }

    #[test]
    fn a_stage_streams_its_materials_and_waits_when_they_run_out() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        let def = d.project_index("miroirs_orbitaux").expect("def");
        s.colony_project_start(ci, def, 2, 0).expect("start");
        advance(&mut s, 60.0);
        let p = &s.colonies[ci].projects[0];
        assert_eq!(p.progress, 0.0, "nothing moves without materials");
        assert_eq!(p.wait, 1, "the stage waits for materials");
        let missing = d.resources[usize::from(p.short)].id.clone();
        assert!(
            ["aluminium", "polymers"].contains(&missing.as_str()),
            "{missing}"
        );
        // With materials for one unit only, one unit is built and the other waits.
        let cost = d.projects[def].total_cost();
        let (alu, pol, fuel, mach, elec) = (
            3.0e6 * 1.01,
            5.0e6 * 1.01,
            2.0e6 * 1.01,
            3.0e4 * 1.01,
            2.0e4 * 1.01,
        );
        fill(
            &mut s,
            ci,
            &[
                ("aluminium", alu),
                ("polymers", pol),
                ("fuel", fuel),
                ("machinery", mach),
                ("electronics", elec),
            ],
        );
        for _ in 0..60 {
            feed(&mut s);
            advance(&mut s, 100.0);
            if s.colonies[ci].projects[0].units >= 1 {
                break;
            }
        }
        let p = &s.colonies[ci].projects[0];
        assert_eq!(p.units, 1, "one unit finished: {p:?}");
        // The materials were really consumed: one unit's worth.
        for (r, kg) in &cost {
            let left = stock(&s, ci, r);
            let before = match r.as_str() {
                "aluminium" => alu,
                "polymers" => pol,
                "fuel" => fuel,
                "machinery" => mach,
                _ => elec,
            };
            // One unit's worth at least (the second unit already draws on what is left).
            assert!(
                before - left >= 0.999 * kg && left >= 0.0,
                "{r}: {before} -> {left}"
            );
        }
        // The unit did what a mirror does: more light on the world.
        let t = s.colonies[ci].terra.as_ref().expect("terra");
        assert!((t.mirror - d.projects[def].param).abs() < 1.0);
        assert!(t.flux() > t.snap.insolation);
        assert!(
            s.colonies[ci].projects[0].wait == 1 || s.colonies[ci].projects[0].units < 2,
            "the second unit lacks material"
        );
    }

    #[test]
    fn a_great_work_takes_its_crew_from_the_colony() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        advance(&mut s, 5.0);
        let before = s.colonies[ci].stats.prod_mult;
        let def = d.project_index("station_geante").expect("def");
        s.colony_project_start(ci, def, 1, 0).expect("start");
        advance(&mut s, 5.0);
        let after = s.colonies[ci].stats.prod_mult;
        assert!(
            after < before,
            "the crew leaves the workshops: {before} -> {after}"
        );
    }

    #[test]
    fn collectors_feed_the_grid_and_lift_the_empire_on_the_kardashev_scale() {
        let (mut s, ci) = busy();
        s.colonies[ci].treasury = 1.0e12;
        advance(&mut s, 3.0);
        let (p0, k0) = (s.empire_power_w(), s.empire_kardashev());
        let d = Arc::clone(&s.data);
        let def = d.project_index("essaim_dyson").expect("def");
        s.colonies[ci].projects.push(ProjectInst {
            units: 5000,
            ..ProjectInst::new(def, 0, 5000, 1.0)
        });
        advance(&mut s, 3.0);
        let c = &s.colonies[ci];
        let fx = c.mega_effects(&d);
        let snap = &c.terra.as_ref().expect("terra").snap;
        let expect = swarm_power(snap, 5000.0, d.projects[def].param) / 1000.0;
        assert!(
            (fx.power / expect - 1.0).abs() < 0.05,
            "{} vs {expect}",
            fx.power
        );
        assert!(c.stats.power_supply > 0.9 * expect, "the grid receives it");
        assert!(s.empire_power_w() > p0 + 0.9 * expect * 1000.0);
        assert!(s.empire_kardashev() > k0);
        // Cheap abundant energy lifts the whole colony.
        let (mut plain, pci) = busy();
        plain.colonies[pci].treasury = 1.0e12;
        advance(&mut plain, 3.0);
        assert!(
            s.colonies[ci].stats.prod_mult > plain.colonies[pci].stats.prod_mult,
            "abundant energy lifts production: {} vs {}",
            s.colonies[ci].stats.prod_mult,
            plain.colonies[pci].stats.prod_mult
        );
    }

    #[test]
    fn drillers_need_a_belt_and_factories_cannot_make_matter_from_nothing() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        let drill = d.project_index("foreuse_asteroides").expect("def");
        let belt = s.colonies[ci].terra.as_ref().expect("terra").snap.belt;
        assert_eq!(s.project_check(ci, drill, 0).is_ok(), belt);
        let iron = d.resource_index("iron").expect("iron");
        for has_belt in [true, false] {
            s.colonies[ci].projects.clear();
            s.colonies[ci].storage[iron] = 0.0;
            s.colonies[ci].terra.as_mut().expect("terra").snap.belt = has_belt;
            s.colonies[ci].projects.push(ProjectInst {
                units: 3,
                ..ProjectInst::new(drill, 0, 3, 1.0)
            });
            advance(&mut s, 10.0);
            let got = s.colonies[ci].storage[iron];
            if has_belt {
                assert!(got > 3.0 * 100_000.0, "ore comes in: {got}");
            } else {
                assert_eq!(got, 0.0, "no belt, no ore");
            }
        }
        // A factory eats more than it makes.
        let fac = d.project_index("usine_orbitale").expect("def");
        s.colonies[ci].projects.clear();
        fill(
            &mut s,
            ci,
            &[("steel", 1.0e7), ("copper_wire", 1.0e6), ("silicon", 1.0e6)],
        );
        s.colonies[ci].projects.push(ProjectInst {
            units: 2,
            ..ProjectInst::new(fac, 0, 2, 1.0)
        });
        let total = |s: &Sim| s.colonies[ci].storage.iter().sum::<f64>();
        let before = total(&s);
        advance(&mut s, 5.0);
        assert!(stock(&s, ci, "machinery") > 0.0, "it makes machines");
        assert!(total(&s) <= before + 1e-6, "no matter from nothing");
    }

    #[test]
    fn works_wear_out_unless_the_upkeep_is_paid() {
        let (s, ci) = busy();
        let d = Arc::clone(&s.data);
        let def = d.project_index("anneau_orbital").expect("def");
        let mut paid = s.colonies[ci].clone();
        paid.projects.push(ProjectInst {
            units: 1,
            ..ProjectInst::new(def, 0, 1, 1.0)
        });
        paid.projects[0].health = 0.8;
        let mut broke = paid.clone();
        paid.treasury = 1.0e9;
        broke.treasury = -1.0e3;
        for c in [&mut paid, &mut broke] {
            for _ in 0..40 {
                c.mega_tick(&d, 30.0, 1.0, 1.0, 0.0, 0.0);
            }
        }
        assert!(
            paid.projects[0].health > broke.projects[0].health,
            "{} vs {}",
            paid.projects[0].health,
            broke.projects[0].health
        );
        assert!(broke.projects[0].health < 0.8, "it decays");
        assert!(paid.treasury < 1.0e9, "the upkeep is paid");
    }

    #[test]
    fn pirates_and_war_go_for_what_there_is_to_take_and_turrets_turn_them_away() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        for id in ["anneau_orbital", "station_geante", "chantier_orbital"] {
            let def = d.project_index(id).expect("def");
            s.colonies[ci].projects.push(ProjectInst {
                units: 1,
                ..ProjectInst::new(def, 0, 1, 1.0)
            });
        }
        s.colonies[ci].unrest = 0.9;
        // The game calls it once per update, which looks back at most 60 days: 80 windows of 50.
        let raid = |s: &mut Sim| {
            for k in 0..80 {
                s.apply_mega_threats(
                    ci,
                    f64::from(k) * 50.0 * 86_400.0,
                    f64::from(k + 1) * 50.0 * 86_400.0,
                );
            }
        };
        let damage = |c: &crate::colony::Colony| -> f64 {
            c.projects
                .iter()
                .map(|p| (1.0 - p.health) + f64::from(1 - p.units.min(1)))
                .sum()
        };
        let base = s.colonies[ci].clone();
        raid(&mut s);
        let open = s.colonies[ci].clone();
        s.colonies[ci] = base.clone();
        raid(&mut s);
        let again = s.colonies[ci].clone();
        assert!(damage(&open) > 0.2, "raids happen: {}", damage(&open));
        assert_eq!(open.projects, again.projects, "the same history every time");
        assert!(open.log.iter().any(|(_, l)| l.contains("Raid")));
        let turret = d.building_index("defense_turret").expect("turret");
        s.colonies[ci] = base;
        for _ in 0..60 {
            s.colonies[ci].buildings.push(BuildingInst {
                def: turret,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
                ops: Default::default(),
            });
        }
        raid(&mut s);
        let guarded = s.colonies[ci].clone();
        assert!(
            damage(&guarded) < damage(&open),
            "turrets help: {} vs {}",
            damage(&guarded),
            damage(&open)
        );
    }

    #[test]
    fn finished_seeding_and_comets_act_on_the_world() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        let sp = any_producer(&d);
        let seed = d.project_index("ensemencement").expect("def");
        let comet = d.project_index("comete_eau").expect("def");
        // Run the stages to their end directly through the colony's own step.
        for (def, arg) in [(seed, sp), (comet, 0)] {
            s.colonies[ci]
                .projects
                .push(ProjectInst::new(def, arg, 1, 1.0));
            let cost = d.projects[def].total_cost();
            for (r, kg) in cost {
                fill(&mut s, ci, &[(r.as_str(), kg * 1.5)]);
            }
        }
        for _ in 0..200 {
            advance(&mut s, 200.0);
            if s.colonies[ci].projects.iter().all(|p| p.units >= 1) {
                break;
            }
        }
        let c = &s.colonies[ci];
        assert!(c.projects.iter().all(|p| p.units >= 1), "{:?}", c.projects);
        let t = c.terra.as_ref().expect("terra");
        assert!(
            t.seeds.iter().any(|x| usize::from(x.species) == sp)
                || !t.notes.is_empty()
                || t.age > 0.0
        );
        assert!(
            (t.delivered - d.projects[comet].param).abs() < 1.0,
            "the comet's mass is on the ledger"
        );
        assert!(t.ledger().is_finite());
    }

    #[test]
    fn terraforming_and_works_are_saved_and_come_back() {
        let (mut s, ci) = busy();
        let d = Arc::clone(&s.data);
        let def = d.project_index("miroirs_orbitaux").expect("def");
        s.colony_project_start(ci, def, 3, 0).expect("start");
        s.colonies[ci].terra.as_mut().expect("terra").mirror = 1.0e11;
        s.colonies[ci].terra.as_mut().expect("terra").process = 2;
        advance(&mut s, 30.0);
        let bytes = s.save_bytes().expect("save");
        let back = Sim::load_bytes(&bytes, Arc::clone(&s.data)).expect("load");
        assert_eq!(back.colonies[ci].terra, s.colonies[ci].terra);
        assert_eq!(back.colonies[ci].projects, s.colonies[ci].projects);
        assert_eq!(back.colonies[ci].terraform, s.colonies[ci].terraform);
    }

    #[test]
    fn a_terraformed_world_shows_in_its_map_and_its_air() {
        let (mut s, ci) = busy();
        let body = s.colonies[ci].body;
        let before = s.planet_map(body).expect("map");
        let p0 = s.system.bodies[body].pressure;
        {
            let t = s.colonies[ci].terra.as_mut().expect("terra");
            t.t += 25.0;
            t.gas[crate::terraform::CO2] += 2.0e16;
            t.version += 1;
        }
        s.update_colonies_pub();
        advance(&mut s, 2.0);
        let after = s.planet_map(body).expect("map");
        let mean = |m: &crate::planet::PlanetMap| {
            m.surface.temp.iter().map(|&v| f64::from(v)).sum::<f64>()
        };
        assert!(mean(&after) > mean(&before), "the map is warmer");
        assert!(
            s.system.bodies[body].pressure != p0,
            "the air of the world follows"
        );
        assert!(after.pressure > before.pressure || p0 > 5.0);
    }
}
