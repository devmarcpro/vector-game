//! Biology of organisms and of the living planet (E2c). Everything here is a pure function of the
//! data, the world's seed and the game time (nothing is stored, nothing reads a real clock):
//!
//! * the organism: basal metabolism (Kleiber, mass^0.75), life cycle (lifespan ~ mass^0.25,
//!   maturity, litter, sexual or clonal reproduction) and behaviour (herd, pack, territory,
//!   nocturnal, hibernating) derived from the species' trophic level, mass and habitats;
//! * evolution: an isolated population (one species in one biome of one world) drifts and adapts
//!   over generations toward what its environment selects (cold -> big body, pale or dark coat
//!   against the ground, hardening against radiation) and, past a threshold of divergence,
//!   counts as a new variant with its own codex card;
//! * the oxygen of a living world: photosynthesis turns the CO2 of the air into O2 (1 mol for
//!   1 mol, the carbon is buried), the more the older the life of the world.
//!
//! The food web itself (who eats whom, how many) lives in `ecology.rs`; this module only reads it.
use crate::ecology::{has_life, Ecosystem};
use crate::planet::{unit, PlanetMap};
use crate::Sim;
use vector_core::hash_coord;
use vector_data::{GameData, Species};
use vector_world::PlanetKind;

// ---------------------------------------------------------------- the organism

/// Biology of one species, derived from its mass and growth.
#[derive(Clone, Debug, PartialEq)]
pub struct Life {
    /// Basal metabolic rate, W (Kleiber: 3.4 x mass^0.75, mass in kg).
    pub watts: f64,
    /// Typical lifespan, days (about 1900 x mass^0.25).
    pub lifespan_days: f64,
    /// Age of maturity, days (a seventh of the lifespan).
    pub maturity_days: f64,
    /// Offspring per birth (1 for big animals, many for small ones).
    pub litter: u32,
    /// Sexual reproduction (two parents) or clonal (division, spores).
    pub sexual: bool,
}

/// Mass below which an organism divides instead of mating (microbes, plankton, spores).
const CLONAL_BELOW_KG: f64 = 0.01;

pub fn life_of(sp: &Species) -> Life {
    let m = sp.body.max(1e-9);
    let lifespan = 1900.0 * m.powf(0.25);
    Life {
        watts: 3.4 * m.powf(0.75),
        lifespan_days: lifespan,
        maturity_days: lifespan / 7.0,
        litter: ((8.0 / m.powf(0.3)).round() as u32).clamp(1, 12),
        sexual: if sp.trophic == "producer" {
            m >= 5.0
        } else {
            m >= CLONAL_BELOW_KG
        },
    }
}

/// Days between two generations (maturity plus half the breeding life).
pub fn generation_days(sp: &Species) -> f64 {
    let l = life_of(sp);
    l.maturity_days + 0.25 * l.lifespan_days
}

/// How a species lives, derived from the data (no table to write by hand).
#[derive(Clone, Debug, PartialEq)]
pub struct Behavior {
    /// Individuals that stay together (1: solitary).
    pub herd: u32,
    /// A pack of carnivores hunting together (else a herd of grazers or a swarm).
    pub pack: bool,
    /// Home range of a carnivore or omnivore, km2.
    pub territory_km2: f64,
    /// Active at night, asleep by day.
    pub nocturnal: bool,
    /// Sleeps through the cold season.
    pub hibernates: bool,
}

/// Biomes of open water: aquatic species do not herd on land, nor hibernate.
fn aquatic(data: &GameData, sp: &Species) -> bool {
    sp.habitat.iter().all(|b| {
        matches!(
            b.as_str(),
            "ocean" | "ocean_profond" | "mer_gelee" | "mer_bouillante"
        )
    }) && data.biome_index(&sp.habitat[0]).is_some()
}

/// Mean temperature of the places a species lives in, K (open-ended ranges are capped).
pub fn home_temperature(data: &GameData, sp: &Species) -> f64 {
    let mids: Vec<f64> = sp
        .habitat
        .iter()
        .filter_map(|b| data.biome_index(b))
        .map(|b| biome_mid_temp(data, b))
        .collect();
    mids.iter().sum::<f64>() / mids.len().max(1) as f64
}

fn biome_mid_temp(data: &GameData, b: usize) -> f64 {
    let t = data.biomes[b].t;
    0.5 * (t.0 + t.1.min(t.0 + 120.0))
}

