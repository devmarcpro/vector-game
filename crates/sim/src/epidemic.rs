//! Epidemics: a deterministic compartmental model (susceptible, exposed, infectious, recovered)
//! shared by every place that can fall ill: the colonies of the player, the sectors of the
//! galaxy (computed from the seed, nothing is stored) and the ship, where the symptoms of a
//! pathogen are a disease of `diseases.ron` carried by a crewmate (one system of illness).
//!
//! * A pathogen (`data/epidemics.ron`) has a transmissibility (R0), an incubation, an infectious
//!   period, an immunity that wears off (or not) and a fatality.
//! * Measures change the dynamics: a quarantine cuts the contacts, medical care and medicine
//!   shorten the illness and spare lives, vaccines move the susceptible to the immune.
//! * Places infect each other: by proximity (the sector's outbreak seeps into the colonies in
//!   it, colonies of one system trade cases) and by travel (a sick crewmate who docks seeds the
//!   colony, a colony in the grip of a pathogen infects the crew who come to land).
//! * The effects are real: lost production, deaths, fear in the colony, prices of the goods
//!   (`goods_factors`, used by the exchange) and the news of the sectors around.
use crate::bourse::DAY;
use crate::colony::Colony;
use crate::planet::unit;
use crate::politics::{cell_of, Cell};
use crate::Sim;
use serde::{Deserialize, Serialize};
use vector_core::hash_coord;
use vector_data::{GameData, Pathogen};

/// Chance per day of a spontaneous outbreak in a colony of 100 people with no medical care.
pub const EPIDEMIC_RATE: f64 = 0.004;
/// Sub-step of the integration, in days.
const STEP_DAYS: f64 = 0.25;
/// Outbreaks of the sectors start (or not) at the turn of a period.
pub const WORLD_PERIOD: f64 = 120.0 * DAY;
/// Chance per period, per sector, of an outbreak of a pathogen of weight 1.
pub const P_WORLD: f64 = 0.01;
/// How many periods back an outbreak may have started and still be running.
const WORLD_LOOKBACK: i64 = 3;
/// How long an outbreak is followed at most, days.
const WORLD_HORIZON: f64 = 420.0;
/// Population of a sector for the model, share of it infected at the start, and its medical care.
const WORLD_POP: f64 = 1.0e6;
const WORLD_SEED_SHARE: f64 = 1.0e-5;
const WORLD_CARE: f64 = 0.3;
/// Share of the infectious at which a sector's outbreak is declared (and makes the news).
pub const DECLARE_AT: f64 = 0.005;
/// Share of the people travelling in and out of a colony each day.
const TRAFFIC: f64 = 0.002;
/// Daily chance that a healthy person who spends the day among the sick of a place falls ill,
/// per share of the place infectious.
const CREW_EXPOSURE: f64 = 0.4;

/// The progress of one pathogen in one place. The shares are of the living, and add up to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Outbreak {
    /// Index into `GameData::pathogens`.
    pub pathogen: usize,
    pub s: f64,
    pub e: f64,
    pub i: f64,
    pub r: f64,
    /// Told once that it has become grave.
    pub grave: bool,
}

impl Outbreak {
    pub fn new(pathogen: usize) -> Outbreak {
        Outbreak {
            pathogen,
            s: 1.0,
            e: 0.0,
            i: 0.0,
            r: 0.0,
            grave: false,
        }
    }

    /// Is somebody still sick or incubating?
    pub fn active(&self) -> bool {
        self.e + self.i > 0.0
    }

    /// Move `share` of the people from susceptible to exposed.
    pub fn infect(&mut self, share: f64) {
        let k = share.min(self.s).max(0.0);
        self.s -= k;
        self.e += k;
    }
}

/// What a place does against an epidemic, over one step.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Measures {
    /// 1 = normal contacts; quarantine and distance lower it, crowding raises it.
    pub contact: f64,
    /// 0..1: medical care (centres, medicine in stock).
    pub care: f64,
    /// Share of the people vaccinated during the step.
    pub vaccinated: f64,
}

impl Measures {
    pub const NONE: Measures = Measures {
        contact: 1.0,
        care: 0.0,
        vaccinated: 0.0,
    };
}

/// Effective reproduction number now: how many people one case infects in these conditions.
pub fn r_eff(p: &Pathogen, o: &Outbreak, m: Measures) -> f64 {
    p.r0 * m.contact * o.s / (1.0 + 0.8 * m.care)
}

