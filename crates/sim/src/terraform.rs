//! Terraforming of a colonised world, as physics over centuries of game time.
//!
//! A world is a stock of gases (kg), water in three phases (ice, ocean, vapour), a reservoir of
//! frozen or adsorbed carbon dioxide, a crust that swallows oxygen and a surface temperature that
//! follows an energy balance: the star's light (plus orbital mirrors) minus what the surface
//! radiates, through a greenhouse whose optical depth comes from the partial pressures of the
//! gases. Every transfer moves mass from one stock to another (the ledger is checked by tests);
//! nothing is a bar that fills up. What the player does is physical: put mirrors in orbit, darken
//! the ice with soot, bring comets, seed producers that turn carbon dioxide into oxygen, run
//! plants that crack gases with the colony's power.
//!
//! Only the *deltas* make the map change: the surface map still comes from the seed, and
//! `Terra::apply_to_map` shifts its temperature and humidity, moves the sea level and re-classifies
//! the biomes. The baseline (gases, water, bands of climate) is kept in `WorldSnap` so a colony
//! far from the player can be simulated without regenerating its system.
use crate::planet::PlanetMap;
use serde::{Deserialize, Serialize};
use vector_data::GameData;
use vector_world::surface::{self, Surface, H, W};
use vector_world::{BodyKind, PlanetKind, System};

pub use vector_core::units::{DAY, SIGMA, SOLAR_CONSTANT, YEAR};
const L_VAP: f64 = 2.5e6;
const R_VAPOUR: f64 = 461.5;
const L_FUSION: f64 = 3.34e5;
/// Climate bands of the baseline (latitude strips of the map).
pub const BANDS: usize = 8;
const BAND_ROWS: usize = H / BANDS;
/// Height of the relief that the sea can climb (m): the hypsometry that links the volume of
/// the ocean to the share of the surface it covers (sea share = sqrt(depth / SEA_DEPTH)).
const SEA_DEPTH: f64 = 5200.0;
/// Stored in crust oxidation: kg of oxygen the ground can still swallow, per m^2.
const CRUST_O2: f64 = 100.0;
/// Biomass density a seeded strain needs to establish (kg/m^2) and net production (kg of dry
/// matter per m^2 and per year at fertility 1).
const BIOMASS_DENSITY: f64 = 5.0;
const NPP_MAX: f64 = 2.0;
/// Share of the carbon fixed that engineered strains bury instead of respiring.
const BURIAL: f64 = 0.5;
/// How much slower than the logistic of the food web a strain spreads over a whole world.
const SPREAD_SLOWDOWN: f64 = 3000.0;
/// Carbon dioxide (bar) a cold world with caps always holds in its thin air.
const CO2_FLOOR_BAR: f64 = 5.0e-4;

/// The gases of the atmospheres, with their molar mass (g/mol) and greenhouse strength (the
/// optical depth is the sum of `k * p^0.6`, p in bar).
pub struct Gas {
    pub id: &'static str,
    pub name: &'static str,
    pub mass: f64,
    pub k: f64,
}

pub const GASES: [Gas; 12] = [
    Gas {
        id: "N2",
        name: "AZOTE",
        mass: 28.0,
        k: 0.0,
    },
    Gas {
        id: "O2",
        name: "OXYGENE",
        mass: 32.0,
        k: 0.0,
    },
    Gas {
        id: "CO2",
        name: "GAZ CARBONIQUE",
        mass: 44.0,
        k: 9.0,
    },
    Gas {
        id: "CH4",
        name: "METHANE",
        mass: 16.0,
        k: 60.0,
    },
    Gas {
        id: "H2O",
        name: "VAPEUR D'EAU",
        mass: 18.0,
        k: 6.0,
    },
    Gas {
        id: "Ar",
        name: "ARGON",
        mass: 40.0,
        k: 0.0,
    },
    Gas {
        id: "H2",
        name: "HYDROGENE",
        mass: 2.0,
        k: 1.0,
    },
    Gas {
        id: "He",
        name: "HELIUM",
        mass: 4.0,
        k: 0.0,
    },
    Gas {
        id: "NH3",
        name: "AMMONIAC",
        mass: 17.0,
        k: 40.0,
    },
    Gas {
        id: "SO2",
        name: "DIOXYDE DE SOUFRE",
        mass: 64.0,
        k: 20.0,
    },
    Gas {
        id: "CO",
        name: "MONOXYDE DE CARBONE",
        mass: 28.0,
        k: 2.0,
    },
    Gas {
        id: "PFC",
        name: "PERFLUOROCARBURES",
        mass: 150.0,
        k: 3000.0,
    },
];
pub const N2: usize = 0;
pub const O2: usize = 1;
pub const CO2: usize = 2;
pub const CH4: usize = 3;
pub const H2O: usize = 4;
pub const H2: usize = 6;
pub const NH3: usize = 8;
pub const PFC: usize = 11;

pub fn gas_index(id: &str) -> Option<usize> {
    GASES.iter().position(|g| g.id == id)
}

/// What a process plant of the colony does with its power (index = `Terra::process`).
pub const PROCESSES: [&str; 5] = [
    "ARRET",
    "CRAQUAGE DU CO2 EN O2",
    "ELECTROLYSE DE L'EAU",
    "CAPTURE DU CO2",
    "FABRIQUE DE PFC",
];
/// Energy per kg processed (J/kg) of each process, and its efficiency.
const PROCESS_ENERGY: [f64; 5] = [1.0, 8.9e6, 1.6e7, 2.0e6, 3.0e7];
const PROCESS_EFFICIENCY: f64 = 0.6;

const KINDS: [PlanetKind; 12] = [
    PlanetKind::Barren,
    PlanetKind::Dwarf,
    PlanetKind::Desert,
    PlanetKind::Terran,
    PlanetKind::Ocean,
    PlanetKind::Tundra,
    PlanetKind::Ice,
    PlanetKind::Lava,
    PlanetKind::Venusian,
    PlanetKind::GasGiant,
    PlanetKind::IceGiant,
    PlanetKind::HotJupiter,
];

/// Bond albedo of the bare world of each kind.
fn albedo_of(k: PlanetKind) -> f64 {
    match k {
        PlanetKind::Barren => 0.12,
        PlanetKind::Dwarf => 0.15,
        PlanetKind::Desert => 0.25,
        PlanetKind::Terran | PlanetKind::Ocean => 0.30,
        PlanetKind::Tundra => 0.35,
        PlanetKind::Ice => 0.60,
        PlanetKind::Lava => 0.10,
        PlanetKind::Venusian => 0.75,
        _ => 0.3,
    }
}

/// Frozen water at the poles and underground, as a global layer of water (m), by kind.
fn ice_depth_of(k: PlanetKind) -> f64 {
    match k {
        PlanetKind::Barren => 20.0,
        PlanetKind::Dwarf => 80.0,
        PlanetKind::Desert => 2.0,
        PlanetKind::Terran => 45.0,
        PlanetKind::Ocean => 10.0,
        PlanetKind::Tundra => 250.0,
        PlanetKind::Ice => 1500.0,
        _ => 0.0,
    }
}

/// Carbon dioxide held in the polar caps and the regolith, as kg per m^2, by kind.
fn co2_reservoir_of(k: PlanetKind) -> f64 {
    match k {
        PlanetKind::Barren => 300.0,
        PlanetKind::Dwarf => 200.0,
        PlanetKind::Desert => 100.0,
        PlanetKind::Tundra => 50.0,
        PlanetKind::Ice => 500.0,
        _ => 0.0,
    }
}

/// Saturation pressure of water vapour (bar) at `t` K (Clausius-Clapeyron from 1 bar at 373 K).
pub fn psat_water(t: f64) -> f64 {
    (-L_VAP / R_VAPOUR * (1.0 / t.max(100.0) - 1.0 / 373.15)).exp()
}

/// Sublimation pressure of carbon dioxide (bar) at `t` K.
pub fn psat_co2(t: f64) -> f64 {
    (-3168.0 * (1.0 / t.max(50.0) - 1.0 / 194.7)).exp()
}

/// Latitude weight of a warming: the poles warm faster than the equator (mean weight 1).
fn polar_amplification(lat: f64) -> f64 {
    0.7 + 0.9 * lat.sin().powi(2)
}

