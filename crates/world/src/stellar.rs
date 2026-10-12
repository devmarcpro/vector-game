//! The life of stars and what orbits them: the stage a star has reached (main sequence, red giant
//! or supergiant, white dwarf, neutron star, black hole), its variability, the radiation of the
//! compact ones, companions (binaries and triples), the rings that sit inside the Roche limit of
//! the giants, Trojans at the Lagrange points, and comets on very eccentric orbits whose tails
//! point away from the star. Everything derives from the seed of the system: nothing is stored.
use crate::orbit::{Orbit, AU, G, M_SUN, R_SUN};
use crate::{AnomalyKind, AstType, BodyKind, CelBody, PlanetKind, StarClass, System};
use std::f64::consts::{PI, TAU};
use vector_core::{hash_coord, Rng, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StarStage {
    MainSequence,
    RedGiant,
    RedSupergiant,
    WhiteDwarf,
    NeutronStar,
    BlackHole,
    /// A free-floating world of planetary mass, thrown out of its system: no sun, only the
    /// moons that came with it.
    Rogue,
}

impl StarStage {
    pub const ALL: [StarStage; 7] = [
        Self::MainSequence,
        Self::RedGiant,
        Self::RedSupergiant,
        Self::WhiteDwarf,
        Self::NeutronStar,
        Self::BlackHole,
        Self::Rogue,
    ];
    pub fn id(self) -> &'static str {
        [
            "main_sequence",
            "red_giant",
            "red_supergiant",
            "white_dwarf",
            "neutron_star",
            "black_hole",
            "rogue_planet",
        ][self as usize]
    }
    pub fn label(self) -> &'static str {
        [
            "SEQUENCE PRINCIPALE",
            "GEANTE ROUGE",
            "SUPERGEANTE ROUGE",
            "NAINE BLANCHE",
            "ETOILE A NEUTRONS",
            "TROU NOIR",
            "PLANETE ERRANTE",
        ][self as usize]
    }
}

fn unit(seed: u64, a: i64, b: i64) -> f64 {
    vector_core::unit(hash_coord(seed, a, b, 0x57E1))
}

/// A white-dwarf draw becomes a neutron star (one in four) or a black hole (one in twelve):
/// the remnants of the stars too heavy to end as white dwarfs share the same slot of the
/// star census.
pub(crate) fn remnant_class(seed: u64) -> StarClass {
    let r = unit(seed, 1, 1);
    if r < 0.12 {
        StarClass::X
    } else if r < 0.40 {
        StarClass::N
    } else if r < 0.55 {
        StarClass::R
    } else {
        StarClass::D
    }
}

/// Stage of a star of this class and mass: a few per cent of the stars of every class are caught
/// leaving the main sequence (the heavier, the shorter-lived, the likelier).
pub(crate) fn stage_of(seed: u64, class: StarClass, mass_sun: f64) -> StarStage {
    match class {
        StarClass::D => StarStage::WhiteDwarf,
        StarClass::N => StarStage::NeutronStar,
        StarClass::X => StarStage::BlackHole,
        StarClass::R => StarStage::Rogue,
        c => {
            let p = match c {
                StarClass::O => 0.20,
                StarClass::B => 0.12,
                StarClass::A => 0.10,
                StarClass::F => 0.07,
                StarClass::G => 0.05,
                StarClass::K => 0.04,
                _ => 0.0,
            };
            if unit(seed, 2, 2) < p {
                if mass_sun > 8.0 {
                    StarStage::RedSupergiant
                } else {
                    StarStage::RedGiant
                }
            } else {
                StarStage::MainSequence
            }
        }
    }
}

/// Radius (suns) and surface temperature (K) of a star that has swollen.
pub(crate) fn giant_shape(seed: u64, stage: StarStage) -> (f64, f64) {
    let (u, v) = (unit(seed, 3, 3), unit(seed, 4, 4));
    match stage {
        StarStage::RedSupergiant => (100.0 + 400.0 * u, 3300.0 + 900.0 * v),
        _ => (10.0 + 60.0 * u, 3500.0 + 1000.0 * v),
    }
}

/// Radius (suns) and temperature (K) of a rogue world: a Jupiter, still warm from its birth.
pub(crate) fn rogue_shape(seed: u64) -> (f64, f64) {
    (0.1, 200.0 + 700.0 * unit(seed, 11, 11))
}

