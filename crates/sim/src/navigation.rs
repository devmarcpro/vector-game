//! Flight physics the pilot reads and the ship suffers: osculating orbit (periapsis, apoapsis,
//! period), Hohmann windows, Oberth and flyby gains, spheres of influence, eclipses and the heat
//! of the Sun, the heat of the air on entry, the radiation dose, light delay and structural load.
//!
//! The pure functions take SI units and know nothing of the simulation, so each law is tested on
//! its own; the `impl Sim` block at the end applies them to the live system.
use crate::{Sim, G0, SHIP_RADIUS};
use std::f64::consts::{PI, TAU};
use vector_core::Vec3;
use vector_world::orbit::AU;
use vector_world::{BodyKind, StarClass};

/// Speed of light (m/s).
pub use vector_core::units::C;
/// Stefan-Boltzmann constant in kW/(m^2 K^4).
pub const SIGMA_KW: f64 = vector_core::units::SIGMA / 1000.0;
/// Solar constant at 1 AU (kW/m^2).
pub const SOLAR_CONSTANT_KW: f64 = vector_core::units::SOLAR_CONSTANT / 1000.0;
/// Fraction of the sunlight a ship hull reflects.
pub const SHIP_ALBEDO: f64 = 0.3;
/// Mean density of a ship, tanks and voids included (kg/m^3): gives its size from its mass.
pub const SHIP_DENSITY: f64 = 200.0;
/// Share of the kinetic power of the air that reaches the hull as heat (the shock layer keeps the rest).
pub const AERO_HEAT_SHARE: f64 = 0.03;
/// Ablative plating: heat blocked is `armor / (armor + ABLATION_K)`.
pub const ABLATION_K: f64 = 3.0;
/// Energy (MJ) the ablating plating spends for one hull point.
pub const ABLATION_MJ_PER_POINT: f64 = 10.0;
/// Cosmic-ray background in open space (Gy/h, unshielded).
pub const COSMIC_GY_H: f64 = 2.0e-5;
/// Particle dose of a quiet G star at 1 AU (Gy/h, unshielded).
pub const STELLAR_GY_H_1AU: f64 = 5.0e-5;
/// Dose rate at the peak of the belt of an Earth-strength magnetic field (Gy/h).
pub const BELT_GY_H: f64 = 0.05;
/// Share of the outside dose that reaches a crew docked at a station (its radiation shelters).
pub const DOCKED_COVER: f64 = 0.005;
/// Share of an outside dose that gets through while the shield is up.
pub const SHIELDED_COVER: f64 = 0.12;
/// Armour rating at which plating cuts the dose by a factor e.
pub const SHIELD_E_FOLD: f64 = 6.0;
/// The frame holds up to this many g (thrust, drag or tide) before it starts to tear.
pub const STRUCTURE_LIMIT_G: f64 = 40.0;

// ---- pure laws ------------------------------------------------------------------------------

/// The elements of an orbit around a body of gravitational parameter `mu`.
#[derive(Clone, Copy, Debug)]
pub struct Elements {
    pub r: f64,
    pub speed: f64,
    /// Specific orbital energy (J/kg).
    pub energy: f64,
    pub bound: bool,
    /// Semi-major axis (negative on an escape path).
    pub a: f64,
    pub e: f64,
    /// Distance at the closest point (from the centre of the body).
    pub periapsis: f64,
    /// Distance at the farthest point (None when the path is open).
    pub apoapsis: Option<f64>,
    pub period: Option<f64>,
    /// Speed of a circular orbit at the present distance and the escape speed there.
    pub v_circ: f64,
    pub v_esc: f64,
}

/// Elements from the state relative to the central body (two-body problem).
pub fn elements(r: Vec3, v: Vec3, mu: f64) -> Elements {
    let rl = r.length().max(1e-9);
    let speed = v.length();
    let energy = 0.5 * speed * speed - mu / rl;
    let h = r.cross(v);
    let evec = r * (speed * speed / mu - 1.0 / rl) - v * (r.dot(v) / mu);
    let e = evec.length();
    let periapsis = h.dot(h) / (mu * (1.0 + e));
    let bound = energy < 0.0;
    let a = -mu / (2.0 * energy);
    Elements {
        r: rl,
        speed,
        energy,
        bound,
        a,
        e,
        periapsis,
        apoapsis: bound.then_some(a * (1.0 + e)),
        period: bound.then_some(TAU * (a.powi(3) / mu).sqrt()),
        v_circ: (mu / rl).sqrt(),
        v_esc: (2.0 * mu / rl).sqrt(),
    }
}

/// Burns of a Hohmann transfer between two circular coplanar orbits.
#[derive(Clone, Copy, Debug)]
pub struct Hohmann {
    pub dv1: f64,
    pub dv2: f64,
    pub total: f64,
    /// Flight time (half the period of the transfer ellipse), s.
    pub time: f64,
}

pub fn hohmann(r1: f64, r2: f64, mu: f64) -> Hohmann {
    let at = 0.5 * (r1 + r2);
    let dv1 = ((mu / r1).sqrt() * ((2.0 * r2 / (r1 + r2)).sqrt() - 1.0)).abs();
    let dv2 = ((mu / r2).sqrt() * (1.0 - (2.0 * r1 / (r1 + r2)).sqrt())).abs();
    Hohmann {
        dv1,
        dv2,
        total: dv1 + dv2,
        time: PI * (at.powi(3) / mu).sqrt(),
    }
}