/// Everything about a world that does not change: its size, light, spin and baseline climate.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WorldSnap {
    pub rocky: bool,
    /// Index into the list of kinds of planets.
    pub kind: u8,
    /// Seed of the surface map (same as `PlanetMap::seed`), for the native life.
    pub seed: u64,
    pub radius: f64,
    pub mass: f64,
    pub gravity: f64,
    /// Sidereal rotation, s.
    pub spin: f64,
    pub locked: bool,
    /// Light of the star on one square metre facing it, W/m^2.
    pub insolation: f64,
    /// Luminosity of the star, solar units, and the distance to it, m.
    pub luminosity: f64,
    pub star_dist: f64,
    /// Magnetic moment relative to the Earth's.
    pub magnetic: f64,
    /// Speed to leave the world, m/s.
    pub escape: f64,
    /// Radius of the sphere where the world rules a moon, m (0: none).
    pub hill: f64,
    /// A belt of asteroids in the system.
    pub belt: bool,
    /// Baseline mean surface temperature, K, and the same worked out by the model.
    pub t_base: f64,
    pub t_model0: f64,
    pub albedo0: f64,
    /// Baseline gases, kg.
    pub gas0: Vec<f64>,
    pub ice0: f64,
    pub ocean0: f64,
    pub co2res0: f64,
    /// Share of the surface under the sea at the start.
    pub sea0: f64,
    /// Temperature at which half of the water would be ice (K), fixed by the baseline.
    pub t_half: f64,
    /// Does the world carry native life (0 or 1)?
    pub life0: f64,
    /// Share of the water vapour's greenhouse this world keeps (its clouds and circulation
    /// damp a feedback that would otherwise make the baseline itself unstable).
    pub h2o_scale: f64,
    /// Per latitude band: mean temperature, humidity, share of land and mean elevation of land.
    pub bands: Vec<f32>,
}

impl WorldSnap {
    pub fn area(&self) -> f64 {
        4.0 * std::f64::consts::PI * self.radius * self.radius
    }

    /// G times the mass of the world.
    pub fn gm(&self) -> f64 {
        vector_world::orbit::G * self.mass
    }

    pub fn kind(&self) -> PlanetKind {
        KINDS[usize::from(self.kind).min(KINDS.len() - 1)]
    }

    /// Radius of the geostationary orbit (m); None when the world is locked to its star or
    /// moon (no synchronous orbit that is stable) or spins so slowly that it falls outside the
    /// sphere the world rules.
    pub fn geostationary(&self) -> Option<f64> {
        if self.locked || self.spin <= 0.0 {
            return None;
        }
        let r = (self.gm() * (self.spin / std::f64::consts::TAU).powi(2)).cbrt();
        if r <= self.radius || (self.hill > 0.0 && r > 0.5 * self.hill) {
            return None;
        }
        Some(r)
    }
}

/// A producer seeded on the world.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Seed {
    /// Index into `GameData::species`.
    pub species: u16,
    /// Share of the suitable surface the strain covers (0..1).
    pub cover: f64,
}

/// The state of a world being terraformed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Terra {
    pub snap: WorldSnap,
    /// Gas stocks, kg (index = `GASES`).
    pub gas: Vec<f64>,
    /// Mean surface temperature, K.
    pub t: f64,
    pub ice: f64,
    pub ocean: f64,
    /// Carbon dioxide frozen in the caps and adsorbed in the ground, kg.
    pub co2res: f64,
    /// Oxygen the crust has swallowed (oxidised rock), kg.
    pub sink_o2: f64,
    /// Carbon buried by living things and by cracking plants, kg.
    pub carbon: f64,
    /// Carbon dioxide locked as carbonate by capture plants, kg.
    pub carbonate: f64,
    /// Fluorocarbons that have decayed into the ground, kg.
    pub pfc_sunk: f64,
    /// Mass lost to space, kg.
    pub escaped: f64,
    /// Matter brought by comets, kg, and taken from the ground (PFC), kg.
    pub delivered: f64,
    pub ground: f64,
    /// Area of orbital mirrors, m^2, and soot spread on the ice, kg.
    pub mirror: f64,
    pub soot: f64,
    pub seeds: Vec<Seed>,
    /// Native life: 1 = thriving, 0 = extinct.
    pub life: f64,
    /// What the plants of the colony do (index into `PROCESSES`).
    pub process: u8,
    /// Mass handled by the plants the last step, kg/s (display).
    pub rate: f64,
    /// Surface temperature drift over the last step, K per year.
    pub drift: f64,
    /// Bits of events already told.
    pub told: u32,
    /// Counter that changes when the map would, for caches.
    pub version: u32,
    /// Game time of the last step (s), kept for the display of time scales.
    pub age: f64,
    /// Messages for the log of the colony (taken by the colony).
    pub notes: Vec<String>,
}

const TOLD_RUNAWAY: u32 = 1;
const TOLD_BOIL: u32 = 2;
const TOLD_OCEAN: u32 = 4;
const TOLD_FROZEN: u32 = 8;
const TOLD_BREATH: u32 = 16;
const TOLD_LIFE: u32 = 32;

/// Weight of each band in area terms (cos of its latitude).
fn band_weight(b: usize) -> f64 {
    Surface::lat(b * BAND_ROWS + BAND_ROWS / 2).cos()
}

fn band_lat(b: usize) -> f64 {
    Surface::lat(b * BAND_ROWS + BAND_ROWS / 2)
}

fn sea_share(ocean: f64, area: f64) -> f64 {
    let depth = ocean / (1000.0 * area);
    (depth / SEA_DEPTH).max(0.0).sqrt().min(0.97)
}

fn ocean_for_share(share: f64, area: f64) -> f64 {
    1000.0 * area * SEA_DEPTH * share * share
}

impl Terra {
    /// The state of body `body` of `system` before anyone touches it. `map` is its surface (None
    /// for bodies without one, which carry no climate to change).
    pub fn new(system: &System, body: usize, map: Option<&PlanetMap>) -> Terra {
        let b = &system.bodies[body];
        let kind = match b.kind {
            BodyKind::Planet(k) | BodyKind::Moon(k) if !k.is_giant() => Some(k),
            _ => None,
        };
        let ph = system.physics(body, 0.4);
        let mut snap = WorldSnap {
            rocky: kind.is_some() && map.is_some(),
            kind: kind.map_or(0, |k| k as u8),
            seed: map.map_or(0, |m| m.seed),
            radius: b.radius,
            mass: b.mass,
            gravity: b.gravity,
            spin: ph.spin_period,
            locked: ph.locked.is_some(),
            insolation: ph.insolation,
            luminosity: system.luminosity,
            star_dist: (system.luminosity * 3.828e26
                / (4.0 * std::f64::consts::PI * ph.insolation.max(1e-9)))
            .sqrt(),
            magnetic: ph.magnetic,
            escape: ph.escape,
            hill: ph.hill.unwrap_or(0.0),
            belt: !system.belts.is_empty(),
            t_base: b.temp,
            t_model0: b.temp,
            albedo0: kind.map_or(0.3, albedo_of),
            gas0: vec![0.0; GASES.len()],
            ice0: 0.0,
            ocean0: 0.0,
            co2res0: 0.0,
            sea0: 0.0,
            t_half: 273.0,
            life0: 0.0,
            h2o_scale: 1.0,
            bands: vec![0.0; BANDS * 4],
        };
        let area = snap.area();
        // Baseline gases from the composition and the pressure on the ground.
        let total = b.pressure * 1.0e5 * area / b.gravity.max(0.01);
        let (mut mol_mass, mut share) = (0.0, 0.0);
        for (g, f) in &b.atmosphere {
            mol_mass += crate::atmosphere::molar_mass(g) * f64::from(*f);
            share += f64::from(*f);
        }
        if share > 0.0 {
            for (g, f) in &b.atmosphere {
                if let Some(i) = gas_index(g) {
                    snap.gas0[i] +=
                        total * crate::atmosphere::molar_mass(g) * f64::from(*f) / mol_mass;
                }
            }
        }
        if let (Some(k), Some(m)) = (kind, map) {
            // Share of the surface under the sea, by area (the map's own figure counts cells).
            let (mut sea_w, mut all_w) = (0.0, 0.0);
            for y in 0..H {
                for x in 0..W {
                    let c = Surface::lat(y).cos();
                    all_w += c;
                    if m.surface.elev[Surface::idx(x, y)] < 0.0 {
                        sea_w += c;
                    }
                }
            }
            snap.sea0 = sea_w / all_w;
            snap.life0 = f64::from(u8::from(crate::ecology::has_life(k, m.seed)));
            // Baseline climate bands and mean temperature (area-weighted).
            let (mut t_sum, mut w_sum) = (0.0, 0.0);
            for band in 0..BANDS {
                let (mut t, mut h, mut land, mut elev) = (0.0, 0.0, 0.0, 0.0);
                let n = (BAND_ROWS * W) as f64;
                for y in band * BAND_ROWS..(band + 1) * BAND_ROWS {
                    for x in 0..W {
                        let i = Surface::idx(x, y);
                        t += f64::from(m.surface.temp[i]);
                        h += f64::from(m.surface.hum[i]);
                        if m.surface.elev[i] >= 0.0 {
                            land += 1.0;
                            elev += f64::from(m.surface.elev[i]);
                        }
                    }
                }
                let w = band_weight(band);
                t_sum += t / n * w;
                w_sum += w;
                snap.bands[band * 4] = (t / n) as f32;
                snap.bands[band * 4 + 1] = (h / n) as f32;
                snap.bands[band * 4 + 2] = (land / n) as f32;
                snap.bands[band * 4 + 3] = if land > 0.0 {
                    (elev / land) as f32
                } else {
                    0.0
                };
            }
            snap.t_base = t_sum / w_sum;
            // Liquid water and ice only where the world is cool enough to keep them under its
            // air: a scorched world has no sea to speak of (its "seas" on the map are lava or
            // steam), whatever the generator drew on the map.
            let liquid_ok = psat_water(snap.t_base) < 0.8 * b.pressure.max(0.006);
            snap.ocean0 = if liquid_ok {
                ocean_for_share(snap.sea0.max(0.0), area)
            } else {
                0.0
            };
            let nominal = 1.0 / (1.0 + ((snap.t_base - 265.0) / 12.0).exp());
            snap.ice0 = if liquid_ok {
                1000.0 * area * ice_depth_of(k) * nominal
            } else {
                0.0
            };
            snap.co2res0 = area * co2_reservoir_of(k);
        }
        // Water vapour in balance with the water there is at the baseline.
        let wet = snap.ice0 + snap.ocean0 > 0.0;
        let g = snap.gravity.max(0.01);
        // Caps and regolith keep a trace of carbon dioxide in the air of any cold world.
        let floor = CO2_FLOOR_BAR * 1.0e5 * area / g;
        if snap.co2res0 > floor && snap.gas0[CO2] < floor {
            snap.co2res0 -= floor - snap.gas0[CO2];
            snap.gas0[CO2] = floor;
        }
        if wet && snap.t_base > 200.0 {
            let rh = 0.6 * (0.3 + 0.7 * snap.sea0.sqrt());
            let p = (rh * psat_water(snap.t_base)).min(0.3 * b.pressure);
            let want = p * 1.0e5 * area / g;
            let have = snap.gas0[H2O];
            // Whatever the generated air held beyond the balance (or lacks) goes to the ocean.
            snap.ocean0 = (snap.ocean0 + have - want).max(0.0);
            snap.gas0[H2O] = want;
        } else if wet {
            // A cold world: the vapour that was drawn freezes out.
            snap.ice0 += snap.gas0[H2O];
            snap.gas0[H2O] = 0.0;
        }
        let ice_share = (snap.ice0 / (snap.ice0 + snap.ocean0).max(1.0)).clamp(0.003, 0.997);
        snap.t_half = snap.t_base - 12.0 * (1.0 / ice_share - 1.0).ln();
        let mut t = Terra {
            gas: snap.gas0.clone(),
            t: snap.t_base,
            ice: snap.ice0,
            ocean: snap.ocean0,
            co2res: snap.co2res0,
            sink_o2: 0.0,
            carbon: 0.0,
            carbonate: 0.0,
            pfc_sunk: 0.0,
            escaped: 0.0,
            delivered: 0.0,
            ground: 0.0,
            mirror: 0.0,
            soot: 0.0,
            seeds: Vec::new(),
            life: snap.life0,
            process: 0,
            rate: 0.0,
            drift: 0.0,
            told: 0,
            version: 0,
            age: 0.0,
            notes: Vec::new(),
            snap,
        };
        if t.pressure_of(O2) > 0.01 {
            t.sink_o2 = t.crust_capacity();
        }
        // The albedo of the bare world is the one that makes the model give the baseline
        // temperature (clouds included), and a baseline whose vapour feedback is already near a
        // runaway would be unstable by itself: the world's own clouds are taken to damp it
        // (the gain is capped at 0.8). Both adjust each other.
        for _ in 0..3 {
            t.calibrate_albedo();
            let gain = t.vapour_gain();
            if gain > 0.8 {
                t.snap.h2o_scale *= 0.8 / gain;
            }
        }
        t.calibrate_albedo();
        t.snap.t_model0 = t.model_temperature(t.snap.t_base);
        t
    }

