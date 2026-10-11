//! Colonies: aggregated populations and buildings simulated at a coarse tick, anywhere in the galaxy.
use crate::crew::DAY;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use vector_core::{hash_coord, Rng};
use vector_data::GameData;
use vector_world::galaxy::StarRef;
use vector_world::{BodyKind, CelBody, PlanetKind};

pub const PERMIT_COST: f64 = 20_000.0;
pub const KIT_MATERIALS: f64 = 3_000.0;
pub const KIT_COLONISTS_KG: f64 = 400.0;
pub const KIT_FOOD: f64 = 300.0;
pub const KIT_WATER: f64 = 600.0;
pub const COLONIST_KG: f64 = 80.0;
/// Landing / docking reach of a colony spaceport above the surface (m).
pub const PORT_RANGE: f64 = 100_000.0;
const BASE_STORAGE: f64 = 5_000.0;
const FOOD_KG_PER_DAY: f64 = 0.6;
const WATER_KG_PER_DAY: f64 = 3.0;
const GOODS_KG_PER_DAY: f64 = 0.04;
const LOG_CAP: usize = 120;
/// Colonies are simulated in steps of this many seconds (6 h), coarser for long absences.
const TICK: f64 = DAY / 4.0;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuildingInst {
    pub def: usize,
    /// 0..1; < 1 means under construction.
    pub progress: f64,
    pub enabled: bool,
    /// Index into the definition's recipe list.
    pub recipe: usize,
    pub health: f64,
}

impl BuildingInst {
    pub fn built(&self) -> bool {
        self.progress >= 1.0
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ColonyStats {
    pub power_supply: f64,
    pub power_demand: f64,
    pub jobs: f64,
    pub workers: f64,
    pub housing: f64,
    pub storage_cap: f64,
    pub food_sat: f64,
    pub water_sat: f64,
    pub goods_sat: f64,
    pub prod_mult: f64,
    pub income: f64,
    pub upkeep: f64,
}

/// An epidemic running in a colony.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Epidemic {
    /// Index into `GameData::diseases`.
    pub disease: usize,
    pub days_left: f32,
    /// 0.5..1: how hard it hits.
    pub strength: f32,
}

/// Chance per day of an outbreak in a colony of 100 people with no medical care.
pub const EPIDEMIC_RATE: f64 = 0.004;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Colony {
    pub id: u32,
    pub name: String,
    pub star: StarRef,
    pub system_seed: u64,
    pub body: usize,
    pub body_name: String,
    pub tags: Vec<String>,
    /// (resource index, relative abundance) of what the ground holds.
    pub deposits: Vec<(usize, f64)>,
    /// Stellar flux relative to 1 AU at the body (for solar farms).
    pub flux: f64,
    pub founded: f64,
    pub last_tick: f64,
    pub pop: f64,
    pub happiness: f64,
    pub unrest: f64,
    /// Production lost to invasive species in the colony's territory, 0..0.5 (see `ecology`).
    pub pest: f64,
    /// An epidemic of people (see `Epidemic`), if one is running.
    pub epidemic: Option<Epidemic>,
    /// Mores of the colony on the axes of `ethos.ron`: they start as those of the people of the
    /// place and drift under the policies (see `culture_target`).
    pub culture: Vec<f64>,
    pub culture_base: Vec<f64>,
    /// Belief of the people (index into `GameData::religions`).
    pub faith: usize,
    pub storage: Vec<f64>,
    pub buildings: Vec<BuildingInst>,
    /// Chosen level of each policy.
    pub policies: Vec<usize>,
    pub treasury: f64,
    pub terraform: f64,
    pub research: f64,
    pub debt_days: f64,
    pub stats: ColonyStats,
    pub log: Vec<(f64, String)>,
}

/// Which site tags a body carries.
pub fn site_tags(b: &CelBody) -> Vec<String> {
    let mut t = vec!["any".to_string()];
    match b.kind {
        BodyKind::Asteroid(a) => {
            t.push("asteroid".into());
            if a == vector_world::AstType::I {
                t.push("icy".into());
            }
        }
        BodyKind::Planet(k) | BodyKind::Moon(k) => {
            if k.is_giant() {
                t.push("gas_giant".into());
            } else {
                t.push("rocky".into());
                if matches!(k, PlanetKind::Ice | PlanetKind::Tundra | PlanetKind::Dwarf) {
                    t.push("icy".into());
                }
                if matches!(
                    k,
                    PlanetKind::Terran | PlanetKind::Ocean | PlanetKind::Tundra
                ) {
                    t.push("habitable".into());
                }
            }
        }
        _ => {}
    }
    t
}

/// How suitable a body is for unaided human life (0..1).
pub fn habitability(b: &CelBody) -> f64 {
    match b.kind {
        BodyKind::Planet(PlanetKind::Terran) | BodyKind::Moon(PlanetKind::Terran) => 1.0,
        BodyKind::Planet(PlanetKind::Ocean) | BodyKind::Moon(PlanetKind::Ocean) => 0.8,
        BodyKind::Planet(PlanetKind::Tundra) | BodyKind::Moon(PlanetKind::Tundra) => 0.6,
        _ => 0.0,
    }
}

/// Ground composition of a body as (resource index, relative abundance); deterministic from the seed.
pub fn site_deposits(data: &GameData, b: &CelBody, seed: u64) -> Vec<(usize, f64)> {
    let mut rng = Rng::new(hash_coord(seed, b.mass.to_bits() as i64, 4, 4));
    let mut v: Vec<(&str, f64)> = Vec::new();
    match b.kind {
        BodyKind::Asteroid(a) => {
            if let Some(c) = data.asteroids.iter().find(|c| c.id == a.id()) {
                v = c
                    .composition
                    .iter()
                    .map(|(r, f)| (r.as_str(), *f))
                    .collect();
            }
        }
        BodyKind::Planet(k) | BodyKind::Moon(k) if k.is_giant() => {
            v = vec![
                ("hydrogen", 0.74),
                ("helium", 0.24),
                ("deuterium", 0.002),
                ("helium3", 0.0004),
            ];
            if k == PlanetKind::IceGiant {
                v.extend([("methane", 0.03), ("ammonia", 0.02), ("water_ice", 0.02)]);
            }
        }
        BodyKind::Planet(k) | BodyKind::Moon(k) => {
            v = vec![
                ("silica", 0.22),
                ("iron", 0.28),
                ("aluminium", 0.09),
                ("magnesium", 0.07),
                ("nickel", 0.04),
                ("titanium", 0.025),
                ("copper", 0.015),
                ("carbon", 0.03),
                ("silicon", 0.04),
                ("sulfur", 0.01),
                ("phosphorus", 0.004),
                ("manganese", 0.004),
                ("chromium", 0.004),
                ("lithium", 0.002),
                ("uranium", 0.0004),
                ("gold", 0.0003),
                ("platinum", 0.0002),
                ("regolith", 0.25),
            ];
            if matches!(k, PlanetKind::Ice | PlanetKind::Tundra | PlanetKind::Dwarf) {
                v.extend([
                    ("water_ice", 0.25),
                    ("co2_ice", 0.05),
                    ("ammonia", 0.04),
                    ("methane", 0.03),
                ]);
            }
            if matches!(k, PlanetKind::Terran | PlanetKind::Ocean) {
                v.extend([("water_ice", 0.1), ("organics", 0.05), ("ammonia", 0.01)]);
            }
            if k == PlanetKind::Lava {
                v.extend([("iron", 0.2), ("tungsten", 0.004), ("cobalt", 0.004)]);
            }
        }
        _ => {}
    }
    v.into_iter()
        .filter_map(|(id, share)| {
            data.resource_index(id)
                .map(|i| (i, share * rng.range(0.5, 1.5)))
        })
        .collect()
}

/// The deposit subset an extraction recipe draws from, normalised to sum to 1.
fn extraction_split(data: &GameData, colony: &Colony, pseudo: &str) -> Vec<(usize, f64)> {
    let pick: Vec<(usize, f64)> = colony
        .deposits
        .iter()
        .copied()
        .filter(|&(i, _)| {
            let r = &data.resources[i];
            match pseudo {
                "ore" => {
                    ["metal", "precious", "nuclear", "mineral"].contains(&r.category.as_str())
                        && r.id != "regolith"
                }
                "volatiles" => ["volatile", "organic"].contains(&r.category.as_str()),
                "gas" => r.category == "gas",
                other => r.id == other,
            }
        })
        .collect();
    let total: f64 = pick.iter().map(|p| p.1).sum();
    if total <= 0.0 {
        return Vec::new();
    }
    pick.into_iter().map(|(i, s)| (i, s / total)).collect()
}

impl Colony {
    pub fn total_stored(&self) -> f64 {
        self.storage.iter().sum()
    }