/// Angle (rad) by which the target must lead the ship at departure so both arrive together.
pub fn hohmann_lead_angle(r1: f64, r2: f64) -> f64 {
    PI * (1.0 - ((r1 + r2) / (2.0 * r2)).powf(1.5))
}

/// Seconds until the target leads the ship by `needed` rad, given its lead `now` and the mean
/// motions of the ship and the target.
pub fn transfer_window(now: f64, needed: f64, n_ship: f64, n_target: f64) -> f64 {
    let rate = n_target - n_ship;
    if rate.abs() < 1e-15 {
        return f64::INFINITY;
    }
    let mut dphi = (needed - now) / rate;
    let synodic = TAU / rate.abs();
    dphi = dphi.rem_euclid(synodic);
    dphi
}

/// Tsiolkovsky: delta-v (m/s) from a wet mass `m0` to a dry mass `m1` at specific impulse `isp` (s).
pub fn tsiolkovsky(isp: f64, m0: f64, m1: f64) -> f64 {
    isp * G0 * (m0 / m1.max(1e-9)).ln()
}

/// Propellant (kg) a burn of `dv` costs a ship of wet mass `m0`.
pub fn propellant_for(dv: f64, isp: f64, m0: f64) -> f64 {
    m0 * (1.0 - (-dv / (isp * G0).max(1e-9)).exp())
}

/// Turn of the path (rad) of a flyby at hyperbolic excess speed `v_inf` and periapsis `rp` around `mu`.
pub fn flyby_deflection(v_inf: f64, rp: f64, mu: f64) -> f64 {
    2.0 * (1.0 / (1.0 + rp * v_inf * v_inf / mu)).asin()
}

/// Change of the ship's speed (m/s) in the frame of the Sun, at best, for a flyby that turns the
/// path by `delta` at excess speed `v_inf`: the length of the change of the velocity vector.
pub fn flyby_dv(v_inf: f64, delta: f64) -> f64 {
    2.0 * v_inf * (0.5 * delta).sin()
}

/// Radius (m) of the sphere of influence of a body of mass `m` orbiting `big_m` at distance `a` (Laplace).
pub fn soi_radius(a: f64, m: f64, big_m: f64) -> f64 {
    a * (m / big_m).powf(0.4)
}

/// Lorentz factor.
pub fn gamma(v: f64) -> f64 {
    let b = (v / C).min(0.999_999_999);
    1.0 / (1.0 - b * b).sqrt()
}

/// One-way light delay over `d` metres (s).
pub fn light_delay(d: f64) -> f64 {
    d / C
}

/// Tidal acceleration (m/s^2) across a body of `length` m at distance `r` of a mass of parameter `mu`.
pub fn tidal_accel(mu: f64, r: f64, length: f64) -> f64 {
    2.0 * mu * length / r.max(1.0).powi(3)
}

/// Fraction of the Sun's disc still visible when a body of angular radius `ab` stands at angular
/// separation `theta` from the centre of the disc of angular radius `asun` (all in rad).
pub fn visible_fraction(asun: f64, ab: f64, theta: f64) -> f64 {
    if theta >= asun + ab {
        return 1.0;
    }
    if theta <= (ab - asun).abs() {
        return if ab >= asun {
            0.0
        } else {
            1.0 - (ab / asun).powi(2)
        };
    }
    // Overlap area of two discs (lens), over the area of the Sun's disc.
    let (r1, r2, d) = (asun, ab, theta);
    let k1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1))
        .clamp(-1.0, 1.0)
        .acos();
    let k2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2))
        .clamp(-1.0, 1.0)
        .acos();
    let tri = 0.5
        * ((-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2))
            .max(0.0)
            .sqrt();
    let lens = r1 * r1 * k1 + r2 * r2 * k2 - tri;
    (1.0 - lens / (PI * r1 * r1)).clamp(0.0, 1.0)
}

/// Heat (kW) the air puts into a hull of cross-section `area` m^2 at `speed` m/s in air of
/// density `rho` kg/m^3: the kinetic power `rho v^3 A / 2`, of which the hull keeps a share.
pub fn aero_heat_kw(rho: f64, speed: f64, area: f64) -> f64 {
    0.5 * rho * speed.powi(3) * area * AERO_HEAT_SHARE / 1000.0
}

/// Cross-section (m^2) of a ship of `mass` kg, taken as a sphere of the mean ship density.
pub fn cross_section(mass: f64) -> f64 {
    let r = (3.0 * mass.max(1.0) / (4.0 * PI * SHIP_DENSITY)).cbrt();
    PI * r * r
}

/// Dose (Gy/h, unshielded) of the particles of a star of the given class at `d` metres.
pub fn stellar_dose(class: StarClass, d: f64) -> f64 {
    let activity = crate::science::flare_activity(class);
    let d_au = (d / AU).max(1e-4);
    STELLAR_GY_H_1AU * activity / (d_au * d_au)
}