    /// Set the albedo of the bare world so that the balance of light and greenhouse gives the
    /// baseline temperature (as far as an albedo between 0.02 and 0.92 can).
    fn calibrate_albedo(&mut self) {
        let tau = self.optical_depth();
        let te = self.snap.t_base / (1.0 + 0.75 * tau).powf(0.25);
        let a = 1.0 - 4.0 * SIGMA * te.powi(4) / self.snap.insolation.max(1e-6);
        // `albedo()` adds nothing at the baseline, so the bare albedo is this one.
        self.snap.albedo0 = a.clamp(0.02, 0.92);
    }

    pub fn crust_capacity(&self) -> f64 {
        CRUST_O2 * self.snap.area()
    }

    /// Partial pressure of a gas on the ground, bar.
    pub fn pressure_of(&self, gas: usize) -> f64 {
        self.gas[gas] * self.snap.gravity / self.snap.area() / 1.0e5
    }

    /// Pressure on the ground, bar.
    pub fn pressure(&self) -> f64 {
        (0..GASES.len()).map(|i| self.pressure_of(i)).sum()
    }

    /// Mean molar mass of the air, g/mol (29 when there is none).
    pub fn mean_mass(&self) -> f64 {
        let (mut m, mut n) = (0.0, 0.0);
        for (i, g) in GASES.iter().enumerate() {
            m += self.gas[i];
            n += self.gas[i] / g.mass;
        }
        if n > 0.0 {
            m / n
        } else {
            29.0
        }
    }

