//! Ecology of a solid world (E2): which worlds have life, and a trophic chain simulated by
//! biomass per species and biome (producers grow logistically up to the biome's capacity,
//! consumers eat what lives in the same biome, saturating, and convert 12 % of it).
//! A world's balance is a pure function of its seed (it settles by itself); only what the player
//! changes (harvest, hunting, introductions) is stored, as the biomass table of colonised worlds.
use crate::planet::{unit, PlanetMap};
use crate::Sim;
use vector_core::hash_coord;
use vector_data::GameData;
use vector_world::surface::{Surface, H, W};
use vector_world::PlanetKind;

/// Fraction of eaten biomass that becomes consumer biomass.
const EFFICIENCY: f64 = 0.12;
/// Half-saturation of predation, as a fraction of the biome's capacity.
const HALF_SAT: f64 = 0.02;
/// Territory of a colony, km2 (5 km x 5 km around the city).
pub const REGION_KM2: f64 = 25.0;
/// Days run to let a freshly seeded world find its balance.
const SETTLE_DAYS: usize = 2500;
/// Biomass below this fraction of the biome capacity counts as extinct.
const EXTINCT: f64 = 1e-12;
/// Weight of other producers on the shared capacity (1 would be pure competition).
const COMPETITION: f64 = 0.5;
/// Fraction of the capacity that prey keep out of reach of predators (refuges).
const REFUGE: f64 = 0.002;

/// Whether a world carries life, by kind of world and seed.
pub fn has_life(kind: PlanetKind, seed: u64) -> bool {
    use PlanetKind::*;
    let p = match kind {
        Terran => 0.95,
        Ocean => 0.90,
        Tundra => 0.70,
        Desert => 0.40,
        Ice => 0.25,
        Venusian => 0.05,
        Lava => 0.06,
        // Airless rock: only radiotrophs (see data/xenobio.ron) can live there.
        Barren => 0.08,
        Dwarf => 0.03,
        GasGiant | IceGiant | HotJupiter => 0.0,
    };
    (hash_coord(seed, 0x11FE, 1, 1) >> 11) as f64 / (1u64 << 53) as f64 <= p
}

/// The living world: biomass in kg per (species, biome).
#[derive(Clone, Debug)]
pub struct Ecosystem {
    pub body: usize,
    pub life: bool,
    pub nb: usize,
    pub ns: usize,
    /// Surface of each biome, km2.
    pub area: Vec<f64>,
    /// Producer capacity of each biome, kg.
    pub cap: Vec<f64>,
    /// Biomass, kg; index `species * nb + biome`.
    pub bio: Vec<f64>,
    /// Game time up to which the food web has been simulated, s.
    pub time: f64,
    /// Species that lived here from the start (the others were introduced). Regions only.
    pub native: Vec<bool>,
    /// Whether a species can live in a biome (index `species * nb + biome`), and what it eats.
    live: Vec<bool>,
    diet: Vec<Vec<usize>>,
    producer: Vec<bool>,
    growth: Vec<f64>,
    /// Share of a species' biomass that crosses to a neighbouring biome per day (animals only).
    mobility: Vec<f64>,
    /// Pairs of neighbouring biomes: (a, b, border cells / cells of a, border cells / cells of b).
    border: Vec<(usize, usize, f64, f64)>,
}

/// Share of a species' biomass that moves per day toward a neighbouring biome, by invasiveness.
fn mobility_of(sp: &vector_data::Species) -> f64 {
    if sp.is_producer() {
        0.0
    } else {
        0.01 * (1.0 + 2.0 * sp.invasive)
    }
}

/// Carrying capacity factor of a world's air: thin air lets the ionising radiation through and
/// the producers of the open ground suffer (at most half of the capacity is lost).
fn air_factor(pressure: f64) -> f64 {
    1.0 - 0.5 * crate::biosphere::surface_radiation(pressure)
}

impl Ecosystem {
    pub fn at(&self, s: usize, b: usize) -> f64 {
        self.bio[s * self.nb + b]
    }

    /// Total biomass of a species on the whole world, kg.
    pub fn total(&self, s: usize) -> f64 {
        (0..self.nb).map(|b| self.at(s, b)).sum()
    }

    /// Species that still live in a biome, with their biomass, most abundant first.
    pub fn present_in(&self, b: usize) -> Vec<(usize, f64)> {
        let mut v: Vec<(usize, f64)> = (0..self.ns)
            .map(|s| (s, self.at(s, b)))
            .filter(|&(s, m)| self.live[s * self.nb + b] && m > EXTINCT * self.cap[b].max(1.0))
            .collect();
        v.sort_by(|a, c| c.1.total_cmp(&a.1));
        v
    }

    /// Total biomass of all living things, kg.
    pub fn living(&self) -> f64 {
        self.bio.iter().sum()
    }