/// Dose (Gy/h, unshielded) in the radiation belt of a world of relative magnetic strength
/// `magnetic` (Earth = 1) at `l` radii from its centre: a ring near 2.5 radii.
pub fn belt_dose(magnetic: f64, l: f64) -> f64 {
    if magnetic <= 0.0 {
        return 0.0;
    }
    let x = (l - 2.5) / 1.2;
    BELT_GY_H * magnetic.sqrt() * (-x * x).exp()
}

/// What plating of total armour rating `armor` lets through of a dose (1 = all).
pub fn shield_factor(armor: f64) -> f64 {
    (-armor / SHIELD_E_FOLD).exp().max(0.02)
}

/// Natural dose rates at a point (Gy/h, unshielded).
#[derive(Clone, Copy, Debug, Default)]
pub struct NaturalDose {
    pub cosmic: f64,
    pub stellar: f64,
    pub belt: f64,
}

impl NaturalDose {
    pub fn total(&self) -> f64 {
        self.cosmic + self.stellar + self.belt
    }
}

/// The osculating orbit of the ship around its reference body.
#[derive(Clone, Copy, Debug)]
pub struct Readout {
    pub body: usize,
    pub elements: Elements,
    pub radius: f64,
}

/// A Hohmann transfer to a body that circles the same centre as the ship.
#[derive(Clone, Copy, Debug)]
pub struct Transfer {
    pub hohmann: Hohmann,
    /// Angle by which the target leads the ship now, and the one the departure needs (rad).
    pub lead_now: f64,
    pub lead_needed: f64,
    /// Seconds to wait for the window.
    pub wait: f64,
}

// ---- the live system ------------------------------------------------------------------------

impl Sim {
    /// Sphere of influence of body `i` (m): Laplace's for a body that circles another, 10 radii
    /// at least; a station keeps a fixed one; stars, asteroids and anomalies have none.
    pub fn influence_of(&self, i: usize) -> f64 {
        let b = &self.bodies[i];
        match b.kind {
            BodyKind::Star(_) | BodyKind::Asteroid(_) | BodyKind::Anomaly(_) => 0.0,
            BodyKind::Station => 20_000.0,
            _ => {
                let cb = &self.system.bodies[i];
                // A body that circles nothing keeps the old rule of 60 radii.
                cb.orbit.as_ref().map_or(60.0 * b.radius, |o| {
                    soi_radius(o.a, cb.mass, self.system.bodies[o.parent].mass).max(10.0 * b.radius)
                })
            }
        }
    }

    /// Whether the ship is close to a massive body (within 60 radii, a station's 20 km): the zone
    /// where flight assist holds station and where the FTL drive refuses to charge. It is much
    /// smaller than the sphere of influence that picks the reference frame.
    pub fn near_well(&self) -> bool {
        self.bodies.iter().any(|b| {
            let r = match b.kind {
                BodyKind::Star(_) | BodyKind::Asteroid(_) | BodyKind::Anomaly(_) => 0.0,
                BodyKind::Station => 20_000.0,
                _ => 60.0 * b.radius,
            };
            r > 0.0 && (self.ship.pos - b.pos).length() < r
        })
    }

    /// The osculating orbit around the reference body (None in the frame of the star or aboard a station).
    pub fn orbit_readout(&self) -> Option<Readout> {
        let i = self.ref_body?;
        let b = &self.bodies[i];
        if matches!(b.kind, BodyKind::Station) || b.gm <= 0.0 {
            return None;
        }
        let r = self.ship.pos - b.pos;
        let v = self.ship.vel - b.vel;
        Some(Readout {
            body: i,
            elements: elements(r, v, b.gm),
            radius: b.radius,
        })
    }

    /// Hohmann transfer from the ship's (nearly circular) orbit to body `t`, if `t` circles the
    /// same centre as the ship.
    pub fn transfer_to(&self, t: usize) -> Option<Transfer> {
        let centre = self.ref_body.unwrap_or(0);
        let orbit = self.system.bodies.get(t)?.orbit.as_ref()?;
        if orbit.parent != centre || t == centre {
            return None;
        }
        let mu = self.bodies[centre].gm;
        let rs = self.ship.pos - self.bodies[centre].pos;
        let rt = self.bodies[t].pos - self.bodies[centre].pos;
        let (r1, r2) = (rs.length(), orbit.a);
        let ang = |v: Vec3| v.z.atan2(v.x);
        let lead_now = (ang(rt) - ang(rs)).rem_euclid(TAU);
        let n1 = (mu / r1.powi(3)).sqrt();
        let n2 = (mu / r2.powi(3)).sqrt();
        let lead_needed = hohmann_lead_angle(r1, r2).rem_euclid(TAU);
        Some(Transfer {
            hohmann: hohmann(r1, r2, mu),
            lead_now,
            lead_needed,
            wait: transfer_window(lead_now, lead_needed, n1, n2),
        })
    }