    /// Composition by number of molecules, biggest first, as (gas id, share).
    pub fn composition(&self) -> Vec<(&'static str, f32)> {
        let moles: Vec<f64> = (0..GASES.len())
            .map(|i| self.gas[i] / GASES[i].mass)
            .collect();
        let total: f64 = moles.iter().sum();
        let mut v: Vec<(&'static str, f32)> = (0..GASES.len())
            .filter(|&i| total > 0.0 && moles[i] / total > 1e-5)
            .map(|i| (GASES[i].id, (moles[i] / total) as f32))
            .collect();
        v.sort_by(|a, b| b.1.total_cmp(&a.1));
        v
    }

    /// Share of the surface under the sea now.
    pub fn sea_fraction(&self) -> f64 {
        let a = self.snap.area();
        (self.snap.sea0 + sea_share(self.ocean, a) - sea_share(self.snap.ocean0, a))
            .clamp(0.0, 0.97)
    }

    /// Rise of the sea level since the start, m of water over the whole ocean.
    pub fn sea_level_rise(&self) -> f64 {
        (self.ocean - self.snap.ocean0) / (1000.0 * self.snap.area())
    }

    /// Everything the ledger keeps, kg: constant except for what comets bring and what the
    /// plants take from the ground.
    pub fn ledger(&self) -> f64 {
        self.gas.iter().sum::<f64>()
            + self.co2res
            + self.ice
            + self.ocean
            + self.sink_o2
            + self.carbon
            + self.carbonate
            + self.pfc_sunk
            + self.escaped
    }

    /// The ledger minus what came from outside: equal to its value at the start.
    pub fn closed_ledger(&self) -> f64 {
        self.ledger() - self.delivered - self.ground
    }

    /// Share of the surface under ice and snow at a mean temperature `t`: the cold covers the
    /// poles first (a tenth of the Earth at 288 K), the whole globe below 230 K.
    pub fn ice_cover_at(t: f64) -> f64 {
        1.0 / (1.0 + ((t - 262.0) / 10.0).exp())
    }

    /// Share of the surface under ice now (0 on a world without water).
    pub fn ice_cover(&self) -> f64 {
        if self.snap.ice0 + self.snap.ocean0 > 0.0 && self.ice + self.ocean > 0.0 {
            Self::ice_cover_at(self.t)
        } else {
            0.0
        }
    }

    /// Albedo now: the bare world's, brighter with more ice cover, darker under soot and plants.
    pub fn albedo(&self) -> f64 {
        let s = &self.snap;
        let mut a = s.albedo0;
        if s.ice0 + s.ocean0 > 0.0 {
            let cover = self.ice_cover();
            a += 0.30 * (cover - Self::ice_cover_at(s.t_base));
            // Soot darkens the ice it lands on, in proportion to the cover of ice.
            let dark = 1.0 - (-self.soot / (s.area() * 1.0e-6)).exp();
            a -= 0.2 * dark * (cover / 0.05).min(1.0);
        }
        let veg: f64 = self.seeds.iter().map(|x| x.cover).sum::<f64>().min(1.0);
        a -= 0.05 * veg;
        a.clamp(0.02, 0.9)
    }

    /// Optical depth of the greenhouse for these stocks.
    pub fn optical_depth(&self) -> f64 {
        self.optical_depth_with(self.pressure_of(H2O))
    }

    /// The same with another water vapour pressure (bar). Water vapour also absorbs through
    /// its continuum, which grows as p^1.5: the moist air of a hot world is nearly opaque, which
    /// is what makes the greenhouse run away.
    fn optical_depth_with(&self, p_h2o: f64) -> f64 {
        let others: f64 = (0..GASES.len())
            .filter(|&i| i != H2O)
            .map(|i| GASES[i].k * self.pressure_of(i).powf(0.6))
            .sum();
        others + self.snap.h2o_scale * (GASES[H2O].k * p_h2o.powf(0.6) + 10.0 * p_h2o.powf(1.5))
    }

    /// How much one more kelvin of the surface warms it again through the water vapour that
    /// the warmth brings (the feedback gain): above 1 the greenhouse runs away.
    pub fn vapour_gain(&self) -> f64 {
        if self.ice + self.ocean <= 0.0 {
            return 0.0;
        }
        let te = vector_core::units::equilibrium_temperature(self.flux(), self.albedo());
        let rh = 0.6 * (0.3 + 0.7 * self.sea_fraction().sqrt());
        let ts = |t: f64| {
            let tau = self.optical_depth_with(rh * psat_water(t));
            te * (1.0 + 0.75 * tau).powf(0.25)
        };
        (ts(self.t + 0.5) - ts(self.t - 0.5)).max(0.0)
    }

    /// Light on a square metre of the world, W/m^2, with the mirrors' share.
    pub fn flux(&self) -> f64 {
        let r = self.snap.radius;
        self.snap.insolation * (1.0 + 0.85 * self.mirror / (std::f64::consts::PI * r * r))
    }

    /// Temperature of the surface at the balance of the light absorbed and the heat emitted
    /// through the greenhouse, K (the model's own scale: see `target_temperature`).
    fn model_temperature(&self, _t: f64) -> f64 {
        let te = vector_core::units::equilibrium_temperature(self.flux(), self.albedo());
        te * (1.0 + 0.75 * self.optical_depth()).powf(0.25)
    }

    /// Temperature the surface heads for: the baseline plus what changed in the balance.
    pub fn target_temperature(&self) -> f64 {
        self.snap.t_base + self.model_temperature(self.t) - self.snap.t_model0
    }

    /// Heat capacity of a square metre of surface and air, J/(m^2 K).
    fn heat_capacity(&self) -> f64 {
        let f = self.sea_fraction();
        4.2e6 * 100.0 * f
            + 8.0e6 * (1.0 - f)
            + 1000.0 * self.pressure() * 1.0e5 / self.snap.gravity.max(0.01)
    }

    /// Time constant of the temperature, s.
    pub fn thermal_time(&self) -> f64 {
        let lambda = 4.0 * SIGMA * self.t.powi(3) / (1.0 + 0.75 * self.optical_depth());
        self.heat_capacity() / lambda.max(1e-3)
    }

    /// Temperature change of the heat `joules` poured on the world at once, K.
    fn heat_to_kelvin(&self, joules: f64) -> f64 {
        joules / (self.heat_capacity() * self.snap.area())
    }

    /// Bring matter from space: `parts` are (gas id, kg). Water joins the ice if it is cold and
    /// the ocean otherwise; the kinetic energy of the impact heats the surface.
    pub fn deliver(&mut self, parts: &[(String, f64)]) {
        let total: f64 = parts.iter().map(|(_, m)| m).sum();
        for (g, m) in parts {
            if g == "H2O" {
                if self.t < 273.0 {
                    self.ice += m;
                } else {
                    self.ocean += m;
                }
            } else if let Some(i) = gas_index(g) {
                self.gas[i] += m;
            }
        }
        self.delivered += total;
        let v2 = self.snap.escape.powi(2) + (1.5e4f64).powi(2);
        let heat = 0.5 * total * v2;
        self.t = (self.t + self.heat_to_kelvin(heat)).min(self.snap.t_base + 150.0);
        self.version = self.version.wrapping_add(1);
    }

    /// Seed a strain of producer `species` with `kg` of living matter.
    pub fn seed(&mut self, data: &GameData, species: usize, kg: f64) {
        let (suit, _) = self.suitability(data, species);
        let area = self.snap.area() * suit.max(1e-3);
        let add = (kg / (area * BIOMASS_DENSITY)).min(1.0);
        match self
            .seeds
            .iter_mut()
            .find(|s| usize::from(s.species) == species)
        {
            Some(s) => s.cover = (s.cover + add).min(1.0),
            None => self.seeds.push(Seed {
                species: species as u16,
                cover: add,
            }),
        }
        self.version = self.version.wrapping_add(1);
    }

    /// Climate of band `b` now: temperature, humidity, share of land, elevation of land.
    fn band_now(&self, b: usize) -> (f64, f64, f64, f64) {
        let s = &self.snap;
        let dt = (self.t - s.t_base) * polar_amplification(band_lat(b));
        let d_wet = self.wetness() - self.wetness_base();
        let hum = (f64::from(s.bands[b * 4 + 1]) + 0.6 * d_wet).clamp(0.0, 1.0);
        let land = (f64::from(s.bands[b * 4 + 2]) - (self.sea_fraction() - s.sea0)).clamp(0.0, 1.0);
        (
            (f64::from(s.bands[b * 4]) + dt).max(2.7),
            hum,
            land,
            f64::from(s.bands[b * 4 + 3]),
        )
    }

    /// How much moisture the air can carry now (0..1.2): from its vapour pressure.
    fn wetness(&self) -> f64 {
        (self.pressure_of(H2O) / 0.012).min(1.2)
    }

    fn wetness_base(&self) -> f64 {
        let s = &self.snap;
        (s.gas0[H2O] * s.gravity / s.area() / 1.0e5 / 0.012).min(1.2)
    }

    /// How well strain `species` fits the climate now: the share of the surface where it can
    /// live, and the mean net production density over the whole surface (kg of dry matter per
    /// m^2 and per year at the species' pace).
    pub fn suitability(&self, data: &GameData, species: usize) -> (f64, f64) {
        let sp = &data.species[species];
        let p = self.pressure();
        let (mut suit, mut npp, mut wsum) = (0.0, 0.0, 0.0);
        let at = |t: f64, h: f64, e: f64| -> Option<usize> {
            let bi = data.classify_biome(t, h, e, p)?;
            sp.habitat.contains(&data.biomes[bi].id).then_some(bi)
        };
        for b in 0..BANDS {
            let (t, h, land, elev) = self.band_now(b);
            let w = band_weight(b);
            wsum += w;
            if let Some(bi) = at(t, h, elev) {
                suit += w * land;
                npp += w * land * (0.05 + data.biomes[bi].fertility);
            }
            if let Some(bi) = at(t, h, -0.2) {
                suit += w * (1.0 - land);
                npp += w * (1.0 - land) * (0.05 + data.biomes[bi].fertility);
            }
        }
        let pace = (sp.growth / 0.1).clamp(0.3, 3.0);
        (suit / wsum, NPP_MAX * npp / wsum * pace)
    }

    /// How fit the air and the climate are for people living outdoors, 0..1: oxygen, carbon
    /// dioxide, toxic gases, pressure and temperature, each from its own range.
    pub fn habitability(&self) -> f64 {
        let ramp = |v: f64, lo0: f64, lo1: f64, hi1: f64, hi0: f64| -> f64 {
            if v <= lo0 || v >= hi0 {
                0.0
            } else if v < lo1 {
                (v - lo0) / (lo1 - lo0)
            } else if v > hi1 {
                (hi0 - v) / (hi0 - hi1)
            } else {
                1.0
            }
        };
        let p = self.pressure();
        let toxic = self.pressure_of(gas_index("SO2").unwrap_or(9))
            + self.pressure_of(gas_index("CO").unwrap_or(10))
            + self.pressure_of(NH3)
            + 0.2 * self.pressure_of(CH4);
        ramp(self.pressure_of(O2), 0.08, 0.14, 0.30, 0.50)
            * (1.0 - ((self.pressure_of(CO2) - 0.005) / 0.045).clamp(0.0, 1.0))
            * (1.0 - (toxic / 0.001).min(1.0))
            * ramp(p, 0.2, 0.5, 2.5, 5.0)
            * ramp(self.t, 258.0, 278.0, 302.0, 322.0)
    }

    /// A short description of what is going on, from the state.
    pub fn status(&self) -> &'static str {
        let t_boil =
            self.t > 373.0 && self.ocean < 0.5 * self.snap.ocean0 && self.snap.ocean0 > 0.0;
        if t_boil {
            "OCEANS EVAPORES (EMBALLEMENT)"
        } else if self.t > self.snap.t_base + 25.0 && self.drift > 0.5 {
            "EMBALLEMENT DE SERRE"
        } else if self.ice_cover() > 0.9 {
            "GLACIATION TOTALE"
        } else if self.habitability() > 0.6 {
            "RESPIRABLE ET TEMPERE"
        } else if self.drift > 0.05 {
            "EN RECHAUFFEMENT"
        } else if self.drift < -0.05 {
            "EN REFROIDISSEMENT"
        } else {
            "STABLE"
        }
    }

    /// The share of the work to do before this world is breathable, 0..1 (the colony's
    /// "terraforming" figure).
    pub fn progress(&self) -> f64 {
        self.habitability()
    }

    fn tell(&mut self, bit: u32, text: impl Into<String>) {
        if self.told & bit == 0 {
            self.told |= bit;
            self.notes.push(text.into());
        }
    }

    /// Add oxygen: the crust swallows part of it first (oxidation of rock), the rest reaches the air.
    fn add_oxygen(&mut self, kg: f64) {
        let room = (self.crust_capacity() - self.sink_o2).max(0.0);
        let take = (0.7 * kg).min(room);
        self.sink_o2 += take;
        self.gas[O2] += kg - take;
    }

    /// Advance by `dt` seconds with `power_w` watts available to the process plants.
    pub fn step(&mut self, data: &GameData, dt: f64, power_w: f64) {
        if !self.snap.rocky || dt <= 0.0 {
            return;
        }
        let mut left = dt;
        let mut rate = 0.0;
        while left > 1.0 {
            let h = left.min(30.0 * DAY);
            left -= h;
            let t0 = self.t;
            rate = self.substep(data, h, power_w);
            self.drift = (self.t - t0) / h * YEAR;
        }
        self.rate = rate;
        self.age += dt;
        self.version = self.version.wrapping_add(1);
    }