    /// The world's surface and capacities with its species seeded but not yet settled.
    pub fn blank(data: &GameData, map: &PlanetMap) -> Ecosystem {
        let (nb, ns) = (data.biomes.len(), data.species.len());
        // Area of each biome in km2: cells weighted by cos(latitude) over the sphere.
        let radius_km = 6371.0 * (map.kind_radius_scale());
        let sphere = 4.0 * std::f64::consts::PI * radius_km * radius_km;
        let mut weight = vec![0.0f64; nb];
        let mut total = 0.0;
        for y in 0..H {
            let w = Surface::lat(y).cos();
            for x in 0..W {
                weight[map.biome_at(x, y)] += w;
                total += w;
            }
        }
        let area: Vec<f64> = weight.iter().map(|w| w / total * sphere).collect();
        let cap: Vec<f64> = (0..nb)
            .map(|b| {
                let air = if data.biomes[b].p.0 >= 0.01 {
                    air_factor(map.pressure)
                } else {
                    1.0
                };
                area[b] * (5.0e4 + 4.0e5 * data.biomes[b].fertility) * air
            })
            .collect();
        // Which biomes touch (the world wraps in longitude): where animals can walk to.
        let mut cells = vec![0.0f64; nb];
        let mut edges: std::collections::BTreeMap<(usize, usize), f64> = Default::default();
        for y in 0..H {
            for x in 0..W {
                let a = map.biome_at(x, y);
                cells[a] += 1.0;
                for (nx, ny) in [((x + 1) % W, y), (x, (y + 1).min(H - 1))] {
                    let b = map.biome_at(nx, ny);
                    if a != b {
                        *edges.entry((a.min(b), a.max(b))).or_default() += 1.0;
                    }
                }
            }
        }
        let border = edges
            .into_iter()
            .map(|((a, b), c)| (a, b, c / cells[a], c / cells[b]))
            .collect();
        let mut live = vec![false; nb * ns];
        for (s, sp) in data.species.iter().enumerate() {
            for b in 0..nb {
                live[s * nb + b] = weight[b] > 0.0 && sp.habitat.contains(&data.biomes[b].id);
            }
        }
        let mut eco = Ecosystem {
            body: map.body,
            life: has_life(map.kind, map.seed),
            nb,
            ns,
            area,
            cap,
            bio: vec![0.0; nb * ns],
            time: 0.0,
            native: Vec::new(),
            live,
            diet: data
                .species
                .iter()
                .map(|sp| {
                    sp.diet
                        .iter()
                        .filter_map(|d| data.species_index(d))
                        .collect()
                })
                .collect(),
            producer: data.species.iter().map(|s| s.is_producer()).collect(),
            growth: data.species.iter().map(|s| s.growth).collect(),
            mobility: data.species.iter().map(mobility_of).collect(),
            border,
        };
        if eco.life {
            for s in 0..ns {
                for b in 0..nb {
                    if eco.live[s * nb + b] {
                        eco.bio[s * nb + b] = eco.cap[b] * if eco.producer[s] { 0.2 } else { 0.01 };
                    }
                }
            }
        }
        eco
    }

    /// Build the world's ecosystem and let it settle. Deterministic from the map.
    pub fn new(data: &GameData, map: &PlanetMap) -> Ecosystem {
        let mut eco = Ecosystem::blank(data, map);
        for _ in 0..SETTLE_DAYS {
            eco.step(1.0);
        }
        eco
    }

    /// The food web of the territory around a colony: `REGION_KM2` of one biome, seeded and
    /// (when `settle`) brought to its balance. This is what a colony hunts, gathers from and
    /// catches invasive species in; it is the only ecology that is saved.
    pub fn region(data: &GameData, map: &PlanetMap, biome: usize, settle: bool) -> Ecosystem {
        let (ns, id) = (data.species.len(), &data.biomes[biome].id);
        let air = if data.biomes[biome].p.0 >= 0.01 {
            air_factor(map.pressure)
        } else {
            1.0
        };
        let cap = REGION_KM2 * (5.0e4 + 4.0e5 * data.biomes[biome].fertility) * air;
        let mut eco = Ecosystem {
            body: map.body,
            life: has_life(map.kind, map.seed),
            nb: 1,
            ns,
            area: vec![REGION_KM2],
            cap: vec![cap],
            bio: vec![0.0; ns],
            time: 0.0,
            native: Vec::new(),
            live: data
                .species
                .iter()
                .map(|sp| sp.habitat.contains(id))
                .collect(),
            diet: data
                .species
                .iter()
                .map(|sp| {
                    sp.diet
                        .iter()
                        .filter_map(|d| data.species_index(d))
                        .collect()
                })
                .collect(),
            producer: data.species.iter().map(|s| s.is_producer()).collect(),
            growth: data.species.iter().map(|s| s.growth).collect(),
            mobility: data.species.iter().map(mobility_of).collect(),
            border: Vec::new(),
        };
        if eco.life {
            for s in 0..ns {
                if eco.live[s] {
                    eco.bio[s] = cap * if eco.producer[s] { 0.2 } else { 0.01 };
                }
            }
            if settle {
                for _ in 0..SETTLE_DAYS {
                    eco.step(1.0);
                }
            }
        }
        eco.native = eco.bio.iter().map(|&m| m > 0.0).collect();
        eco
    }

    /// Advance the food web to game time `now` (s): whole days only, in steps of up to 5 days;
    /// a gap of more than 5000 days is capped (the web has long found its balance by then).
    pub fn advance_to(&mut self, now: f64) {
        let days = ((now - self.time) / 86_400.0).floor();
        if days < 1.0 {
            return;
        }
        let mut left = days.min(5000.0);
        while left > 0.0 {
            let dt = left.min(5.0);
            self.step(dt);
            left -= dt;
        }
        self.time += days * 86_400.0;
    }

    /// Advance the food web by `dt` days, in sub-steps of at most half a day (the rates are of the
    /// order of 1 per day, a longer step would make predators and prey oscillate wildly).
    pub fn step(&mut self, dt: f64) {
        let n = (dt / 0.5).ceil().max(1.0);
        for _ in 0..n as usize {
            self.substep(dt / n);
        }
    }