/// Advance an outbreak by `days`; returns the share of the people (before the step) who died.
/// `pop` is the number of people, to let an outbreak with less than one case die out.
pub fn step(p: &Pathogen, o: &mut Outbreak, m: Measures, days: f64, pop: f64) -> f64 {
    if days <= 0.0 {
        return 0.0;
    }
    let vaccinated = o.s.min(m.vaccinated.max(0.0)) * p.vaccine_eff;
    o.s -= vaccinated;
    o.r += vaccinated;
    let n = ((days / STEP_DAYS).ceil() as usize).clamp(1, 800);
    let dt = days / n as f64;
    let beta = p.r0 / p.infectious_days * m.contact.max(0.0);
    let sigma = 1.0 / p.incubation_days;
    let gamma = (1.0 + 0.8 * m.care) / p.infectious_days;
    let omega = if p.immunity_days > 0.0 {
        1.0 / p.immunity_days
    } else {
        0.0
    };
    let fatality = p.fatality * (1.0 - 0.6 * m.care.clamp(0.0, 1.0));
    let mut dead = 0.0;
    for _ in 0..n {
        let infected = (beta * o.s * o.i * dt).min(o.s);
        let incubated = (sigma * dt).min(1.0) * o.e;
        let recovered = (gamma * dt).min(1.0) * o.i;
        let waned = (omega * dt).min(1.0) * o.r;
        o.s += waned - infected;
        o.e += infected - incubated;
        o.i += incubated - recovered;
        o.r += recovered * (1.0 - fatality) - waned;
        dead += recovered * fatality;
    }
    let living = (1.0 - dead).max(1e-9);
    for v in [&mut o.s, &mut o.e, &mut o.i, &mut o.r] {
        *v = (*v / living).max(0.0);
    }
    // Less than one case left in the whole place: the chain of contagion is broken.
    if (o.e + o.i) * pop < 0.5 {
        o.r += o.e + o.i;
        o.e = 0.0;
        o.i = 0.0;
    }
    dead
}

/// What makes a spontaneous outbreak of a pathogen likelier in a colony.
#[derive(Clone, Copy, Debug)]
pub struct EpiCtx {
    /// Medical services coverage, 0..1.
    pub med: f64,
    /// Overcrowding, 0..0.5.
    pub crowd: f64,
    pub food_sat: f64,
    pub water_sat: f64,
    /// Multiplier on spontaneous outbreaks (the world setting of the start).
    pub freq: f64,
}

fn trigger_factor(data: &GameData, trigger: &str, c: &Colony, x: &EpiCtx) -> f64 {
    match trigger {
        "crowd" => 0.3 + 6.0 * x.crowd + (c.pop / 400.0).min(1.0),
        "hunger" => 0.3 + 5.0 * (1.0 - x.food_sat),
        "water" => 0.3 + 6.0 * (1.0 - x.water_sat),
        "wild" => 0.4 + 8.0 * c.pest + 0.3 * c.policy_value(data, "gather") as f64,
        "lab" => 0.1 + 2.0 * c.research / (c.research + 400.0),
        "alien" => 0.4,
        _ => 1.0,
    }
}

/// Contact reduction of each level of the quarantine policy (none, checks, strict, total).
const QUARANTINE_CONTACT: [f64; 4] = [1.0, 0.8, 0.5, 0.2];
/// Share of the travellers' cases a quarantine keeps out.
const QUARANTINE_BLOCK: [f64; 4] = [0.0, 0.3, 0.7, 0.95];
/// Medicine used per person per day of an outbreak, and the care it brings.
const MEDICINE_KG: f64 = 0.0004;
/// Vaccine used per person vaccinated, and the share of the people a campaign vaccinates per day.
const VACCINE_KG: f64 = 0.1;
const CAMPAIGN_PER_DAY: f64 = 0.04;

impl Colony {
    /// Chosen level of the policy `id` (0 when the policy does not exist).
    pub fn policy_value(&self, data: &GameData, id: &str) -> usize {
        data.policy_index(id)
            .and_then(|p| self.policies.get(p))
            .copied()
            .unwrap_or(0)
    }

    fn axis_value(&self, data: &GameData, id: &str) -> f64 {
        data.ethos
            .axis(id)
            .and_then(|a| self.culture.get(a))
            .copied()
            .unwrap_or(0.0)
    }

    /// Quarantine level 0..3.
    fn quarantine_level(&self, data: &GameData) -> usize {
        self.policy_value(data, "quarantine").min(3)
    }

    /// Effective reproduction number of an outbreak of the colony under its quarantine and
    /// mores (before medical care): above 1 it grows, below 1 it ebbs.
    pub fn r_effective(&self, data: &GameData, o: &Outbreak) -> f64 {
        let m = Measures {
            contact: QUARANTINE_CONTACT[self.quarantine_level(data)]
                * (1.0 + 0.15 * self.axis_value(data, "ouverture")),
            ..Measures::NONE
        };
        r_eff(&data.pathogens[o.pathogen], o, m)
    }

    /// Share of the production lost to the sick.
    pub fn labor_loss(&self, data: &GameData) -> f64 {
        self.outbreaks
            .iter()
            .map(|o| data.pathogens[o.pathogen].prod_hit * (o.i / 0.1).min(3.0))
            .sum::<f64>()
            .min(0.8)
    }

    /// Fear of the people: lowers the mood of a colony where many are sick.
    pub fn fear(&self) -> f64 {
        (1.5 * self.outbreaks.iter().map(|o| o.i).sum::<f64>()).min(0.3)
    }

    /// People infectious now, of all pathogens.
    pub fn sick(&self) -> f64 {
        self.outbreaks.iter().map(|o| o.i).sum::<f64>() * self.pop
    }