    /// One substep (h seconds); returns the mass the process plants handled per second.
    fn substep(&mut self, data: &GameData, h: f64, power_w: f64) -> f64 {
        let s_area = self.snap.area();
        let g = self.snap.gravity.max(0.01);
        let years = h / YEAR;
        // --- temperature: relax toward the energy balance --------------------------------
        let target = self.target_temperature();
        let tau = self.thermal_time();
        self.t += (target - self.t) * (1.0 - (-h / tau).exp());
        // --- water: ice and ocean follow the temperature, vapour follows the ocean --------
        let wet = self.ice + self.ocean + self.gas[H2O] > 1.0;
        if wet {
            let water = self.ice + self.ocean;
            let share = 1.0 / (1.0 + ((self.t - self.snap.t_half) / 12.0).exp());
            let want_ice = share * water;
            let cap = 0.2 * s_area * 2.0 * (self.t - self.snap.t_half).abs() * h / L_FUSION;
            let flow = (want_ice - self.ice).clamp(-cap, cap);
            let flow = flow.min(self.ocean).max(-self.ice);
            self.ice += flow;
            self.ocean -= flow;
            // Vapour: saturation pressure times a humidity that grows with the sea.
            let rh = 0.6 * (0.3 + 0.7 * self.sea_fraction().sqrt());
            let m_eq = rh * psat_water(self.t) * 1.0e5 * s_area / g;
            let ex = (m_eq - self.gas[H2O]) * (1.0 - (-h / (5.0 * DAY)).exp());
            if ex > 0.0 {
                let from_ocean = ex.min(self.ocean);
                self.ocean -= from_ocean;
                let rest = if self.t > 273.0 {
                    (ex - from_ocean).min(self.ice)
                } else {
                    0.0
                };
                self.ice -= rest;
                self.gas[H2O] += from_ocean + rest;
            } else {
                let rain = (-ex).min(self.gas[H2O]);
                self.gas[H2O] -= rain;
                if self.t < 273.0 {
                    self.ice += rain;
                } else {
                    self.ocean += rain;
                }
            }
            // Photolysis of the vapour that reaches the high air: hydrogen leaves, oxygen stays.
            if self.t > 330.0 {
                let f = 5.0e-3
                    * ((self.t - 330.0) / 70.0).clamp(0.0, 1.0)
                    * (1.0 - 0.8 * self.snap.magnetic.min(1.0))
                    * years;
                let lost = self.gas[H2O] * f.min(1.0);
                self.gas[H2O] -= lost;
                self.escaped += lost / 9.0;
                self.add_oxygen(lost * 8.0 / 9.0);
            }
        }
        // --- carbon dioxide: the caps and the ground hold what the warmth does not free ----
        if self.snap.co2res0 > 0.0 || self.co2res > 0.0 {
            // The air holds the carbon dioxide of the baseline when the poles are at their
            // baseline temperature (60 K under the mean); warmer, the caps give more
            // (the vapour pressure of dry ice climbs steeply), colder they take it back.
            let total = self.gas[CO2] + self.co2res;
            let floor = CO2_FLOOR_BAR * 1.0e5 * s_area / g;
            let base = self.snap.gas0[CO2].max(floor);
            let k =
                psat_co2((self.t - 60.0).max(50.0)) / psat_co2((self.snap.t_base - 60.0).max(50.0));
            let want = total.min(base * k);
            let ex = (want - self.gas[CO2]) * (1.0 - (-years / 50.0).exp());
            self.gas[CO2] += ex;
            self.co2res -= ex;
        }
        // --- chemistry and loss to space ---------------------------------------------------
        // Ammonia is broken by the light of the star (half-life 30 years).
        let f = 1.0 - (-years * std::f64::consts::LN_2 / 30.0).exp();
        let nh3 = self.gas[NH3] * f;
        self.gas[NH3] -= nh3;
        self.gas[N2] += nh3 * 14.0 / 17.0;
        self.gas[H2] += nh3 * 3.0 / 17.0;
        // Methane burns where there is oxygen (10 years).
        if self.pressure_of(O2) > 0.01 {
            let f = 1.0 - (-years / 10.0).exp();
            let burn = (self.gas[CH4] * f).min(self.gas[O2] * 16.0 / 64.0);
            self.gas[CH4] -= burn;
            self.gas[O2] -= burn * 64.0 / 16.0;
            self.gas[CO2] += burn * 44.0 / 16.0;
            self.gas[H2O] += burn * 36.0 / 16.0;
        }
        // Fluorocarbons last ten thousand years.
        let f = 1.0 - (-years / 1.0e4).exp();
        let lost = self.gas[PFC] * f;
        self.gas[PFC] -= lost;
        self.pfc_sunk += lost;
        // Jeans escape of the light gases: the faster a molecule goes compared with the
        // speed that frees it, the sooner it is gone.
        let top = (self.t - 100.0).max(100.0);
        for i in [H2, 7] {
            let v_th2 = 2.0 * 8.314 * top / (GASES[i].mass / 1000.0);
            let lambda = self.snap.escape.powi(2) / v_th2;
            let rate = 0.1 * (1.0 + lambda) * (-lambda).exp();
            let f = 1.0 - (-rate * years).exp();
            let out = self.gas[i] * f;
            self.gas[i] -= out;
            self.escaped += out;
        }
        // --- life ----------------------------------------------------------------------------
        let stress = (((self.t - self.snap.t_base).abs() - 8.0) / 20.0).clamp(0.0, 1.0);
        if self.snap.life0 > 0.0 {
            if stress > 0.0 {
                self.life = (self.life - 0.5 * stress * years).max(0.0);
            } else {
                self.life = (self.life + 0.02 * years).min(1.0);
            }
            if self.life < 0.1 {
                self.tell(
                    TOLD_LIFE,
                    "Extinction de masse : la vie native s'effondre sous le changement de climat.",
                );
            }
        }
        let mut seeds = std::mem::take(&mut self.seeds);
        let co2_factor = (self.pressure_of(CO2) / 0.00015).clamp(0.0, 1.0);
        let mut fixed_c = 0.0;
        let mut extinct = Vec::new();
        for sd in &mut seeds {
            let (suit, npp) = self.suitability(data, usize::from(sd.species));
            let growth = data.species[usize::from(sd.species)].growth;
            if suit < 0.01 {
                sd.cover *= (-years / 0.5).exp();
                if sd.cover < 1e-15 {
                    extinct.push(usize::from(sd.species));
                }
                continue;
            }
            let r = growth / SPREAD_SLOWDOWN * (h / DAY) * co2_factor;
            sd.cover = (sd.cover + r * sd.cover * (1.0 - sd.cover)).clamp(0.0, 1.0);
            // Carbon fixed and buried (kg C): net production of the covered surface.
            fixed_c += sd.cover * npp * s_area * 0.45 * (h / YEAR) * co2_factor;
        }
        seeds.retain(|s| !extinct.contains(&usize::from(s.species)));
        for sp in extinct {
            self.notes.push(format!(
                "Extinction : la souche {} disparaît.",
                data.species[sp].name
            ));
        }
        self.seeds = seeds;
        let buried = (fixed_c * BURIAL).min(self.gas[CO2] * 12.0 / 44.0);
        if buried > 0.0 {
            self.gas[CO2] -= buried * 44.0 / 12.0;
            self.carbon += buried;
            self.add_oxygen(buried * 32.0 / 12.0);
        }
        // --- plants of the colony -------------------------------------------------------------
        let mode = usize::from(self.process).min(PROCESSES.len() - 1);
        let mut handled = 0.0;
        if mode > 0 && power_w > 0.0 {
            let mass = power_w * h * PROCESS_EFFICIENCY / PROCESS_ENERGY[mode];
            handled = match mode {
                1 => {
                    let m = mass.min(self.gas[CO2]);
                    self.gas[CO2] -= m;
                    self.carbon += m * 12.0 / 44.0;
                    self.add_oxygen(m * 32.0 / 44.0);
                    m
                }
                2 => {
                    let m = mass.min(self.ocean);
                    self.ocean -= m;
                    self.gas[H2] += m / 9.0;
                    self.add_oxygen(m * 8.0 / 9.0);
                    m
                }
                3 => {
                    let m = mass.min(self.gas[CO2]);
                    self.gas[CO2] -= m;
                    self.carbonate += m;
                    m
                }
                _ => {
                    self.gas[PFC] += mass;
                    self.ground += mass;
                    mass
                }
            };
        }
        // --- the tales -------------------------------------------------------------------------
        if self.t > 373.0 && self.snap.ocean0 > 0.0 && self.ocean < 0.5 * self.snap.ocean0 {
            self.tell(
                TOLD_BOIL,
                "Les océans bouillent : l'emballement de la serre est irréversible.",
            );
        } else if self.t > self.snap.t_base + 25.0 && self.drift > 0.5 {
            self.tell(
                TOLD_RUNAWAY,
                "Emballement de serre : la vapeur d'eau amplifie le réchauffement.",
            );
        }
        if self.snap.ocean0 < 1.0 && self.ocean > 1.0e-6 * s_area * 1000.0 {
            self.tell(
                TOLD_OCEAN,
                "Premières mers : l'eau liquide reste en surface.",
            );
        }
        if self.ice_cover() > 0.9 && Self::ice_cover_at(self.snap.t_base) < 0.5 {
            self.tell(
                TOLD_FROZEN,
                "Glaciation totale : le gel gagne tout le globe.",
            );
        }
        if self.habitability() > 0.8 {
            self.tell(TOLD_BREATH, "L'air est respirable : on peut vivre dehors.");
        }
        handled / h
    }