    pub(crate) fn free_space(&self) -> f64 {
        (self.stats.storage_cap.max(BASE_STORAGE) - self.total_stored()).max(0.0)
    }

    pub fn log_line(&mut self, t: f64, s: impl Into<String>) {
        self.log.push((t, s.into()));
        if self.log.len() > LOG_CAP {
            self.log.remove(0);
        }
    }

    pub fn has_port(&self, data: &GameData) -> bool {
        self.buildings
            .iter()
            .any(|b| b.built() && b.health > 0.2 && data.buildings[b.def].port > 0.0)
    }

    pub fn policy_level(&self, data: &GameData, id: &str) -> (f64, f64, f64) {
        data.policy_index(id)
            .map(|p| {
                let l = &data.policies[p].levels
                    [self.policies[p].min(data.policies[p].levels.len() - 1)];
                (l.1, l.2, l.3)
            })
            .unwrap_or((0.0, 0.0, 0.0))
    }

    /// Mores the colony tends to: its founders' mores pushed by the policies (martial law makes
    /// people more martial and authoritarian, open borders more open, strict quarantine less).
    pub fn culture_target(&self, data: &GameData) -> Vec<f64> {
        let mut t = self.culture_base.clone();
        let level = |id: &str| {
            data.policy_index(id)
                .map_or(1.0, |p| self.policies[p] as f64)
        };
        let mut push = |axis: &str, v: f64| {
            if let Some(i) = data.ethos.axis(axis) {
                if let Some(x) = t.get_mut(i) {
                    *x = (*x + v).clamp(-1.0, 1.0);
                }
            }
        };
        push("violence", (level("security") - 1.0) * 0.5);
        push("autorite", (level("security") - 1.0) * 0.5);
        push("ouverture", (level("immigration") - 1.0) * 0.6);
        push("ouverture", -(level("quarantine") - 1.0) * 0.2);
        t
    }

    /// How much the policies clash with the culture (0 = none): people resent what is against
    /// their mores.
    pub fn culture_friction(&self, data: &GameData) -> f64 {
        let get = |axis: &str| {
            data.ethos
                .axis(axis)
                .and_then(|i| self.culture.get(i))
                .copied()
                .unwrap_or(0.0)
        };
        let level = |id: &str| data.policy_index(id).map_or(1, |p| self.policies[p]);
        let mut f = 0.0;
        if level("security") >= 2 && get("violence") < -0.3 {
            f += 0.08;
        }
        if level("security") == 0 && get("autorite") > 0.4 {
            f += 0.05;
        }
        if level("immigration") >= 2 && get("ouverture") < -0.3 {
            f += 0.08;
        }
        if level("immigration") == 0 && get("ouverture") > 0.4 {
            f += 0.05;
        }
        f
    }

    /// Advance the colony to `now` with adaptively sized steps.
    pub fn update(&mut self, data: &GameData, now: f64) {
        let mut guard = 0;
        while now - self.last_tick >= TICK && guard < 4000 {
            guard += 1;
            let gap = now - self.last_tick;
            let h = if gap < 5.0 * DAY {
                TICK
            } else if gap < 200.0 * DAY {
                DAY
            } else {
                5.0 * DAY
            };
            self.last_tick += h;
            self.tick(data, h);
        }
        if guard >= 4000 {
            self.last_tick = now;
        }
    }