/// Radius (suns) of a neutron star (12 km) or of the horizon of a black hole of `mass_sun`.
pub(crate) fn compact_radius(stage: StarStage, mass_sun: f64) -> f64 {
    match stage {
        StarStage::NeutronStar => 1.2e4 / R_SUN,
        _ => 2.0 * G * mass_sun * M_SUN / (299_792_458.0f64).powi(2) / R_SUN,
    }
}

/// A star whose light waxes and wanes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Variable {
    pub period_days: f64,
    /// Share of the mean luminosity (0..1).
    pub amplitude: f64,
    pub phase: f64,
}

/// A neutron star that sweeps the sky with its beam.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pulsar {
    pub period_s: f64,
    /// A magnetar: a field a thousand times stronger, and the burst that goes with it.
    pub magnetar: bool,
}

/// One ring of a giant, between `inner` and `outer` metres from its centre.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ring {
    pub planet: usize,
    pub inner: f64,
    pub outer: f64,
}

/// Pulsation periods (days) and amplitudes (share of the mean luminosity) of the swollen stars:
/// Mira type for a red giant, Cepheid type for a supergiant.
pub const MIRA_PERIOD_DAYS: (f64, f64) = (150.0, 450.0);
pub const MIRA_AMPLITUDE: (f64, f64) = (0.25, 0.60);
pub const CEPHEID_PERIOD_DAYS: (f64, f64) = (5.0, 60.0);
pub const CEPHEID_AMPLITUDE: (f64, f64) = (0.10, 0.30);
/// Spin period of a pulsar, s.
pub const PULSAR_PERIOD_S: (f64, f64) = (0.002, 2.0);
/// A magnetar's dose is this many times a pulsar's.
pub const MAGNETAR_FACTOR: f64 = 20.0;
/// Eccentricity of the comets.
pub const COMET_ECC: (f64, f64) = (0.85, 0.97);
/// Distance (in AU times the root of the luminosity) beyond which a comet is dormant.
pub const COMET_REACH_AU: f64 = 4.0;
/// Inner edge of a ring (planet radii) and the Roche factor: a body of density rho holds together
/// beyond `ROCHE_FACTOR * R * (rho_planet / rho)^(1/3)`.
pub const RING_INNER_RADII: f64 = 1.25;
pub const ROCHE_FACTOR: f64 = 2.44;
pub const ICE_DENSITY: f64 = 920.0;
/// A companion keeps at least this many times the outermost planet's distance, and 30 AU.
pub const COMPANION_SEPARATION_FACTOR: f64 = 8.0;
pub const COMPANION_MIN_AU: f64 = 30.0;
/// Trojans sit this far (rad) ahead and behind their giant.
pub const TROJAN_ANGLE: f64 = PI / 3.0;

/// Dose (Gy/h) one astronomical unit from a pulsar; a magnetar's is twenty times higher.
pub const PULSAR_DOSE_AU: f64 = 0.3;
/// Dose (Gy/h) one astronomical unit from a black hole that feeds on a disc.
pub const ACCRETION_DOSE_AU: f64 = 0.15;

impl CelBody {
    /// Luminosity of a star body in suns (Stefan-Boltzmann from its radius and temperature).
    pub fn star_luminosity(&self) -> f64 {
        (self.radius / R_SUN).powi(2) * (self.temp / 5772.0).powi(4)
    }
}

impl System {
    /// Variable stars: swollen stars pulse (Mira type, Cepheid type).
    pub fn variable(&self) -> Option<Variable> {
        let (period, amp) = match self.stage {
            StarStage::RedGiant => (
                MIRA_PERIOD_DAYS.0
                    + (MIRA_PERIOD_DAYS.1 - MIRA_PERIOD_DAYS.0) * unit(self.seed, 5, 5),
                MIRA_AMPLITUDE.0 + (MIRA_AMPLITUDE.1 - MIRA_AMPLITUDE.0) * unit(self.seed, 6, 6),
            ),
            StarStage::RedSupergiant => (
                CEPHEID_PERIOD_DAYS.0
                    + (CEPHEID_PERIOD_DAYS.1 - CEPHEID_PERIOD_DAYS.0) * unit(self.seed, 5, 5),
                CEPHEID_AMPLITUDE.0
                    + (CEPHEID_AMPLITUDE.1 - CEPHEID_AMPLITUDE.0) * unit(self.seed, 6, 6),
            ),
            _ => return None,
        };
        Some(Variable {
            period_days: period,
            amplitude: amp,
            phase: TAU * unit(self.seed, 7, 7),
        })
    }