    /// Shift the surface map of this world to its state now: temperature (poles first), sea
    /// level, humidity from the new sea and the vapour, then the biomes again.
    pub fn apply_to_map(&self, data: &GameData, map: &mut PlanetMap) {
        let s = &self.snap;
        let d_t = self.t - s.t_base;
        for y in 0..H {
            let amp = polar_amplification(Surface::lat(y));
            for x in 0..W {
                let i = Surface::idx(x, y);
                map.surface.temp[i] = (f64::from(map.surface.temp[i]) + d_t * amp).max(2.7) as f32;
            }
        }
        // Sea level: the threshold of elevation under which the area share equals the sea's.
        let share = self.sea_fraction();
        let old_elev = map.surface.elev.clone();
        if (share - s.sea0).abs() > 1e-4 {
            let mut order: Vec<usize> = (0..W * H).collect();
            order.sort_by(|&a, &b| old_elev[a].total_cmp(&old_elev[b]));
            let weight = |i: usize| Surface::lat(i / W).cos();
            let total: f64 = (0..W * H).map(weight).sum();
            let (mut acc, mut th) = (0.0, f64::from(old_elev[order[0]]) - 1e-6);
            if share > 0.0 {
                for &i in &order {
                    acc += weight(i);
                    th = f64::from(old_elev[i]);
                    if acc / total >= share {
                        break;
                    }
                }
            }
            for e in map.surface.elev.iter_mut() {
                let v = f64::from(*e);
                *e = if v < th {
                    -((th - v) / (th + 1.0).max(1e-6)).min(1.0)
                } else {
                    ((v - th) / (1.0 - th).max(1e-6)).min(1.0)
                } as f32;
            }
            map.surface.sea_frac =
                map.surface.elev.iter().filter(|&&e| e < 0.0).count() as f32 / (W * H) as f32;
            // Humidity: the moisture the new sea gives, in place of the old one.
            let wet0 = surface::profile(map.kind).map_or(0.0, |p| p.wet);
            let wet1 = (wet0 + self.wetness() - self.wetness_base()).clamp(0.0, 1.0);
            let near_old = blur_sea(&old_elev);
            let near_new = blur_sea(&map.surface.elev);
            for i in 0..W * H {
                let old = (0.15 + 0.85 * near_old[i]) * wet0;
                let new = (0.15 + 0.85 * near_new[i]) * wet1;
                map.surface.hum[i] =
                    (f64::from(map.surface.hum[i]) + new - old).clamp(0.0, 1.0) as f32;
            }
        } else {
            let d_wet = self.wetness() - self.wetness_base();
            if d_wet.abs() > 1e-3 {
                for h in map.surface.hum.iter_mut() {
                    *h = (f64::from(*h) + 0.4 * d_wet).clamp(0.0, 1.0) as f32;
                }
            }
        }
        map.pressure = self.pressure();
        for i in 0..W * H {
            if let Some(b) = data.classify_biome(
                f64::from(map.surface.temp[i]),
                f64::from(map.surface.hum[i]),
                f64::from(map.surface.elev[i]),
                map.pressure,
            ) {
                map.biome[i] = b as u8;
            }
        }
    }
}

/// Share of sea within six cells of each cell (wrapped in longitude): the moisture the sea gives.
fn blur_sea(elev: &[f32]) -> Vec<f64> {
    let r = 6isize;
    let mut out = vec![0.0f64; W * H];
    for y in 0..H {
        for x in 0..W {
            let (mut s, mut n) = (0.0f64, 0.0f64);
            for dy in -r..=r {
                let yy = y as isize + dy;
                if !(0..H as isize).contains(&yy) {
                    continue;
                }
                for dx in -r..=r {
                    let xx = (x as isize + dx).rem_euclid(W as isize) as usize;
                    s += f64::from(u8::from(elev[yy as usize * W + xx] < 0.0));
                    n += 1.0;
                }
            }
            out[y * W + x] = s / n;
        }
    }
    out
}

impl crate::Sim {
    /// The state of the terraforming of colony `ci` (None when its world has no climate).
    pub fn terra_of(&self, ci: usize) -> Option<&Terra> {
        self.colonies
            .get(ci)?
            .terra
            .as_ref()
            .filter(|t| t.snap.rocky)
    }

    /// The terraformed colony on body `body` of the system we are in, if any.
    pub fn terra_here(&self, body: usize) -> Option<&Terra> {
        self.colonies
            .iter()
            .find(|c| c.star == self.here && c.body == body)
            .and_then(|c| c.terra.as_ref())
            .filter(|t| t.snap.rocky && t.version > 0)
    }

    /// A number that changes when the surface of a terraformed world has changed enough for
    /// a view to rebuild its map (about every 30 days of game time).
    pub fn terra_stamp(&self) -> u64 {
        let sum: u64 = self
            .colonies
            .iter()
            .filter(|c| c.star == self.here)
            .filter_map(|c| c.terra.as_ref())
            .map(|t| (t.age / (30.0 * DAY)) as u64)
            .sum();
        sum
    }

    /// Put the air of terraformed colonies of this system into the bodies (pressure and
    /// composition: drag, sky, scale height and the map's pressure follow).
    pub(crate) fn sync_terra_air(&mut self) {
        let here = self.here;
        for c in self.colonies.iter().filter(|c| c.star == here) {
            let Some(t) = c.terra.as_ref().filter(|t| t.snap.rocky && t.version > 0) else {
                continue;
            };
            if let Some(b) = self.system.bodies.get_mut(c.body) {
                b.pressure = t.pressure();
                b.atmosphere = t.composition();
            }
        }
    }

    /// Choose what the process plants of colony `ci` do (an index of `PROCESSES`).
    pub fn colony_set_process(&mut self, ci: usize, mode: usize) -> Result<(), &'static str> {
        let c = self.colonies.get_mut(ci).ok_or("colonie inconnue")?;
        let t = c
            .terra
            .as_mut()
            .filter(|t| t.snap.rocky)
            .ok_or("ce monde n'a pas de climat")?;
        if mode >= PROCESSES.len() {
            return Err("procede inconnu");
        }
        t.process = mode as u8;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::planet::build_map;
    use std::sync::Arc;
    use vector_world::generate_system;

    pub(crate) fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    /// The first solid world (over many systems) that fits `pred`, with its map.
    pub(crate) fn find(
        d: &GameData,
        pred: impl Fn(&Terra, PlanetKind) -> bool,
    ) -> (Terra, PlanetMap) {
        for seed in 1..4000u64 {
            let sys = generate_system(seed);
            for i in 1..sys.bodies.len() {
                let (BodyKind::Planet(k) | BodyKind::Moon(k)) = sys.bodies[i].kind else {
                    continue;
                };
                if k.is_giant() {
                    continue;
                }
                let Some(map) = build_map(d, &sys, i) else {
                    continue;
                };
                let t = Terra::new(&sys, i, Some(&map));
                if pred(&t, k) {
                    return (t, map);
                }
            }
        }
        panic!("no such world");
    }

    fn kind_world(d: &GameData, kind: PlanetKind) -> Terra {
        find(d, |_, k| k == kind).0
    }

    /// An earthlike world: air with oxygen, liquid water, a temperate mean.
    pub(crate) fn earthlike(d: &GameData) -> (Terra, PlanetMap) {
        static CACHE: std::sync::OnceLock<(Terra, PlanetMap)> = std::sync::OnceLock::new();
        CACHE.get_or_init(|| earthlike_search(d)).clone()
    }

    fn earthlike_search(d: &GameData) -> (Terra, PlanetMap) {
        find(d, |t, k| {
            k == PlanetKind::Terran
                && (280.0..300.0).contains(&t.snap.t_base)
                && t.pressure_of(O2) > 0.1
                && t.snap.sea0 > 0.4
                && (1200.0..1450.0).contains(&t.snap.insolation)
                && t.vapour_gain() < 0.62
        })
    }

    fn run(t: &mut Terra, d: &GameData, years: f64, step_years: f64) {
        let n = (years / step_years).round() as usize;
        for _ in 0..n {
            t.step(d, step_years * YEAR, 0.0);
        }
    }