fn id_hash(id: &str) -> u64 {
    id.bytes().fold(0xCBF2_9CE4_8422_2325u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x100_0000_01B3)
    })
}

/// Below this temperature (K) a hibernating species is asleep.
pub const HIBERNATION_K: f64 = 268.0;

pub fn behavior_of(data: &GameData, sp: &Species) -> Behavior {
    let h = id_hash(&sp.id);
    let r = |salt: u64| unit(hash_coord(h, salt as i64, 0xBE_4A, 1));
    let none = Behavior {
        herd: 1,
        pack: false,
        territory_km2: 0.0,
        nocturnal: false,
        hibernates: false,
    };
    if sp.is_producer() {
        return none;
    }
    let water = aquatic(data, sp);
    let m = sp.body;
    let mut b = none;
    let grazer = sp.trophic == "herbivore";
    if grazer {
        // Swarms of small things, herds of middle-sized grazers, small bands of giants.
        b.herd = if m < 0.01 {
            50
        } else if m < 5.0 {
            6 + (r(1) * 14.0) as u32
        } else if m < 600.0 {
            8 + (r(2) * 30.0) as u32
        } else {
            3 + (r(3) * 5.0) as u32
        };
    } else {
        // Carnivores and omnivores hold a territory (home range grows about with the mass).
        b.territory_km2 = (2.0 * m.powf(0.9)).clamp(0.01, 5000.0);
        if sp.trophic == "carnivore" && (15.0..=120.0).contains(&m) && r(4) < 0.67 {
            b.pack = true;
            b.herd = 3 + (r(5) * 6.0) as u32;
        }
    }
    let heat = sp
        .habitat
        .iter()
        .any(|x| matches!(x.as_str(), "desert" | "savane" | "plaine_brulante"));
    let night_p = (if grazer { 0.25 } else { 0.5 }) + if heat { 0.35 } else { 0.0 };
    b.nocturnal = !water && m >= 0.01 && r(6) < night_p;
    b.hibernates = !water && m >= 0.5 && home_temperature(data, sp) <= 285.0;
    b
}

impl Behavior {
    /// Whether the species is asleep at this temperature (K) and time of day.
    pub fn asleep(&self, temp: f64, night: bool) -> bool {
        (self.hibernates && temp < HIBERNATION_K) || (self.nocturnal && !night)
    }

    /// Individuals shown together on the walking map (the grid is small).
    pub fn group_on_grid(&self) -> usize {
        (self.herd as usize).clamp(1, 4)
    }
}

// ---------------------------------------------------------------- evolution

/// Generations an isolated population needs to adapt (63 %) to what its place selects.
const ADAPT_GENS: f64 = 4000.0;
/// Generations between two knots of the random drift.
const DRIFT_GENS: f64 = 3000.0;
/// Divergence thresholds: mass factor (ln), coat tint, radiation hardening.
const SIZE_THR: f64 = 0.5;
const TINT_THR: f64 = 0.6;
const RESIST_THR: f64 = 0.4;

/// Radiation reaching the ground, 0..1: the air shields (the column halves it about every
/// 0.1 bar), so an airless world is bathed in it and a thick air stops it.
pub fn surface_radiation(pressure: f64) -> f64 {
    radiation_through_air(pressure, vector_core::units::G0)
}

/// Share of the particle radiation that crosses an atmosphere of `pressure` bar on a world of
/// surface gravity `gravity` (m/s^2): the column of air above (pressure / gravity, in g/cm^2)
/// against a stopping length of 150 g/cm^2. The one law of the ground dose, the biosphere and
/// the dose in flight under an atmosphere (audit B6).
pub fn radiation_through_air(pressure: f64, gravity: f64) -> f64 {
    let column = pressure.max(0.0) * 1.0e5 / gravity.max(0.05) * 0.1;
    (-column / 150.0).exp()
}

/// One isolated population (a species in a biome of a world) and how far it has gone from its
/// ancestors. Traits are relative to the species as written in `species.ron`.
#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub species: usize,
    pub biome: usize,
    /// ln of the mass factor (cold places select big bodies, Bergmann's rule).
    pub size: f64,
    /// Coat tint, -1 dark .. +1 pale (camouflage against the ground).
    pub tint: f64,
    /// Radiation hardening, 0..1 (selected on worlds with thin air).
    pub resist: f64,
    pub generations: f64,
    /// 1.0 = distinct enough to count as a new variant.
    pub divergence: f64,
}

impl Variant {
    pub fn mass_factor(&self) -> f64 {
        self.size.exp()
    }

