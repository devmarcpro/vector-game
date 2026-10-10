//! Procedural universe: stellar systems generated deterministically from a seed.
pub mod names;
pub mod orbit;

use names::{pick, roman, star_name};
use orbit::{Orbit, AU, G, M_EARTH, M_SUN, R_EARTH, R_SUN};
use vector_core::{Rng, Vec3};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StarClass {
    O,
    B,
    A,
    F,
    G,
    K,
    M,
}

impl StarClass {
    pub const ALL: [StarClass; 7] = [
        Self::O,
        Self::B,
        Self::A,
        Self::F,
        Self::G,
        Self::K,
        Self::M,
    ];
    pub fn letter(self) -> &'static str {
        ["O", "B", "A", "F", "G", "K", "M"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlanetKind {
    Barren,
    Dwarf,
    Desert,
    Terran,
    Ocean,
    Tundra,
    Ice,
    Lava,
    Venusian,
    GasGiant,
    IceGiant,
    HotJupiter,
}

impl PlanetKind {
    pub const ALL: [PlanetKind; 12] = [
        Self::Barren,
        Self::Dwarf,
        Self::Desert,
        Self::Terran,
        Self::Ocean,
        Self::Tundra,
        Self::Ice,
        Self::Lava,
        Self::Venusian,
        Self::GasGiant,
        Self::IceGiant,
        Self::HotJupiter,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Barren => "Aride",
            Self::Dwarf => "Naine",
            Self::Desert => "Desertique",
            Self::Terran => "Tellurique temperee",
            Self::Ocean => "Oceanique",
            Self::Tundra => "Toundra",
            Self::Ice => "Glacee",
            Self::Lava => "Lavique",
            Self::Venusian => "Venusienne",
            Self::GasGiant => "Geante gazeuse",
            Self::IceGiant => "Geante de glace",
            Self::HotJupiter => "Jupiter chaud",
        }
    }
    pub fn is_giant(self) -> bool {
        matches!(self, Self::GasGiant | Self::IceGiant | Self::HotJupiter)
    }
}

/// Asteroid taxonomy: C carbonaceous, S silicate, M metallic, I icy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AstType {
    C,
    S,
    M,
    I,
}