    #[test]
    fn a_world_left_alone_stays_what_it_is() {
        let d = data();
        for kind in [
            PlanetKind::Terran,
            PlanetKind::Ocean,
            PlanetKind::Desert,
            PlanetKind::Barren,
            PlanetKind::Venusian,
            PlanetKind::Tundra,
            PlanetKind::Ice,
        ] {
            let mut t = kind_world(&d, kind);
            let (p0, s0, l0) = (t.pressure(), t.sea_fraction(), t.closed_ledger());
            run(&mut t, &d, 300.0, 5.0);
            assert!(
                (t.t - t.snap.t_base).abs() < 0.6,
                "{kind:?}: {} vs {}",
                t.t,
                t.snap.t_base
            );
            assert!(
                (t.pressure() / p0.max(1e-9) - 1.0).abs() < 0.03 || p0 < 1e-3,
                "{kind:?}: {p0} -> {}",
                t.pressure()
            );
            assert!((t.sea_fraction() - s0).abs() < 0.02, "{kind:?}");
            assert!(
                (t.closed_ledger() / l0 - 1.0).abs() < 1e-9,
                "{kind:?}: ledger"
            );
        }
    }

    #[test]
    fn more_light_warms_and_a_greenhouse_gas_warms_but_an_inert_one_does_not() {
        let d = data();
        let (t0, _) = earthlike(&d);
        let base = t0.target_temperature();
        assert!(
            (base - t0.snap.t_base).abs() < 1e-6,
            "baseline balance is the baseline"
        );
        let mut lit = t0.clone();
        lit.mirror = 0.05 * std::f64::consts::PI * lit.snap.radius.powi(2) / 0.85;
        assert!(
            lit.target_temperature() > base + 1.0,
            "5 % more light warms"
        );
        let mass = 0.2 * t0.gas.iter().sum::<f64>();
        let mut co2 = t0.clone();
        co2.gas[CO2] += mass;
        let mut n2 = t0.clone();
        n2.gas[N2] += mass;
        assert!(co2.target_temperature() > base + 2.0, "CO2 traps heat");
        assert!(
            (n2.target_temperature() - base).abs() < 1e-6,
            "nitrogen is transparent"
        );
        // Soot on the ice darkens it.
        let mut dark = t0.clone();
        dark.soot = 1.0e5 * dark.snap.area() * 1.0e-6;
        assert!(dark.albedo() < t0.albedo());
        // More ice reflects more.
        let mut icy = t0.clone();
        icy.t -= 15.0;
        assert!(
            icy.albedo() > t0.albedo() + 0.02,
            "a colder world has more ice"
        );
    }