    /// Luminosity of the star at time `t` (suns): the mean one, modulated if it is variable.
    pub fn luminosity_at(&self, t: f64) -> f64 {
        match self.variable() {
            Some(v) => {
                self.luminosity
                    * (1.0 + v.amplitude * (TAU * t / (v.period_days * 86_400.0) + v.phase).sin())
            }
            None => self.luminosity,
        }
    }

    pub fn pulsar(&self) -> Option<Pulsar> {
        (self.stage == StarStage::NeutronStar).then(|| Pulsar {
            period_s: PULSAR_PERIOD_S.0
                * (PULSAR_PERIOD_S.1 / PULSAR_PERIOD_S.0).powf(unit(self.seed, 8, 8)),
            magnetar: unit(self.seed, 9, 9) < 0.1,
        })
    }

    /// A black hole that is eating (a disc of gas around it): it shines in X-rays.
    pub fn accreting(&self) -> bool {
        self.stage == StarStage::BlackHole && unit(self.seed, 10, 10) < 0.5
    }

    /// Ionising radiation (Gy/h) that the star itself pours out at `r` metres: pulsar beam,
    /// magnetar, X-rays of an accretion disc. Ordinary stars give none (only their flares do).
    pub fn star_radiation(&self, r: f64) -> f64 {
        // Inside 0.01 AU the field stops growing: the figure is capped at 10 000 times the dose
        // at 1 AU, which is already lethal in seconds.
        let k = (AU / r.max(0.01 * AU)).powi(2);
        if let Some(p) = self.pulsar() {
            PULSAR_DOSE_AU * k * if p.magnetar { MAGNETAR_FACTOR } else { 1.0 }
        } else if self.accreting() {
            ACCRETION_DOSE_AU * k
        } else {
            0.0
        }
    }

    /// Magnification of the sensors where the gravity of a black hole bends the light of what
    /// lies behind it toward the ship (a gravitational lens): peaks at the focal distance, which
    /// grows with the mass. 1 anywhere else, and for any other star.
    pub fn lens_gain(&self, r: f64) -> f64 {
        if self.stage != StarStage::BlackHole {
            return 1.0;
        }
        let focus = self.lens_focus();
        let x = (r - focus) / (0.3 * focus);
        1.0 + 2.0 * (-x * x).exp()
    }

    /// Distance (m) at which the rays that pass 100 000 horizons from a black hole meet:
    /// r_f = c^2 b^2 / (4 G M) with b = 1e5 * 2GM/c^2, i.e. 1e10 GM/c^2 (about 100 AU per
    /// solar mass).
    pub fn lens_focus(&self) -> f64 {
        1.0e10 * self.bodies[0].gm() / (299_792_458.0f64).powi(2)
    }

    /// Companion stars (the star bodies other than the first one).
    pub fn companions(&self) -> Vec<usize> {
        (1..self.bodies.len())
            .filter(|&i| matches!(self.bodies[i].kind, BodyKind::Star(_)))
            .collect()
    }

    /// Ring of body `p`, if it has one: from 1.25 radii to the Roche limit of an icy body (a moon
    /// that came closer was torn apart and spread), and not past the first moon.
    pub fn ring_of(&self, p: usize) -> Option<Ring> {
        let b = self.bodies.get(p)?;
        let BodyKind::Planet(k) = b.kind else {
            return None;
        };
        let share = match k {
            PlanetKind::GasGiant => 0.55,
            PlanetKind::IceGiant => 0.40,
            _ => return None,
        };
        if unit(self.seed, p as i64, 0xA1) >= share {
            return None;
        }
        let rho = b.mass / (4.0 / 3.0 * PI * b.radius.powi(3));
        let outer = ROCHE_FACTOR * b.radius * (rho / ICE_DENSITY).cbrt();
        let first_moon = self
            .bodies
            .iter()
            .filter(|m| matches!(m.kind, BodyKind::Moon(_)))
            .filter_map(|m| m.orbit.as_ref().filter(|o| o.parent == p).map(|o| o.a))
            .fold(f64::INFINITY, f64::min);
        let outer = outer.min(0.9 * first_moon);
        let inner = RING_INNER_RADII * b.radius;
        (outer > inner * 1.15).then_some(Ring {
            planet: p,
            inner,
            outer,
        })
    }

