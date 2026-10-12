//! Arriving at a world without crashing on it: the air slows a ship that dives into it, a gentle
//! contact is a landing (the ship stays parked on the ground until it thrusts), and the orbit
//! assist puts the ship on a circular orbit at its present altitude, paid in fuel like any burn.
use crate::{Event, Sim, G0};
use vector_core::Vec3;
use vector_world::BodyKind;

/// Contact speed (m/s) into the ground up to which the gear lands the ship cleanly (see landing.rs).
pub const LANDING_SPEED: f64 = crate::landing::GEAR_V_IN;
/// The strongest braking of the air (m/s^2).
pub const MAX_DRAG: f64 = 150.0;
/// The orbit assist works within this many radii of the world.
pub const ORBIT_RANGE_RADII: f64 = 12.0;

impl Sim {
    /// Whether body `i` is a solid world a ship can land on.
    pub fn landable(&self, i: usize) -> bool {
        matches!(self.bodies[i].kind, BodyKind::Planet(k) | BodyKind::Moon(k) if !k.is_giant())
    }

    /// The world the ship rests on, if it is landed.
    pub fn landed_on(&self) -> Option<usize> {
        self.parked
            .map(|(i, _)| i)
            .filter(|&i| self.landable(i) && self.docked.is_none())
            .filter(|&i| {
                (self.ship.pos - self.bodies[i].pos).length()
                    < self.bodies[i].radius + self.local_ground(i).0 + crate::SHIP_RADIUS + 1.0
            })
    }

    /// The air brakes a ship moving fast through the atmosphere of a world: the thicker the air at
    /// its height (pressure, in bar), the harder.
    pub(crate) fn atmosphere_drag(&mut self, dt: f64) {
        let mut decel = Vec3::ZERO;
        self.aero_heat_kw = 0.0;
        for i in 0..self.bodies.len() {
            let Some(air) = self.air_of(i) else { continue };
            let alt = (self.ship.pos - self.bodies[i].pos).length() - self.bodies[i].radius;
            if alt >= air.top() || alt < 0.0 {
                continue;
            }
            // The wind of the day over this ground moves the air itself (see `links`).
            let wind = self.wind_over_ship(i);
            let b = &self.bodies[i];
            // The air turns with the world: speed is measured against it, not against the centre.
            let rel =
                self.ship.vel - b.vel - self.surface_velocity(i, self.ship.pos - b.pos) - wind;
            let v = rel.length();
            if v < 1.0 {
                continue;
            }
            let density = air.pressure(alt).min(10.0);
            // Heat of the entry: kinetic power of the air on the hull (rho from the ideal gas law).
            let rho = air.pressure(alt) * 1.0e5 * air.mass / 1000.0 / (8.314 * air.temp.max(30.0));
            self.aero_heat_kw += crate::navigation::aero_heat_kw(
                rho,
                v,
                crate::navigation::cross_section(self.ship.mass()),
            );
            let a = (0.0015 * density * v * v)
                .min(MAX_DRAG)
                .min(v / dt.max(1e-6));
            decel = decel - rel * (a / v);
        }
        if decel != Vec3::ZERO {
            self.ship.vel = self.ship.vel + decel * dt;
        }
    }

    /// Put the ship on a circular orbit around the nearest world at its present altitude.
    pub fn orbit_here(&mut self) -> Result<String, &'static str> {
        if self.docked.is_some() || self.jump.is_some() {
            return Err("impossible pour l'instant");
        }
        let (i, _) = (0..self.bodies.len())
            .filter(|&i| {
                matches!(
                    self.bodies[i].kind,
                    BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Star(_)
                ) && self.bodies[i].gm > 0.0
            })
            .map(|i| {
                (
                    i,
                    (self.ship.pos - self.bodies[i].pos).length() / self.bodies[i].radius,
                )
            })
            .filter(|&(_, r)| r < ORBIT_RANGE_RADII)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or("aucun monde assez proche pour une orbite")?;
        let b = &self.bodies[i];
        let rel_pos = self.ship.pos - b.pos;
        let r = rel_pos.length();
        if r < b.radius * 1.02 {
            return Err("trop bas pour une orbite : montez d'abord");
        }
        let radial = rel_pos * (1.0 / r);
        let rel_vel = self.ship.vel - b.vel;
        let mut tang = rel_vel - radial * rel_vel.dot(radial);
        if tang.length() < 1.0 {
            tang = radial.cross(Vec3::new(0.0, 1.0, 0.0));
            if tang.length() < 1e-6 {
                tang = radial.cross(Vec3::new(1.0, 0.0, 0.0));
            }
        }
        let v_circ = (b.gm / r).sqrt();
        let new_rel = tang.normalized() * v_circ;
        let dv = (new_rel - rel_vel).length();
        let burn = {
            let s = &self.ship;
            s.mass() * (1.0 - (-dv / (s.stats.isp.max(1.0) * G0)).exp())
        };
        if burn > self.ship.fuel {
            return Err("carburant insuffisant pour la mise en orbite");
        }
        self.ship.fuel -= burn;
        self.ship.vel = b.vel + new_rel;
        self.autopilot = None;
        self.parked = None;
        let period = std::f64::consts::TAU * r / v_circ;
        let msg = format!(
            "ORBITE CIRCULAIRE : ALTITUDE {:.0} KM, PERIODE {:.0} MIN, DELTA-V {:.0} M/S",
            (r - b.radius) / 1000.0,
            period / 60.0,
            dv
        );
        self.events.push(Event::Orbit { body: i });
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use crate::ShipInput;
    use std::sync::Arc;
    use vector_core::Mat3;
    use vector_data::GameData;