    /// Expose `persons` people of the colony to pathogen `k` (travellers, the sector's outbreak,
    /// a neighbour colony). Returns true if this starts an outbreak.
    pub fn expose(&mut self, data: &GameData, k: usize, persons: f64, now: f64) -> bool {
        if self.pop < 1.0 || persons <= 0.0 {
            return false;
        }
        let share = (persons / self.pop).min(1.0);
        let idx = match self.outbreaks.iter().position(|o| o.pathogen == k) {
            Some(i) => i,
            None => {
                self.outbreaks.push(Outbreak::new(k));
                self.outbreaks.len() - 1
            }
        };
        let o = &mut self.outbreaks[idx];
        let was = o.active();
        o.infect(share);
        let started = !was && o.active();
        if started {
            let name = data.pathogens[k].name.clone();
            self.log_line(now, format!("Epidemie : {name} !"));
        }
        started
    }

    /// One tick of every epidemic of the colony: spontaneous outbreaks, the dynamics under the
    /// colony's measures, the vaccines and the medicine used. Returns the deaths.
    pub(crate) fn epidemics_tick(
        &mut self,
        data: &GameData,
        now: f64,
        days: f64,
        x: &EpiCtx,
        messages: &mut Vec<String>,
    ) -> f64 {
        if data.pathogens.is_empty() {
            return 0.0;
        }
        // Spontaneous outbreaks: crowding and bad care start them, medical centres hold them off.
        if self.pop > 20.0 {
            let day = (now / DAY) as i64;
            let q = self.quarantine_level(data);
            let p = EPIDEMIC_RATE
                * x.freq
                * days
                * (self.pop / 100.0).clamp(0.2, 6.0)
                * (1.0 - 0.7 * x.med)
                * (1.0 - 0.2 * q as f64).max(0.2);
            let key = hash_coord(self.system_seed, i64::from(self.id), day, 91);
            if unit(key) < p {
                let weights: Vec<f64> = data
                    .pathogens
                    .iter()
                    .enumerate()
                    .map(|(k, pa)| {
                        if self.outbreaks.iter().any(|o| o.pathogen == k && o.active()) {
                            0.0
                        } else {
                            pa.weight * trigger_factor(data, &pa.trigger, self, x)
                        }
                    })
                    .collect();
                let total: f64 = weights.iter().sum();
                if total > 0.0 {
                    let mut pick = unit(hash_coord(key, 1, 2, 3)) * total;
                    let mut chosen = 0;
                    for (k, w) in weights.iter().enumerate() {
                        if pick < *w {
                            chosen = k;
                            break;
                        }
                        pick -= w;
                    }
                    self.expose(data, chosen, 2.0, now);
                }
            }
        }
        if self.outbreaks.is_empty() {
            return 0.0;
        }
        // Measures.
        let q = self.quarantine_level(data);
        let mut contact = QUARANTINE_CONTACT[q]
            * (1.0 + x.crowd)
            * (1.0 + 0.15 * self.axis_value(data, "ouverture"));
        contact = contact.max(0.05);
        let faith_care = 1.0 - 0.25 * self.axis_value(data, "foi").max(0.0);
        let mut care = x.med * faith_care;
        let any_active = self.outbreaks.iter().any(Outbreak::active);
        let pop = self.pop;
        if any_active {
            if let Some(m) = data.resource_index("medicine") {
                let need = pop * MEDICINE_KG * days;
                if need > 0.0 && self.storage[m] >= need {
                    self.storage[m] -= need;
                    care += 0.3;
                }
            }
        }
        let care = care.clamp(0.0, 1.0);
        let vaccine = data.resource_index("vaccine");
        let mut dead_people = 0.0;
        let mut reports: Vec<String> = Vec::new();
        for oi in 0..self.outbreaks.len() {
            let k = self.outbreaks[oi].pathogen;
            let pa = &data.pathogens[k];
            let was_active = self.outbreaks[oi].active();
            // A vaccination campaign while the pathogen runs.
            let mut vaccinated = 0.0;
            if was_active && pa.vaccine_eff > 0.0 {
                if let Some(v) = vaccine {
                    let want = pop * CAMPAIGN_PER_DAY * days * VACCINE_KG;
                    let used = self.storage[v].min(want);
                    if used > 0.0 {
                        self.storage[v] -= used;
                        vaccinated = used / VACCINE_KG / pop.max(1.0);
                    }
                }
            }
            let m = Measures {
                contact,
                care,
                vaccinated,
            };
            let d = step(pa, &mut self.outbreaks[oi], m, days, pop);
            dead_people += d * pop;
            let o = &mut self.outbreaks[oi];
            if was_active && !o.active() {
                reports.push(format!("L'epidemie de {} est terminee.", pa.name));
            } else if o.i >= 0.1 && !o.grave {
                o.grave = true;
                reports.push(format!(
                    "{} : {:.0} malades, la colonie vacille.",
                    pa.name,
                    o.i * pop
                ));
            }
            if o.i < 0.02 {
                o.grave = false;
            }
        }
        self.outbreaks
            .retain(|o| o.active() || o.r > 1e-3 || o.s < 0.999);
        messages.extend(reports);
        dead_people
    }
}

// ---------------------------------------------------------------------------------------------
// The sectors of the galaxy: outbreaks that are a function of the seed, the sector and the date.
// ---------------------------------------------------------------------------------------------

/// An outbreak of a sector: it starts at `start` with a few cases and follows the model.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldOutbreak {
    pub pathogen: usize,
    pub cell: Cell,
    pub start: f64,
}

fn cell_key(c: Cell) -> i64 {
    c[0] * 7919 + c[1] * 104_729 + c[2] * 1_299_709
}