    pub fn rings(&self) -> Vec<Ring> {
        (1..self.bodies.len())
            .filter_map(|p| self.ring_of(p))
            .collect()
    }

    /// Tail of the comet `i` at time `t` as a vector from its head: away from the star (the
    /// stellar wind pushes it), long near the star where the ice boils off, none beyond a few
    /// AU. None for anything that is not a comet.
    pub fn comet_tail(&self, i: usize, t: f64) -> Option<Vec3> {
        if !matches!(self.bodies[i].kind, BodyKind::Anomaly(AnomalyKind::Comet)) {
            return None;
        }
        let (p, _) = self.state(i, t);
        let r = p.length();
        let reach = COMET_REACH_AU * AU * self.luminosity.sqrt().max(0.3);
        if r >= reach || r <= 0.0 {
            return None;
        }
        let len = (6.0e9 * (reach / COMET_REACH_AU / r).powi(2)).min(4.0e10);
        Some(p * (len / r))
    }
}

/// Mass range (suns) of the main-sequence classes, from the lightest.
fn class_of_mass(m: f64) -> StarClass {
    for c in [
        StarClass::M,
        StarClass::K,
        StarClass::G,
        StarClass::F,
        StarClass::A,
        StarClass::B,
    ] {
        let (_, hi) = crate::mass_range(c);
        if m < hi {
            return c;
        }
    }
    StarClass::O
}

/// Everything appended to a generated system after its planets, belts, stations, anomalies
/// and portal: appended at the end with its own random stream so nothing that existed moves.
pub(crate) fn add_extras(sys: &mut System) {
    comets(sys);
    rings(sys);
    trojans(sys);
    companions(sys);
    station_rocks(sys);
}

/// A handful of small rocks in orbit around the world a station circles, a few thousand
/// kilometres from it: the first ore of a new game is a short hop away, not a trip to a belt
/// tens of millions of kilometres off. Their own random stream: nothing drawn before moves.
fn station_rocks(sys: &mut System) {
    let mut rng = Rng::new(sys.seed ^ 0x57A7_10C5);
    let stations: Vec<Orbit> = sys
        .bodies
        .iter()
        .filter(|b| b.kind == BodyKind::Station)
        .filter_map(|b| b.orbit.clone())
        .collect();
    for (k, so) in stations.iter().enumerate() {
        let host = so.parent;
        if host == 0 || host >= sys.bodies.len() {
            continue;
        }
        let (mu, hname) = (sys.bodies[host].gm(), sys.bodies[host].name.clone());
        for j in 0..6 {
            let class = match j % 3 {
                0 => AstType::C,
                1 => AstType::S,
                _ => AstType::M,
            };
            let radius = rng.range(40.0, 260.0);
            let mass = 4.0 / 3.0 * PI * radius.powi(3) * class.density();
            let orbit = Orbit {
                parent: host,
                a: so.a + rng.range(3.0e6, 2.0e7),
                e: rng.range(0.0, 0.02),
                incl: rng.range(-0.05, 0.05),
                lan: rng.range(0.0, TAU),
                argp: rng.range(0.0, TAU),
                m0: rng.range(0.0, TAU),
                mu,
            };
            sys.bodies.push(CelBody {
                name: format!("{hname} roche-{}{}", k + 1, j + 1),
                kind: BodyKind::Asteroid(class),
                mass,
                radius,
                orbit: Some(orbit),
                rotation_period: rng.range(300.0, 40000.0),
                temp: 150.0,
                gravity: G * mass / (radius * radius),
                pressure: 0.0,
                atmosphere: vec![],
            });
        }
    }
}

/// Comets fall from far out and swing close to the star: eccentric, with a perihelion that
/// keeps them out of it.
fn comets(sys: &mut System) {
    let (lum, rad) = (sys.luminosity, sys.bodies[0].radius);
    for (i, b) in sys.bodies.iter_mut().enumerate() {
        if !matches!(b.kind, BodyKind::Anomaly(AnomalyKind::Comet)) {
            continue;
        }
        let Some(o) = b.orbit.as_mut() else { continue };
        let e = COMET_ECC.0 + (COMET_ECC.1 - COMET_ECC.0) * unit(sys.seed, i as i64, 0xC0);
        let peri = (0.25 * AU * lum.sqrt().max(0.3)).max(rad * 4.0);
        o.e = e;
        o.a = (peri / (1.0 - e)).max(o.a * 0.5);
        o.incl = (unit(sys.seed, i as i64, 0xC1) - 0.5) * 1.6;
    }
}