    fn sim() -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(5, Arc::new(GameData::load(&dir).expect("data")))
    }

    /// A solid world with air (and one without) and the ship put near it.
    fn world(with_air: bool) -> (Sim, usize) {
        for seed in 1..80u64 {
            let mut s = new_sim(
                seed,
                Arc::new(
                    GameData::load(
                        &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"),
                    )
                    .expect("data"),
                ),
            );
            for i in 0..s.bodies.len() {
                if s.landable(i) && (s.system.bodies[i].pressure > 0.3) == with_air {
                    s.ship.fuel = s.ship.stats.fuel_cap;
                    return (s, i);
                }
            }
        }
        panic!("no such world");
    }

    #[test]
    fn a_gentle_contact_is_a_landing_and_a_hard_one_a_crash() {
        let (mut s, i) = world(false);
        let b = s.bodies[i].clone();
        let up = Vec3::new(0.0, 0.0, -1.0);
        // Gentle: 5 m/s into the ground.
        s.ship.pos = b.pos + up * (b.radius + crate::SHIP_RADIUS + 0.5);
        s.ship.vel = b.vel + s.surface_velocity(i, s.ship.pos - b.pos) - up * 5.0;
        s.gear_down = true;
        s.ship.rot = Mat3::look_at(up, Vec3::new(0.0, 1.0, 0.0));
        let hull = s.ship.hull;
        let mut told = false;
        for _ in 0..40 {
            s.step(&ShipInput::default());
            told |= s
                .events
                .iter()
                .any(|e| matches!(e, Event::Landed { body } if *body == i));
        }
        assert!(told, "the landing is announced");
        assert_eq!(s.landed_on(), Some(i), "landed");
        assert_eq!(s.ship.hull, hull, "no damage");
        assert!(
            (s.ship.vel - s.bodies[i].vel - s.surface_velocity(i, s.ship.pos - s.bodies[i].pos))
                .length()
                < 1.0
        );
        // Staying put over time, even as the world moves.
        for _ in 0..300 {
            s.step(&ShipInput::default());
        }
        assert_eq!(s.landed_on(), Some(i));
        // Thrust beyond the weight takes it off the ground again (nose up, main engine).
        s.ship.rot = Mat3::look_at(up, Vec3::new(0.0, 1.0, 0.0));
        let up_thrust = ShipInput {
            fwd: 1.0,
            ..Default::default()
        };
        for _ in 0..60 {
            s.step(&up_thrust);
        }
        assert!(s.parked.is_none());
        // Hard: 500 m/s into the ground hurts.
        let (mut t, j) = world(false);
        let b = t.bodies[j].clone();
        t.ship.pos = b.pos + up * (b.radius + crate::SHIP_RADIUS + 5.0);
        t.ship.vel = b.vel - up * 500.0;
        let h = t.ship.hull;
        for _ in 0..20 {
            t.step(&ShipInput::default());
        }
        assert!(t.ship.hull < h, "a crash costs hull");
        assert_eq!(t.landed_on(), None);
    }

    #[test]
    fn the_air_brakes_a_dive_but_the_void_does_not() {
        let dive = |with_air: bool| -> f64 {
            let (mut s, i) = world(with_air);
            let b = s.bodies[i].clone();
            let up = Vec3::new(0.0, 0.0, -1.0);
            // Entering the air at 3 km/s, 60 % of the way up.
            let alt = s.air_of(i).map_or(100_000.0, |a| a.height);
            s.ship.pos = b.pos + up * (b.radius + alt);
            s.ship.vel = b.vel - up * 3000.0;
            s.rules.invulnerable = true;
            let v0 = (s.ship.vel - b.vel).length();
            for _ in 0..60 {
                s.step(&ShipInput::default());
            }
            (s.ship.vel - s.bodies[i].vel).length() / v0
        };
        let (air, void) = (dive(true), dive(false));
        assert!(air < 0.95, "air slows the dive: {air}");
        assert!(void > 0.99, "no air, no braking: {void}");
    }

    #[test]
    fn the_orbit_assist_makes_a_stable_circular_orbit_for_some_fuel() {
        let (mut s, i) = world(true);
        let b = s.bodies[i].clone();
        // Falling toward the world from 4 radii, fast.
        let dir = Vec3::new(1.0, 0.0, 0.0);
        s.ship.pos = b.pos + dir * (b.radius * 4.0);
        s.ship.vel = b.vel - dir * 3000.0;
        let fuel = s.ship.fuel;
        let msg = s.orbit_here().expect("orbit");
        assert!(msg.starts_with("ORBITE"), "{msg}");
        assert!(s.ship.fuel < fuel, "paid in fuel");
        let r0 = (s.ship.pos - s.bodies[i].pos).length();
        let (mut lo, mut hi) = (r0, r0);
        for _ in 0..2000 {
            s.step(&ShipInput::default());
            let r = (s.ship.pos - s.bodies[i].pos).length();
            lo = lo.min(r);
            hi = hi.max(r);
        }
        assert!(
            lo > r0 * 0.9 && hi < r0 * 1.1,
            "the orbit holds: {lo:.0}..{hi:.0} around {r0:.0}"
        );
        assert!(s.landed_on().is_none());
        // No fuel, no orbit; too low, no orbit; far from everything, no orbit.
        let (mut t, k) = world(true);
        let bb = t.bodies[k].clone();
        t.ship.pos = bb.pos + dir * (bb.radius * 4.0);
        t.ship.vel = bb.vel - dir * 3000.0;
        t.ship.fuel = 0.0;
        assert!(t.orbit_here().is_err());
        let mut u = sim();
        u.ship.pos = u.ship.pos + Vec3::new(1.0e14, 0.0, 0.0);
        assert!(u.orbit_here().is_err());
    }
}