/// The outbreaks of sector `c` that started in the look-back before `t1` and not after it, i.e.
/// those that may be running at some time in [t0, t1].
pub fn world_outbreaks(
    data: &GameData,
    seed: u64,
    c: Cell,
    t0: f64,
    t1: f64,
) -> Vec<WorldOutbreak> {
    let mut out = Vec::new();
    let (w0, w1) = (
        (t0 / WORLD_PERIOD).floor() as i64 - WORLD_LOOKBACK,
        (t1 / WORLD_PERIOD).floor() as i64,
    );
    for (k, pa) in data.pathogens.iter().enumerate() {
        for w in w0..=w1 {
            let h = |n: i64| {
                hash_coord(
                    seed ^ 0xE91D ^ (k as u64 + 1).wrapping_mul(0x9E37_79B9),
                    w,
                    cell_key(c),
                    n,
                )
            };
            if unit(h(0)) >= P_WORLD * pa.weight {
                continue;
            }
            let start = w as f64 * WORLD_PERIOD + unit(h(1)) * WORLD_PERIOD;
            if start <= t1 {
                out.push(WorldOutbreak {
                    pathogen: k,
                    cell: c,
                    start,
                });
            }
        }
    }
    out
}

/// How freely people mix in this outbreak's sector: it decides whether it takes off or dies.
fn world_contact(seed: u64, ob: &WorldOutbreak) -> f64 {
    0.65 + 0.35
        * unit(hash_coord(
            seed ^ 0xC0DE,
            ob.pathogen as i64,
            cell_key(ob.cell),
            (ob.start / DAY) as i64,
        ))
}

/// Share of the sector infectious at `t` (0 before the start), and the time the outbreak was
/// declared (the infectious reached `DECLARE_AT`), if before `t`.
fn world_curve(data: &GameData, seed: u64, ob: &WorldOutbreak, t: f64) -> (f64, Option<f64>) {
    if t < ob.start {
        return (0.0, None);
    }
    let p = &data.pathogens[ob.pathogen];
    let m = Measures {
        contact: world_contact(seed, ob),
        care: WORLD_CARE,
        vaccinated: 0.0,
    };
    let mut o = Outbreak::new(ob.pathogen);
    o.infect(WORLD_SEED_SHARE);
    let end = t.min(ob.start + WORLD_HORIZON * DAY);
    let mut now = ob.start;
    let mut declared = None;
    // Whole days first: the declaration is dated on that grid, whatever `t` is asked.
    while now + DAY <= end {
        step(p, &mut o, m, 1.0, WORLD_POP);
        now += DAY;
        if declared.is_none() && o.i >= DECLARE_AT {
            declared = Some(now);
        }
    }
    if now < end {
        step(p, &mut o, m, (end - now) / DAY, WORLD_POP);
    }
    (o.i, declared)
}

/// The pathogens running around `pos` at time `t` with the share of the people infectious.
pub fn prevalence_at(data: &GameData, seed: u64, pos: [f64; 3], t: f64) -> Vec<(usize, f64)> {
    world_outbreaks(data, seed, cell_of(pos), t, t)
        .iter()
        .filter_map(|ob| {
            let i = world_curve(data, seed, ob, t).0;
            (i > 1e-6).then_some((ob.pathogen, i))
        })
        .collect()
}

/// Where in its sector an outbreak began (a point of space, to name the place).
pub fn world_place(seed: u64, ob: &WorldOutbreak) -> [f64; 3] {
    let c = crate::politics::cell_center(ob.cell);
    let j = |n: i64| {
        (unit(hash_coord(
            seed ^ 0x9F1A,
            ob.pathogen as i64 + n,
            cell_key(ob.cell),
            (ob.start / DAY) as i64,
        )) - 0.5)
            * crate::politics::SECTOR_LY
    };
    [c[0] + j(0), c[1] + j(100), c[2] + j(200)]
}

/// Outbreaks of sector `c` declared in (t0, t1]: (day of the declaration, pathogen, place).
pub fn declarations_in_cell(
    data: &GameData,
    seed: u64,
    c: Cell,
    t0: f64,
    t1: f64,
) -> Vec<(f64, usize, [f64; 3])> {
    world_outbreaks(data, seed, c, t0, t1)
        .iter()
        .filter_map(|ob| {
            let (_, declared) = world_curve(data, seed, ob, t1);
            declared
                .filter(|d| *d > t0 && *d <= t1)
                .map(|d| (d, ob.pathogen, world_place(seed, ob)))
        })
        .collect()
}

/// Price multiplier of every resource around `pos` at time `t` from the epidemics there: the
/// sick buy medicine and food and make less of what they build.
pub fn goods_factors(data: &GameData, seed: u64, pos: [f64; 3], t: f64) -> Vec<f64> {
    let mut f = vec![1.0; data.resources.len()];
    for (k, i) in prevalence_at(data, seed, pos, t) {
        let k = &data.pathogens[k];
        for (res, delta) in &k.prices {
            if let Some(r) = data.resource_index(res) {
                f[r] *= 1.0 + delta * (i / 0.1).min(1.0);
            }
        }
    }
    f
}

// ---------------------------------------------------------------------------------------------
// The ship and the colonies: who infects whom.
// ---------------------------------------------------------------------------------------------