    pub fn speciated(&self) -> bool {
        self.divergence >= 1.0
    }

    /// The trait that moved the most, as a name (empty if none moved much).
    pub fn epithet(&self) -> &'static str {
        let (s, t, r) = (
            self.size.abs() / SIZE_THR,
            self.tint.abs() / TINT_THR,
            self.resist / RESIST_THR,
        );
        if s.max(t).max(r) < 0.25 {
            ""
        } else if s >= t && s >= r {
            if self.size > 0.0 {
                "geante"
            } else {
                "naine"
            }
        } else if t >= r {
            if self.tint > 0.0 {
                "pale"
            } else {
                "sombre"
            }
        } else {
            "endurcie"
        }
    }

    /// Name of the variant as shown: the species plus its epithet once it has diverged enough
    /// to count as a variant, the species' own name before.
    pub fn name(&self, data: &GameData) -> String {
        let base = &data.species[self.species].name;
        match self.epithet() {
            e if !e.is_empty() && self.speciated() => format!("{base} (variante {e})"),
            _ => base.clone(),
        }
    }
}

fn luminance(color: u32) -> f64 {
    let c = |s: u32| f64::from((color >> s) & 0xFF) / 255.0;
    0.30 * c(16) + 0.59 * c(8) + 0.11 * c(0)
}

/// Smooth random value in [-1, 1] along the generations (value noise on knots).
fn drift(seed: u64, axis: i64, gens: f64) -> f64 {
    let (i, f) = ((gens / DRIFT_GENS).floor(), (gens / DRIFT_GENS).fract());
    let n = |k: f64| unit(hash_coord(seed, axis, k as i64, 0xD21F)) * 2.0 - 1.0;
    let f = f * f * (3.0 - 2.0 * f);
    n(i) + (n(i + 1.0) - n(i)) * f
}

/// The population of `species` in `biome` after `generations` generations of isolation on a
/// world of seed `world` whose ground receives `radiation` (0..1).
pub fn variant_after(
    data: &GameData,
    world: u64,
    radiation: f64,
    species: usize,
    biome: usize,
    generations: f64,
) -> Variant {
    let sp = &data.species[species];
    let key = hash_coord(world, species as i64, biome as i64, 0x7A21);
    let adapt = 1.0 - (-generations / ADAPT_GENS).exp();
    let gate = (generations / DRIFT_GENS).min(1.0);
    // What the place selects, against the places the species comes from.
    let size_opt =
        (0.5 * (home_temperature(data, sp) - biome_mid_temp(data, biome)) / 30.0).clamp(-1.0, 1.0);
    let home_lum = sp
        .habitat
        .iter()
        .filter_map(|b| data.biome_index(b))
        .map(|b| luminance(data.biomes[b].color))
        .sum::<f64>()
        / sp.habitat.len().max(1) as f64;
    let tint_opt = ((luminance(data.biomes[biome].color) - home_lum) * 3.0).clamp(-1.0, 1.0);
    let base_resist = if data
        .chemistry_of(species)
        .is_some_and(|c| data.chemistries[c].id == "radiotrophes")
    {
        1.0
    } else {
        0.0
    };
    let size = size_opt * adapt + 0.4 * gate * drift(key, 1, generations);
    let tint = (tint_opt * adapt + 0.5 * gate * drift(key, 2, generations)).clamp(-1.0, 1.0);
    let resist = (base_resist + (radiation - base_resist) * adapt).clamp(0.0, 1.0);
    let divergence = ((size / SIZE_THR).powi(2)
        + (tint / TINT_THR).powi(2)
        + ((resist - base_resist) / RESIST_THR).powi(2))
    .sqrt();
    Variant {
        species,
        biome,
        size,
        tint,
        resist,
        generations,
        divergence,
    }
}

/// Years the populations of a world have been isolated at game start (100 to 1 million, by seed),
/// plus the game time: a world whose life is old has adapted, a young one has barely moved.
pub fn isolation_years(world: u64, species: usize, biome: usize, now: f64) -> f64 {
    let u = unit(hash_coord(world, species as i64, biome as i64, 0x150));
    10f64.powf(2.0 + 4.0 * u) + now / 3.15576e7
}

/// The variant of a population now (game time `now`, s).
pub fn variant_now(
    data: &GameData,
    world: u64,
    radiation: f64,
    species: usize,
    biome: usize,
    now: f64,
) -> Variant {
    let gens = isolation_years(world, species, biome, now) * 365.25
        / generation_days(&data.species[species]);
    variant_after(data, world, radiation, species, biome, gens)
}