    fn tick(&mut self, data: &GameData, dt: f64) {
        let days = dt / DAY;
        let now = self.last_tick;
        // Construction.
        let mut finished = Vec::new();
        for b in &mut self.buildings {
            if !b.built() {
                b.progress = (b.progress + days / data.buildings[b.def].days).min(1.0);
                if b.built() {
                    finished.push(data.buildings[b.def].name.clone());
                }
            }
        }
        for n in finished {
            self.log_line(now, format!("Construction terminee : {n}."));
        }

        // Capacities.
        let active = |b: &&BuildingInst| b.built() && b.enabled && b.health > 0.2;
        let housing: f64 = self
            .buildings
            .iter()
            .filter(|b| b.built() && b.health > 0.2)
            .map(|b| data.buildings[b.def].housing * b.health)
            .sum();
        let storage_cap: f64 = BASE_STORAGE
            + self
                .buildings
                .iter()
                .filter(|b| b.built())
                .map(|b| data.buildings[b.def].storage)
                .sum::<f64>();
        let jobs: f64 = self
            .buildings
            .iter()
            .filter(active)
            .map(|b| data.buildings[b.def].workers)
            .sum();
        let workers = 0.55 * self.pop;
        let staffing = if jobs > 0.0 {
            (workers / jobs).min(1.0)
        } else {
            1.0
        };

        // Power: solar scales with the star, fuel plants burn stock, others are steady.
        let (mut supply, mut demand) = (0.0, 0.0);
        let mut burn_needs: Vec<(usize, f64)> = Vec::new();
        for b in self.buildings.iter().filter(active) {
            let d = &data.buildings[b.def];
            if d.power > 0.0 {
                if d.burns.is_empty() {
                    let k = if d.id == "solar_farm" { self.flux } else { 1.0 };
                    supply += d.power * b.health * k;
                } else {
                    // Fuel plants run only while their fuel lasts.
                    let ok = d.burns.iter().all(|(r, kg)| {
                        data.resource_index(r)
                            .is_some_and(|i| self.storage[i] >= kg * days)
                    });
                    if ok {
                        supply += d.power * b.health;
                        for (r, kg) in &d.burns {
                            if let Some(i) = data.resource_index(r) {
                                burn_needs.push((i, kg * days));
                            }
                        }
                    }
                }
            } else {
                demand += -d.power;
            }
        }
        for (i, kg) in burn_needs {
            self.storage[i] = (self.storage[i] - kg).max(0.0);
        }
        let power_sat = if demand > 0.0 {
            (supply / demand).min(1.0)
        } else {
            1.0
        };

        // Population needs.
        let (_, _, rations_h) = self.policy_level(data, "rations");
        let ration_factor = if rations_h < -0.01 {
            0.75
        } else if rations_h > 0.01 {
            1.1
        } else {
            1.0
        };
        let recycling = self
            .buildings
            .iter()
            .filter(active)
            .map(|b| data.buildings[b.def].recycling)
            .sum::<f64>();
        let recycle = 0.8 + 0.15 * (recycling * 1000.0 / self.pop.max(1.0)).min(1.0);
        let take = |name: &str, need: f64, storage: &mut Vec<f64>| -> f64 {
            match data.resource_index(name) {
                Some(i) if need > 0.0 => {
                    let got = storage[i].min(need);
                    storage[i] -= got;
                    got / need
                }
                _ => 1.0,
            }
        };
        let food_sat = take(
            "food",
            self.pop * FOOD_KG_PER_DAY * ration_factor * days,
            &mut self.storage,
        );
        let water_sat = take(
            "water",
            self.pop * WATER_KG_PER_DAY * (1.0 - recycle) * days,
            &mut self.storage,
        );
        let goods_sat = take(
            "consumer_goods",
            self.pop * GOODS_KG_PER_DAY * days,
            &mut self.storage,
        );

        // Productivity.
        let (_, work_mult, _) = self.policy_level(data, "work");
        let (_, sec_mult, _) = self.policy_level(data, "security");
        let (_, quarantine_mult, _) = self.policy_level(data, "quarantine");
        let rioting = self.unrest > 0.7;
        let broke = self.treasury < 0.0;
        let prod_mult = (staffing
            * power_sat
            * (0.35 + 0.9 * self.happiness)
            * (1.0 + work_mult + sec_mult + quarantine_mult)
            * (1.0 - self.pest.clamp(0.0, 0.5))
            * (1.0
                - 0.4
                    * self
                        .epidemic
                        .as_ref()
                        .map_or(0.0, |e| f64::from(e.strength)))
            * if rioting { 0.5 } else { 1.0 }
            * if broke { 0.3 } else { 1.0 })
        .max(0.0);

        // Production.
        let mut research_gain = 0.0;
        let mut messages: Vec<String> = Vec::new();
        for bi in 0..self.buildings.len() {
            let inst = self.buildings[bi].clone();
            if !(inst.built() && inst.enabled && inst.health > 0.2) {
                continue;
            }
            let def = &data.buildings[inst.def];
            if def.recipes.is_empty() {
                continue;
            }
            let Some(ri) = data.recipe_index(&def.recipes[inst.recipe % def.recipes.len()]) else {
                continue;
            };
            let r = &data.recipes[ri];
            let mut batches = def.throughput * days * prod_mult * inst.health;
            if batches <= 1e-9 {
                continue;
            }
            if r.kind == "extract" {
                let split = extraction_split(data, self, &r.outputs[0].0);
                if split.is_empty() {
                    continue;
                }
                let mass = r.outputs[0].1 * batches;
                let free = self.free_space();
                let scale = if mass > free { free / mass } else { 1.0 };
                for (i, share) in split {
                    self.storage[i] += mass * scale * share;
                }
                continue;
            }
            for (id, kg) in &r.inputs {
                if let Some(i) = data.resource_index(id) {
                    batches = batches.min(self.storage[i] / kg);
                }
            }
            let net: f64 = r.outputs.iter().map(|o| o.1).sum::<f64>()
                - r.inputs.iter().map(|i| i.1).sum::<f64>();
            if r.kind != "data" && net > 0.0 {
                batches = batches.min(self.free_space() / net);
            }
            if batches <= 1e-9 {
                continue;
            }
            for (id, kg) in &r.inputs {
                if let Some(i) = data.resource_index(id) {
                    self.storage[i] -= kg * batches;
                }
            }
            for (id, kg) in &r.outputs {
                if r.kind == "data" {
                    research_gain += kg * batches * def.research.max(1.0);
                } else if let Some(i) = data.resource_index(id) {
                    self.storage[i] += kg * batches;
                }
            }
        }
        self.research +=
            research_gain * (1.0 + data.religions.get(self.faith).map_or(0.0, |r| r.research));

        // Culture drifts slowly towards what the policies push it to.
        if self.culture.len() == self.culture_base.len() && !self.culture.is_empty() {
            let target = self.culture_target(data);
            let k = 1.0 - (-days / 600.0).exp();
            for (c, t) in self.culture.iter_mut().zip(&target) {
                *c += (t - *c) * k;
            }
        }
        // Services and happiness.
        let svc = |f: fn(&vector_data::Building) -> f64, per: f64, colony: &Colony| -> f64 {
            let cap: f64 = colony
                .buildings
                .iter()
                .filter(|b| b.built() && b.enabled && b.health > 0.2)
                .map(|b| f(&data.buildings[b.def]))
                .sum();
            (cap / (colony.pop / per).max(1e-9)).min(1.0)
        };
        let med = svc(|b| b.medical, 400.0, self);
        let rec = svc(|b| b.recreation, 600.0, self);
        let edu = svc(|b| b.education, 800.0, self);
        let sec = svc(|b| b.security, 700.0, self);
        let (_, _, tax_h) = self.policy_level(data, "tax");
        let (_, _, work_h) = self.policy_level(data, "work");
        let (_, _, sec_h) = self.policy_level(data, "security");
        let (_, _, imm_h) = self.policy_level(data, "immigration");
        let (_, _, gather_h) = self.policy_level(data, "gather");
        let (_, _, quarantine_h) = self.policy_level(data, "quarantine");
        let crowd = if self.pop > housing + 1.0 {
            (0.4 * (self.pop / housing.max(1.0) - 1.0)).min(0.5)
        } else {
            0.0
        };
        let target = (0.10
            + 0.30 * food_sat
            + 0.20 * water_sat
            + 0.08 * goods_sat
            + 0.12 * power_sat
            + 0.10 * med
            + 0.06 * rec
            + 0.04 * edu
            + 0.04 * sec
            + tax_h
            + work_h
            + rations_h
            + sec_h
            + imm_h
            + gather_h
            + quarantine_h
            - crowd
            - self.culture_friction(data)
            + data.religions.get(self.faith).map_or(0.0, |r| r.morale)
            - if broke { 0.1 } else { 0.0 })
        .clamp(0.0, 1.0);
        self.happiness += (target - self.happiness) * (1.0 - (-days / 8.0).exp());
        self.happiness = self.happiness.clamp(0.0, 1.0);

        // Unrest.
        if self.happiness < 0.25 {
            self.unrest = (self.unrest + 0.1 * days * (1.0 - 0.5 * sec)).min(1.0);
        } else {
            self.unrest = (self.unrest - 0.15 * days).max(0.0);
        }
        if self.unrest > 0.7 && !self.buildings.is_empty() {
            let mut rng = Rng::new(hash_coord(
                self.system_seed,
                self.id as i64,
                (now / DAY) as i64,
                21,
            ));
            let k = (rng.next_u64() % self.buildings.len() as u64) as usize;
            self.buildings[k].health = (self.buildings[k].health - 0.03 * days).max(0.0);
            if rng.f64() < 0.1 {
                messages.push("Emeutes : des installations sont endommagees.".to_string());
            }
        }

        // Population.
        let free = (housing - self.pop).max(0.0);
        let births = if free > 0.0 {
            self.pop * 0.0006 * ((self.happiness - 0.3) / 0.7).clamp(0.0, 1.0) * days
        } else {
            0.0
        };
        // Epidemics: crowding and bad care start them, medical centres and a strict quarantine
        // hold them off and cut them short.
        let mut epidemic_deaths = 0.0;
        if let Some(e) = &mut self.epidemic {
            let d = &data.diseases[e.disease];
            epidemic_deaths =
                self.pop * d.lethality * 0.04 * f64::from(e.strength) * (1.0 - 0.7 * med) * days;
            e.days_left -= (days * (1.0 + 2.0 * med)) as f32;
            if e.days_left <= 0.0 {
                messages.push(format!("L'epidemie de {} est terminee.", d.name));
                self.epidemic = None;
            }
        } else if self.pop > 20.0 {
            let mut rng = Rng::new(hash_coord(
                self.system_seed,
                i64::from(self.id),
                (now / DAY) as i64,
                91,
            ));
            let quarantine = data
                .policy_index("quarantine")
                .map_or(0, |p| self.policies[p]) as f64;
            let p = EPIDEMIC_RATE
                * days
                * (self.pop / 100.0).clamp(0.2, 6.0)
                * (1.0 - 0.7 * med)
                * (1.0 - 0.2 * quarantine).max(0.2);
            if rng.f64() < p {
                // The disease: weighted by how contagious it is.
                let cands: Vec<usize> = (0..data.diseases.len())
                    .filter(|&i| data.diseases[i].contagion >= 0.05)
                    .collect();
                let total: f64 = cands.iter().map(|&i| data.diseases[i].contagion).sum();
                let mut pick = rng.f64() * total;
                let mut chosen = cands[0];
                for &i in &cands {
                    if pick < data.diseases[i].contagion {
                        chosen = i;
                        break;
                    }
                    pick -= data.diseases[i].contagion;
                }
                let strength = 0.5 + 0.5 * rng.f64() as f32;
                self.epidemic = Some(Epidemic {
                    disease: chosen,
                    days_left: data.diseases[chosen].duration_days as f32 * 1.5,
                    strength,
                });
                messages.push(format!("Epidemie : {} !", data.diseases[chosen].name));
            }
        }
        let deaths = self.pop
            * (0.00008
                + 0.00005 * (1.0 - med)
                + 0.01 * (1.0 - food_sat).powi(2)
                + 0.03 * (1.0 - water_sat).powi(2))
            * days
            + epidemic_deaths;
        let immigration_rate = match data
            .policy_index("immigration")
            .map_or(1, |p| self.policies[p])
        {
            0 => 0.0,
            1 => 0.003,
            _ => 0.01,
        };
        let immigrants = if self.happiness > 0.6 {
            (immigration_rate * free * days).min(5.0 * days)
        } else {
            0.0
        };
        let leavers = if self.happiness < 0.25 {
            self.pop * 0.003 * days
        } else {
            0.0
        };
        let was = self.pop;
        self.pop = (self.pop + births + immigrants - deaths - leavers)
            .clamp(0.0, housing.max(self.pop.min(housing)));
        if self.pop < 1.0 && was >= 1.0 {
            messages.push("La colonie est abandonnee.".to_string());
        }
        if food_sat < 0.5 && was > 5.0 {
            messages.push("Famine : les rations manquent.".to_string());
        }

        // Treasury.
        let (tax_cr, _, _) = self.policy_level(data, "tax");
        let income = self.pop * tax_cr * days * self.happiness;
        let upkeep: f64 = self
            .buildings
            .iter()
            .filter(|b| b.built())
            .map(|b| data.buildings[b.def].upkeep)
            .sum::<f64>()
            * days;
        self.treasury += income - upkeep;
        self.debt_days = if self.treasury < 0.0 {
            self.debt_days + days
        } else {
            0.0
        };

        // Terraforming.
        let terra: f64 = self
            .buildings
            .iter()
            .filter(active)
            .map(|b| data.buildings[b.def].terraform)
            .sum();
        self.terraform = (self.terraform + terra * days * power_sat * prod_mult).min(1.0);

        self.stats = ColonyStats {
            power_supply: supply,
            power_demand: demand,
            jobs,
            workers,
            housing,
            storage_cap,
            food_sat,
            water_sat,
            goods_sat,
            prod_mult,
            income: if days > 0.0 { income / days } else { 0.0 },
            upkeep: if days > 0.0 { upkeep / days } else { 0.0 },
        };
        for m in messages {
            // Avoid flooding the log with the same warning every tick.
            if self
                .log
                .last()
                .is_none_or(|(t, l)| *l != m || now - t > DAY)
            {
                self.log_line(now, m);
            }
        }
    }
}

impl Sim {
    pub fn colony_at(&self, body: usize) -> Option<usize> {
        self.colonies
            .iter()
            .position(|c| c.star == self.here && c.body == body)
    }