fn roll(seed: u64, a: i64, b: i64, c: i64) -> f64 {
    unit(hash_coord(seed ^ 0xE91, a, b, c))
}

impl Sim {
    /// Whether crewmate `idx` can catch pathogen `k` now.
    fn crew_can_catch(&self, idx: usize, k: usize) -> bool {
        let p = &self.crew.members[idx];
        p.alive && p.disease.is_none() && !p.immunity.iter().any(|(i, _)| *i == k)
    }

    /// Infect crewmate `idx` with pathogen `k`: the symptoms are its disease.
    fn crew_catch(&mut self, idx: usize, k: usize) {
        let data = std::sync::Arc::clone(&self.data);
        let Some(d) = data.disease_index(&data.pathogens[k].disease) else {
            return;
        };
        let t = self.time;
        let p = &mut self.crew.members[idx];
        p.disease = Some((d, data.diseases[d].duration_days as f32));
        let text = format!("{} tombe malade : {}.", p.name, data.diseases[d].name);
        self.crew.log_event(t, text);
    }

    /// Expose every healthy crewmate to pathogen `k` with chance `p_each`.
    fn expose_crew(&mut self, k: usize, p_each: f64, day: i64) {
        for idx in 0..self.crew.members.len() {
            let id = i64::from(self.crew.members[idx].id);
            if self.crew_can_catch(idx, k) && roll(self.galaxy.seed, id, day, k as i64) < p_each {
                self.crew_catch(idx, k);
            }
        }
    }