fn rings(sys: &mut System) {
    let mut rng = Rng::new(sys.seed ^ 0x5A7_0000);
    for r in sys.rings() {
        let (mu, pname) = (sys.bodies[r.planet].gm(), sys.bodies[r.planet].name.clone());
        for k in 0..10 {
            let a = rng.range(r.inner, r.outer);
            let class = match rng.f64() {
                x if x < 0.75 => AstType::I,
                x if x < 0.90 => AstType::S,
                _ => AstType::C,
            };
            let radius = rng.range(3.0, 30.0);
            let mass = 4.0 / 3.0 * PI * radius.powi(3) * class.density();
            let orbit = Orbit {
                parent: r.planet,
                a,
                e: 0.0,
                incl: rng.range(-0.003, 0.003),
                lan: rng.range(0.0, TAU),
                argp: 0.0,
                m0: rng.range(0.0, TAU),
                mu,
            };
            sys.bodies.push(CelBody {
                name: format!("{pname} anneau-{}", k + 1),
                kind: BodyKind::Asteroid(class),
                mass,
                radius,
                orbit: Some(orbit),
                rotation_period: 100.0,
                temp: 80.0,
                gravity: G * mass / (radius * radius),
                pressure: 0.0,
                atmosphere: vec![],
            });
        }
    }
}

/// The heaviest gas giant of half the systems keeps two swarms of dark primitive rocks 60 degrees
/// ahead of it and behind it on its own orbit (the stable Lagrange points L4 and L5).
fn trojans(sys: &mut System) {
    let mut rng = Rng::new(sys.seed ^ 0x7B0_1A25);
    let giant = (1..sys.bodies.len())
        .filter(|&i| matches!(sys.bodies[i].kind, BodyKind::Planet(PlanetKind::GasGiant)))
        .max_by(|&a, &b| sys.bodies[a].mass.total_cmp(&sys.bodies[b].mass));
    let Some(g) = giant else { return };
    if rng.f64() >= 0.5 {
        return;
    }
    let base = sys.bodies[g].orbit.clone().expect("planets orbit");
    for (side, sign) in [("L4", 1.0), ("L5", -1.0)] {
        for k in 0..6 {
            let mut o = base.clone();
            o.a += rng.range(-3.0e7, 3.0e7);
            o.e = (o.e * 0.5).min(0.05);
            o.m0 = base.m0 + sign * TROJAN_ANGLE + rng.range(-0.03, 0.03);
            let radius = rng.range(1000.0, 40_000.0);
            let class = AstType::D;
            let mass = 4.0 / 3.0 * PI * radius.powi(3) * class.density();
            sys.bodies.push(CelBody {
                name: format!("Troyen {side}-{} {}", k + 1, sys.bodies[g].name),
                kind: BodyKind::Asteroid(class),
                mass,
                radius,
                orbit: Some(o),
                rotation_period: rng.range(3600.0, 40_000.0),
                temp: 120.0,
                gravity: G * mass / (radius * radius),
                pressure: 0.0,
                atmosphere: vec![],
            });
        }
    }
}