/// Every population of a living world, most diverged first.
pub fn world_variants(data: &GameData, eco: &Ecosystem, map: &PlanetMap, now: f64) -> Vec<Variant> {
    let rad = surface_radiation(map.pressure);
    let mut v: Vec<Variant> = Vec::new();
    if !eco.life {
        return v;
    }
    for b in 0..eco.nb {
        for (s, _) in eco.present_in(b) {
            v.push(variant_now(data, map.seed, rad, s, b, now));
        }
    }
    v.sort_by(|a, b| b.divergence.total_cmp(&a.divergence));
    v
}

// ---------------------------------------------------------------- the web, as seen

/// (What a species eats, what eats it), each with the biomass in a biome, kg.
pub type Web = (Vec<(usize, f64)>, Vec<(usize, f64)>);

/// What a species eats and what eats it in a biome, with the biomass of each (kg), the
/// neighbourhood of one node of the food web.
pub fn web_of(data: &GameData, eco: &Ecosystem, biome: usize, species: usize) -> Web {
    let eats = data.species[species]
        .diet
        .iter()
        .filter_map(|d| data.species_index(d))
        .map(|d| (d, eco.at(d, biome)))
        .filter(|&(_, m)| m > 0.0)
        .collect();
    let eaten_by = (0..eco.ns)
        .filter(|&c| {
            data.species[c]
                .diet
                .iter()
                .any(|d| *d == data.species[species].id)
        })
        .map(|c| (c, eco.at(c, biome)))
        .filter(|&(_, m)| m > 0.0)
        .collect();
    (eats, eaten_by)
}

// ---------------------------------------------------------------- the air of a living world

/// Years of photosynthesis after which half of the possible oxygen is there (ln 2 x 1.2 Gyr).
const OXYGEN_YEARS: f64 = 1.2e9;
/// Most oxygen a biosphere puts in the air, fraction (the Earth's 21 % and a little more).
const OXYGEN_MAX: f64 = 0.30;

/// Fraction of the atmosphere that photosynthesis has turned from CO2 into O2 on a world of this
/// kind, temperature (K) and pressure (bar) with this seed, `now` seconds into the game. Needs
/// life, an air and liquid water (otherwise no oxygenic photosynthesis); an old life has made
/// more of it.
pub fn oxygenation(kind: PlanetKind, temp: f64, pressure: f64, seed: u64, now: f64) -> f64 {
    if pressure < 0.05 || !(255.0..=340.0).contains(&temp) || !has_life(kind, seed) {
        return 0.0;
    }
    let bell = (-((temp - 288.0) / 40.0).powi(2)).exp();
    let age = 10f64.powf(7.5 + 2.0 * unit(hash_coord(seed, 0, 0x02, 0xA6E))) + now / 3.15576e7;
    OXYGEN_MAX * bell * (1.0 - (-age / OXYGEN_YEARS).exp())
}

/// The seed of a world (the one `build_map` gives its map), without building the map.
pub fn world_seed(system_seed: u64, body: usize) -> u64 {
    hash_coord(system_seed, body as i64, 0xB10E, 1)
}

impl Sim {
    pub fn world_seed_of(&self, body: usize) -> u64 {
        world_seed(self.system.seed, body)
    }

    /// The composition of the air of body `i` with what its life has done to it: photosynthesis
    /// moves CO2 into O2 mole for mole (the carbon goes into the ground), never more CO2 than
    /// there is.
    pub fn atmosphere_of(&self, i: usize) -> Vec<(String, f32)> {
        let Some(b) = self.system.bodies.get(i) else {
            return Vec::new();
        };
        let mut gas: Vec<(String, f32)> = b
            .atmosphere
            .iter()
            .map(|(g, f)| ((*g).to_string(), *f))
            .collect();
        let kind = match b.kind {
            vector_world::BodyKind::Planet(k) | vector_world::BodyKind::Moon(k) => k,
            _ => return gas,
        };
        let o2 = oxygenation(kind, b.temp, b.pressure, self.world_seed_of(i), self.time);
        let Some(co2) = gas.iter().position(|(g, _)| g == "CO2") else {
            return gas;
        };
        let moved = (o2 as f32).min(gas[co2].1);
        if moved <= 0.0 {
            return gas;
        }
        gas[co2].1 -= moved;
        match gas.iter().position(|(g, _)| g == "O2") {
            Some(k) => gas[k].1 += moved,
            None => gas.push(("O2".to_string(), moved)),
        }
        gas
    }