    /// Once a day: the epidemics travel. The sector's outbreaks seep into the colonies in it,
    /// the colonies of a system trade their cases, the crew carries pathogens to the colony
    /// where the ship docks and catches those it finds there.
    pub(crate) fn epidemic_tick(&mut self) {
        let day = (self.time / DAY).floor() as i64;
        let last = self.epi_day;
        self.epi_day = day;
        if last == i64::MIN || day <= last {
            return;
        }
        let days = (day - last).clamp(1, 30) as f64;
        let data = std::sync::Arc::clone(&self.data);
        let seed = self.galaxy.seed;
        let now = self.time;
        // The ship's carriers, by pathogen.
        let mut carriers = vec![0usize; data.pathogens.len()];
        for p in self.crew.members.iter().filter(|p| p.alive) {
            if let Some(k) = p.disease.and_then(|(d, _)| data.pathogen_of_disease(d)) {
                carriers[k] += 1;
            }
        }
        // What each colony gets from outside: (colony, pathogen, persons).
        let mut inflow: Vec<(usize, usize, f64)> = Vec::new();
        let docked_colony = self.docked.and_then(|b| self.colony_at(b));
        for (ci, col) in self.colonies.iter().enumerate() {
            if col.pop < 1.0 {
                continue;
            }
            let block = QUARANTINE_BLOCK[col.quarantine_level(&data)];
            let pos = self.galaxy.star(col.star).map_or(self.here_pos, |e| e.pos);
            // The sector around.
            for (k, i) in prevalence_at(&data, seed, pos, now) {
                inflow.push((ci, k, TRAFFIC * col.pop * i * (1.0 - block) * days));
            }
            // Neighbour colonies of the same system.
            for (cj, other) in self.colonies.iter().enumerate() {
                if cj != ci && other.star == col.star {
                    for o in other.outbreaks.iter().filter(|o| o.i > 0.0) {
                        inflow.push((
                            ci,
                            o.pathogen,
                            5.0 * TRAFFIC * col.pop * o.i * (1.0 - block) * days,
                        ));
                    }
                }
            }
            // The ship that is docked here: its sick walk the streets.
            if docked_colony == Some(ci) {
                for (k, n) in carriers.iter().enumerate() {
                    if *n > 0 {
                        inflow.push((ci, k, 0.5 * *n as f64 * (1.0 - block) * days));
                    }
                }
            }
        }
        for (ci, k, expected) in inflow {
            let id = i64::from(self.colonies[ci].id);
            let whole = expected.floor();
            let extra = f64::from(roll(seed, id, day, k as i64 + 1000) < expected - whole);
            let persons = whole + extra;
            if persons > 0.0 {
                self.colonies[ci].expose(&data, k, persons, now);
            }
        }
        // The crew catches what runs where the ship docks.
        if let Some(b) = self.docked {
            let around: Vec<(usize, f64)> = match self.colony_at(b) {
                Some(ci) => self.colonies[ci]
                    .outbreaks
                    .iter()
                    .filter(|o| o.i > 0.0)
                    .map(|o| (o.pathogen, o.i))
                    .collect(),
                None => prevalence_at(&data, seed, self.here_pos, now),
            };
            for (k, i) in around {
                self.expose_crew(k, (CREW_EXPOSURE * i * days).min(1.0), day);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colony::tests::founded;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    fn flu(d: &GameData) -> Pathogen {
        d.pathogens[0].clone()
    }

    /// A ship with a small crew (the bare test sim has none).
    fn crewed(s: &mut Sim) {
        let d = Arc::clone(&s.data);
        for i in 0..3 {
            let p = crate::crew::generate_person(&d, 900 + i, None);
            s.crew.hire(&d, p, 1, s.time);
        }
        s.crew.last = s.time;
    }

    fn run(p: &Pathogen, m: Measures, days: usize) -> (Outbreak, f64) {
        let mut o = Outbreak::new(0);
        o.infect(0.001);
        let mut dead = 0.0;
        for _ in 0..days {
            dead += step(p, &mut o, m, 1.0, 10_000.0);
        }
        (o, dead)
    }

    #[test]
    fn people_are_conserved_and_the_curve_rises_then_falls() {
        let d = data();
        let mut p = flu(&d);
        p.immunity_days = 0.0;
        let mut o = Outbreak::new(0);
        o.infect(0.001);
        let mut peak = 0.0f64;
        for _ in 0..300 {
            step(&p, &mut o, Measures::NONE, 1.0, 10_000.0);
            peak = peak.max(o.i);
            let total = o.s + o.e + o.i + o.r;
            assert!((total - 1.0).abs() < 1e-9, "{total}");
        }
        assert!(peak > 0.05, "R0 {} takes off: peak {peak}", p.r0);
        assert!(o.i < peak * 0.1, "and it ebbs");
    }

    #[test]
    fn below_one_it_dies_out_above_one_it_takes_off() {
        let d = data();
        let mut p = flu(&d);
        p.r0 = 0.8;
        let (o, _) = run(&p, Measures::NONE, 120);
        assert!(!o.active() && o.r < 0.01, "R0 < 1 fades out: {o:?}");
        p.r0 = 2.5;
        let (o, _) = run(&p, Measures::NONE, 120);
        assert!(o.r > 0.5, "R0 > 1 sweeps the place: {o:?}");
    }

    #[test]
    fn quarantine_cuts_the_spread() {
        let d = data();
        let p = flu(&d);
        let attack = |contact: f64| {
            run(
                &p,
                Measures {
                    contact,
                    ..Measures::NONE
                },
                200,
            )
            .0
            .r
        };
        let (free, strict, total) = (attack(1.0), attack(0.5), attack(0.2));
        assert!(
            free > strict + 0.1 && strict > total,
            "{free} {strict} {total}"
        );
        assert!(total < 0.01, "contact below 1/R0 starves it: {total}");
    }

    #[test]
    fn care_spares_lives_and_vaccines_move_people_to_immunity() {
        let d = data();
        let mut p = flu(&d);
        p.fatality = 0.1;
        let (_, dead_none) = run(&p, Measures::NONE, 200);
        let (_, dead_care) = run(
            &p,
            Measures {
                care: 1.0,
                ..Measures::NONE
            },
            200,
        );
        assert!(dead_care < dead_none * 0.6, "{dead_care} vs {dead_none}");
        let mut o = Outbreak::new(0);
        o.infect(0.001);
        step(
            &p,
            &mut o,
            Measures {
                vaccinated: 0.5,
                ..Measures::NONE
            },
            1.0,
            10_000.0,
        );
        assert!((o.r - 0.5 * p.vaccine_eff).abs() < 0.02, "{}", o.r);
        // A pathogen no vaccine works against (the glass fever) is not moved by it.
        let alien = d
            .pathogens
            .iter()
            .find(|p| p.vaccine_eff == 0.0)
            .expect("one has no vaccine");
        let mut o = Outbreak::new(0);
        o.infect(0.001);
        step(
            alien,
            &mut o,
            Measures {
                vaccinated: 0.5,
                ..Measures::NONE
            },
            1.0,
            10_000.0,
        );
        assert!(o.r < 0.01, "{}", o.r);
    }

    #[test]
    fn immunity_wears_off_unless_it_is_for_life() {
        let d = data();
        let mut p = flu(&d);
        p.immunity_days = 40.0;
        let mut o = Outbreak::new(0);
        o.r = 0.8;
        o.s = 0.2;
        for _ in 0..200 {
            step(&p, &mut o, Measures::NONE, 1.0, 10_000.0);
        }
        assert!(o.r < 0.1, "waned: {}", o.r);
        p.immunity_days = 0.0;
        let mut o = Outbreak::new(0);
        o.r = 0.8;
        o.s = 0.2;
        for _ in 0..200 {
            step(&p, &mut o, Measures::NONE, 1.0, 10_000.0);
        }
        assert!((o.r - 0.8).abs() < 1e-9, "for life: {}", o.r);
    }

    #[test]
    fn a_colony_under_strict_quarantine_is_hit_less_and_the_sick_cost_production_and_lives() {
        let free_or_shut = |level: usize| {
            let (mut s, ci) = founded();
            let k = s
                .data
                .pathogen_of_disease(s.data.disease_index("grippe").unwrap());
            let k = k.expect("flu is a pathogen");
            let qp = s.data.policy_index("quarantine").unwrap();
            s.colonies[ci].policies[qp] = level;
            let d = Arc::clone(&s.data);
            s.colonies[ci].pop = 150.0;
            s.colonies[ci].expose(&d, k, 3.0, s.time);
            let mut lost = 0.0;
            for _ in 0..60 {
                let st = &mut s.colonies[ci];
                st.pop = 150.0;
                for (id, kg) in [
                    ("food", 5000.0),
                    ("water", 5000.0),
                    ("consumer_goods", 500.0),
                ] {
                    st.storage[d.resource_index(id).unwrap()] = kg;
                }
                s.time += DAY;
                s.update_colonies();
                lost += s.colonies[ci].labor_loss(&d);
            }
            let r = s.colonies[ci]
                .outbreaks
                .iter()
                .find(|o| o.pathogen == k)
                .map_or(0.0, |o| o.r);
            (r, lost)
        };
        let (open, lost_open) = free_or_shut(0);
        let (shut, lost_shut) = free_or_shut(3);
        assert!(open > 0.3, "the open colony is swept: {open}");
        assert!(shut < open * 0.5, "{shut} vs {open}");
        assert!(lost_open > lost_shut, "{lost_open} vs {lost_shut}");
        assert!(lost_open > 0.2, "the sick do not work");
    }

    #[test]
    fn vaccines_in_store_are_used_against_a_running_outbreak() {
        let (mut s, ci) = founded();
        let d = Arc::clone(&s.data);
        let k = d
            .pathogen_of_disease(d.disease_index("grippe").unwrap())
            .unwrap();
        let v = d.resource_index("vaccine").unwrap();
        s.colonies[ci].pop = 100.0;
        s.colonies[ci].storage[v] = 100.0;
        s.colonies[ci].expose(&d, k, 5.0, s.time);
        s.time += 2.0 * DAY;
        s.update_colonies();
        let o = s.colonies[ci]
            .outbreaks
            .iter()
            .find(|o| o.pathogen == k)
            .unwrap();
        assert!(s.colonies[ci].storage[v] < 100.0, "doses were used");
        assert!(o.r > 0.05, "and people are immune: {o:?}");
    }

    #[test]
    fn a_sick_crewmate_who_docks_infects_the_colony_and_the_colony_infects_the_crew() {
        let (mut s, ci) = founded();
        crewed(&mut s);
        let d = Arc::clone(&s.data);
        let flu_d = d.disease_index("fievre_bleue").unwrap();
        let k = d.pathogen_of_disease(flu_d).unwrap();
        s.colonies[ci].pop = 80.0;
        s.docked = Some(s.colonies[ci].body);
        assert!(s.colonies[ci].outbreaks.is_empty());
        s.crew.members[0].disease = Some((flu_d, 30.0));
        for _ in 0..12 {
            s.time += DAY;
            s.epidemic_tick();
        }
        let o = s.colonies[ci].outbreaks.iter().find(|o| o.pathogen == k);
        assert!(o.is_some_and(Outbreak::active), "docking seeded the colony");
        // Now the other way: a healthy crew, a colony in the grip of the disease.
        let (mut t, cj) = founded();
        crewed(&mut t);
        t.colonies[cj].pop = 100.0;
        t.docked = Some(t.colonies[cj].body);
        t.colonies[cj].outbreaks.push(Outbreak {
            pathogen: k,
            s: 0.6,
            e: 0.0,
            i: 0.4,
            r: 0.0,
            grave: true,
        });
        let sick = |t: &Sim| {
            t.crew
                .members
                .iter()
                .filter(|p| p.disease.is_some())
                .count()
        };
        assert_eq!(sick(&t), 0);
        for _ in 0..30 {
            t.time += DAY;
            t.epidemic_tick();
        }
        assert!(sick(&t) >= 1, "the crew caught it ashore");
        // The immune are spared.
        let (mut u, cu) = founded();
        crewed(&mut u);
        u.docked = Some(u.colonies[cu].body);
        u.colonies[cu].outbreaks.push(Outbreak {
            pathogen: k,
            s: 0.6,
            e: 0.0,
            i: 0.4,
            r: 0.0,
            grave: true,
        });
        for p in &mut u.crew.members {
            p.immunity.push((k, 1.0e9));
        }
        for _ in 0..30 {
            u.time += DAY;
            u.epidemic_tick();
        }
        assert_eq!(sick(&u), 0, "immune crew do not fall ill");
    }

    #[test]
    fn the_recovered_crew_are_immune_to_the_pathogen() {
        let (mut s, _) = founded();
        crewed(&mut s);
        let d = Arc::clone(&s.data);
        let dis = d.disease_index("grippe").unwrap();
        let k = d.pathogen_of_disease(dis).unwrap();
        s.crew.members[0].disease = Some((dis, 0.05));
        s.time += 2.0 * DAY;
        s.update_crew();
        let p = &s.crew.members[0];
        assert!(p.disease.is_none(), "healed");
        assert!(p.immunity.iter().any(|(i, _)| *i == k), "and immune");
    }

    #[test]
    fn sectors_have_outbreaks_that_are_local_deterministic_and_move_prices() {
        let d = data();
        let seed = 5;
        let find = || {
            for x in -3..4 {
                for y in -3..4 {
                    let c = [x, y, 0];
                    for ob in world_outbreaks(&d, seed, c, 0.0, 3000.0 * DAY) {
                        let (i, decl) = world_curve(&d, seed, &ob, ob.start + 90.0 * DAY);
                        if i > 0.05 && decl.is_some() {
                            return Some(ob);
                        }
                    }
                }
            }
            None
        };
        let ob = find().expect("an outbreak that takes off exists nearby");
        assert_eq!(Some(ob.clone()), find(), "deterministic");
        let pos = crate::politics::cell_center(ob.cell);
        // Peak: find the time of the maximum prevalence.
        let (peak_t, peak_i) = (0..300)
            .map(|k| {
                let t = ob.start + f64::from(k) * DAY;
                (t, world_curve(&d, seed, &ob, t).0)
            })
            .fold((0.0, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        assert!(peak_i > 0.05 && peak_i < 1.0);
        let f_peak = goods_factors(&d, seed, pos, peak_t);
        let med = d.resource_index("medicine").unwrap();
        assert!(f_peak[med] > 1.05, "the sick buy medicine: {}", f_peak[med]);
        // The exchange sees it: the world factor of the sector includes the epidemic.
        let wf = crate::bourse::world_factors(&d, seed, pos, peak_t);
        let without = {
            let mut v = wf.clone();
            for (a, b) in v.iter_mut().zip(&f_peak) {
                *a /= b;
            }
            v
        };
        assert!(wf[med] / without[med] > 1.05);
        // A far sector is not touched (effects are local): its factors are those of its own.
        let far = [pos[0] + 50_000.0, pos[1], pos[2]];
        let ff = goods_factors(&d, seed, far, peak_t);
        let own = prevalence_at(&d, seed, far, peak_t);
        assert!(
            own.iter().all(|(k, _)| *k != ob.pathogen) || ff[med] >= 1.0,
            "local only"
        );
        // Before it starts there is nothing, and the outbreak is declared once.
        assert!(prevalence_at(&d, seed, pos, ob.start - DAY)
            .iter()
            .all(|(k, _)| *k != ob.pathogen));
        let decl = declarations_in_cell(&d, seed, ob.cell, ob.start, ob.start + 300.0 * DAY);
        assert!(decl.iter().any(|(_, k, _)| *k == ob.pathogen));
    }

    #[test]
    fn the_sectors_outbreak_seeps_into_a_colony_unless_quarantine_shuts_the_border() {
        let (s, ci) = founded();
        let d = Arc::clone(&s.data);
        let seed = s.galaxy.seed;
        let pos = s
            .galaxy
            .star(s.colonies[ci].star)
            .map_or(s.here_pos, |e| e.pos);
        // A moment when the colony's sector has a big outbreak running.
        let mut at: Option<(f64, f64)> = None;
        for day in (0..9000).step_by(4) {
            let t = f64::from(day) * DAY;
            for (_, i) in prevalence_at(&d, seed, pos, t) {
                if at.is_none_or(|(_, best)| i > best) {
                    at = Some((t, i));
                }
            }
        }
        let (t0, peak) = at.expect("the sector has an outbreak at some time");
        assert!(peak > 0.05, "a real outbreak: {peak}");
        let qp = d.policy_index("quarantine").unwrap();
        let cases = |level: usize| {
            let (mut s, ci) = founded();
            s.colonies[ci].policies[qp] = level;
            s.colonies[ci].pop = 400.0;
            s.time = t0;
            s.epi_day = (t0 / DAY).floor() as i64 - 1;
            let mut n = 0;
            for _ in 0..40 {
                s.time += DAY;
                let before = s.colonies[ci]
                    .outbreaks
                    .iter()
                    .map(|o| o.e + o.i)
                    .sum::<f64>();
                s.epidemic_tick();
                let after = s.colonies[ci]
                    .outbreaks
                    .iter()
                    .map(|o| o.e + o.i)
                    .sum::<f64>();
                if after > before {
                    n += 1;
                }
            }
            n
        };
        let (open, shut) = (cases(0), cases(3));
        assert!(open >= 1, "the border is open: {open}");
        assert!(shut < open, "quarantine keeps cases out: {shut} vs {open}");
    }

    #[test]
    fn the_pathogens_are_varied() {
        let d = data();
        assert!(d.pathogens.len() >= 12);
        for o in ["natural", "lab", "alien"] {
            assert!(d.pathogens.iter().any(|p| p.origin == o), "{o}");
        }
        let r0 = d.pathogens.iter().map(|p| p.r0);
        assert!(r0.clone().fold(f64::MAX, f64::min) < 1.5 && r0.fold(0.0, f64::max) > 4.0);
        assert!(d.pathogens.iter().any(|p| p.immunity_days == 0.0));
        assert!(d.pathogens.iter().any(|p| p.vaccine_eff == 0.0));
    }

    #[test]
    fn the_start_setting_scales_spontaneous_outbreaks() {
        let count = |freq: f64| {
            let (mut s, ci) = founded();
            let d = Arc::clone(&s.data);
            s.colonies[ci].pop = 400.0;
            s.colonies[ci].outbreaks.clear();
            let mut msgs = Vec::new();
            let x = EpiCtx {
                med: 0.0,
                crowd: 0.3,
                food_sat: 1.0,
                water_sat: 1.0,
                freq,
            };
            let mut n = 0;
            for day in 0..40 {
                let before = s.colonies[ci].outbreaks.len();
                s.colonies[ci].epidemics_tick(&d, (day as f64 + 1.0) * DAY, 1.0, &x, &mut msgs);
                n += usize::from(s.colonies[ci].outbreaks.len() > before);
            }
            n
        };
        assert_eq!(count(0.0), 0, "no spontaneous outbreak at all");
        assert!(count(200.0) > count(1.0), "a crowded world falls ill more");
    }
}