    #[test]
    fn a_moist_greenhouse_runs_away_and_the_oceans_boil() {
        let d = data();
        let (t0, _) = earthlike(&d);
        let area = std::f64::consts::PI * t0.snap.radius.powi(2) / 0.85;
        let mut gentle = t0.clone();
        gentle.mirror = 0.02 * area;
        run(&mut gentle, &d, 2000.0, 5.0);
        assert!(
            gentle.t < t0.snap.t_base + 15.0,
            "a little more light settles: {}",
            gentle.t
        );
        assert!(gentle.ocean > 0.9 * t0.snap.ocean0, "the oceans stay");
        let mut hot = t0.clone();
        hot.mirror = 0.4 * area;
        run(&mut hot, &d, 500.0, 5.0);
        assert!(hot.t > 373.0, "runaway: {}", hot.t);
        assert!(hot.ocean < 0.5 * t0.snap.ocean0, "the oceans boil away");
        assert!(hot.status().contains("EMBALLEMENT"), "{}", hot.status());
        assert_eq!(hot.habitability(), 0.0);
        assert!(hot
            .notes
            .iter()
            .any(|n| n.contains("bouillent") || n.contains("Emballement")));
        // Over the millennia the steam is split by the light: the hydrogen leaves, the oxygen
        // is swallowed by the rock, and the world ends up dry (Venus).
        let water0 = t0.snap.ice0 + t0.snap.ocean0 + t0.gas[H2O];
        run(&mut hot, &d, 20_000.0, 10.0);
        assert!(
            hot.ocean < 0.05 * t0.snap.ocean0,
            "dry: {}",
            hot.ocean / t0.snap.ocean0
        );
        assert!(
            hot.escaped > 0.05 * water0,
            "hydrogen lost: {}",
            hot.escaped / water0
        );
        let o2_made = hot.gas[O2] + hot.sink_o2 - (t0.gas[O2] + t0.sink_o2);
        assert!(
            o2_made > 0.1 * water0,
            "the oxygen of the water stays behind: {}",
            o2_made / water0
        );
        assert!((hot.closed_ledger() / t0.closed_ledger() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn a_dimming_star_freezes_the_world_through_the_ice_albedo() {
        let d = data();
        let (t0, _) = earthlike(&d);
        let mut dim = t0.clone();
        dim.snap.insolation *= 0.62;
        run(&mut dim, &d, 4000.0, 5.0);
        assert!(dim.t < 240.0, "snowball: {}", dim.t);
        assert!(
            dim.ice_cover() > 0.9,
            "ice covers the globe: {}",
            dim.ice_cover()
        );
        assert!(dim.notes.iter().any(|n| n.contains("Glaciation")));
        let mut slight = t0.clone();
        slight.snap.insolation *= 0.98;
        run(&mut slight, &d, 4000.0, 5.0);
        assert!(
            slight.t > 270.0,
            "a slightly dimmer star does not: {}",
            slight.t
        );
    }

    #[test]
    fn frozen_carbon_dioxide_makes_warming_feed_itself() {
        let d = data();
        let (cold, _) = find(&d, |t, _| {
            t.snap.co2res0 > 0.0
                && t.snap.t_base < 240.0
                && t.snap.t_base > 150.0
                && t.snap.ice0 > 0.0
        });
        let area = std::f64::consts::PI * cold.snap.radius.powi(2) / 0.85;
        let mut with = cold.clone();
        with.mirror = 0.3 * area;
        let mut without = with.clone();
        without.co2res = 0.0;
        without.snap.co2res0 = 0.0;
        run(&mut with, &d, 600.0, 2.0);
        run(&mut without, &d, 600.0, 2.0);
        assert!(
            with.pressure_of(CO2) > 3.0 * without.pressure_of(CO2).max(1e-6),
            "the caps give their carbon dioxide: {} vs {}",
            with.pressure_of(CO2),
            without.pressure_of(CO2)
        );
        assert!(
            with.t > without.t + 1.0,
            "and it warms: {} vs {}",
            with.t,
            without.t
        );
        assert!((with.closed_ledger() / cold.closed_ledger() - 1.0).abs() < 1e-9);
    }

    /// The producer that fits the climate of this world best.
    pub(crate) fn best_producer(d: &GameData, t: &Terra) -> usize {
        (0..d.species.len())
            .filter(|&i| d.species[i].is_producer())
            .max_by(|&a, &b| t.suitability(d, a).0.total_cmp(&t.suitability(d, b).0))
            .expect("producer")
    }

    pub(crate) fn any_producer(d: &GameData) -> usize {
        (0..d.species.len())
            .find(|&i| d.species[i].is_producer())
            .expect("producer")
    }

    /// A temperate world with a thick carbon dioxide air and no free oxygen (a dimmer light
    /// balances the greenhouse): the starting point of a biological terraforming.
    fn thick_co2_world(d: &GameData) -> Terra {
        let (mut t, _) = earthlike(d);
        let o2 = t.gas[O2];
        t.gas[N2] += o2;
        t.gas[O2] = 0.0;
        t.sink_o2 = 0.0;
        t.gas[CO2] += 0.03 * 1.0e5 * t.snap.area() / t.snap.gravity;
        t.snap.insolation *= 0.65;
        run(&mut t, d, 300.0, 5.0);
        t
    }

    #[test]
    fn seeded_producers_turn_carbon_dioxide_into_oxygen_over_millennia() {
        let d = data();
        let mut t = thick_co2_world(&d);
        assert!(t.t > 270.0 && t.t < 310.0, "{}", t.t);
        let sp = best_producer(&d, &t);
        assert!(t.suitability(&d, sp).0 > 0.1, "a fit strain");
        let start = t.closed_ledger();
        t.seed(&d, sp, 1.0e6);
        assert_eq!(t.seeds.len(), 1);
        let c0 = t.seeds[0].cover;
        assert!(c0 > 0.0 && c0 < 1e-6, "a founding population is tiny: {c0}");
        let co2_0 = t.gas[CO2];
        run(&mut t, &d, 40_000.0, 10.0);
        assert!(t.seeds[0].cover > 0.5, "it has spread over the world");
        assert!(
            t.pressure_of(O2) > 0.01,
            "oxygen builds up: {}",
            t.pressure_of(O2)
        );
        assert!(t.gas[CO2] < co2_0, "carbon dioxide is consumed");
        assert!(t.carbon > 0.0);
        // Mass: the carbon buried and the oxygen made (in the air and in the crust) match.
        let o2_made = t.gas[O2] + t.sink_o2;
        assert!(
            (o2_made * 12.0 / 32.0 / t.carbon - 1.0).abs() < 0.01,
            "{o2_made} vs {}",
            t.carbon
        );
        assert!((t.closed_ledger() / start - 1.0).abs() < 1e-9);
        // It takes long: after a century nothing measurable has happened.
        let mut early = thick_co2_world(&d);
        early.seed(&d, sp, 1.0e6);
        run(&mut early, &d, 100.0, 10.0);
        assert!(early.pressure_of(O2) < 1e-4, "{}", early.pressure_of(O2));
    }

    #[test]
    fn a_strain_dies_where_the_climate_does_not_suit_it() {
        let d = data();
        let (mut t, _) = find(&d, |t, _| t.snap.t_base < 200.0 && t.snap.rocky);
        let sp = any_producer(&d);
        t.seed(&d, sp, 1.0e9);
        run(&mut t, &d, 60.0, 1.0);
        assert!(t.seeds.is_empty(), "extinct in the cold");
        assert!(t.notes.iter().any(|n| n.contains("Extinction")));
    }

    #[test]
    fn comets_bring_their_mass_and_a_heat_that_fades() {
        let d = data();
        let (mut t, _) = find(&d, |t, _| {
            t.snap.t_base < 240.0 && t.snap.ice0 > 0.0 && t.snap.co2res0 > 0.0
        });
        let start = t.closed_ledger();
        let (water0, t0) = (t.ice + t.ocean, t.t);
        t.deliver(&[("H2O".to_string(), 5.0e14)]);
        assert!(t.t > t0 + 0.5, "the impact heats: {} -> {}", t0, t.t);
        let heated = t.t;
        assert!((t.ice + t.ocean) - water0 > 4.99e14);
        assert!(
            (t.ledger() - start - 5.0e14).abs() < 1e6,
            "the ledger takes the delivery"
        );
        assert!((t.closed_ledger() / start - 1.0).abs() < 1e-12);
        run(&mut t, &d, 30.0, 0.5);
        assert!(
            t.t < heated - 0.5 * (heated - t0),
            "the heat dissipates: {} -> {}",
            heated,
            t.t
        );
        // Ammonia decays into nitrogen and hydrogen over decades.
        t.deliver(&[("NH3".to_string(), 1.0e14)]);
        let nh3 = t.gas[NH3];
        let n2 = t.gas[N2];
        run(&mut t, &d, 200.0, 1.0);
        assert!(
            t.gas[NH3] < 0.05 * nh3.max(1.0) + 1.0 && t.gas[N2] > n2,
            "ammonia becomes nitrogen"
        );
        assert!((t.closed_ledger() / start - 1.0).abs() < 1e-9);
    }

    #[test]
    fn every_process_moves_matter_and_the_ledger_balances() {
        let d = data();
        let (mut t, _) = earthlike(&d);
        let start = t.closed_ledger();
        let sp = best_producer(&d, &t);
        t.seed(&d, sp, 1.0e9);
        t.deliver(&[("CO2".to_string(), 1.0e15), ("H2O".to_string(), 1.0e15)]);
        for (mode, name) in PROCESSES.iter().enumerate().skip(1) {
            t.process = mode as u8;
            for _ in 0..40 {
                t.step(&d, 0.5 * YEAR, 1.0e12);
            }
            assert!(t.rate > 0.0, "{name}");
        }
        assert!(t.carbonate > 0.0 && t.carbon > 0.0 && t.ground > 0.0);
        let expected = start + t.delivered + t.ground;
        assert!((t.ledger() / expected - 1.0).abs() < 1e-9);
        // Each plant handles what its power allows: twice the power, twice the matter.
        let mut a = earthlike(&d).0;
        a.gas[CO2] += 1.0e17;
        let mut b = a.clone();
        a.process = 1;
        b.process = 1;
        a.step(&d, 100.0 * DAY, 1.0e11);
        b.step(&d, 100.0 * DAY, 2.0e11);
        assert!(
            (b.carbon / a.carbon - 2.0).abs() < 0.05,
            "{} vs {}",
            a.carbon,
            b.carbon
        );
        let m = 1.0e11 * 100.0 * DAY * PROCESS_EFFICIENCY / PROCESS_ENERGY[1];
        assert!(
            (a.carbon / (m * 12.0 / 44.0) - 1.0).abs() < 0.05,
            "energy per kg"
        );
    }

    #[test]
    fn light_gases_leave_a_small_world_and_heavy_ones_stay() {
        let d = data();
        let (mut small, _) = earthlike(&d);
        small.snap.escape = 3000.0;
        small.t = 300.0;
        small.gas[H2] += 1.0e15;
        small.gas[CO2] += 1.0e15;
        let co2 = small.gas[CO2];
        let mut big = small.clone();
        big.snap.escape = 11_000.0;
        run(&mut small, &d, 1000.0, 10.0);
        run(&mut big, &d, 1000.0, 10.0);
        assert!(
            small.gas[H2] < 0.2e15,
            "hydrogen escapes a small world: {}",
            small.gas[H2]
        );
        assert!(big.gas[H2] > 0.95e15, "but not a big one: {}", big.gas[H2]);
        assert!(small.gas[CO2] > 0.9 * co2, "carbon dioxide stays");
        assert!(small.escaped > 0.8e15);
    }

    #[test]
    fn the_map_follows_the_climate_and_the_state_is_small() {
        let d = data();
        let (t0, map0) = earthlike(&d);
        // At the baseline the map is exactly the seed's.
        let mut same = map0.clone();
        t0.apply_to_map(&d, &mut same);
        assert_eq!(same.surface.temp, map0.surface.temp);
        assert_eq!(same.surface.elev, map0.surface.elev);
        assert_eq!(same.biome, map0.biome);
        // Warmer, with a risen sea: hotter cells, more sea, other biomes.
        let mut t = t0.clone();
        t.t += 8.0;
        t.ocean += 0.15 * t0.ocean;
        t.ice -= 0.5 * t.ice;
        let mut warm = map0.clone();
        t.apply_to_map(&d, &mut warm);
        // Mean over the sphere: rows weigh as the cosine of their latitude.
        let mean = |m: &PlanetMap| {
            let (mut s, mut w) = (0.0, 0.0);
            for y in 0..H {
                for x in 0..W {
                    let c = Surface::lat(y).cos();
                    s += c * f64::from(m.surface.temp[Surface::idx(x, y)]);
                    w += c;
                }
            }
            s / w
        };
        assert!(
            (mean(&warm) - mean(&map0) - 8.0).abs() < 1.0,
            "{} vs {}",
            mean(&warm),
            mean(&map0)
        );
        let sea = |m: &PlanetMap| m.surface.elev.iter().filter(|&&e| e < 0.0).count();
        assert!(sea(&warm) > sea(&map0), "the sea rose");
        let changed = (0..W * H)
            .filter(|&i| warm.biome[i] != map0.biome[i])
            .count();
        assert!(changed > 50, "{changed} cells changed biome");
        // The polar cells warm more than the equatorial ones.
        let row = |m: &PlanetMap, y: usize| {
            (0..W)
                .map(|x| f64::from(m.surface.temp[Surface::idx(x, y)]))
                .sum::<f64>()
        };
        assert!(row(&warm, 0) - row(&map0, 0) > row(&warm, H / 2) - row(&map0, H / 2));
        // What is saved is small: the deltas and the baseline summary, never the map.
        let bytes = postcard::to_allocvec(&t).expect("encode");
        assert!(bytes.len() < 3000, "{} bytes", bytes.len());
        let back: Terra = postcard::from_bytes(&bytes).expect("decode");
        assert_eq!(back, t);
    }

    #[test]
    fn the_same_inputs_give_the_same_world() {
        let d = data();
        let (t0, _) = earthlike(&d);
        let go = || {
            let mut t = t0.clone();
            t.mirror = 2.0e12;
            t.seed(&d, best_producer(&d, &t0), 1.0e8);
            t.deliver(&[("H2O".to_string(), 1.0e14)]);
            t.process = 3;
            for _ in 0..200 {
                t.step(&d, 3.0 * YEAR, 5.0e10);
            }
            t
        };
        assert_eq!(go(), go());
    }

    #[test]
    fn people_can_live_outdoors_only_with_the_right_air_and_climate() {
        let d = data();
        let (t0, _) = earthlike(&d);
        assert!(
            t0.habitability() > 0.5,
            "an earthlike world is fit: {}",
            t0.habitability()
        );
        let mut no_o2 = t0.clone();
        no_o2.gas[N2] += no_o2.gas[O2];
        no_o2.gas[O2] = 0.0;
        assert_eq!(no_o2.habitability(), 0.0);
        let mut co2 = t0.clone();
        co2.gas[CO2] += 0.05 * 1.0e5 * co2.snap.area() / co2.snap.gravity;
        assert!(co2.habitability() < 0.05);
        let mut cold = t0.clone();
        cold.t = 250.0;
        assert_eq!(cold.habitability(), 0.0);
        let mut toxic = t0.clone();
        toxic.gas[NH3] += 0.01 * 1.0e5 * toxic.snap.area() / toxic.snap.gravity;
        assert_eq!(toxic.habitability(), 0.0);
        // The table has every gas of the generated atmospheres.
        for g in [
            "N2", "O2", "CO2", "CH4", "H2O", "Ar", "H2", "He", "NH3", "SO2", "CO",
        ] {
            assert!(gas_index(g).is_some(), "{g}");
        }
    }
}