    /// Whether it is night at the place of a landing (false on a world that keeps one face to
    /// its star or has no day to speak of).
    pub fn is_night(&self, map: &PlanetMap) -> bool {
        let ph = self.system.physics(map.body, f64::from(map.surface.tilt));
        match (ph.locked, ph.solar_day) {
            (Some((1, 1)), _) | (_, None) => false,
            (_, Some(d)) => (self.time / d).fract() >= 0.5,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    fn sp(d: &GameData, id: &str) -> usize {
        d.species_index(id).expect("species")
    }

    #[test]
    fn metabolism_follows_three_quarters_and_the_life_cycle_scales_with_size() {
        let d = data();
        let (mouse, bull) = (
            &d.species[sp(&d, "lagomorphe")],
            &d.species[sp(&d, "bovide")],
        );
        let (a, b) = (life_of(mouse), life_of(bull));
        // Mass ratio r gives metabolic ratio r^0.75 and lifespan ratio r^0.25 exactly.
        let r = bull.body / mouse.body;
        assert!((b.watts / a.watts - r.powf(0.75)).abs() < 1e-9 * r);
        assert!((b.lifespan_days / a.lifespan_days - r.powf(0.25)).abs() < 1e-9 * r);
        // Big animals: fewer young, later maturity.
        assert!(a.litter > b.litter && b.litter >= 1);
        assert!(b.maturity_days > a.maturity_days);
        // A bacterium divides, a herd animal mates.
        assert!(!life_of(&d.species[sp(&d, "plancton")]).sexual);
        assert!(b.sexual);
        // Across the whole table, the faster a species grows the lighter it is (allometry).
        let (mut xs, mut ys) = (Vec::new(), Vec::new());
        for s in &d.species {
            xs.push(s.body.ln());
            ys.push(s.growth.ln());
        }
        let n = xs.len() as f64;
        let (mx, my) = (xs.iter().sum::<f64>() / n, ys.iter().sum::<f64>() / n);
        let cov: f64 = xs.iter().zip(&ys).map(|(x, y)| (x - mx) * (y - my)).sum();
        assert!(cov < 0.0, "growth rate falls with mass: cov {cov}");
    }

    #[test]
    fn behaviour_follows_the_trophic_level_and_the_climate() {
        let d = data();
        let wolf = behavior_of(&d, &d.species[sp(&d, "loup_des_steppes")]);
        assert!(wolf.territory_km2 > 10.0 && wolf.herd >= 1);
        let bovid = behavior_of(&d, &d.species[sp(&d, "bovide")]);
        assert!(bovid.herd >= 3 && bovid.territory_km2 == 0.0);
        let locust = behavior_of(&d, &d.species[sp(&d, "criquet")]);
        assert!(locust.herd >= 50, "swarm");
        // Plants and fish do not hibernate; a cold-country grazer does, and sleeps in the cold.
        assert!(!behavior_of(&d, &d.species[sp(&d, "chene")]).hibernates);
        assert!(!behavior_of(&d, &d.species[sp(&d, "poisson_argente")]).hibernates);
        let elk = behavior_of(&d, &d.species[sp(&d, "elan_des_neiges")]);
        assert!(elk.hibernates);
        assert!(elk.asleep(250.0, true) && !elk.asleep(290.0, true));
        // A nocturnal one sleeps by day whatever the temperature.
        let n = d
            .species
            .iter()
            .map(|s| behavior_of(&d, s))
            .find(|b| b.nocturnal && !b.hibernates)
            .expect("a nocturnal species");
        assert!(n.asleep(290.0, false) && !n.asleep(290.0, true));
        // Every kind of behaviour exists in the table.
        let all: Vec<Behavior> = d.species.iter().map(|s| behavior_of(&d, s)).collect();
        assert!(all.iter().any(|b| b.pack) && all.iter().any(|b| b.nocturnal));
        assert!(all.iter().any(|b| b.hibernates));
    }

    #[test]
    fn isolated_populations_drift_toward_what_their_place_selects() {
        let d = data();
        let hare = sp(&d, "lagomorphe");
        let (steppe, tundra) = (
            d.biome_index("steppe").unwrap(),
            d.biome_index("toundra").unwrap(),
        );
        // No time, no change: the population is still the ancestor.
        let v0 = variant_after(&d, 7, 0.0, hare, tundra, 0.0);
        assert!(v0.size == 0.0 && v0.tint == 0.0 && !v0.speciated());
        // The same species, long isolated, is BIGGER in the colder biome than in the warmer one
        // (Bergmann) once the random drift is averaged over many worlds.
        let mean = |b: usize| -> f64 {
            (0..60u64)
                .map(|w| variant_after(&d, w, 0.0, hare, b, 1.0e6).size)
                .sum::<f64>()
                / 60.0
        };
        assert!(
            mean(tundra) > mean(steppe),
            "{} {}",
            mean(tundra),
            mean(steppe)
        );
        // Thin air selects radiation hardening; thick air does not.
        let thin = variant_after(&d, 7, surface_radiation(0.01), hare, steppe, 1.0e6);
        let thick = variant_after(&d, 7, surface_radiation(1.0), hare, steppe, 1.0e6);
        assert!(thin.resist > 0.9 && thick.resist < 0.05);
        // Determinism, and the divergence is finite.
        assert_eq!(
            thin,
            variant_after(&d, 7, surface_radiation(0.01), hare, steppe, 1.0e6)
        );
        assert!(thin.divergence.is_finite());
    }

    #[test]
    fn speciation_needs_isolation_and_small_creatures_get_there_first() {
        let d = data();
        let steppe = d.biome_index("steppe").unwrap();
        let (hare, bull) = (sp(&d, "lagomorphe"), sp(&d, "bovide"));
        // Young isolation (a century): the big slow breeder has not moved, the world's
        // radiation-hardened lineage appears only with enough generations.
        let young = |s: usize| variant_now(&d, 3, 0.0, s, steppe, 0.0).generations;
        let spec = |s: usize, gens: f64| {
            (0..80u64)
                .filter(|&w| variant_after(&d, w, 0.9, s, steppe, gens).speciated())
                .count()
        };
        assert!(spec(hare, 10.0) == 0, "ten generations are not enough");
        assert!(spec(hare, 2.0e5) > 40, "{}", spec(hare, 2.0e5));
        // A hare lives short: the same years make many more generations than for the bull.
        let years = 5.0e4 * 365.25;
        assert!(
            years / generation_days(&d.species[hare])
                > 3.0 * years / generation_days(&d.species[bull])
        );
        assert!(young(hare) >= 0.0);
        // Time passing in the game moves a population (accelerated time evolves it).
        let a = variant_now(&d, 3, 0.0, hare, steppe, 0.0);
        let b = variant_now(&d, 3, 0.0, hare, steppe, 3.15576e7 * 1.0e5);
        assert!(b.generations > a.generations);
    }

    #[test]
    fn photosynthesis_turns_co2_into_o2_mole_for_mole_and_needs_life() {
        // Sterile, airless or frozen worlds make none.
        assert_eq!(oxygenation(PlanetKind::Barren, 288.0, 1.0, 5, 0.0), 0.0);
        assert_eq!(oxygenation(PlanetKind::Terran, 288.0, 0.01, 5, 0.0), 0.0);
        assert_eq!(oxygenation(PlanetKind::Terran, 150.0, 1.0, 5, 0.0), 0.0);
        // Among living temperate worlds the oxygen is positive, bounded, and grows with time.
        let living: Vec<u64> = (1..200u64)
            .filter(|&s| has_life(PlanetKind::Terran, s))
            .collect();
        assert!(living.len() > 100);
        for &s in &living {
            let o = oxygenation(PlanetKind::Terran, 288.0, 1.0, s, 0.0);
            let later = oxygenation(PlanetKind::Terran, 288.0, 1.0, s, 3.15576e7 * 5.0e8);
            assert!((0.0..=OXYGEN_MAX).contains(&o) && later >= o, "{o} {later}");
        }
        // In a Sim: the total of CO2 + O2 of a living world is conserved.
        let mut s = crate::scene::new_sim(5, data());
        let mut tested = 0;
        for i in 0..s.system.bodies.len() {
            let b = &s.system.bodies[i];
            let before: f32 = b
                .atmosphere
                .iter()
                .filter(|(g, _)| *g == "CO2" || *g == "O2")
                .map(|(_, f)| *f)
                .sum();
            let after: f32 = s
                .atmosphere_of(i)
                .iter()
                .filter(|(g, _)| g == "CO2" || g == "O2")
                .map(|(_, f)| *f)
                .sum();
            assert!((before - after).abs() < 1e-5);
            tested += 1;
        }
        assert!(tested > 0);
        s.time = 0.0;
    }
}