    /// Is the ship close enough, and slow enough, to found a colony on body `i`?
    fn near_for_colony(&self, i: usize) -> Result<(), &'static str> {
        let b = &self.bodies[i];
        let alt = (b.pos - self.ship.pos).length() - b.radius;
        if alt > (2.0 * b.radius).max(20_000.0) {
            return Err("trop loin du corps");
        }
        if (self.ship.vel - b.vel).length() > 250.0 {
            return Err("vitesse relative trop elevee");
        }
        Ok(())
    }

    /// Found a colony on body `i` using a colony kit from the hold.
    pub fn found_colony(&mut self, i: usize, name: Option<String>) -> Result<usize, &'static str> {
        if self.docked.is_some() {
            return Err("decollez d'abord");
        }
        let cb = self.system.bodies.get(i).ok_or("corps inconnu")?.clone();
        if !matches!(
            cb.kind,
            BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Asteroid(_)
        ) {
            return Err("ce corps ne peut pas etre colonise");
        }
        if self.colony_at(i).is_some() {
            return Err("deja colonise");
        }
        self.near_for_colony(i)?;
        let data = std::sync::Arc::clone(&self.data);
        let need = [
            ("construction_materials", KIT_MATERIALS),
            ("colonists", KIT_COLONISTS_KG),
            ("food", KIT_FOOD),
            ("water", KIT_WATER),
        ];
        for (id, kg) in need {
            let r = data.resource_index(id).ok_or("donnees incompletes")?;
            if self.ship.cargo.kg[r] + 1e-6 < kg {
                return Err("kit de colonisation incomplet (materiaux, colons, vivres, eau)");
            }
        }
        if self.credits < PERMIT_COST {
            return Err("permis de colonisation : fonds insuffisants");
        }
        for (id, kg) in need {
            let r = data.resource_index(id).ok_or("donnees incompletes")?;
            self.ship.cargo.remove(&data, r, kg);
        }
        self.credits -= PERMIT_COST;
        let tags = site_tags(&cb);
        let id = self.next_colony;
        self.next_colony += 1;
        let d_au = (self.bodies[i].pos.length() / vector_world::orbit::AU).max(0.01);
        let founders = self
            .data
            .ethos
            .profile(self.faction.map(|f| self.data.factions[f].id.as_str()));
        // The belief: among those that fit the faith of the founders, by the hash of the place.
        let foi = self
            .data
            .ethos
            .axis("foi")
            .and_then(|a| founders.get(a))
            .copied()
            .unwrap_or(0.0);
        let fitting: Vec<usize> = (0..self.data.religions.len())
            .filter(|&r| self.data.religions[r].fits(foi))
            .collect();
        let faith = if fitting.is_empty() {
            0
        } else {
            fitting[(hash_coord(self.system.seed, i as i64, id as i64, 53) % fitting.len() as u64)
                as usize]
        };
        let mut c = Colony {
            id,
            name: name.unwrap_or_else(|| {
                let nh = hash_coord(self.system.seed, i as i64, id as i64, 0xC010);
                let culture = self.faction.map(|f| self.data.factions[f].culture.as_str());
                vector_world::names::Spec::new("colony")
                    .culture(Some(
                        culture.unwrap_or_else(|| vector_world::names::culture_id(nh)),
                    ))
                    .host(&cb.name)
                    .name(nh)
            }),
            star: self.here,
            system_seed: self.system.seed,
            body: i,
            body_name: cb.name.clone(),
            deposits: site_deposits(&data, &cb, self.system.seed),
            tags,
            flux: self.system.luminosity / (d_au * d_au),
            founded: self.time,
            last_tick: self.time,
            pop: KIT_COLONISTS_KG / COLONIST_KG,
            happiness: 0.7,
            unrest: 0.0,
            pest: 0.0,
            epidemic: None,
            culture: founders.clone(),
            culture_base: founders,
            faith,
            storage: vec![0.0; data.resources.len()],
            buildings: Vec::new(),
            policies: data.policies.iter().map(|p| p.default_level).collect(),
            treasury: 2000.0,
            terraform: 0.0,
            research: 0.0,
            debt_days: 0.0,
            stats: ColonyStats::default(),
            log: Vec::new(),
        };
        for def_id in ["habitat_dome", "solar_farm", "spaceport"] {
            if let Some(d) = data.building_index(def_id) {
                if data.buildings[d].sites.iter().any(|s| c.tags.contains(s)) {
                    c.buildings.push(BuildingInst {
                        def: d,
                        progress: 1.0,
                        enabled: true,
                        recipe: 0,
                        health: 1.0,
                    });
                }
            }
        }
        c.log_line(self.time, format!("Fondation de {}.", c.name));
        c.update(&data, self.time);
        self.colonies.push(c);
        self.events.push(Event::ColonyFounded {
            index: self.colonies.len() - 1,
        });
        Ok(self.colonies.len() - 1)
    }

    /// The colony the ship is docked at (spaceport services).
    pub fn docked_colony(&self) -> Option<usize> {
        self.docked.and_then(|b| self.colony_at(b))
    }

    pub fn colony_deposit(
        &mut self,
        ci: usize,
        res: usize,
        kg: Option<f64>,
    ) -> Result<f64, &'static str> {
        self.docked_colony()
            .filter(|&c| c == ci)
            .ok_or("non amarre a cette colonie")?;
        let data = std::sync::Arc::clone(&self.data);
        let have = self.ship.cargo.kg[res];
        let want = kg.unwrap_or(have).min(have);
        let c = &mut self.colonies[ci];
        let free = c.free_space();
        let amount = want.min(free);
        if amount <= 0.0 {
            return Err("rien a deposer (ou entrepot plein)");
        }
        self.ship.cargo.remove(&data, res, amount);
        c.storage[res] += amount;
        Ok(amount)
    }

    pub fn colony_withdraw(
        &mut self,
        ci: usize,
        res: usize,
        kg: Option<f64>,
    ) -> Result<f64, &'static str> {
        self.docked_colony()
            .filter(|&c| c == ci)
            .ok_or("non amarre a cette colonie")?;
        let data = std::sync::Arc::clone(&self.data);
        let c = &mut self.colonies[ci];
        let want = kg.unwrap_or(c.storage[res]).min(c.storage[res]);
        let got = self.ship.cargo.add(&data, res, want);
        if got <= 0.0 {
            return Err("rien a retirer (ou soute pleine)");
        }
        c.storage[res] -= got;
        Ok(got)
    }

    /// Order the construction of building `def` (materials come from the colony stores).
    pub fn colony_build(&mut self, ci: usize, def: usize) -> Result<(), &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let d = data.buildings.get(def).ok_or("batiment inconnu")?;
        if !self.is_unlocked("building", &d.id) {
            return Err("technologie non recherchee (F4, ou regle: technologies)");
        }
        let (mult, free) = (self.rules.build_cost, self.rules.infinite_resources);
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        if !d.sites.iter().any(|s| c.tags.contains(s)) {
            return Err("site inadapte a ce batiment");
        }
        for (r, kg) in &d.cost {
            let i = data.resource_index(r).ok_or("donnees incompletes")?;
            if !free && c.storage[i] + 1e-6 < *kg * mult {
                return Err("materiaux insuffisants dans les entrepots");
            }
        }
        if c.buildings.len() >= 80 {
            return Err("trop de batiments");
        }
        if !free {
            for (r, kg) in &d.cost {
                let i = data.resource_index(r).ok_or("donnees incompletes")?;
                c.storage[i] -= kg * mult;
            }
        }
        c.buildings.push(BuildingInst {
            def,
            progress: 0.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
        });
        let t = self.time;
        c.log_line(t, format!("Construction lancee : {}.", d.name));
        Ok(())
    }

    pub fn colony_demolish(&mut self, ci: usize, bi: usize) -> Result<(), &'static str> {
        let data = std::sync::Arc::clone(&self.data);
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        if bi >= c.buildings.len() {
            return Err("batiment inconnu");
        }
        let inst = c.buildings.remove(bi);
        // Half of the materials come back.
        let refund = if inst.built() {
            0.5
        } else {
            0.8 * (1.0 - inst.progress) + 0.2
        };
        for (r, kg) in &data.buildings[inst.def].cost {
            if let Some(i) = data.resource_index(r) {
                c.storage[i] += kg * refund * 0.5;
            }
        }
        Ok(())
    }

    pub fn colony_set_policy(
        &mut self,
        ci: usize,
        policy: usize,
        level: usize,
    ) -> Result<(), &'static str> {
        let n = self
            .data
            .policies
            .get(policy)
            .ok_or("politique inconnue")?
            .levels
            .len();
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        c.policies[policy] = level.min(n - 1);
        Ok(())
    }

    pub fn colony_withdraw_treasury(&mut self, ci: usize) -> Result<f64, &'static str> {
        self.docked_colony()
            .filter(|&c| c == ci)
            .ok_or("non amarre a cette colonie")?;
        let c = &mut self.colonies[ci];
        let amount = c.treasury.max(0.0);
        if amount < 1.0 {
            return Err("tresorerie vide");
        }
        c.treasury -= amount;
        self.credits += amount;
        Ok(amount)
    }

    /// Advance every colony (also those in other systems) to the current time.
    pub(crate) fn update_colonies(&mut self) {
        if self.colonies.is_empty() {
            return;
        }
        let data = std::sync::Arc::clone(&self.data);
        let now = self.time;
        let mut advanced = Vec::new();
        for (ci, c) in self.colonies.iter_mut().enumerate() {
            let (before, t0) = (c.log.len(), c.last_tick);
            c.update(&data, now);
            if c.last_tick > t0 {
                advanced.push((ci, t0, c.last_tick));
            }
            if c.log.len() > before && c.star == self.here {
                if let Some((_, text)) = c.log.last() {
                    self.events
                        .push(Event::ColonyNews(format!("{}: {}", c.name, text)));
                }
            }
        }
        // Catastrophes of the days each colony just lived through (see `planet`).
        for (ci, t0, t1) in advanced {
            let before = self.colonies[ci].log.len();
            self.apply_catastrophes(ci, t0, t1);
            self.apply_ecology(ci, t1);
            let c = &self.colonies[ci];
            if c.log.len() > before && c.star == self.here {
                if let Some((_, text)) = c.log.last() {
                    self.events
                        .push(Event::ColonyNews(format!("{}: {}", c.name, text)));
                }
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::ShipInput;
    use std::sync::Arc;
    use vector_core::Vec3;
    use vector_world::galaxy::Galaxy;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn sim() -> Sim {
        let g = Galaxy::new(5);
        let e = g
            .nearby([0.0; 3], 60.0)
            .into_iter()
            .find(|e| {
                let s = generate_system(e.seed);
                s.bodies
                    .iter()
                    .any(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            })
            .expect("system with a rocky planet");
        let mut s = Sim::new_in_galaxy(g, &e, data());
        s.reveal_all();
        s
    }

    fn rocky(s: &Sim) -> usize {
        s.system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()))
            .expect("rocky planet")
    }

    fn stock_kit(s: &mut Sim) {
        let data = Arc::clone(&s.data);
        s.ship.cargo.cap_mass = 1.0e6;
        s.ship.cargo.cap_vol = 1.0e6;
        for (id, kg) in [
            ("construction_materials", 3000.0),
            ("colonists", 400.0),
            ("food", 300.0),
            ("water", 600.0),
        ] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.credits = 100_000.0;
    }

    fn park_at(s: &mut Sim, body: usize) {
        s.ship.pos = s.bodies[body].pos - Vec3::new(0.0, 0.0, s.bodies[body].radius + 5000.0);
        s.ship.vel = s.bodies[body].vel;
        s.update_ref();
    }

    pub(crate) fn founded() -> (Sim, usize) {
        let mut s = sim();
        let b = rocky(&s);
        stock_kit(&mut s);
        park_at(&mut s, b);
        let c = s.found_colony(b, None).expect("found");
        (s, c)
    }

    fn fill(s: &mut Sim, ci: usize, items: &[(&str, f64)]) {
        let data = Arc::clone(&s.data);
        for (id, kg) in items {
            s.colonies[ci].storage[data.resource_index(id).expect("res")] += kg;
        }
    }

    #[test]
    fn founding_needs_the_kit_the_funds_and_proximity() {
        let mut s = sim();
        let b = rocky(&s);
        assert!(s.found_colony(b, None).is_err(), "too far");
        park_at(&mut s, b);
        assert!(s.found_colony(b, None).is_err(), "no kit");
        stock_kit(&mut s);
        s.credits = 100.0;
        assert!(s.found_colony(b, None).is_err(), "no money");
        s.credits = 100_000.0;
        s.ship.vel = s.bodies[b].vel + Vec3::new(500.0, 0.0, 0.0);
        assert!(s.found_colony(b, None).is_err(), "too fast");
        s.ship.vel = s.bodies[b].vel;
        let ci = s
            .found_colony(b, Some("Nouvel Eden".into()))
            .expect("found");
        assert_eq!(s.colonies[ci].name, "Nouvel Eden");
        assert!((s.colonies[ci].pop - 5.0).abs() < 1e-9);
        assert!(
            s.colonies[ci].buildings.len() >= 2
                && s.colonies[ci].buildings.iter().all(|b| b.built())
        );
        assert!(s.credits < 100_000.0 - PERMIT_COST + 1.0);
        let r = s
            .data
            .resource_index("construction_materials")
            .expect("res");
        assert!(s.ship.cargo.kg[r] < 1e-9, "the kit is consumed");
        assert!(s.found_colony(b, None).is_err(), "already colonised");
        assert_eq!(s.colony_at(b), Some(ci));
    }

    #[test]
    fn deposits_follow_the_body_type() {
        let s = sim();
        let d = &s.data;
        let rocky_body = &s.system.bodies[rocky(&s)];
        let dep = site_deposits(d, rocky_body, 1);
        assert!(dep.iter().any(|&(i, _)| d.resources[i].id == "iron"));
        let giant = vector_world::generate_system(0)
            .bodies
            .iter()
            .find(|b| matches!(b.kind, BodyKind::Planet(k) if k.is_giant()))
            .cloned();
        if let Some(g) = giant {
            let dg = site_deposits(d, &g, 1);
            let gas: f64 = dg
                .iter()
                .filter(|&&(i, _)| d.resources[i].category == "gas")
                .map(|p| p.1)
                .sum();
            let all: f64 = dg.iter().map(|p| p.1).sum();
            assert!(gas / all > 0.9, "giants are mostly gas: {}", gas / all);
            assert!(site_tags(&g).contains(&"gas_giant".to_string()));
        }
        assert!(site_tags(rocky_body).contains(&"rocky".to_string()));
    }

    #[test]
    fn a_supplied_colony_grows_and_stays_happy() {
        let (mut s, ci) = founded();
        fill(
            &mut s,
            ci,
            &[
                ("food", 5000.0),
                ("water", 2000.0),
                ("consumer_goods", 400.0),
            ],
        );
        s.time += 60.0 * DAY;
        s.update_colonies();
        let c = &s.colonies[ci];
        assert!(c.happiness > 0.55, "{}", c.happiness);
        assert!(c.pop >= 5.0 && c.pop <= c.stats.housing + 1e-6);
        assert!(c.stats.food_sat > 0.99);
        assert!(
            c.stats.power_supply > c.stats.power_demand,
            "a solar farm powers a dome"
        );
    }

    #[test]
    fn epidemics_break_out_in_big_colonies_kill_and_end() {
        // Outbreaks happen, and are logged, in a crowded colony left to itself.
        let (mut s, ci) = founded();
        s.colonies[ci].pop = 150.0;
        let mut outbreaks = 0;
        for _ in 0..80 {
            fill(
                &mut s,
                ci,
                &[
                    ("food", 20000.0),
                    ("water", 20000.0),
                    ("consumer_goods", 2000.0),
                ],
            );
            let had = s.colonies[ci].epidemic.is_some();
            s.time += 10.0 * DAY;
            s.update_colonies();
            s.colonies[ci].pop = s.colonies[ci].pop.max(150.0);
            if !had && s.colonies[ci].epidemic.is_some() {
                outbreaks += 1;
            }
        }
        assert!(outbreaks >= 1, "no outbreak in 800 days");
        assert!(s.colonies[ci]
            .log
            .iter()
            .any(|(_, l)| l.contains("Epidemie")));

        // A running lethal epidemic costs lives that the same colony without it keeps.
        let lethal = s
            .data
            .diseases
            .iter()
            .position(|d| d.id == "variole_spatiale")
            .expect("smallpox");
        let run = |epidemic: bool| {
            let (mut t, c) = founded();
            t.colonies[c].pop = 60.0;
            fill(
                &mut t,
                c,
                &[
                    ("food", 5000.0),
                    ("water", 5000.0),
                    ("consumer_goods", 500.0),
                ],
            );
            if epidemic {
                t.colonies[c].epidemic = Some(Epidemic {
                    disease: lethal,
                    days_left: 100.0,
                    strength: 1.0,
                });
            }
            t.time += 10.0 * DAY;
            t.update_colonies();
            t.colonies[c].pop
        };
        assert!(
            run(true) < run(false) - 0.5,
            "{} vs {}",
            run(true),
            run(false)
        );

        // And it ends by itself.
        let (mut t, c) = founded();
        t.colonies[c].pop = 60.0;
        fill(&mut t, c, &[("food", 5000.0), ("water", 5000.0)]);
        t.colonies[c].epidemic = Some(Epidemic {
            disease: lethal,
            days_left: 3.0,
            strength: 0.5,
        });
        t.time += 10.0 * DAY;
        t.update_colonies();
        let ended = t.colonies[c].epidemic.is_none()
            || t.colonies[c].epidemic.as_ref().unwrap().days_left > 3.0;
        assert!(
            ended,
            "the first epidemic ended (a new one may have started)"
        );
        assert!(t.colonies[c]
            .log
            .iter()
            .any(|(_, l)| l.contains("terminee")));
    }

    #[test]
    fn culture_starts_as_the_founders_drifts_with_policies_and_resents_what_clashes() {
        let (mut s, ci) = founded();
        let d = Arc::clone(&s.data);
        let axes = d.ethos.axes.len();
        assert_eq!(s.colonies[ci].culture.len(), axes);
        assert_eq!(s.colonies[ci].culture, s.colonies[ci].culture_base);
        let sec = d.policy_index("security").unwrap();
        let violence = d.ethos.axis("violence").unwrap();
        let before = s.colonies[ci].culture[violence];
        // Martial law for years pushes the culture towards violence.
        s.colonies[ci].policies[sec] = 2;
        fill(
            &mut s,
            ci,
            &[
                ("food", 50000.0),
                ("water", 50000.0),
                ("consumer_goods", 5000.0),
            ],
        );
        for _ in 0..30 {
            s.time += 60.0 * DAY;
            fill(
                &mut s,
                ci,
                &[
                    ("food", 50000.0),
                    ("water", 50000.0),
                    ("consumer_goods", 5000.0),
                ],
            );
            s.update_colonies();
        }
        let after = s.colonies[ci].culture[violence];
        assert!(after > before + 0.1 || after >= 0.99, "{before} -> {after}");
        // A pacifist people under martial law is unhappier than a neutral one.
        let mut c = s.colonies[ci].clone();
        c.policies[sec] = 2;
        c.culture[violence] = -0.8;
        let angry = c.culture_friction(&d);
        c.culture[violence] = 0.5;
        let calm = c.culture_friction(&d);
        assert!(angry > calm + 0.05, "{angry} vs {calm}");
        // The target of a colony without any push is its base.
        let mut free = s.colonies[ci].clone();
        free.policies = d.policies.iter().map(|p| p.default_level).collect();
        assert_eq!(free.culture_target(&d), free.culture_base);
    }

    #[test]
    fn a_colony_has_a_belief_that_fits_its_people_and_it_changes_morale() {
        let (mut s, ci) = founded();
        let d = Arc::clone(&s.data);
        let c = &s.colonies[ci];
        let foi = d.ethos.axis("foi").map_or(0.0, |a| c.culture_base[a]);
        assert!(
            d.religions[c.faith].fits(foi),
            "{foi} vs {}",
            d.religions[c.faith].name
        );
        let cheerful = (0..d.religions.len())
            .max_by(|&a, &b| d.religions[a].morale.total_cmp(&d.religions[b].morale))
            .unwrap();
        let dry = (0..d.religions.len())
            .min_by(|&a, &b| d.religions[a].morale.total_cmp(&d.religions[b].morale))
            .unwrap();
        let run = |s: &mut Sim, faith: usize| {
            s.colonies[ci].faith = faith;
            s.colonies[ci].happiness = 0.5;
            fill(
                s,
                ci,
                &[
                    ("food", 50000.0),
                    ("water", 50000.0),
                    ("consumer_goods", 5000.0),
                ],
            );
            s.time += 30.0 * DAY;
            s.update_colonies();
            s.colonies[ci].happiness
        };
        let a = run(&mut s, cheerful);
        let b = run(&mut s, dry);
        assert!(a > b + 0.02, "{a} vs {b}");
    }

    #[test]
    fn starvation_kills_and_breeds_unrest() {
        let (mut s, ci) = founded();
        s.colonies[ci].pop = 40.0;
        s.colonies[ci].storage.fill(0.0);
        s.time += 50.0 * DAY;
        s.update_colonies();
        let c = &s.colonies[ci];
        assert!(c.pop < 40.0, "no food, population falls: {}", c.pop);
        assert!(c.happiness < 0.5, "{}", c.happiness);
        assert!(c.log.iter().any(|(_, l)| l.contains("Famine")));
    }

    #[test]
    fn construction_consumes_materials_and_respects_sites() {
        let (mut s, ci) = founded();
        let mine = s.data.building_index("surface_mine").expect("def");
        assert!(s.colony_build(ci, mine).is_err(), "no materials in storage");
        fill(
            &mut s,
            ci,
            &[
                ("construction_materials", 5000.0),
                ("machinery", 500.0),
                ("steel", 2000.0),
            ],
        );
        let cm = s
            .data
            .resource_index("construction_materials")
            .expect("res");
        let before = s.colonies[ci].storage[cm];
        s.colony_build(ci, mine).expect("build");
        assert!(s.colonies[ci].storage[cm] < before);
        let n = s.colonies[ci].buildings.len();
        assert!(!s.colonies[ci].buildings[n - 1].built());
        s.time += 12.0 * DAY;
        s.update_colonies();
        assert!(
            s.colonies[ci].buildings[n - 1].built(),
            "finished after its build time"
        );
        let scoop = s.data.building_index("gas_scoop").expect("def");
        fill(
            &mut s,
            ci,
            &[
                ("hull_plates", 500.0),
                ("steel", 2000.0),
                ("machinery", 500.0),
            ],
        );
        assert!(
            s.colony_build(ci, scoop).is_err(),
            "a gas scoop needs a gas giant"
        );
    }

    #[test]
    fn mining_extracts_deposits_into_storage_and_a_smelter_conserves_mass() {
        let (mut s, ci) = founded();
        for id in ["surface_mine", "smelter", "habitat_dome", "habitat_dome"] {
            let d = s.data.building_index(id).expect("def");
            s.colonies[ci].buildings.push(BuildingInst {
                def: d,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
            });
        }
        fill(
            &mut s,
            ci,
            &[
                ("food", 9000.0),
                ("water", 3000.0),
                ("consumer_goods", 500.0),
            ],
        );
        s.colonies[ci].pop = 120.0; // enough workers
        s.time += 30.0 * DAY;
        s.update_colonies();
        let c = &s.colonies[ci];
        let iron = s.data.resource_index("iron").expect("iron");
        assert!(
            c.storage[iron] > 0.0
                || c.storage[s.data.resource_index("steel").expect("steel")] > 0.0,
            "ore must be coming in"
        );
        // Direct mass check: run a smelter batch on controlled stock.
        let (mut s2, ci2) = founded();
        let sm = s2.data.building_index("smelter").expect("def");
        s2.colonies[ci2].buildings.clear();
        s2.colonies[ci2].buildings.push(BuildingInst {
            def: sm,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
        });
        s2.colonies[ci2].pop = 100.0;
        s2.colonies[ci2].happiness = 1.0;
        fill(
            &mut s2,
            ci2,
            &[
                ("iron", 4000.0),
                ("carbon", 400.0),
                ("food", 9000.0),
                ("water", 3000.0),
                ("consumer_goods", 500.0),
            ],
        );
        let total_before: f64 = s2.colonies[ci2].storage.iter().sum();
        // Power is missing without a plant, so give it one.
        let solar = s2.data.building_index("solar_farm").expect("def");
        s2.colonies[ci2].buildings.push(BuildingInst {
            def: solar,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
        });
        let dome = s2.data.building_index("habitat_dome").expect("def");
        s2.colonies[ci2].buildings.push(BuildingInst {
            def: dome,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
        });
        s2.time += 5.0 * DAY;
        s2.update_colonies();
        let c2 = &s2.colonies[ci2];
        let steel = s2.data.resource_index("steel").expect("steel");
        assert!(
            c2.storage[steel] > 100.0,
            "the smelter must make steel: {}",
            c2.storage[steel]
        );
        let total_after: f64 = c2.storage.iter().sum();
        assert!(
            total_after <= total_before + 1e-6,
            "mass cannot appear from nowhere: {total_before} -> {total_after}"
        );
    }

    #[test]
    fn policies_trade_money_for_happiness_and_research_accumulates() {
        let (mut s, ci) = founded();
        let tax = s.data.policy_index("tax").expect("policy");
        s.colonies[ci].pop = 50.0;
        fill(
            &mut s,
            ci,
            &[
                ("food", 9000.0),
                ("water", 3000.0),
                ("consumer_goods", 900.0),
            ],
        );
        let dome = s.data.building_index("habitat_dome").expect("def");
        for _ in 0..2 {
            s.colonies[ci].buildings.push(BuildingInst {
                def: dome,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
            });
        }
        let mut a = s.colonies[ci].clone();
        let mut b = s.colonies[ci].clone();
        a.policies[tax] = 0;
        b.policies[tax] = 3;
        a.update(&s.data, s.time + 90.0 * DAY);
        b.update(&s.data, s.time + 90.0 * DAY);
        assert!(
            a.happiness > b.happiness,
            "no tax is more popular: {} vs {}",
            a.happiness,
            b.happiness
        );
        assert!(b.treasury > a.treasury, "heavy tax fills the treasury");
        // A lab turns electronics into research.
        let (mut s3, c3) = founded();
        let lab = s3.data.building_index("research_lab").expect("def");
        s3.colonies[c3].buildings.push(BuildingInst {
            def: lab,
            progress: 1.0,
            enabled: true,
            recipe: 0,
            health: 1.0,
        });
        s3.colonies[c3].pop = 60.0;
        fill(
            &mut s3,
            c3,
            &[
                ("electronics", 5000.0),
                ("food", 9000.0),
                ("water", 3000.0),
                ("consumer_goods", 500.0),
            ],
        );
        s3.time += 20.0 * DAY;
        s3.update_colonies();
        assert!(
            s3.colonies[c3].research > 10.0,
            "{}",
            s3.colonies[c3].research
        );
    }

    #[test]
    fn docked_colony_transfers_and_treasury() {
        let (mut s, ci) = founded();
        let b = s.colonies[ci].body;
        s.try_dock_colony().expect("dock at the colony port");
        assert_eq!(s.docked_colony(), Some(ci));
        let data = Arc::clone(&s.data);
        let iron = data.resource_index("iron").expect("iron");
        s.ship.cargo.add(&data, iron, 500.0);
        assert_eq!(s.colony_deposit(ci, iron, None).expect("deposit"), 500.0);
        assert_eq!(s.colonies[ci].storage[iron], 500.0);
        assert_eq!(
            s.colony_withdraw(ci, iron, Some(200.0)).expect("withdraw"),
            200.0
        );
        s.colonies[ci].treasury = 1500.0;
        let c0 = s.credits;
        let got = s.colony_withdraw_treasury(ci).expect("treasury");
        assert!((s.credits - c0 - got).abs() < 1e-9);
        assert!(s.sell(iron, None).is_err(), "no market at a colony port");
        s.undock();
        assert!(s.colony_deposit(ci, iron, None).is_err(), "must be docked");
        let _ = b;
    }

    #[test]
    fn a_century_of_colony_life_is_stable() {
        let (mut s, ci) = founded();
        for id in [
            "habitat_dome",
            "habitat_dome",
            "hydroponics",
            "water_plant",
            "surface_mine",
            "medical_center",
            "life_support_plant",
            "warehouse",
        ] {
            let d = s.data.building_index(id).expect("def");
            s.colonies[ci].buildings.push(BuildingInst {
                def: d,
                progress: 1.0,
                enabled: true,
                recipe: 0,
                health: 1.0,
            });
        }
        s.colonies[ci].pop = 80.0;
        fill(&mut s, ci, &[("food", 20_000.0), ("water", 8_000.0)]);
        for year in 1..=100 {
            s.time = f64::from(year) * 365.0 * DAY;
            // Imports keep the colony alive so we exercise the long-run numerics rather than starvation.
            fill(
                &mut s,
                ci,
                &[
                    ("food", 8_000.0),
                    ("water", 3_000.0),
                    ("consumer_goods", 400.0),
                    ("construction_materials", 100.0),
                ],
            );
            let cap = s.colonies[ci].stats.storage_cap.max(BASE_STORAGE);
            let total: f64 = s.colonies[ci].storage.iter().sum();
            if total > cap {
                let k = cap / total;
                s.colonies[ci].storage.iter_mut().for_each(|v| *v *= k);
            }
            s.update_colonies();
            let c = &s.colonies[ci];
            assert!(
                c.pop.is_finite() && c.pop >= 0.0 && c.pop <= c.stats.housing.max(80.0) + 1e-6,
                "year {year}: pop {}",
                c.pop
            );
            assert!(c.happiness.is_finite() && (0.0..=1.0).contains(&c.happiness));
            assert!(
                c.storage.iter().all(|v| v.is_finite() && *v >= -1e-6),
                "negative or NaN storage in year {year}"
            );
            assert!(c.treasury.is_finite());
        }
        let _ = ShipInput::default();
    }
}