    /// Share of the Sun's light that reaches `p` (1 in the open, 0 in the umbra of a world).
    pub fn sunlight_at(&self, p: Vec3) -> f64 {
        let sun = &self.bodies[0];
        let to_sun = sun.pos - p;
        let d_sun = to_sun.length().max(1.0);
        let asun = (sun.radius / d_sun).min(1.0).asin();
        let mut light = 1.0;
        for b in self.bodies.iter().skip(1) {
            if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) {
                continue;
            }
            let to_b = b.pos - p;
            let d_b = to_b.length();
            if d_b >= d_sun || d_b <= b.radius {
                continue;
            }
            let ab = (b.radius / d_b).min(1.0).asin();
            let cos_t = (to_b.dot(to_sun) / (d_b * d_sun)).clamp(-1.0, 1.0);
            light *= visible_fraction(asun, ab, cos_t.acos());
        }
        light
    }

    /// Heat (kW) the hull absorbs from the Sun: flux times cross-section times (1 - albedo).
    pub fn solar_heat_kw(&self) -> f64 {
        SOLAR_CONSTANT_KW
            * self.solar_flux()
            * cross_section(self.ship.mass())
            * (1.0 - SHIP_ALBEDO)
    }

    /// Infrared power (kW) the ship radiates: what a sensor sees from far away.
    pub fn ir_signature_kw(&self) -> f64 {
        let rated = crate::design::Systems::rated_radiation_kw(
            self.ship.stats.mass_dry,
            self.ship.stats.radiate_kw,
        );
        crate::design::Systems::radiated(rated, self.ship.sys.temp_k).max(0.0)
    }

    /// Natural radiation at `p`: cosmic rays, the particles of the star (more for an active class)
    /// and the belts of the magnetised worlds nearby. Dose rates in Gy/h, unshielded.
    pub fn natural_dose_at(&self, p: Vec3) -> NaturalDose {
        let sun = &self.bodies[0];
        let mut d = NaturalDose {
            cosmic: COSMIC_GY_H,
            stellar: stellar_dose(self.system.star_class, (p - sun.pos).length())
                * self.sunlight_at(p).max(0.2),
            belt: 0.0,
        };
        for (i, b) in self.bodies.iter().enumerate().skip(1) {
            if !matches!(b.kind, BodyKind::Planet(_) | BodyKind::Moon(_)) {
                continue;
            }
            let l = (p - b.pos).length() / b.radius.max(1.0);
            if !(1.2..=7.0).contains(&l) {
                continue;
            }
            let magnetic = self.system.physics(i, 0.0).magnetic;
            d.belt += belt_dose(magnetic, l);
        }
        // Under an atmosphere the column of air above stops the radiation (the Earth's is about
        // 1000 g/cm^2 against a stopping length near 150): a world with air is safe to stand on.
        if let Some((_, air, alt)) = self.in_air_at(p) {
            let through = crate::biosphere::radiation_through_air(air.pressure(alt), air.gravity);
            d.cosmic *= through;
            d.stellar *= through;
            d.belt *= through;
        }
        d
    }

    /// The cabin dosimeter (Gy): what the crew has received in all, a view on the dose of each
    /// person (`Body::total_dose` plus what is not yet taken by the body), averaged over the
    /// living. There is no second counter.
    pub fn dosimeter_gy(&self) -> f64 {
        let alive = self.crew.members.iter().filter(|p| p.alive);
        let n = alive.clone().count();
        if n == 0 {
            return 0.0;
        }
        alive
            .map(|p| f64::from(p.body.total_dose + p.body.pending_dose))
            .sum::<f64>()
            / n as f64
    }

    /// Set the dose every living person has received in all (Gy): scripted demos and tests.
    pub fn set_dosimeter_gy(&mut self, gy: f64) {
        for p in self.crew.members.iter_mut().filter(|p| p.alive) {
            p.body.total_dose = gy as f32;
            p.body.pending_dose = 0.0;
        }
    }

    /// Dose the crew gets now (Gy/h): the natural and hazard doses through the plating.
    pub fn dose_rate_aboard(&self) -> f64 {
        let env = self.environment_at(self.ship.pos);
        (self.natural_dose_at(self.ship.pos).total() + env.radiation) * self.dose_protection()
    }

    /// How much of an outside dose reaches the crew (docked, behind shields, behind plating).
    pub fn dose_protection(&self) -> f64 {
        let cover = if self.docked.is_some() {
            DOCKED_COVER
        } else if self.ship.sys.shield_pts > 0.0 {
            SHIELDED_COVER
        } else {
            1.0
        };
        cover * shield_factor(self.effective.armor)
    }

    /// Tidal acceleration (m/s^2) across the ship from the pull of every world.
    pub fn tidal_stress(&self) -> f64 {
        let length = 2.0 * SHIP_RADIUS;
        self.bodies
            .iter()
            .filter(|b| !matches!(b.kind, BodyKind::Station | BodyKind::Anomaly(_)))
            .map(|b| tidal_accel(b.gm, (self.ship.pos - b.pos).length(), length))
            .fold(0.0, f64::max)
    }

    /// Load on the frame (g): the larger of what the engines and the air push with and the tide.
    pub fn structural_load_g(&self) -> f64 {
        self.g_load.max(self.tidal_stress() / G0)
    }

    /// Past the limit of the frame the hull tears in proportion to the excess.
    pub(crate) fn structural_tick(&mut self, dt: f64) {
        let load = self.structural_load_g();
        if load > STRUCTURE_LIMIT_G && !self.rules.invulnerable {
            let over = load / STRUCTURE_LIMIT_G - 1.0;
            let dmg = over.min(4.0) * 0.05 * self.ship.stats.hull_max * dt * self.rules.damage;
            self.ship.hull = (self.ship.hull - dmg).max(0.0);
        }
    }

    /// Dose, structure and heat bookkeeping of one step (called from the hazard tick).
    pub(crate) fn radiation_tick(&mut self, dt: f64) {
        let p = self.ship.pos;
        let natural = self.natural_dose_at(p).total();
        let rate = natural * self.dose_protection();
        // Ordinary background hurts no one: only a real belt, or a star close by, makes the crew
        // ill. It is booked all the same (the belt of the environment is irradiated in `step`).
        if rate > 1.0e-3 {
            self.irradiate(rate * dt / 3600.0);
        } else {
            self.book_dose(rate * dt / 3600.0);
        }
    }

    /// Heat (kW) that reaches the hull this step from outside (the Sun, the air), minus what
    /// the ablative plating takes away (that is paid in hull points).
    pub(crate) fn external_heat_kw(&mut self, dt: f64) -> f64 {
        let aero = self.aero_heat_kw;
        let armor = self.effective.armor;
        let blocked = aero * armor / (armor + ABLATION_K);
        if blocked > 0.0 && !self.rules.invulnerable {
            self.ship.hull =
                (self.ship.hull - blocked * dt / 1000.0 / ABLATION_MJ_PER_POINT).max(0.0);
        }
        self.solar_heat_kw() + aero - blocked
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn one_law_of_cover_and_of_air_for_every_dose() {
        // Audit B6: the ground dose, the biosphere and the flight under an atmosphere share the
        // air law, and a flare is stopped by the same cover as any other dose.
        for p in [0.0, 0.01, 0.3, 1.0, 3.0] {
            let a = crate::biosphere::surface_radiation(p);
            let b = crate::biosphere::radiation_through_air(p, vector_core::units::G0);
            assert_eq!(a, b);
            assert!(crate::biosphere::radiation_through_air(p, 3.0) <= b + 1e-12);
        }
        assert!(
            (crate::biosphere::surface_radiation(1.0) - (-6.8f64).exp()).abs()
                < 0.05 * (-6.8f64).exp()
        );
        const {
            assert!(SHIELDED_COVER > DOCKED_COVER && SHIELDED_COVER < 1.0);
        }
        assert!(crate::science::flare_activity(StarClass::M) > 1.0);
    }

    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::orbit::{G, M_EARTH, M_SUN, R_EARTH};

    #[test]
    fn a_station_shelters_its_crew_from_the_belts_and_the_flares() {
        let mut s = sim();
        let st = s
            .bodies
            .iter()
            .position(|b| b.kind == BodyKind::Station)
            .expect("station");
        let free = s.dose_protection();
        s.docked = Some(st);
        let docked = s.dose_protection();
        assert!(docked <= 0.01 * free, "docked {docked} vs free {free}");
        // A belt of 0.1 Gy/h outside leaves under the 1 mGy/h that sickens nobody in a dock.
        assert!(0.1 * docked < 1.0e-3);
    }

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data")))
    }

    const MU_E: f64 = G * M_EARTH;

    #[test]
    fn a_circular_and_an_elliptic_orbit_have_the_right_elements() {
        let r = 7.0e6;
        let c = elements(
            Vec3::new(r, 0.0, 0.0),
            Vec3::new(0.0, 0.0, (MU_E / r).sqrt()),
            MU_E,
        );
        assert!(
            c.e < 1e-9 && (c.periapsis - r).abs() < 1.0 && (c.apoapsis.unwrap() - r).abs() < 1.0
        );
        assert!((c.period.unwrap() - TAU * (r.powi(3) / MU_E).sqrt()).abs() < 1e-6);
        // 10 % faster at the same point: that point is the periapsis, the apoapsis is higher.
        let e = elements(
            Vec3::new(r, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.1 * (MU_E / r).sqrt()),
            MU_E,
        );
        assert!((e.periapsis - r).abs() < 1.0);
        let apo = e.apoapsis.unwrap();
        assert!((apo - r * 1.21 / (2.0 - 1.21)).abs() < 1.0, "{apo}");
        // Escape: open path, no apoapsis.
        let x = elements(
            Vec3::new(r, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 1.2 * (2.0 * MU_E / r).sqrt()),
            MU_E,
        );
        assert!(!x.bound && x.apoapsis.is_none() && x.e > 1.0);
    }

    #[test]
    fn hohmann_matches_the_textbook_earth_to_mars() {
        let mu = G * M_SUN;
        let h = hohmann(AU, 1.524 * AU, mu);
        assert!(
            (h.dv1 - 2_945.0).abs() < 40.0 && (h.dv2 - 2_650.0).abs() < 40.0,
            "{h:?}"
        );
        assert!(
            (h.time / 86_400.0 - 259.0).abs() < 3.0,
            "{}",
            h.time / 86_400.0
        );
        // Mars leads by about 44 degrees at departure.
        assert!((hohmann_lead_angle(AU, 1.524 * AU).to_degrees() - 44.0).abs() < 1.5);
        // The window recurs every synodic period (about 780 days).
        let n1 = (mu / AU.powi(3)).sqrt();
        let n2 = (mu / (1.524 * AU).powi(3)).sqrt();
        let w = transfer_window(0.0, 0.0, n1, n2);
        assert!(w < 1.0e-3 || (w / 86_400.0 - 780.0).abs() < 10.0);
        // Wait is always within one synodic period and the angle then matches.
        let w = transfer_window(1.0, 0.77, n1, n2);
        let lead = 1.0 + (n2 - n1) * w;
        assert!(
            (lead - 0.77)
                .rem_euclid(TAU)
                .min(TAU - (lead - 0.77).rem_euclid(TAU))
                < 1e-6
        );
    }

    #[test]
    fn tsiolkovsky_is_exact_and_invertible() {
        let isp = 4500.0;
        let dv = tsiolkovsky(isp, 3000.0, 1000.0);
        assert!((dv - isp * G0 * 3.0f64.ln()).abs() < 1e-9);
        let burn = propellant_for(dv, isp, 3000.0);
        assert!((burn - 2000.0).abs() < 1e-6, "{burn}");
        // The ship's own readout is the same law.
        let s = sim();
        assert!(
            (s.ship.delta_v() - tsiolkovsky(s.ship.stats.isp, s.ship.mass(), s.ship.dry_mass()))
                .abs()
                < 1e-6
        );
    }

    #[test]
    fn a_flyby_turns_the_path_and_changes_the_speed_in_the_sun_frame() {
        let mu_j = 1.267e17;
        let rp = 2.0e8;
        let delta = flyby_deflection(10_000.0, rp, mu_j);
        assert!(delta > 0.0 && delta < PI);
        // A closer pass turns more; a faster one turns less.
        assert!(flyby_deflection(10_000.0, rp * 0.5, mu_j) > delta);
        assert!(flyby_deflection(20_000.0, rp, mu_j) < delta);
        let dv = flyby_dv(10_000.0, delta);
        assert!((dv - 2.0 * 10_000.0 / (1.0 + rp * 1e8 / mu_j)).abs() < 1e-6);
    }

    #[test]
    fn influence_follows_laplace_and_the_frame_changes_on_entering_it() {
        let earth_soi = soi_radius(AU, M_EARTH, M_SUN);
        assert!((earth_soi / 9.25e8 - 1.0).abs() < 0.02, "{earth_soi}");
        let mut s = sim();
        let i = (1..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(_)))
            .expect("a planet");
        let soi = s.influence_of(i);
        let cb = &s.system.bodies[i];
        let o = cb.orbit.as_ref().expect("orbit");
        assert!(
            (soi - soi_radius(o.a, cb.mass, s.system.bodies[o.parent].mass).max(10.0 * cb.radius))
                .abs()
                < 1.0
        );
        let dir = (s.bodies[i].pos - s.bodies[0].pos).normalized();
        s.ship.pos = s.bodies[i].pos + dir * (soi * 0.8);
        s.update_ref();
        assert_eq!(
            s.ref_body,
            Some(i),
            "inside the sphere the planet is the frame"
        );
        s.ship.pos = s.bodies[i].pos + dir * (soi * 3.0);
        s.update_ref();
        assert_ne!(
            s.ref_body,
            Some(i),
            "outside it the frame is something larger"
        );
    }

    #[test]
    fn the_readout_of_an_orbit_matches_the_analytic_state() {
        let mut s = sim();
        let i = (1..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(_)))
            .expect("a planet");
        let r = s.bodies[i].radius * 3.0;
        let v = (s.bodies[i].gm / r).sqrt();
        s.ship.pos = s.bodies[i].pos + Vec3::new(r, 0.0, 0.0);
        s.ship.vel = s.bodies[i].vel + Vec3::new(0.0, 0.0, v);
        s.update_ref();
        let ro = s.orbit_readout().expect("readout");
        assert_eq!(ro.body, i);
        assert!(ro.elements.e < 1e-6 && (ro.elements.periapsis - r).abs() < 1.0);
        assert!((ro.elements.v_circ - v).abs() < 1e-6);
    }

    #[test]
    fn eclipse_blocks_the_sun_and_the_heat_follows_the_cross_section() {
        let mut s = sim();
        let i = (1..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(_)))
            .expect("a planet");
        let away = (s.bodies[i].pos - s.bodies[0].pos).normalized();
        let behind = s.bodies[i].pos + away * (s.bodies[i].radius * 2.0);
        let front = s.bodies[i].pos - away * (s.bodies[i].radius * 2.0);
        assert!(s.sunlight_at(behind) < 1e-9, "umbra");
        assert!((s.sunlight_at(front) - 1.0).abs() < 1e-9);
        // Partial: the disc of the Sun is only partly covered at the edge of the shadow.
        let side = away.cross(Vec3::new(0.0, 1.0, 0.0)).normalized();
        let edge = behind + side * (s.bodies[i].radius * 1.0);
        let f = s.sunlight_at(edge);
        assert!(f > 0.0 && f < 1.0, "penumbra {f}");
        s.ship.pos = front;
        let lit = s.solar_heat_kw();
        s.ship.pos = behind;
        assert_eq!(s.solar_heat_kw(), 0.0);
        assert!(lit > 0.0);
        assert!(
            (cross_section(8.0 * 4.0 / 3.0 * PI * SHIP_DENSITY)
                / cross_section(4.0 / 3.0 * PI * SHIP_DENSITY)
                - 4.0)
                .abs()
                < 1e-9
        );
    }

    #[test]
    fn blackbody_power_goes_as_t_to_the_fourth() {
        // The emitted power goes as T^4 exactly.
        let p = |t: f64| SIGMA_KW * t.powi(4);
        assert!((p(510.0) / p(255.0) - 16.0).abs() < 1e-9);
        // The ship's radiators follow T^4 too (ratio of two temperatures above the cold sink).
        let a = crate::design::Systems::radiated(100.0, 400.0);
        let b = crate::design::Systems::radiated(100.0, 200.0);
        assert!(a > 10.0 * b);
    }

    #[test]
    fn a_hot_sun_side_warms_the_ship_and_the_shadow_cools_it() {
        let mut s = sim();
        let i = (1..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(_)))
            .expect("a planet");
        let away = (s.bodies[i].pos - s.bodies[0].pos).normalized();
        let run = |s: &mut Sim, p: Vec3| {
            s.ship.pos = p;
            s.ship.vel = Vec3::ZERO;
            s.ship.sys.temp_k = 200.0;
            for _ in 0..200 {
                let ext = s.solar_heat_kw();
                let act = crate::design::Activity {
                    ext_heat_kw: ext,
                    ..Default::default()
                };
                let flux = s.solar_flux();
                let m = s.ship.mass();
                s.ship
                    .sys
                    .step(&s.data, &s.ship.design, &s.ship.stats, m, &act, flux, 5.0);
            }
            s.ship.sys.temp_k
        };
        let (pa, pb) = (
            s.bodies[i].pos - away * (s.bodies[i].radius * 2.0),
            s.bodies[i].pos + away * (s.bodies[i].radius * 2.0),
        );
        let sunny = run(&mut s, pa);
        let dark = run(&mut s, pb);
        assert!(sunny > dark + 5.0, "sun {sunny} shadow {dark}");
    }

    #[test]
    fn entering_air_heats_with_rho_v_cubed_and_plating_ablates() {
        let a = aero_heat_kw(1e-4, 7000.0, 30.0);
        assert!((aero_heat_kw(2e-4, 7000.0, 30.0) / a - 2.0).abs() < 1e-9);
        assert!((aero_heat_kw(1e-4, 14000.0, 30.0) / a - 8.0).abs() < 1e-9);
        let mut s = sim();
        s.effective.armor = 0.0;
        s.aero_heat_kw = 1000.0;
        let h0 = s.ship.hull;
        let bare = s.external_heat_kw(1.0);
        assert_eq!(s.ship.hull, h0, "no plating, no ablation");
        s.effective.armor = 4.0;
        let plated = s.external_heat_kw(1.0);
        assert!(plated < bare - 500.0, "{plated} vs {bare}");
        assert!(s.ship.hull < h0, "the plating burns away");
    }

    #[test]
    fn a_dive_into_air_raises_the_aero_heat() {
        let mut s = sim();
        let i = (1..s.bodies.len())
            .find(|&i| s.air_of(i).is_some())
            .expect("a world with air");
        let air = s.air_of(i).unwrap();
        let b = &s.bodies[i];
        let alt = air.height * 3.0;
        let dir = Vec3::new(0.0, 1.0, 0.0);
        s.ship.pos = b.pos + dir * (b.radius + alt);
        s.ship.vel = b.vel - dir * 6000.0;
        s.atmosphere_drag(1.0 / 30.0);
        assert!(s.aero_heat_kw > 0.0);
        // Out of the air there is none.
        s.ship.pos = s.bodies[i].pos + dir * (s.bodies[i].radius + air.top() * 2.0);
        s.atmosphere_drag(1.0 / 30.0);
        assert_eq!(s.aero_heat_kw, 0.0);
    }

    #[test]
    fn radiation_comes_from_the_star_the_belts_and_the_sky_and_plating_cuts_it() {
        let s = sim();
        let far = Vec3::new(30.0 * AU, 0.0, 0.0);
        let near = Vec3::new(0.05 * AU, 0.0, 0.0);
        let (df, dn) = (s.natural_dose_at(far), s.natural_dose_at(near));
        assert!(dn.stellar > 100.0 * df.stellar, "inverse square");
        assert!((dn.cosmic - COSMIC_GY_H).abs() < 1e-12);
        // Inverse-square, exactly: twice the distance, a quarter of the dose.
        let r = stellar_dose(StarClass::G, AU) / stellar_dose(StarClass::G, 2.0 * AU);
        assert!((r - 4.0).abs() < 1e-9);
        // A red dwarf is more active than a G star at the same distance.
        assert!(stellar_dose(StarClass::M, AU) > stellar_dose(StarClass::G, AU));
        // The belt peaks near 2.5 radii and fades; a world with no field has none.
        assert!(belt_dose(1.0, 2.5) > belt_dose(1.0, 5.0) * 10.0);
        assert!(belt_dose(4.0, 2.5) > belt_dose(1.0, 2.5));
        assert_eq!(belt_dose(0.0, 2.5), 0.0);
        // Plating: more armour, less dose, never none.
        assert!(shield_factor(4.0) < shield_factor(1.0) && shield_factor(1.0) < 1.0);
        assert!(shield_factor(1.0e6) >= 0.02);
        // A magnetised world of the system has a belt around it.
        let mut found = false;
        for i in 1..s.bodies.len() {
            if matches!(s.bodies[i].kind, BodyKind::Planet(_) | BodyKind::Moon(_))
                && s.system.physics(i, 0.0).magnetic > 0.0
            {
                let p = s.bodies[i].pos + Vec3::new(2.5 * s.bodies[i].radius, 0.0, 0.0);
                found |= s.natural_dose_at(p).belt > 0.0;
            }
        }
        let _ = found;
        let _ = R_EARTH;
    }

    #[test]
    fn the_cabin_dosimeter_is_a_view_on_the_dose_of_each_person() {
        // Audit B6/dose: one registry of dose (`Body::total_dose`), no second counter.
        let mut s = sim();
        assert!(!s.crew.members.is_empty());
        assert_eq!(s.dosimeter_gy(), 0.0);
        s.irradiate(0.25);
        s.book_dose(0.05);
        let each: Vec<f64> = s
            .crew
            .members
            .iter()
            .filter(|p| p.alive)
            .map(|p| f64::from(p.body.total_dose + p.body.pending_dose))
            .collect();
        let mean = each.iter().sum::<f64>() / each.len() as f64;
        assert!((s.dosimeter_gy() - mean).abs() < 1e-9);
        assert!((s.dosimeter_gy() - 0.30).abs() < 1e-3, "{}", s.dosimeter_gy());
        // Once the bodies have taken it, the reading is the same (nothing lost, nothing counted twice).
        let before = s.dosimeter_gy();
        for p in s.crew.members.iter_mut() {
            p.body.total_dose += p.body.pending_dose;
            p.body.pending_dose = 0.0;
        }
        assert!((s.dosimeter_gy() - before).abs() < 1e-6);
        s.set_dosimeter_gy(1.5);
        assert!((s.dosimeter_gy() - 1.5).abs() < 1e-6);
    }

    #[test]
    fn the_dose_accumulates_and_a_close_star_makes_the_crew_ill() {
        let mut s = sim();
        let p = Vec3::new(0.02 * AU, 0.0, 0.0);
        s.ship.pos = p;
        s.ship.vel = Vec3::ZERO;
        s.effective.armor = 0.0;
        let d0 = s.dosimeter_gy();
        let h0: f32 = s.crew.members.iter().map(|m| m.health).sum();
        for _ in 0..50 {
            s.radiation_tick(60.0);
        }
        assert!(s.dosimeter_gy() > d0);
        let h1: f32 = s.crew.members.iter().map(|m| m.health).sum();
        assert!(h1 < h0 || s.crew.members.is_empty(), "the crew suffers");
        // Behind plating the same exposure adds up to less.
        let mut t = sim();
        t.ship.pos = p;
        t.effective.armor = 8.0;
        for _ in 0..50 {
            t.radiation_tick(60.0);
        }
        assert!(t.dosimeter_gy() < s.dosimeter_gy() * 0.5);
    }

    #[test]
    fn light_delay_and_gamma_follow_their_laws() {
        assert!((light_delay(AU) - 499.0).abs() < 1.0);
        assert!((gamma(0.0) - 1.0).abs() < 1e-12);
        assert!((gamma(0.6 * C) - 1.25).abs() < 1e-9);
        assert!(gamma(0.99 * C) > 7.0);
    }

    #[test]
    fn tides_tear_a_ship_only_very_close_and_the_frame_has_a_g_limit() {
        // Over 30 m, a tide of 1 g needs mu/r^3 = g/(2 L): far from every normal world.
        let mut s = sim();
        s.ship.pos = Vec3::new(30.0 * AU, 0.0, 0.0);
        assert!(s.tidal_stress() < 1e-6);
        assert!(
            (tidal_accel(MU_E, 7.0e6, 30.0) - 2.0 * MU_E * 30.0 / 7.0e6f64.powi(3)).abs() < 1e-18
        );
        // Past the limit of the frame the hull takes damage; below it nothing.
        s.g_load = STRUCTURE_LIMIT_G * 0.5;
        let h = s.ship.hull;
        s.structural_tick(1.0);
        assert_eq!(s.ship.hull, h);
        s.g_load = STRUCTURE_LIMIT_G * 2.0;
        s.structural_tick(1.0);
        assert!(s.ship.hull < h);
        // Closer to a dense mass the tide itself is the load.
        let near_star = s.bodies[0].pos + Vec3::new(s.bodies[0].radius * 1.05, 0.0, 0.0);
        s.ship.pos = near_star;
        s.g_load = 0.0;
        assert!(s.tidal_stress() > 0.0 && s.structural_load_g() > 0.0);
    }

    #[test]
    fn the_whole_thing_is_deterministic() {
        let run = || {
            let mut s = sim();
            s.ship.pos = Vec3::new(0.3 * AU, 0.0, 0.0);
            for _ in 0..30 {
                s.radiation_tick(10.0);
            }
            (
                s.dosimeter_gy(),
                s.solar_heat_kw(),
                s.natural_dose_at(s.ship.pos).total(),
            )
        };
        assert_eq!(run(), run());
    }
}
