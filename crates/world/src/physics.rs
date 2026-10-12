//! The physics of a world that follows from what it is made of: mass, radius, spin and orbit give
//! its density, how fast one must go to leave it, its moment of inertia and angular momentum,
//! how much it bulges at the equator, whether its parent has stopped its spin (tidal lock) and
//! what its day is then, its tides, its magnetic field, the heat of its inside and what the
//! ground does with it (plates, volcanoes, nothing), the sphere in which it can hold a moon and
//! the one in which a moon would be torn apart. Nothing is stored: the same seed gives the same
//! numbers, and one function (`System::physics`) is the single source for every screen.
use crate::orbit::{G, M_EARTH, R_EARTH};
use crate::{BodyKind, CelBody, PlanetKind, System};
use std::f64::consts::{PI, TAU};
use vector_core::hash_coord;

/// Angular speed of the Earth, rad/s.
const OMEGA_EARTH: f64 = TAU / 86_164.0;
/// Luminosity of the Sun, W.
use vector_core::units::L_SUN;
/// Heat flowing out of the Earth's ground, W/m^2.
const FLUX_EARTH: f64 = 0.09;
/// Seconds in a year.
pub use vector_core::units::YEAR;

/// What the ground of a world does with the heat of its inside.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Activity {
    /// Cold and solid to the core: nothing moves any more.
    Dead,
    /// One rigid shell (Mars, Mercury): a few old volcanoes.
    Stagnant,
    /// Moving plates: mountains, trenches, earthquakes.
    Plates,
    /// Molten ground or a tidal furnace: eruptions everywhere.
    Volcanic,
    /// No ground at all (a star, a giant): it is fluid to the depths.
    Fluid,
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Activity::Dead => "INERTE",
            Activity::Stagnant => "COUVERCLE FIGE",
            Activity::Plates => "PLAQUES TECTONIQUES",
            Activity::Volcanic => "VOLCANISME GENERALISE",
            Activity::Fluid => "FLUIDE JUSQU'AU CENTRE",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Physics {
    /// Mean density, kg/m^3.
    pub density: f64,
    /// Speed to leave the surface for good, m/s.
    pub escape: f64,
    /// Speed of a circular orbit just above the surface, m/s.
    pub orbital: f64,
    /// Moment of inertia as a share of M R^2 (0.4 for a uniform ball, less when dense at the core).
    pub moment_factor: f64,
    /// Moment of inertia, kg m^2.
    pub inertia: f64,
    /// The spin that counts now (s): the one it was born with, or the one its parent has imposed.
    pub spin_period: f64,
    /// 1 when the spin is locked to the orbit, 3 when it is in the 3:2 resonance of Mercury, else 0.
    pub locked: Option<(u32, u32)>,
    /// Angular speed, rad/s.
    pub omega: f64,
    /// Spin angular momentum, kg m^2/s.
    pub angular_momentum: f64,
    /// Kinetic energy of the spin, J.
    pub spin_energy: f64,
    /// Speed of the ground at the equator, m/s.
    pub equator_speed: f64,
    /// Share of the weight at the equator the spin takes away (omega^2 R / g).
    pub centrifugal: f64,
    /// Flattening of the poles, (equatorial - polar) / equatorial.
    pub flattening: f64,
    /// Axial tilt, rad.
    pub tilt: f64,
    /// Length of the year (s) around its star, or around its parent for a moon.
    pub year: Option<f64>,
    /// From one noon to the next (s): None when the same face stays toward the star.
    pub solar_day: Option<f64>,
    /// Power of the light of the star on one square metre facing it, W/m^2.
    pub insolation: f64,
    /// Radius of the sphere where it, rather than its star, rules a moon (m).
    pub hill: Option<f64>,
    /// Distance under which a moon of rock would be torn apart by it (m).
    pub roche: f64,
    /// Share of the mass in the core.
    pub core_fraction: f64,
    /// Heat that comes out of the ground, W/m^2.
    pub heat_flux: f64,
    pub activity: Activity,
    /// Magnetic moment relative to the Earth's (0: none, so the star's wind strips the air).
    pub magnetic: f64,
    /// Height difference between high and low tide on an ocean of this world (m).
    pub tide: f64,
}

fn unit(seed: u64, k: i64) -> f64 {
    vector_core::unit(hash_coord(seed, k, 11, 3))
}

impl System {
    /// Age of the system, years (always younger than the life of its star).
    pub fn age(&self) -> f64 {
        let m = self.bodies[0].mass / crate::orbit::M_SUN;
        let life = 1.0e10 * m.powf(-2.5);
        (0.3e9 + unit(self.seed, 1) * 9.0e9).min(life * 0.9)
    }