    fn substep(&mut self, dt: f64) {
        if !self.life {
            return;
        }
        let (nb, ns) = (self.nb, self.ns);
        let mut delta = vec![0.0f64; nb * ns];
        let mut eaten = vec![0.0f64; ns];
        for b in 0..nb {
            let k = self.cap[b];
            if k <= 0.0 {
                continue;
            }
            // Producers: logistic growth; other producers weigh half as much as the species
            // itself on the capacity, so several of them coexist.
            let producer_mass: f64 = (0..ns)
                .filter(|&s| self.producer[s] && self.live[s * nb + b])
                .map(|s| self.bio[s * nb + b])
                .sum();
            for s in (0..ns).filter(|&s| self.producer[s] && self.live[s * nb + b]) {
                let m = self.bio[s * nb + b];
                let load = (m + COMPETITION * (producer_mass - m)) / k;
                delta[s * nb + b] += self.growth[s] * m * (1.0 - load) * dt;
            }
            // Consumers: saturating predation on what exceeds the prey's refuge, 12 % conversion,
            // density-dependent mortality.
            let refuge = REFUGE * k;
            eaten.iter_mut().for_each(|e| *e = 0.0);
            for c in (0..ns).filter(|&c| !self.producer[c] && self.live[c * nb + b]) {
                let mc = self.bio[c * nb + b];
                if mc <= 0.0 {
                    continue;
                }
                let free = |d: usize| {
                    if self.live[d * nb + b] {
                        (self.bio[d * nb + b] - refuge).max(0.0)
                    } else {
                        0.0
                    }
                };
                let avail: f64 = self.diet[c].iter().map(|&d| free(d)).sum();
                let sat = avail / (avail + HALF_SAT * k);
                let g = 2.0 * self.growth[c] / EFFICIENCY;
                let intake = (g * mc * sat * dt).min(0.5 * avail);
                if avail > 0.0 {
                    for &d in &self.diet[c] {
                        eaten[d] += intake * free(d) / avail;
                    }
                }
                let crowd = 1.0 + mc / (0.3 * k);
                delta[c * nb + b] += EFFICIENCY * intake - self.growth[c] * mc * crowd * dt;
            }
            for s in 0..ns {
                delta[s * nb + b] -= eaten[s];
            }
        }
        // Migration: animals diffuse between neighbouring biomes where they can live (what is
        // lost on one side is gained on the other), which also recolonises a biome emptied
        // by hunting or famine from its neighbours, and spreads the invasive.
        for &(a, b, ra, rb) in &self.border {
            for s in (0..ns).filter(|&s| self.mobility[s] > 0.0) {
                if !(self.live[s * nb + a] && self.live[s * nb + b]) {
                    continue;
                }
                let flow =
                    self.mobility[s] * dt * (self.bio[s * nb + a] * ra - self.bio[s * nb + b] * rb);
                delta[s * nb + a] -= flow;
                delta[s * nb + b] += flow;
            }
        }
        for (i, d) in delta.iter().enumerate() {
            let b = i % nb;
            let v = self.bio[i] + d;
            self.bio[i] = if v < EXTINCT * self.cap[b] { 0.0 } else { v };
        }
    }

    /// What happened to the web between a past state of the biomass and now, as journal lines:
    /// a native species gone (extinction), one fallen to a hair of its capacity (famine, decline),
    /// an introduced one that came to hold a tenth of all the living mass (invasion).
    pub fn events_since(&self, before: &[f64], data: &GameData) -> Vec<String> {
        const CRITICAL: f64 = 1e-4;
        const INVADES: f64 = 0.10;
        let mut out = Vec::new();
        if before.len() != self.bio.len() || self.nb != 1 {
            return out;
        }
        let k = self.cap[0].max(1.0);
        let (old_total, total) = (before.iter().sum::<f64>(), self.living());
        for (s, &was) in before.iter().enumerate() {
            let now = self.bio[s];
            let name = &data.species[s].name;
            let native = self.native.get(s).copied().unwrap_or(true);
            if was > 0.0 && now == 0.0 && native {
                out.push(format!("Extinction : {name} a disparu du territoire"));
            } else if !self.producer[s] && was >= CRITICAL * k && now > 0.0 && now < CRITICAL * k {
                out.push(format!("Famine : {name} décline, au bord de l'extinction"));
            } else if !native
                && total > 0.0
                && now / total >= INVADES
                && (old_total <= 0.0 || was / old_total < INVADES)
            {
                out.push(format!("Espèce envahissante : {name} prolifère"));
            }
        }
        out
    }

    /// Take `kg` of biomass from a species in a biome (hunting, gathering); returns what was
    /// really taken (never more than 80 % of what lives there).
    pub fn harvest(&mut self, s: usize, b: usize, kg: f64) -> f64 {
        let have = self.bio[s * self.nb + b];
        let taken = kg.min(0.8 * have).max(0.0);
        self.bio[s * self.nb + b] = have - taken;
        taken
    }

    /// Put `kg` of a species back in a biome (animals released from the pens): added to what
    /// lives there, never above the capacity of the biome. False when the species cannot live
    /// in this biome or the world has no life.
    pub fn restock(&mut self, s: usize, b: usize, kg: f64) -> bool {
        if !self.life || !self.live[s * self.nb + b] {
            return false;
        }
        let at = s * self.nb + b;
        self.bio[at] = (self.bio[at] + kg.max(0.0)).min(self.cap[b]);
        true
    }

    /// Introduce a species in a biome with a small founding population (kg). Returns false if
    /// it cannot live there or is already established.
    pub fn introduce(&mut self, s: usize, b: usize, kg: f64) -> bool {
        if !self.life || !self.live[s * self.nb + b] || self.at(s, b) > 0.0 {
            return false;
        }
        self.bio[s * self.nb + b] = kg.max(EXTINCT * self.cap[b] * 10.0);
        true
    }
}

impl PlanetMap {
    /// Radius of the world relative to Earth, used for areas: moons are smaller. The map does not
    /// know its body's radius, so the world kind stands in for it.
    pub fn kind_radius_scale(&self) -> f64 {
        match self.kind {
            PlanetKind::Dwarf => 0.3,
            PlanetKind::Barren | PlanetKind::Ice => 0.6,
            PlanetKind::Ocean | PlanetKind::Terran => 1.0,
            _ => 0.9,
        }
    }
}