impl AstType {
    pub fn id(self) -> &'static str {
        ["C", "S", "M", "I"][self as usize]
    }
    pub fn density(self) -> f64 {
        [1400.0, 2700.0, 5300.0, 1000.0][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Asteroid(AstType),
    Star(StarClass),
    Planet(PlanetKind),
    Moon(PlanetKind),
    Station,
}

#[derive(Clone, Debug)]
pub struct CelBody {
    pub name: String,
    pub kind: BodyKind,
    /// kg
    pub mass: f64,
    /// m
    pub radius: f64,
    pub orbit: Option<Orbit>,
    /// Sidereal rotation period, s.
    pub rotation_period: f64,
    /// Equilibrium temperature, K (surface temperature for the star).
    pub temp: f64,
    /// Surface gravity, m/s^2.
    pub gravity: f64,
    /// Surface pressure, bar (0 = none).
    pub pressure: f64,
    pub atmosphere: Vec<(&'static str, f32)>,
}

impl CelBody {
    pub fn gm(&self) -> f64 {
        G * self.mass
    }
}

#[derive(Clone, Debug)]
pub struct Belt {
    pub inner: f64,
    pub outer: f64,
}

#[derive(Clone, Debug)]
pub struct System {
    pub seed: u64,
    pub name: String,
    pub designation: String,
    pub star_class: StarClass,
    /// Luminosity in solar units.
    pub luminosity: f64,
    /// Body 0 is always the star.
    pub bodies: Vec<CelBody>,
    pub belts: Vec<Belt>,
}

impl System {
    /// Absolute (star-centred) position and velocity of body `i` at time `t`.
    pub fn state(&self, i: usize, t: f64) -> (Vec3, Vec3) {
        match &self.bodies[i].orbit {
            None => (Vec3::ZERO, Vec3::ZERO),
            Some(o) => {
                let (pp, pv) = self.state(o.parent, t);
                let (p, v) = o.state(t);
                (pp + p, pv + v)
            }
        }
    }
    pub fn frost_line(&self) -> f64 {
        2.7 * self.luminosity.sqrt() * AU
    }
}

fn pick_class(rng: &mut Rng) -> StarClass {
    let w = [0.01, 0.03, 0.04, 0.08, 0.14, 0.20, 0.50];
    let mut x = rng.f64();
    for (c, w) in StarClass::ALL.iter().zip(w) {
        if x < w {
            return *c;
        }
        x -= w;
    }
    StarClass::M
}

fn mass_range(c: StarClass) -> (f64, f64) {
    match c {
        StarClass::O => (16.0, 60.0),
        StarClass::B => (2.1, 16.0),
        StarClass::A => (1.4, 2.1),
        StarClass::F => (1.04, 1.4),
        StarClass::G => (0.8, 1.04),
        StarClass::K => (0.45, 0.8),
        StarClass::M => (0.08, 0.45),
    }
}

fn luminosity(m: f64) -> f64 {
    if m < 0.43 {
        0.23 * m.powf(2.3)
    } else if m < 2.0 {
        m.powi(4)
    } else {
        1.4 * m.powf(3.5)
    }
}

fn eq_temp(lum: f64, a_au: f64, albedo: f64) -> f64 {
    278.6 * lum.powf(0.25) / a_au.sqrt() * (1.0 - albedo).powf(0.25)
}

fn rocky_kind(rng: &mut Rng, t: f64, mass_e: f64) -> PlanetKind {
    let r = rng.f64();
    if mass_e < 0.05 {
        return if t < 150.0 {
            PlanetKind::Ice
        } else {
            PlanetKind::Dwarf
        };
    }
    if t > 1000.0 {
        PlanetKind::Lava
    } else if t > 600.0 {
        if mass_e > 0.3 {
            PlanetKind::Lava
        } else {
            PlanetKind::Barren
        }
    } else if t > 400.0 {
        if mass_e > 0.5 {
            PlanetKind::Venusian
        } else {
            PlanetKind::Desert
        }
    } else if t > 340.0 {
        PlanetKind::Desert
    } else if t > 240.0 && mass_e > 0.3 {
        match r {
            x if x < 0.3 => PlanetKind::Terran,
            x if x < 0.55 => PlanetKind::Ocean,
            x if x < 0.75 => PlanetKind::Desert,
            _ => PlanetKind::Barren,
        }
    } else if t > 150.0 {
        if r < 0.4 {
            PlanetKind::Tundra
        } else if r < 0.8 {
            PlanetKind::Ice
        } else {
            PlanetKind::Barren
        }
    } else if r < 0.7 {
        PlanetKind::Ice
    } else {
        PlanetKind::Dwarf
    }
}

fn atmosphere(rng: &mut Rng, k: PlanetKind) -> (f64, Vec<(&'static str, f32)>) {
    match k {
        PlanetKind::Barren | PlanetKind::Dwarf => (0.0, vec![]),
        PlanetKind::Desert => (rng.range(0.01, 1.0), vec![("CO2", 0.95), ("N2", 0.04)]),
        PlanetKind::Terran => (
            rng.range(0.6, 1.6),
            vec![("N2", 0.78), ("O2", 0.21), ("Ar", 0.01)],
        ),
        PlanetKind::Ocean => (
            rng.range(0.8, 3.0),
            vec![("N2", 0.75), ("O2", 0.18), ("H2O", 0.05)],
        ),
        PlanetKind::Tundra => (
            rng.range(0.3, 1.5),
            vec![("N2", 0.9), ("CO2", 0.06), ("Ar", 0.03)],
        ),
        PlanetKind::Ice => (rng.range(0.0, 0.3), vec![("N2", 0.8), ("CH4", 0.1)]),
        PlanetKind::Lava => (rng.range(0.0, 5.0), vec![("SO2", 0.6), ("CO2", 0.3)]),
        PlanetKind::Venusian => (rng.range(20.0, 95.0), vec![("CO2", 0.96), ("N2", 0.035)]),
        PlanetKind::GasGiant | PlanetKind::HotJupiter => (1.0, vec![("H2", 0.9), ("He", 0.1)]),
        PlanetKind::IceGiant => (1.0, vec![("H2", 0.8), ("He", 0.15), ("CH4", 0.05)]),
    }
}

#[allow(clippy::too_many_arguments)]
fn make_body(
    rng: &mut Rng,
    name: String,
    kind: BodyKind,
    pk: PlanetKind,
    mass: f64,
    radius: f64,
    orbit: Orbit,
    temp: f64,
) -> CelBody {
    let (pressure, atmosphere) = atmosphere(rng, pk);
    CelBody {
        name,
        kind,
        mass,
        radius,
        orbit: Some(orbit),
        rotation_period: rng.range(4.0, 60.0) * 3600.0,
        temp,
        gravity: G * mass / (radius * radius),
        pressure,
        atmosphere,
    }
}

fn random_orbit(rng: &mut Rng, parent: usize, a: f64, e_max: f64, i_max: f64, mu: f64) -> Orbit {
    Orbit {
        parent,
        a,
        e: rng.f64() * e_max,
        incl: rng.range(-i_max, i_max),
        lan: rng.range(0.0, std::f64::consts::TAU),
        argp: rng.range(0.0, std::f64::consts::TAU),
        m0: rng.range(0.0, std::f64::consts::TAU),
        mu,
    }
}

/// Generate a complete stellar system deterministically from `seed`.
pub fn generate_system(seed: u64) -> System {
    let mut rng = Rng::new(seed);
    let class = pick_class(&mut rng);
    let (m0, m1) = mass_range(class);
    let m = rng.range(m0, m1);
    let lum = luminosity(m);
    let rad = if m < 1.0 { m.powf(0.8) } else { m.powf(0.57) };
    let teff = 5772.0 * (lum / (rad * rad)).powf(0.25);
    let name = star_name(&mut rng);
    let designation = format!("HD {}", 1000 + rng.next_u64() % 900_000);
    let star_mass = m * M_SUN;
    let mu_star = G * star_mass;
    let star = CelBody {
        name: name.clone(),
        kind: BodyKind::Star(class),
        mass: star_mass,
        radius: rad * R_SUN,
        orbit: None,
        rotation_period: rng.range(10.0, 35.0) * 86400.0,
        temp: teff,
        gravity: mu_star / (rad * R_SUN).powi(2),
        pressure: 0.0,
        atmosphere: vec![],
    };
    let mut bodies = vec![star];
    let mut belts = Vec::new();

    let n_planets = ((rng.f64().powf(0.8)) * 9.0) as usize;
    let mut a = (rng.range(0.04, 0.4) * lum.sqrt()).max(rad * R_SUN * 4.0 / AU);
    let frost = 2.7 * lum.sqrt();
    let mut planet_idx = Vec::new();
    for pi in 0..n_planets {
        a *= rng.range(1.45, 2.1);
        let a_m = a * AU;
        let beyond = a > frost;
        let albedo = rng.range(0.1, 0.5);
        let t = eq_temp(lum, a, albedo);
        let (pk, mass_e) = if beyond && rng.f64() < 0.75 {
            if rng.f64() < 0.6 {
                (PlanetKind::GasGiant, rng.range(50.0, 400.0))
            } else {
                (PlanetKind::IceGiant, rng.range(8.0, 30.0))
            }
        } else if !beyond && a < frost * 0.5 && rng.f64() < 0.04 {
            (PlanetKind::HotJupiter, rng.range(80.0, 600.0))
        } else {
            let me = (rng.range(0.01f64.ln(), 5.0f64.ln())).exp();
            (rocky_kind(&mut rng, t, me), me)
        };
        let mass = mass_e * M_EARTH;
        let radius = if pk.is_giant() {
            R_EARTH * (3.5 + 7.0 * (mass_e / 400.0).min(1.0).sqrt())
        } else {
            R_EARTH * mass_e.powf(0.27)
        };
        let orbit = random_orbit(&mut rng, 0, a_m, 0.15, 0.06, mu_star);
        let pname = format!("{} {}", name, roman(pi));
        let idx = bodies.len();
        bodies.push(make_body(
            &mut rng,
            pname.clone(),
            BodyKind::Planet(pk),
            pk,
            mass,
            radius,
            orbit,
            t,
        ));
        planet_idx.push(idx);

        // Moons.
        let n_moons = if pk.is_giant() {
            (rng.f64() * 5.0) as usize
        } else if mass_e > 0.5 && rng.f64() < 0.3 {
            1
        } else {
            0
        };
        let hill = a_m * (mass / (3.0 * star_mass)).cbrt();
        let mut ma = radius * rng.range(2.5, 6.0);
        let mu_p = G * mass;
        for mi in 0..n_moons {
            ma *= rng.range(1.6, 2.6);
            if ma > hill * 0.4 {
                break;
            }
            let moon_mass_e = if pk.is_giant() {
                rng.range(1e-4, 0.03)
            } else {
                rng.range(1e-4, 0.01)
            };
            let mk = if t < 150.0 || ma > radius * 20.0 {
                PlanetKind::Ice
            } else if ma < radius * 6.0 && pk.is_giant() {
                PlanetKind::Lava
            } else {
                PlanetKind::Barren
            };
            let mmass = moon_mass_e * M_EARTH;
            let mrad = R_EARTH * moon_mass_e.powf(0.3) * 1.2;
            let morbit = random_orbit(&mut rng, idx, ma, 0.05, 0.05, mu_p);
            let mname = format!("{}-{}", pname, (b'a' + mi as u8) as char);
            bodies.push(make_body(
                &mut rng,
                mname,
                BodyKind::Moon(mk),
                mk,
                mmass,
                mrad,
                morbit,
                t,
            ));
        }
    }

    // Asteroid belt in a gap, 45% of systems.
    if rng.f64() < 0.45 {
        let c = if planet_idx.len() >= 2 {
            let k = (rng.next_u64() % (planet_idx.len() as u64 - 1)) as usize;
            let (a0, a1) = (
                orbit_a(&bodies[planet_idx[k]]),
                orbit_a(&bodies[planet_idx[k + 1]]),
            );
            (a0 * a1).sqrt()
        } else {
            frost * AU * 0.8
        };
        belts.push(Belt {
            inner: c * 0.92,
            outer: c * 1.08,
        });
    }

    // Asteroid families: clumps sharing nearly identical orbits, so they travel together.
    for (bi, belt) in belts.clone().iter().enumerate() {
        let n_clumps = 5 + (rng.next_u64() % 4) as usize;
        for c in 0..n_clumps {
            let base_a = rng.range(belt.inner, belt.outer);
            let base = random_orbit(&mut rng, 0, base_a, 0.08, 0.05, mu_star);
            let class = match rng.f64() {
                x if x < 0.45 => AstType::C,
                x if x < 0.75 => AstType::S,
                x if x < 0.90 => AstType::M,
                _ => AstType::I,
            };
            for k in 0..8 {
                let mut o = base.clone();
                o.a += rng.range(-30.0e3, 30.0e3);
                o.m0 += rng.range(-3.0e4, 3.0e4) / o.a;
                o.incl += rng.range(-1.0e4, 1.0e4) / o.a;
                let radius = (rng.range(30.0f64.ln(), 1500.0f64.ln())).exp();
                let mass = 4.0 / 3.0 * std::f64::consts::PI * radius.powi(3) * class.density();
                bodies.push(CelBody {
                    name: format!("{} B{}-{}{}", name, bi + 1, (b'A' + c as u8) as char, k + 1),
                    kind: BodyKind::Asteroid(class),
                    mass,
                    radius,
                    orbit: Some(o),
                    rotation_period: rng.range(300.0, 40000.0),
                    temp: 150.0,
                    gravity: G * mass / (radius * radius),
                    pressure: 0.0,
                    atmosphere: vec![],
                });
            }
        }
    }

    // Stations around a random planet.
    if !planet_idx.is_empty() && rng.f64() < 0.6 {
        let host = *pick(&mut rng, &planet_idx);
        let h = &bodies[host];
        let r = h.radius;
        let orb_a = r + (r * 0.3).max(4.0e5);
        let mu_h = h.gm();
        let hname = h.name.clone();
        let t = h.temp;
        let orbit = random_orbit(&mut rng, host, orb_a, 0.0, 0.5, mu_h);
        let sname = format!("Station {}", hname);
        bodies.push(CelBody {
            name: sname,
            kind: BodyKind::Station,
            mass: 1.0e7,
            radius: 120.0,
            orbit: Some(orbit),
            rotation_period: 600.0,
            temp: t,
            gravity: 0.0,
            pressure: 1.0,
            atmosphere: vec![("N2", 0.78), ("O2", 0.21)],
        });
    }

    System {
        seed,
        name,
        designation,
        star_class: class,
        luminosity: lum,
        bodies,
        belts,
    }
}

fn orbit_a(b: &CelBody) -> f64 {
    b.orbit.as_ref().map_or(0.0, |o| o.a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        for s in 0..50u64 {
            let a = generate_system(s);
            let b = generate_system(s);
            assert_eq!(a.bodies.len(), b.bodies.len());
            for (x, y) in a.bodies.iter().zip(&b.bodies) {
                assert_eq!(x.name, y.name);
                assert_eq!(x.mass, y.mass);
            }
        }
    }

    #[test]
    fn systems_are_physically_sane() {
        let mut classes = std::collections::BTreeSet::new();
        let mut kinds = std::collections::BTreeSet::new();
        for s in 0..2000u64 {
            let sys = generate_system(s.wrapping_mul(0x9E37_79B9));
            classes.insert(sys.star_class as usize);
            for (i, b) in sys.bodies.iter().enumerate() {
                assert!(
                    b.mass > 0.0 && b.radius > 0.0 && b.temp > 0.0,
                    "{} {i}",
                    b.name
                );
                assert!(b.gravity.is_finite() && b.pressure >= 0.0);
                if let BodyKind::Planet(k) | BodyKind::Moon(k) = b.kind {
                    kinds.insert(k as usize);
                }
                if let Some(o) = &b.orbit {
                    assert!(o.parent < i, "parents come first");
                    assert!(
                        o.a > sys.bodies[o.parent].radius,
                        "{} orbits inside parent",
                        b.name
                    );
                    let (p, v) = sys.state(i, 1.0e7);
                    assert!(p.length().is_finite() && v.length().is_finite());
                }
            }
        }
        assert_eq!(classes.len(), 7, "all star classes appear");
        assert!(kinds.len() >= 11, "planet kinds seen: {}", kinds.len());
    }

    #[test]
    #[test]
    fn belts_contain_clumped_asteroids() {
        let mut found = false;
        for s in 0..400u64 {
            let sys = generate_system(s);
            let rocks: Vec<usize> = (0..sys.bodies.len())
                .filter(|&i| matches!(sys.bodies[i].kind, BodyKind::Asteroid(_)))
                .collect();
            if rocks.is_empty() {
                continue;
            }
            found = true;
            assert_eq!(rocks.len() % 8, 0);
            // First clump: 8 asteroids within a few hundred km of each other.
            let (p0, _) = sys.state(rocks[0], 0.0);
            for &r in &rocks[..8] {
                let (p, _) = sys.state(r, 0.0);
                assert!(
                    (p - p0).length() < 2.0e6,
                    "clump spread {} seed {} a {:e} inner {:e}",
                    (p - p0).length(),
                    s,
                    sys.bodies[r].orbit.as_ref().unwrap().a,
                    sys.belts[0].inner
                );
            }
        }
        assert!(found);
    }

    #[test]
    fn temperate_zone_scales_with_luminosity() {
        assert!((eq_temp(1.0, 1.0, 0.3) - 255.0).abs() < 5.0);
    }
}