    /// The planet (the body that circles the star) that carries body `i`, `i` itself if it is one.
    fn host(&self, i: usize) -> usize {
        let mut k = i;
        while let Some(o) = &self.bodies[k].orbit {
            if o.parent == 0 {
                return k;
            }
            k = o.parent;
        }
        k
    }

    /// The spin that counts for body `i` now (s), with a parent's tidal lock applied.
    pub fn spin_period(&self, i: usize) -> f64 {
        self.lock_of(i)
            .map_or(self.bodies[i].rotation_period, |(p, q, orbit)| {
                orbit * f64::from(q) / f64::from(p)
            })
    }

    /// (spin turns, orbits, orbital period) when the parent has locked body `i`.
    fn lock_of(&self, i: usize) -> Option<(u32, u32, f64)> {
        let b = &self.bodies[i];
        let o = b.orbit.as_ref()?;
        if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) {
            return None;
        }
        let parent = &self.bodies[o.parent];
        let rho = b.mass / (4.0 / 3.0 * PI * b.radius.powi(3));
        let c = moment_factor(b, rho);
        let omega0 = TAU / b.rotation_period.max(60.0);
        // Gladman et al: t = omega a^6 I Q / (3 G m_p^2 k2 R^5), with Q = 100 and k2 = 0.3.
        let t_lock = omega0 * o.a.powi(6) * (c * b.mass * b.radius * b.radius) * 100.0
            / (3.0 * G * parent.mass * parent.mass * 0.3 * b.radius.powi(5));
        if t_lock / YEAR > self.age() {
            return None;
        }
        // A rounder orbit settles on 1:1; a very eccentric one is caught in 3:2 like Mercury.
        let (p, q) = if o.e > 0.2 { (3, 2) } else { (1, 1) };
        Some((p, q, o.period()))
    }

    /// Everything that follows from body `i` being what it is. `tilt` is its axial tilt (rad).
    pub fn physics(&self, i: usize, tilt: f64) -> Physics {
        let b = &self.bodies[i];
        let vol = 4.0 / 3.0 * PI * b.radius.powi(3);
        let density = b.mass / vol;
        let g = G * b.mass / (b.radius * b.radius);
        let locked = self.lock_of(i);
        let spin_period = self.spin_period(i);
        let omega = TAU / spin_period.max(1.0);
        let c = moment_factor(b, density);
        let inertia = c * b.mass * b.radius * b.radius;
        // Darwin-Radau: how much a fluid ball bulges while it turns.
        let q = omega * omega * b.radius.powi(3) / (G * b.mass);
        let flattening = (2.5 * q / (1.0 + (2.5 * (1.0 - 1.5 * c)).powi(2))).clamp(0.0, 0.5);
        let host = self.host(i);
        let orbit = b.orbit.as_ref();
        let year = orbit.map(|o| o.period());
        let solar_day = match (locked, self.bodies[host].orbit.as_ref()) {
            (Some(_), _) if host == i => None,
            (_, Some(ho)) => {
                let w = TAU / spin_period.max(1.0) - TAU / ho.period();
                (w.abs() > 1e-12).then(|| TAU / w.abs())
            }
            _ => None,
        };
        let star_dist = self.bodies[host]
            .orbit
            .as_ref()
            .map_or(f64::INFINITY, |o| o.a);
        let insolation = if i == 0 {
            0.0
        } else {
            self.luminosity * L_SUN / (4.0 * PI * star_dist * star_dist)
        };
        let hill = orbit.map(|_| {
            let o = self.bodies[host]
                .orbit
                .as_ref()
                .expect("hosts orbit the star");
            o.a * (1.0 - o.e) * (b.mass / (3.0 * self.bodies[0].mass)).cbrt()
        });
        let roche = 2.44 * b.radius * (density / 3000.0).cbrt();
        let solid = matches!(b.kind, BodyKind::Planet(k) | BodyKind::Moon(k) if !giant(k))
            && matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_));
        let core_fraction = if solid {
            (0.1 + (density - 3000.0) * 0.00009).clamp(0.05, 0.6)
        } else {
            0.1
        };
        let mass_e = b.mass / M_EARTH;
        let age = self.age() / 4.5e9;
        let heat_flux = if solid {
            // The heat of the inside leaves by the surface: more for a bigger world, less when old.
            FLUX_EARTH * (b.radius / R_EARTH) / age.max(0.2) * (1.0 + self.tidal_heat(i))
        } else {
            0.0
        };
        let activity = if !solid {
            Activity::Fluid
        } else if b.temp > 900.0 || heat_flux > 0.4 {
            Activity::Volcanic
        } else if heat_flux < 0.015 {
            Activity::Dead
        } else if (0.5..5.0).contains(&mass_e)
            && b.pressure > 0.3
            && b.temp > 220.0
            && b.temp < 420.0
        {
            Activity::Plates
        } else {
            Activity::Stagnant
        };
        let magnetic = if solid {
            // A dynamo needs a fluid core, spin, and enough size to still be warm inside.
            let alive = !matches!(activity, Activity::Dead) && mass_e > 0.08;
            if alive {
                (omega / OMEGA_EARTH).min(3.0)
                    * (b.radius / R_EARTH).powi(3)
                    * (core_fraction / 0.33).powi(2)
            } else {
                0.0
            }
        } else if matches!(b.kind, BodyKind::Star(_)) {
            1.0e3
        } else {
            // Giants: metallic hydrogen, very fast spin.
            20.0 * (omega / OMEGA_EARTH).min(5.0) * (b.radius / R_EARTH).powi(2) / 10.0
        };
        Physics {
            density,
            escape: (2.0 * G * b.mass / b.radius).sqrt(),
            orbital: (G * b.mass / b.radius).sqrt(),
            moment_factor: c,
            inertia,
            spin_period,
            locked: locked.map(|(p, q, _)| (p, q)),
            omega,
            angular_momentum: inertia * omega,
            spin_energy: 0.5 * inertia * omega * omega,
            equator_speed: omega * b.radius,
            centrifugal: omega * omega * b.radius / g.max(1e-9),
            flattening,
            tilt,
            year,
            solar_day,
            insolation,
            hill,
            roche,
            core_fraction,
            heat_flux,
            activity,
            magnetic,
            tide: self.tide_of(i),
        }
    }

    /// Extra heat from being kneaded by a parent: a share of the heat the world makes itself
    /// (Io), growing with the eccentricity of its orbit and the closeness of its parent.
    fn tidal_heat(&self, i: usize) -> f64 {
        let b = &self.bodies[i];
        let Some(o) = &b.orbit else { return 0.0 };
        let p = &self.bodies[o.parent];
        // Same formula as the tide: (m_p / m)(R / a)^3 gives the stretch, e gives the kneading.
        let stretch = (p.mass / b.mass) * (b.radius / o.a).powi(3);
        (stretch * o.e * o.e * 40.0).min(20.0)
    }

    /// Height of the tide (high minus low, m) the other bodies raise on an ocean of body `i`.
    pub fn tide_of(&self, i: usize) -> f64 {
        let b = &self.bodies[i];
        let mut sum = 0.0;
        for (j, other) in self.bodies.iter().enumerate() {
            if j == i {
                continue;
            }
            let Some(o) = &other.orbit else { continue };
            // A moon of this world, or the parent it orbits, or the star if this world orbits it.
            let d = if o.parent == i {
                o.a
            } else if self.bodies[i].orbit.as_ref().is_some_and(|m| m.parent == j) {
                self.bodies[i].orbit.as_ref().map_or(0.0, |m| m.a)
            } else {
                continue;
            };
            sum += (other.mass / b.mass) * (b.radius / d).powi(3) * b.radius;
        }
        // Equilibrium tide: half the stretch each way; high minus low doubles it.
        sum
    }
}