/// Share of a colony's people who gather and hunt at each level of the "gather" policy
/// (none, measured, intensive, over-exploitation).
pub const GATHER_SHARE: [f64; 4] = [0.0, 0.05, 0.20, 0.50];
/// Kilograms of biomass one gatherer brings in per day.
const LABOUR_KG: f64 = 40.0;
/// No species loses more than this share of its stock to gatherers in a day.
const MAX_TAKE: f64 = 0.05;
/// Most useful output per kg of biomass taken (a grain plant weighs less than its grain).
const MAX_YIELD_PER_KG: f64 = 0.6;

impl Sim {
    /// The food web of colony `ci`'s territory, created (and settled) the first time it is needed.
    pub(crate) fn colony_ecology(&mut self, ci: usize) -> Option<&mut Ecosystem> {
        let (map, city) = self.colony_world(ci)?;
        let id = self.colonies[ci].id;
        if !self.ecologies.contains_key(&id) {
            let b = map.biome_at(city.0, city.1);
            let mut eco = Ecosystem::region(&self.data, &map, b, true);
            eco.time = self.time;
            self.ecologies.insert(id, eco);
        }
        self.ecologies.get_mut(&id)
    }

    /// Let the colony's territory live up to `now`, then let the colony gather from it according
    /// to its policy. The labour (a share of the people, 40 kg each per day) is shared equally
    /// between the species that give something useful, so rare big game is hit hardest, and the
    /// yields go into the colony's stores while the biomass leaves the web.
    pub(crate) fn apply_ecology(&mut self, ci: usize, now: f64) {
        let data = std::sync::Arc::clone(&self.data);
        let level = data
            .policy_index("gather")
            .map_or(0, |p| self.colonies[ci].policies[p].min(3));
        let pop = self.colonies[ci].pop;
        let Some(eco) = self.colony_ecology(ci) else {
            return;
        };
        let before = eco.time;
        let bio_before = eco.bio.clone();
        eco.advance_to(now);
        let events = eco.events_since(&bio_before, &data);
        let days = ((eco.time - before) / 86_400.0).max(0.0);
        // Introduced species crowd out the natives and ruin crops: the more of the living
        // mass they make up, the more production the colony loses.
        let total = eco.living();
        let exotic: f64 = (0..eco.ns)
            .filter(|&s| !eco.native.get(s).copied().unwrap_or(true))
            .map(|s| eco.bio[s])
            .sum();
        let pest = if total > 0.0 {
            (2.0 * exotic / total).min(0.5)
        } else {
            0.0
        };
        let life = eco.life;
        self.colonies[ci].pest = pest;
        for e in events {
            self.colonies[ci].log_line(now, e);
        }
        if days < 1.0 || !life || level == 0 {
            return;
        }
        let Some(eco) = self.colony_ecology(ci) else {
            return;
        };
        // Herds and farms: domestic species regrow faster under the measured and intensive
        // levels, which is what makes gathering sustainable.
        if level <= 2 {
            let k = eco.cap[0];
            for (s, sp) in data.species.iter().enumerate() {
                let m = eco.bio[s];
                if sp.domestic && m > 0.0 {
                    eco.bio[s] = m + 0.5 * sp.growth * m * (1.0 - m / k).max(0.0) * days;
                }
            }
        }
        let useful: Vec<usize> = eco
            .present_in(0)
            .into_iter()
            .map(|(s, _)| s)
            .filter(|&s| !data.species[s].yields.is_empty())
            .collect();
        if useful.is_empty() {
            return;
        }
        let labour = pop * GATHER_SHARE[level] * LABOUR_KG * days;
        let each = labour / useful.len() as f64;
        let mut gains: Vec<(usize, f64)> = Vec::new();
        for s in useful {
            let stock = eco.bio[s];
            let taken = eco.harvest(s, 0, each.min(stock * (1.0 - (1.0 - MAX_TAKE).powf(days))));
            let sp = &data.species[s];
            for (res, per_individual) in &sp.yields {
                let per_kg = (per_individual / sp.body).min(MAX_YIELD_PER_KG);
                if let Some(r) = data.resource_index(res) {
                    gains.push((r, taken * per_kg));
                }
            }
        }
        let c = &mut self.colonies[ci];
        for (r, kg) in gains {
            let put = kg.min(c.free_space());
            c.storage[r] += put;
        }
    }
}

/// Chance that a ship carrying biological cargo contaminates the colony it docks at, before
/// the quarantine policy (none, checks, strict, total).
pub const CONTAMINATION: f64 = 0.25;
/// Share of that chance that gets through at each quarantine level.
pub const QUARANTINE_PASS: [f64; 4] = [1.0, 0.5, 0.15, 0.0];