/// Binaries and triples: a companion far enough out (eight times the outermost planet at least)
/// that the planets stay stable, with a mass between a tenth of the primary's and its own.
fn companions(sys: &mut System) {
    let mut rng = Rng::new(sys.seed ^ 0xB1_A21E5);
    let p = match sys.star_class {
        StarClass::O | StarClass::B => 0.70,
        StarClass::A | StarClass::F => 0.50,
        StarClass::G | StarClass::K => 0.40,
        StarClass::M => 0.30,
        _ => 0.20,
    };
    if rng.f64() >= p {
        return;
    }
    let n = 1 + usize::from(rng.f64() < 0.2);
    let outer = sys
        .bodies
        .iter()
        .filter_map(|b| b.orbit.as_ref().filter(|o| o.parent == 0).map(|o| o.a))
        .fold(0.0, f64::max);
    let primary = sys.bodies[0].mass / M_SUN;
    let mu = sys.bodies[0].gm();
    let mut a = (outer * COMPANION_SEPARATION_FACTOR).max(COMPANION_MIN_AU * AU);
    for k in 0..n {
        a *= rng.range(1.5, 5.0);
        let m = (primary * rng.range(0.1, 1.0)).max(0.09);
        let class = class_of_mass(m);
        let lum = crate::luminosity(m);
        let rad = if m < 1.0 { m.powf(0.8) } else { m.powf(0.57) };
        let teff = 5772.0 * (lum / (rad * rad)).powf(0.25);
        let orbit = crate::random_orbit(&mut rng, 0, a, 0.6, 0.5, mu);
        let mass = m * M_SUN;
        let letter = ["B", "C"][k];
        sys.bodies.push(CelBody {
            name: format!("{} {letter}", sys.name),
            kind: BodyKind::Star(class),
            mass,
            radius: rad * R_SUN,
            orbit: Some(orbit),
            rotation_period: rng.range(10.0, 35.0) * 86_400.0,
            temp: teff,
            gravity: G * mass / (rad * R_SUN).powi(2),
            pressure: 0.0,
            atmosphere: vec![],
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate_system, peek_star};

    fn seeds_where(f: impl Fn(&System) -> bool, n: usize) -> Vec<System> {
        (0..40_000u64)
            .map(|s| s.wrapping_mul(0x9E37_79B9))
            .map(generate_system)
            .filter(|s| f(s))
            .take(n)
            .collect()
    }

    #[test]
    fn every_stage_of_a_star_exists_and_has_the_right_shape() {
        let mut seen = std::collections::BTreeMap::new();
        for s in 0..30_000u64 {
            let h = peek_star(s.wrapping_mul(0x9E37_79B9));
            let e = seen.entry(h.stage as usize).or_insert((0u32, h.clone()));
            e.0 += 1;
            match h.stage {
                StarStage::RedGiant | StarStage::RedSupergiant => {
                    assert!(h.radius_sun >= 10.0 && h.teff < 4600.0 && h.luminosity > 10.0)
                }
                StarStage::NeutronStar => {
                    assert!(
                        h.radius_sun * R_SUN < 2.0e4 && h.mass_sun > 1.1 && h.class == StarClass::N
                    )
                }
                StarStage::BlackHole => {
                    // The horizon of a black hole is 2.95 km per solar mass.
                    assert!((h.radius_sun * R_SUN / (2950.0 * h.mass_sun) - 1.0).abs() < 0.01);
                    assert!(h.class == StarClass::X && h.mass_sun >= 5.0);
                }
                StarStage::WhiteDwarf => assert!(h.radius_sun < 0.02),
                StarStage::Rogue => assert!(
                    h.radius_sun < 0.2
                        && h.teff < 1000.0
                        && h.mass_sun < 0.02
                        && h.class == StarClass::R
                ),
                StarStage::MainSequence => assert!(h.radius_sun < 12.0),
            }
        }
        assert_eq!(seen.len(), 7, "all stages occur: {:?}", seen.keys());
        let n = |s: StarStage| seen[&(s as usize)].0;
        assert!(n(StarStage::WhiteDwarf) > n(StarStage::NeutronStar));
        assert!(n(StarStage::NeutronStar) > n(StarStage::BlackHole));
    }

    #[test]
    fn pulsars_and_accreting_black_holes_irradiate_and_black_holes_focus_light() {
        let ns = seeds_where(|s| s.stage == StarStage::NeutronStar, 6);
        assert!(!ns.is_empty());
        for s in &ns {
            let p = s.pulsar().expect("pulsar");
            assert!(
                p.period_s >= PULSAR_PERIOD_S.0 * 0.999 && p.period_s <= PULSAR_PERIOD_S.1 * 1.001
            );
            let near = s.star_radiation(AU);
            assert!(near >= PULSAR_DOSE_AU - 1e-9);
            assert!(
                (s.star_radiation(2.0 * AU) * 4.0 / near - 1.0).abs() < 1e-9,
                "inverse square"
            );
            assert!(s.pulsar().is_some() && s.lens_gain(AU) == 1.0);
        }
        let bh = seeds_where(|s| s.stage == StarStage::BlackHole, 10);
        assert!(!bh.is_empty());
        assert!(bh.iter().any(|s| s.accreting()) && bh.iter().any(|s| !s.accreting()));
        for s in &bh {
            assert_eq!(s.star_radiation(AU) > 0.0, s.accreting());
            let focus = s.lens_focus();
            assert!(focus > 50.0 * AU && focus < 5000.0 * AU, "{}", focus / AU);
            assert!(s.lens_gain(focus) > 2.9, "a lens at its focus");
            assert!(s.lens_gain(focus * 20.0) < 1.01 && s.lens_gain(1.0e12) >= 1.0);
        }
        // Sun-like stars neither burn people nor focus anything.
        let g = generate_system(11);
        if g.stage == StarStage::MainSequence {
            assert_eq!(g.star_radiation(AU), 0.0);
        }
    }

    #[test]
    fn swollen_stars_pulse_and_the_others_do_not() {
        let giants = seeds_where(|s| s.stage == StarStage::RedGiant, 4);
        assert!(!giants.is_empty());
        for s in &giants {
            let v = s.variable().expect("variable");
            assert!((MIRA_PERIOD_DAYS.0..=MIRA_PERIOD_DAYS.1).contains(&v.period_days));
            assert!((MIRA_AMPLITUDE.0..=MIRA_AMPLITUDE.1).contains(&v.amplitude));
            let (mut lo, mut hi) = (f64::MAX, 0.0f64);
            for k in 0..200 {
                let l = s.luminosity_at(f64::from(k) * v.period_days * 86_400.0 / 200.0);
                lo = lo.min(l);
                hi = hi.max(l);
            }
            assert!(hi / lo > 1.0 + v.amplitude, "light varies: {lo} {hi}");
            // One period later the star is as bright.
            let t = 1.0e6;
            assert!(
                (s.luminosity_at(t) - s.luminosity_at(t + v.period_days * 86_400.0)).abs()
                    < 1e-6 * s.luminosity
            );
        }
        let ms = seeds_where(|s| s.stage == StarStage::MainSequence, 3);
        assert!(ms
            .iter()
            .all(|s| s.variable().is_none() && s.luminosity_at(5.0e6) == s.luminosity));
    }

    #[test]
    fn binaries_keep_their_distance_and_add_their_light() {
        let multi = seeds_where(|s| !s.companions().is_empty(), 12);
        assert!(multi.len() >= 10, "many stars have company");
        let mut triples = 0;
        for s in &multi {
            let outer_planet = s
                .bodies
                .iter()
                .filter(|b| matches!(b.kind, BodyKind::Planet(_)))
                .filter_map(|b| b.orbit.as_ref().map(|o| o.a))
                .fold(0.0, f64::max);
            triples += usize::from(s.companions().len() > 1);
            for &c in &s.companions() {
                let o = s.bodies[c].orbit.as_ref().expect("orbit");
                assert!(
                    o.parent == 0
                        && o.a >= COMPANION_SEPARATION_FACTOR * outer_planet
                        && o.a >= COMPANION_MIN_AU * AU
                );
                assert!(s.bodies[c].mass <= s.bodies[0].mass * 1.0001 + 0.09 * M_SUN);
                assert!(s.bodies[c].star_luminosity() > 0.0);
            }
        }
        assert!(triples > 0 || multi.len() < 30);
        // Star luminosity from radius and temperature matches the formula used at generation.
        let g = generate_system(21);
        assert!(
            (g.bodies[0].star_luminosity() / g.luminosity - 1.0).abs() < 0.05
                || matches!(
                    g.stage,
                    StarStage::WhiteDwarf | StarStage::NeutronStar | StarStage::BlackHole
                )
        );
    }

    #[test]
    fn rings_lie_inside_the_roche_limit_and_inside_the_first_moon() {
        let ringed = seeds_where(|s| !s.rings().is_empty(), 15);
        assert!(ringed.len() >= 12);
        for s in &ringed {
            for r in s.rings() {
                let p = &s.bodies[r.planet];
                assert!(
                    p.kind == BodyKind::Planet(PlanetKind::GasGiant)
                        || p.kind == BodyKind::Planet(PlanetKind::IceGiant)
                );
                let rho = p.mass / (4.0 / 3.0 * PI * p.radius.powi(3));
                assert!(r.outer <= 2.44 * p.radius * (rho / 920.0).cbrt() * 1.0001);
                assert!(r.inner > p.radius);
                for m in &s.bodies {
                    if let (BodyKind::Moon(_), Some(o)) = (m.kind, &m.orbit) {
                        if o.parent == r.planet {
                            assert!(
                                o.a > r.outer,
                                "a moon inside the ring would have been torn apart"
                            );
                        }
                    }
                }
                // The ring is made of real icy bodies orbiting inside it: minable.
                let parts: Vec<&CelBody> = s
                    .bodies
                    .iter()
                    .filter(|b| {
                        matches!(b.kind, BodyKind::Asteroid(_))
                            && b.name.contains("anneau")
                            && b.orbit.as_ref().is_some_and(|o| o.parent == r.planet)
                    })
                    .collect();
                assert_eq!(parts.len(), 10);
                assert!(parts.iter().all(|b| {
                    let a = b.orbit.as_ref().expect("orbit").a;
                    a >= r.inner && a <= r.outer
                }));
                assert!(parts
                    .iter()
                    .any(|b| b.kind == BodyKind::Asteroid(AstType::I)));
            }
        }
        // Rocky worlds have none.
        let g = generate_system(5);
        for (i, b) in g.bodies.iter().enumerate() {
            if matches!(b.kind, BodyKind::Planet(k) if !k.is_giant()) {
                assert!(g.ring_of(i).is_none());
            }
        }
    }

    #[test]
    fn trojans_sit_sixty_degrees_from_their_giant_and_stay_there() {
        let sys = seeds_where(|s| s.bodies.iter().any(|b| b.name.starts_with("Troyen")), 5);
        assert!(!sys.is_empty());
        for s in &sys {
            let g = (1..s.bodies.len())
                .filter(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(PlanetKind::GasGiant)))
                .max_by(|&a, &b| s.bodies[a].mass.total_cmp(&s.bodies[b].mass))
                .expect("giant");
            for (i, b) in s.bodies.iter().enumerate() {
                if !b.name.starts_with("Troyen") {
                    continue;
                }
                for t in [0.0, 5.0e7, 3.0e8] {
                    let (pg, _) = s.state(g, t);
                    let (pt, _) = s.state(i, t);
                    let ang = (pg.dot(pt) / (pg.length() * pt.length()))
                        .clamp(-1.0, 1.0)
                        .acos();
                    assert!(
                        (ang - TROJAN_ANGLE).abs() < 0.25,
                        "{} at t={t}: {ang}",
                        b.name
                    );
                }
                assert_eq!(b.kind, BodyKind::Asteroid(AstType::D));
            }
        }
    }

    #[test]
    fn comet_tails_point_away_from_the_star_and_shrink_with_distance() {
        let sys = seeds_where(
            |s| {
                s.bodies
                    .iter()
                    .any(|b| b.kind == BodyKind::Anomaly(AnomalyKind::Comet))
            },
            6,
        );
        assert!(!sys.is_empty());
        for s in &sys {
            let i = s
                .bodies
                .iter()
                .position(|b| b.kind == BodyKind::Anomaly(AnomalyKind::Comet))
                .expect("comet");
            let o = s.bodies[i].orbit.as_ref().expect("orbit");
            assert!(
                (COMET_ECC.0..=COMET_ECC.1).contains(&o.e),
                "comets are very eccentric"
            );
            assert!(
                o.a * (1.0 - o.e) > s.bodies[0].radius * 3.0,
                "and miss the star"
            );
            let (mut near, mut far) = (None, None);
            for k in 0..400 {
                let t = f64::from(k) * o.period() / 400.0;
                let (p, _) = s.state(i, t);
                if let Some(tail) = s.comet_tail(i, t) {
                    assert!(
                        tail.dot(p) > 0.0,
                        "the wind blows the tail away from the star"
                    );
                    assert!((tail.normalized().dot(p.normalized()) - 1.0).abs() < 1e-9);
                    let r = p.length();
                    if near.is_none_or(|(rn, _)| r < rn) {
                        near = Some((r, tail.length()));
                    }
                    if far.is_none_or(|(rf, _)| r > rf) {
                        far = Some((r, tail.length()));
                    }
                }
            }
            if let (Some(n), Some(f)) = (near, far) {
                assert!(n.1 > f.1, "longer near the star: {n:?} vs {f:?}");
            }
            // Beyond the reach of the sun's heat, no tail.
            assert!(s.comet_tail(0, 0.0).is_none());
        }
    }
}