fn giant(k: PlanetKind) -> bool {
    matches!(
        k,
        PlanetKind::GasGiant | PlanetKind::HotJupiter | PlanetKind::IceGiant
    )
}

/// Share of M R^2: a uniform ball is 0.4, a dense core brings it down (Earth 0.33, Jupiter 0.254).
fn moment_factor(b: &CelBody, density: f64) -> f64 {
    match b.kind {
        BodyKind::Star(_) => 0.07,
        BodyKind::Planet(k) | BodyKind::Moon(k) if giant(k) => 0.25,
        _ => (0.4 - 0.1 * ((density - 2500.0) / 3500.0).clamp(0.0, 1.0)).clamp(0.3, 0.4),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::generate_system;
    use crate::orbit::AU;

    fn solid_planets(s: &System) -> Vec<usize> {
        (1..s.bodies.len())
            .filter(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(k) if !giant(k)))
            .collect()
    }

    #[test]
    fn what_follows_from_mass_and_radius_is_consistent() {
        for seed in 1..40 {
            let s = generate_system(seed);
            for i in 0..s.bodies.len() {
                if !matches!(
                    s.bodies[i].kind,
                    BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Star(_)
                ) {
                    continue;
                }
                let p = s.physics(i, 0.3);
                let b = &s.bodies[i];
                // v_esc = sqrt(2) * v_orb, whatever the world.
                assert!((p.escape / p.orbital - 2f64.sqrt()).abs() < 1e-9);
                // I = C M R^2 and L = I w.
                assert!(
                    (p.inertia - p.moment_factor * b.mass * b.radius * b.radius).abs()
                        < p.inertia * 1e-9
                );
                assert!(
                    (p.angular_momentum - p.inertia * p.omega).abs() <= p.angular_momentum * 1e-9
                );
                assert!(p.flattening >= 0.0 && p.flattening <= 0.5);
                assert!(
                    p.density > 0.0 && p.density < 5.0e6,
                    "{seed}/{i}: {}",
                    p.density
                );
                assert!(p.heat_flux.is_finite() && p.magnetic.is_finite() && p.tide.is_finite());
            }
        }
    }

    #[test]
    fn the_fast_spinners_bulge_more_and_the_giants_bulge_most() {
        let mut giants = Vec::new();
        let mut rocks = Vec::new();
        for seed in 1..60 {
            let s = generate_system(seed);
            for i in 1..s.bodies.len() {
                if let BodyKind::Planet(k) = s.bodies[i].kind {
                    let p = s.physics(i, 0.0);
                    if giant(k) {
                        giants.push(p.flattening)
                    } else {
                        rocks.push(p.flattening)
                    }
                }
            }
        }
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        assert!(!giants.is_empty() && !rocks.is_empty());
        assert!(
            mean(&giants) > 3.0 * mean(&rocks),
            "{} vs {}",
            mean(&giants),
            mean(&rocks)
        );
    }

    #[test]
    fn a_close_world_is_locked_to_its_parent_and_a_far_one_is_not() {
        let (mut locked, mut free) = (0, 0);
        for seed in 1..80 {
            let s = generate_system(seed);
            for i in solid_planets(&s) {
                let a = s.bodies[i].orbit.as_ref().expect("orbit").a;
                let p = s.physics(i, 0.0);
                if p.locked.is_some() {
                    locked += 1;
                    assert!(p.solar_day.is_none() || p.locked == Some((3, 2)));
                    // The locked spin is the orbital period (or two thirds of it).
                    let year = p.year.expect("year");
                    assert!(
                        (p.spin_period / year - 1.0).abs() < 1e-9
                            || (p.spin_period / year - 2.0 / 3.0).abs() < 1e-9
                    );
                } else {
                    free += 1;
                    assert!(a > 0.0);
                    assert!((p.spin_period - s.bodies[i].rotation_period).abs() < 1.0);
                }
            }
        }
        assert!(locked > 0 && free > 0, "locked {locked}, free {free}");
        // Far enough, never; as close as a star's skin, always (the lock time grows as a^6).
        let s = generate_system(3);
        let i = solid_planets(&s)[0];
        let mut near = s.clone();
        near.bodies[i].orbit.as_mut().expect("orbit").a = 0.02 * AU;
        assert!(near.physics(i, 0.0).locked.is_some());
        let mut far = s.clone();
        far.bodies[i].orbit.as_mut().expect("orbit").a = 40.0 * AU;
        assert!(far.physics(i, 0.0).locked.is_none());
    }

    #[test]
    fn tides_and_hill_spheres_scale_as_in_physics() {
        // The Earth and its Moon: about 0.3 m of tide from the Moon, 0.15 m from the Sun.
        let mut s = generate_system(5);
        let i = solid_planets(&s)[0];
        s.bodies[i].mass = M_EARTH;
        s.bodies[i].radius = R_EARTH;
        s.bodies[i].orbit.as_mut().expect("orbit").a = AU;
        let before = s.tide_of(i);
        // A massive moon on a close orbit raises a much higher tide than none.
        let o = s.bodies[i].orbit.clone().expect("orbit");
        let mut moon = s.bodies[i].clone();
        moon.mass = 7.3e22;
        moon.radius = 1.7e6;
        moon.kind = BodyKind::Moon(PlanetKind::Barren);
        moon.orbit = Some(crate::orbit::Orbit {
            parent: i,
            a: 3.84e8,
            e: 0.05,
            ..o
        });
        s.bodies.push(moon);
        let after = s.tide_of(i);
        assert!(
            after > before + 0.1 && after < before + 1.0,
            "{before} -> {after}"
        );
        let h = s.physics(i, 0.0).hill.expect("hill");
        assert!(h > 3.84e8, "the Moon must be inside the Hill sphere: {h}");
    }
}