impl Sim {
    /// The ship docks at colony `ci`: if it comes from another living colony carrying anything
    /// organic, a species of that world may slip into this one, unless the quarantine stops it.
    pub(crate) fn biosecurity_check(&mut self, ci: usize) {
        let dest = self.colonies[ci].id;
        let from = self.ship.biosphere;
        let carries = ["food", "organics", "colonists", "fertilizer"]
            .iter()
            .filter_map(|r| self.data.resource_index(r))
            .any(|r| self.ship.cargo.kg[r] > 0.0);
        if self.colony_ecology(ci).is_some_and(|e| e.life) {
            self.ship.biosphere = Some(dest);
        }
        let Some(src) = from.filter(|&s| s != dest && carries) else {
            return;
        };
        let Some(si) = self.colonies.iter().position(|c| c.id == src) else {
            return;
        };
        let level = self
            .data
            .policy_index("quarantine")
            .map_or(0, |p| self.colonies[ci].policies[p].min(3));
        let roll = unit(hash_coord(
            0x1BAD,
            i64::from(dest),
            i64::from(src),
            (self.time / 3600.0) as i64,
        ));
        if roll >= CONTAMINATION {
            return;
        }
        let now = self.time;
        if roll >= CONTAMINATION * QUARANTINE_PASS[level] {
            self.colonies[ci]
                .log_line(now, "Quarantaine : une cargaison contaminée a été détruite");
            return;
        }
        // Species of the source's territory that could live here and are not here yet,
        // the more invasive the more likely.
        let Some(source) = self.colony_ecology(si).map(|e| e.present_in(0)) else {
            return;
        };
        let data = std::sync::Arc::clone(&self.data);
        let Some(target) = self.colony_ecology(ci) else {
            return;
        };
        let candidates: Vec<(usize, f64)> = source
            .iter()
            .filter(|&&(s, _)| target.live[s] && target.bio[s] == 0.0)
            .map(|&(s, _)| (s, 0.05 + data.species[s].invasive))
            .collect();
        let total: f64 = candidates.iter().map(|c| c.1).sum();
        if total <= 0.0 {
            return;
        }
        let mut pick = unit(hash_coord(
            0x1BAE,
            i64::from(dest),
            i64::from(src),
            (now / 3600.0) as i64,
        )) * total;
        let mut chosen = candidates[0].0;
        for &(s, w) in &candidates {
            if pick < w {
                chosen = s;
                break;
            }
            pick -= w;
        }
        let founding = target.cap[0] * 1e-5;
        if target.introduce(chosen, 0, founding) {
            let from_name = self.colonies[si].name.clone();
            let sp = &data.species[chosen].name;
            self.colonies[ci].log_line(
                now,
                format!("Espèce introduite : {sp} (venue de {from_name})"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use vector_world::{generate_system, BodyKind};

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    /// A world of this kind that carries life.
    fn living_world(kind: PlanetKind) -> (Sim, PlanetMap, Ecosystem) {
        for seed in 1..6000u64 {
            let sys = generate_system(seed);
            for (i, b) in sys.bodies.iter().enumerate() {
                if matches!(b.kind, BodyKind::Planet(k) | BodyKind::Moon(k) if k == kind) {
                    let s = Sim::new(sys.clone(), data());
                    let map = s.planet_map(i).expect("map");
                    if has_life(kind, map.seed) {
                        let eco = Ecosystem::new(&s.data, &map);
                        return (s, map, eco);
                    }
                }
            }
        }
        panic!("no living {kind:?}");
    }

    #[test]
    fn dead_worlds_stay_dead_and_life_depends_on_the_kind_of_world() {
        let lives = |k| (0..400u64).filter(|&s| has_life(k, s)).count();
        // Airless worlds only carry the radiotrophs of the xenobiology, rarely.
        assert!(lives(PlanetKind::Barren) > 5 && lives(PlanetKind::Barren) < 70);
        assert!(lives(PlanetKind::Dwarf) < lives(PlanetKind::Barren));
        assert_eq!(lives(PlanetKind::GasGiant), 0);
        assert!(lives(PlanetKind::Terran) > 340);
        assert!(lives(PlanetKind::Desert) > 100 && lives(PlanetKind::Desert) < 220);
        assert!(lives(PlanetKind::Lava) < 40);
    }

    #[test]
    fn a_settled_world_is_finite_bounded_and_rich_in_species() {
        let (s, _, eco) = living_world(PlanetKind::Terran);
        assert!(eco.bio.iter().all(|v| v.is_finite() && *v >= 0.0));
        for b in 0..eco.nb {
            let producers: f64 = (0..eco.ns)
                .filter(|&sp| s.data.species[sp].is_producer())
                .map(|sp| eco.at(sp, b))
                .sum();
            assert!(producers <= eco.cap[b] * 2.05, "biome {b} over capacity");
        }
        let alive = (0..eco.ns).filter(|&sp| eco.total(sp) > 0.0).count();
        assert!(alive >= 20, "{alive} species alive");
        let animals = (0..eco.ns)
            .filter(|&sp| !s.data.species[sp].is_producer() && eco.total(sp) > 0.0)
            .count();
        assert!(animals >= 8, "{animals} animals alive");
        // Plants outweigh herbivores, which outweigh carnivores (the trophic pyramid).
        let mass = |t: &str| -> f64 {
            (0..eco.ns)
                .filter(|&sp| s.data.species[sp].trophic == t)
                .map(|sp| eco.total(sp))
                .sum()
        };
        assert!(
            mass("producer") > mass("herbivore"),
            "{} {}",
            mass("producer"),
            mass("herbivore")
        );
        assert!(
            mass("herbivore") > mass("carnivore"),
            "{} {}",
            mass("herbivore"),
            mass("carnivore")
        );
    }

    #[test]
    fn the_balance_is_deterministic_and_holds_for_decades() {
        let (s, map, eco) = living_world(PlanetKind::Terran);
        let again = Ecosystem::new(&s.data, &map);
        assert_eq!(eco.bio, again.bio);
        let mut run = eco.clone();
        for _ in 0..(365 * 10 / 5) {
            run.step(5.0);
        }
        assert!(run.bio.iter().all(|v| v.is_finite() && *v >= 0.0));
        let (before, after) = (eco.living(), run.living());
        assert!(
            after > 0.5 * before && after < 2.0 * before,
            "{before} -> {after}"
        );
    }

    #[test]
    fn over_hunting_a_herbivore_starves_its_predators_and_lets_plants_grow() {
        let (s, _, eco) = living_world(PlanetKind::Terran);
        let sp = |id: &str| s.data.species_index(id).expect("species");
        // Pick a biome where a predator and a herbivore it eats both live.
        let mut found = None;
        'outer: for c in (0..eco.ns).filter(|&c| s.data.species[c].trophic == "carnivore") {
            for d in &s.data.species[c].diet {
                let di = sp(d);
                if s.data.species[di].trophic != "herbivore" {
                    continue;
                }
                for b in 0..eco.nb {
                    if eco.at(c, b) > 0.0 && eco.at(di, b) > 0.0 {
                        found = Some((c, di, b));
                        break 'outer;
                    }
                }
            }
        }
        let (pred, prey, b) = found.expect("a predator/prey pair on this world");
        let mut hunted = eco.clone();
        let mut ctrl = eco.clone();
        for day in 0..720 {
            // Hunters kill nearly all of the prey every week.
            if day % 7 == 0 {
                let m = hunted.at(prey, b);
                hunted.harvest(prey, b, m);
            }
            hunted.step(1.0);
            ctrl.step(1.0);
        }
        assert!(hunted.at(prey, b) < 0.2 * ctrl.at(prey, b).max(1e-9) || hunted.at(prey, b) == 0.0);
        assert!(
            hunted.at(pred, b) < ctrl.at(pred, b),
            "predators suffer: {} vs {}",
            hunted.at(pred, b),
            ctrl.at(pred, b)
        );
    }

    #[test]
    fn harvest_is_capped_and_introductions_respect_habitat() {
        let (s, _, mut eco) = living_world(PlanetKind::Terran);
        let b = (0..eco.nb)
            .find(|&b| eco.cap[b] > 0.0 && !eco.present_in(b).is_empty())
            .expect("a living biome");
        let (sp, mass) = eco.present_in(b)[0];
        let taken = eco.harvest(sp, b, mass * 10.0);
        assert!((taken - 0.8 * mass).abs() < 1e-6 * mass.max(1.0));
        // A deep-sea species cannot be introduced on dry land.
        let sea = s.data.species_index("krill").expect("krill");
        let land = s.data.biome_index("foret_temperee").expect("forest");
        assert!(!eco.introduce(sea, land, 1.0));
    }

    #[test]
    #[ignore]
    fn dump_a_settled_world() {
        let (s, _, eco) = living_world(PlanetKind::Terran);
        for sp in 0..eco.ns {
            let tot = eco.total(sp);
            println!(
                "{:<22} {:<10} {:>14.1} kg",
                s.data.species[sp].id, s.data.species[sp].trophic, tot
            );
        }
        for b in 0..eco.nb {
            println!(
                "biome {:<18} area {:>12.0} km2 cap {:>14.0}",
                s.data.biomes[b].id, eco.area[b], eco.cap[b]
            );
        }
    }

    /// A colony on a living Terran world (kit in the hold, ship parked next to it).
    fn colony_on_living_world() -> (Sim, usize, usize) {
        let (mut s, map, _) = living_world(PlanetKind::Terran);
        let data = Arc::clone(&s.data);
        s.reveal_all();
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
        let b = &s.bodies[map.body];
        s.ship.pos = b.pos - vector_core::Vec3::new(0.0, 0.0, b.radius + 5000.0);
        s.ship.vel = b.vel;
        s.update_ref_pub();
        let ci = s.found_colony(map.body, None).expect("found");
        s.colonies[ci].pop = 100.0;
        (s, ci, map.body)
    }

    fn live_for(s: &mut Sim, ci: usize, days: u32) -> f64 {
        let start = s.time;
        let food = s.data.resource_index("food").expect("food");
        let before = s.colonies[ci].storage[food];
        for d in 1..=days {
            s.apply_ecology(ci, start + f64::from(d) * 86_400.0);
        }
        s.colonies[ci].storage[food] - before
    }

    /// Biomass of the animals in a colony's territory.
    fn animals(s: &Sim, ci: usize) -> f64 {
        let eco = &s.ecologies[&s.colonies[ci].id];
        (0..eco.ns)
            .filter(|&sp| !s.data.species[sp].is_producer())
            .map(|sp| eco.at(sp, 0))
            .sum()
    }

    #[test]
    fn gathering_feeds_the_colony_and_over_exploitation_empties_the_land() {
        let set = |s: &mut Sim, ci: usize, level: usize| {
            let p = s.data.policy_index("gather").expect("policy");
            s.colonies[ci].policies[p] = level;
        };
        let (mut none, c0, _) = colony_on_living_world();
        let (mut mild, c1, _) = colony_on_living_world();
        let (mut hard, c2, _) = colony_on_living_world();
        set(&mut none, c0, 0);
        set(&mut mild, c1, 1);
        set(&mut hard, c2, 3);
        // The first weeks, then the long run.
        let e0 = live_for(&mut none, c0, 20);
        let e1 = live_for(&mut mild, c1, 20);
        let e2 = live_for(&mut hard, c2, 20);
        let l0 = live_for(&mut none, c0, 380);
        let l1 = live_for(&mut mild, c1, 380);
        let l2 = live_for(&mut hard, c2, 380);
        assert_eq!(e0 + l0, 0.0, "no policy, no gathering");
        assert!(e1 > 0.0, "measured gathering brings in food: {e1}");
        assert!(
            e2 > e1,
            "more gatherers bring in more at first: {e2} vs {e1}"
        );
        assert!(l2 < l1, "but the game is gone later: {l2} vs {l1}");
        let (a0, a1, a2) = (animals(&none, c0), animals(&mild, c1), animals(&hard, c2));
        assert!(
            a2 < a0 * 0.75,
            "over-exploitation depletes the game: {a2} vs {a0}"
        );
        assert!(a1 > a2, "measured gathering leaves more game: {a1} vs {a2}");
    }

    #[test]
    fn colonised_worlds_keep_their_food_web_through_a_save() {
        let (mut s, ci, _) = colony_on_living_world();
        let p = s.data.policy_index("gather").expect("policy");
        s.colonies[ci].policies[p] = 2;
        live_for(&mut s, ci, 60);
        let id = s.colonies[ci].id;
        let before = s.ecologies[&id].clone();
        let bytes = s.save_bytes().expect("save");
        let loaded = Sim::load_bytes(&bytes, Arc::clone(&s.data)).expect("load");
        let after = &loaded.ecologies[&id];
        assert_eq!(after.bio.len(), before.bio.len());
        assert!((after.time - before.time).abs() < 1e-6);
        for (a, b) in after.bio.iter().zip(&before.bio) {
            assert!((a - b).abs() <= 1e-6 * b.abs().max(1.0), "{a} vs {b}");
        }
    }

    /// Two colonies on the same world, the ship coming from the first with food in the hold.
    fn two_colonies() -> (Sim, usize, usize) {
        let (mut s, a, _) = colony_on_living_world();
        let mut other = s.colonies[a].clone();
        other.id = s.next_colony;
        s.next_colony += 1;
        other.name = "Seconde".into();
        s.colonies.push(other);
        let b = s.colonies.len() - 1;
        s.colony_ecology(a).expect("eco a");
        s.colony_ecology(b).expect("eco b");
        let food = s.data.resource_index("food").expect("food");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, food, 50.0);
        s.ship.biosphere = Some(s.colonies[a].id);
        (s, a, b)
    }

    /// Dock-time checks every hour for a while; returns how many species ended up introduced.
    fn contaminate(s: &mut Sim, dest: usize, tries: u32) -> usize {
        let before = s.colonies[dest].log.len();
        let src = s.ship.biosphere;
        for _ in 0..tries {
            s.time += 3600.0;
            s.ship.biosphere = src;
            s.biosecurity_check(dest);
        }
        s.colonies[dest].log[before..]
            .iter()
            .filter(|(_, l)| l.starts_with("Espèce introduite"))
            .count()
    }

    #[test]
    fn released_animals_join_the_local_population_up_to_the_capacity() {
        // Audit B16: a release is an addition to the food web of the territory, a capture a
        // subtraction (`harvest`); both read the same biomass table.
        let (mut s, _, b) = two_colonies();
        let key = s.colonies[b].id;
        let eco = s.ecologies.get_mut(&key).expect("eco");
        let sp = (0..eco.ns)
            .find(|&i| eco.live[i] && !eco.producer[i])
            .expect("an animal that lives here");
        eco.bio[sp] = 10.0;
        assert!(eco.restock(sp, 0, 5.0));
        assert!((eco.at(sp, 0) - 15.0).abs() < 1e-9);
        let taken = eco.harvest(sp, 0, 5.0);
        assert!((taken - 5.0).abs() < 1e-9 && (eco.at(sp, 0) - 10.0).abs() < 1e-9);
        assert!(eco.restock(sp, 0, 1.0e30));
        assert!(eco.at(sp, 0) <= eco.cap[0] + 1e-6, "never above the capacity");
        let dead = (0..eco.ns).find(|&i| !eco.live[i]).expect("a species that cannot live here");
        assert!(!eco.restock(dead, 0, 5.0), "it cannot live in this biome");
        assert_eq!(eco.at(dead, 0), 0.0);
    }

    #[test]
    fn a_ship_from_another_colony_can_bring_an_invasive_species_unless_quarantined() {
        let (mut s, _, b) = two_colonies();
        // Make the destination miss every species the source has, so any can be introduced.
        let key = s.colonies[b].id;
        for sp in 0..s.ecologies[&key].ns {
            let eco = s.ecologies.get_mut(&key).expect("eco");
            eco.bio[sp] = 0.0;
        }
        let q = s.data.policy_index("quarantine").expect("policy");
        s.colonies[b].policies[q] = 0;
        let open = contaminate(&mut s, b, 400);
        assert!(
            open >= 1,
            "with no quarantine something gets through in 400 docks"
        );
        let present = s.ecologies[&key].present_in(0).len();
        assert!(present >= 1, "the introduced species lives on");
        // Total quarantine: nothing, whatever the number of docks.
        for sp in 0..s.ecologies[&key].ns {
            s.ecologies.get_mut(&key).expect("eco").bio[sp] = 0.0;
        }
        s.colonies[b].policies[q] = 3;
        let sealed = contaminate(&mut s, b, 400);
        assert_eq!(sealed, 0, "total quarantine stops everything");
    }

    #[test]
    fn invasive_species_cost_the_colony_production_and_empty_holds_are_safe() {
        let (mut s, _, b) = two_colonies();
        // No organic cargo, no contamination.
        let food = s.data.resource_index("food").expect("food");
        s.ship.cargo.kg[food] = 0.0;
        let key = s.colonies[b].id;
        let natives = s.ecologies[&key].present_in(0).len();
        s.ecologies
            .get_mut(&key)
            .expect("eco")
            .bio
            .iter_mut()
            .for_each(|m| *m = 0.0);
        let clean = contaminate(&mut s, b, 200);
        assert_eq!(clean, 0);
        assert!(natives >= 5);
        // Exotic species make up a share of the living mass: the colony loses production.
        let eco = s.ecologies.get_mut(&key).expect("eco");
        let (producer, other) = (0..eco.ns)
            .filter(|&sp| eco.live[sp])
            .partition::<Vec<_>, _>(|&sp| eco.producer[sp]);
        eco.native = vec![true; eco.ns];
        eco.bio[producer[0]] = eco.cap[0] * 0.2;
        eco.native[other[0]] = false;
        eco.bio[other[0]] = eco.cap[0] * 0.2;
        let t = s.time + 2.0 * 86_400.0;
        s.apply_ecology(b, t);
        assert!(s.colonies[b].pest > 0.2, "pest {}", s.colonies[b].pest);
        s.ecologies.get_mut(&key).expect("eco").bio[other[0]] = 0.0;
        s.apply_ecology(b, t + 3.0 * 86_400.0);
        assert!(s.colonies[b].pest < 0.05, "pest {}", s.colonies[b].pest);
    }

    #[test]
    fn a_colony_lives_for_decades_with_gathering_without_blowing_up() {
        let (mut s, ci, _) = colony_on_living_world();
        let p = s.data.policy_index("gather").expect("policy");
        s.colonies[ci].policies[p] = 2;
        let t = std::time::Instant::now();
        for _ in 0..(20 * 365 * 4) {
            s.time += 6.0 * 3600.0;
            s.update_colonies();
        }
        assert!(
            t.elapsed().as_secs_f64() < 20.0,
            "20 years took {:?}",
            t.elapsed()
        );
        let eco = &s.ecologies[&s.colonies[ci].id];
        assert!(eco.bio.iter().all(|v| v.is_finite() && *v >= 0.0));
        assert!(s.colonies[ci].pop.is_finite() && s.colonies[ci].pest.is_finite());
        assert!(eco.living() > 0.0, "life goes on");
    }

    #[test]
    fn airless_worlds_hold_only_radiotrophs_and_hostile_worlds_other_chemistries() {
        let (s, _, eco) = living_world(PlanetKind::Barren);
        let alive: Vec<usize> = (0..eco.ns).filter(|&sp| eco.total(sp) > 0.0).collect();
        assert!(alive.len() >= 3, "a food web of {} species", alive.len());
        for &sp in &alive {
            let c = s
                .data
                .chemistry_of(sp)
                .expect("only xenobiology lives here");
            assert_eq!(s.data.chemistries[c].id, "radiotrophes");
        }
        // The chain is whole: a producer, a grazer and a hunter.
        for t in ["producer", "herbivore", "carnivore"] {
            assert!(
                alive.iter().any(|&sp| s.data.species[sp].trophic == t),
                "{t}"
            );
        }
        // Lava worlds hold silicon life, when they hold any.
        let (s, _, lava) = living_world(PlanetKind::Lava);
        let xeno = (0..lava.ns)
            .filter(|&sp| lava.total(sp) > 0.0)
            .filter_map(|sp| s.data.chemistry_of(sp))
            .any(|c| s.data.chemistries[c].id == "silicones");
        assert!(xeno, "silicones on a lava world");
    }

    #[test]
    fn animals_migrate_between_neighbouring_biomes_and_recolonise_an_emptied_one() {
        let (s, _, eco) = living_world(PlanetKind::Terran);
        assert!(!eco.border.is_empty(), "biomes touch");
        // An animal that lives, and is established, on both sides of a border.
        let mut found = None;
        'o: for &(a, b, _, _) in &eco.border {
            for sp in 0..eco.ns {
                if eco.mobility[sp] > 0.0 && eco.at(sp, a) > 0.0 && eco.at(sp, b) > 0.0 {
                    found = Some((sp, a, b));
                    break 'o;
                }
            }
        }
        let (sp, a, _) = found.expect("an animal on both sides of a border");
        let mut with = eco.clone();
        let mut without = eco.clone();
        without.border.clear();
        with.bio[sp * with.nb + a] = 0.0;
        without.bio[sp * without.nb + a] = 0.0;
        for _ in 0..40 {
            with.step(1.0);
            without.step(1.0);
        }
        // Without migration the emptied biome stays empty; with it the neighbours refill it.
        assert_eq!(without.at(sp, a), 0.0, "{}", s.data.species[sp].id);
        assert!(with.at(sp, a) > 0.0, "{}", s.data.species[sp].id);
        // The move itself creates no biomass: with no growth and no predation the total of
        // the species is unchanged by a step.
        let mut m = eco.clone();
        let total = |e: &Ecosystem| e.bio[sp * e.nb..(sp + 1) * e.nb].iter().sum::<f64>();
        m.growth.iter_mut().for_each(|g| *g = 0.0);
        m.diet.iter_mut().for_each(|d| d.clear());
        let before = total(&m);
        m.step(1.0);
        assert!(
            (total(&m) - before).abs() < 1e-6 * before,
            "{} {}",
            total(&m),
            before
        );
    }

    #[test]
    fn thin_air_costs_capacity_and_the_journal_reports_extinctions_and_invasions() {
        assert!(air_factor(0.05) < air_factor(0.5) && air_factor(0.5) < air_factor(1.0));
        assert!(air_factor(1.0) > 0.99 && air_factor(0.0) == 0.5);
        let (mut s, ci, _) = colony_on_living_world();
        let data = Arc::clone(&s.data);
        let id = s.colonies[ci].id;
        s.colony_ecology(ci);
        let eco = s.ecologies.get_mut(&id).expect("eco");
        let before = eco.bio.clone();
        let native = (0..eco.ns)
            .find(|&sp| eco.native[sp] && eco.bio[sp] > 0.0)
            .expect("a native species");
        assert!(
            eco.events_since(&before, &data).is_empty(),
            "nothing happened"
        );
        eco.bio[native] = 0.0;
        let lines = eco.events_since(&before, &data);
        assert!(
            lines
                .iter()
                .any(|l| l.starts_with("Extinction") && l.contains(&data.species[native].name)),
            "{lines:?}"
        );
        // A predator reduced to a hair of the capacity is a famine.
        let pred = (0..eco.ns)
            .find(|&sp| !eco.producer[sp] && eco.bio[sp] > 1e-3 * eco.cap[0])
            .expect("a consumer");
        let before = eco.bio.clone();
        eco.bio[pred] = 1e-6 * eco.cap[0];
        assert!(eco
            .events_since(&before, &data)
            .iter()
            .any(|l| l.starts_with("Famine")));
        // An introduced species taking a fifth of the living mass is an invasion.
        let exotic = (0..eco.ns)
            .find(|&sp| eco.live[sp] && eco.bio[sp] == 0.0 && eco.producer[sp])
            .expect("a species that could be introduced");
        eco.native[exotic] = false;
        let before = eco.bio.clone();
        eco.bio[exotic] = 0.25 * eco.living();
        assert!(eco
            .events_since(&before, &data)
            .iter()
            .any(|l| l.starts_with("Espèce envahissante")));
    }
}
