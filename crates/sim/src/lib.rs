//! Deterministic simulation: flight model, gravity, collisions, autopilot.
//! Units: SI (m, s, kg, N). See docs/SIM_MODEL.md.
pub mod autopilot;

use autopilot::Autopilot;
use vector_core::{Mat3, Vec3};
use vector_world::{BodyKind, System};

pub const G0: f64 = 9.806_65;
/// Base fixed step (30 Hz).
pub const SIM_DT: f64 = 1.0 / 30.0;
/// Smallest sub-step used near surfaces.
pub const MIN_DT: f64 = 1.0 / 240.0;

pub const DRY_MASS: f64 = 20_000.0;
pub const FUEL_CAP: f64 = 8_000.0;
pub const MAIN_THRUST: f64 = 1.4e6;
pub const RCS_THRUST: f64 = 2.8e5;
pub const ISP: f64 = 200_000.0;
pub const MAX_ANG_RATE: f64 = 1.5;
pub const ANG_ACCEL: f64 = 2.0;
pub const HULL_MAX: f64 = 1000.0;
pub const SHIP_RADIUS: f64 = 15.0;

/// Player (or NPC) control demand, each axis in [-1, 1].
/// pitch>0 = nose down, yaw>0 = nose right, roll>0 = roll left (right-handed about body axes).
#[derive(Clone, Copy, Debug, Default)]
pub struct ShipInput {
    pub pitch: f64,
    pub yaw: f64,
    pub roll: f64,
    pub fwd: f64,
    pub lat: f64,
    pub vert: f64,
}

#[derive(Clone, Debug)]
pub struct Ship {
    /// Position in the system's inertial star-centred frame.
    pub pos: Vec3,
    pub vel: Vec3,
    /// Ship-to-world rotation; columns are right, up, forward.
    pub rot: Mat3,
    /// Angular velocity in body frame (pitch, yaw, roll) rad/s.
    pub ang: Vec3,
    pub fuel: f64,
    pub hull: f64,
    pub assist: bool,
}

impl Ship {
    pub fn new(pos: Vec3, vel: Vec3) -> Self {
        Ship {
            pos,
            vel,
            rot: Mat3::IDENTITY,
            ang: Vec3::ZERO,
            fuel: FUEL_CAP,
            hull: HULL_MAX,
            assist: true,
        }
    }
    pub fn mass(&self) -> f64 {
        DRY_MASS + self.fuel
    }
    pub fn forward(&self) -> Vec3 {
        self.rot.col(2)
    }
    /// Remaining delta-v (m/s), Tsiolkovsky.
    pub fn delta_v(&self) -> f64 {
        ISP * G0 * (self.mass() / DRY_MASS).ln()
    }
}

/// Live state of a system body at the current time.
#[derive(Clone, Debug)]
pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub radius: f64,
    pub gm: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Impact {
        body: usize,
        speed: f64,
        damage: f64,
    },
    OutOfFuel,
    Arrived {
        body: usize,
    },
}

pub struct Sim {
    pub system: System,
    pub ship: Ship,
    pub bodies: Vec<Body>,
    /// Seconds since the system epoch.
    pub time: f64,
    pub events: Vec<Event>,
    pub autopilot: Option<Autopilot>,
    /// Body used as velocity reference for flight assist (within its influence radius).
    pub ref_body: Option<usize>,
}

/// Tsiolkovsky: propellant mass flow (kg/s) for a thrust force (N).
pub fn mass_flow(force: f64) -> f64 {
    force.abs() / (ISP * G0)
}

fn influence_radius(b: &Body) -> f64 {
    match b.kind {
        BodyKind::Star(_) => 0.0,
        BodyKind::Station => 20_000.0,
        _ => 60.0 * b.radius,
    }
}

impl Sim {
    pub fn new(system: System, ship: Ship) -> Self {
        let bodies = system
            .bodies
            .iter()
            .map(|b| Body {
                name: b.name.clone(),
                kind: b.kind,
                pos: Vec3::ZERO,
                vel: Vec3::ZERO,
                radius: b.radius,
                gm: b.gm(),
            })
            .collect();
        let mut s = Sim {
            system,
            ship,
            bodies,
            time: 0.0,
            events: Vec::new(),
            autopilot: None,
            ref_body: None,
        };
        s.refresh_bodies();
        s.update_ref();
        s
    }

    /// Place the ship at rest relative to body `i`, `dist` metres behind it (-Z), inheriting its velocity.
    pub fn spawn_near(&mut self, i: usize, dist: f64) {
        let b = &self.bodies[i];
        self.ship.pos = b.pos - Vec3::new(0.0, 0.0, dist);
        self.ship.vel = b.vel;
        self.update_ref();
    }

    pub fn refresh_bodies(&mut self) {
        for (i, b) in self.bodies.iter_mut().enumerate() {
            let (p, v) = self.system.state(i, self.time);
            b.pos = p;
            b.vel = v;
        }
    }

    fn update_ref(&mut self) {
        let mut best: Option<(usize, f64)> = None;
        for (i, b) in self.bodies.iter().enumerate() {
            let infl = influence_radius(b);
            if infl <= 0.0 {
                continue;
            }
            let ratio = (self.ship.pos - b.pos).length() / infl;
            if ratio < 1.0 && best.is_none_or(|(_, r)| ratio < r) {
                best = Some((i, ratio));
            }
        }
        self.ref_body = best.map(|(i, _)| i);
    }

    /// Ship velocity relative to the reference body (or the star frame when none).
    pub fn rel_velocity(&self) -> Vec3 {
        match self.ref_body {
            Some(i) => self.ship.vel - self.bodies[i].vel,
            None => self.ship.vel,
        }
    }

    /// Gravitational acceleration at `p` (stations are massless for gravity).
    pub fn gravity_at(&self, p: Vec3) -> Vec3 {
        let mut a = Vec3::ZERO;
        for b in &self.bodies {
            if b.kind == BodyKind::Station {
                continue;
            }
            let d = b.pos - p;
            let r2 = d.dot(d).max(1.0);
            a = a + d * (b.gm / (r2 * r2.sqrt()));
        }
        a
    }

    /// Largest step that keeps integration accurate near attractors and surfaces.
    pub fn safe_dt(&self) -> f64 {
        let mut dt = f64::MAX;
        for b in &self.bodies {
            let d = (self.ship.pos - b.pos).length();
            let alt = (d - b.radius).max(1.0);
            if b.gm > 1e6 {
                dt = dt.min(0.02 * (d.powi(3) / b.gm).sqrt());
            }
            let vrel = (self.ship.vel - b.vel).length().max(1e-3);
            dt = dt.min(0.15 * alt / vrel);
        }
        if let Some(ap) = &self.autopilot {
            dt = dt.min(ap.max_dt(self));
        }
        dt.clamp(MIN_DT, 120.0)
    }

    /// Advance by `seconds` of game time, sub-stepping adaptively (time acceleration).
    pub fn advance(&mut self, seconds: f64, input: &ShipInput) {
        let mut left = seconds;
        let mut guard = 0;
        while left > 1e-9 && guard < 200_000 {
            guard += 1;
            let dt = left.min(self.safe_dt());
            self.step_dt(input, dt);
            left -= dt;
        }
    }

    /// One standard fixed step.
    pub fn step(&mut self, input: &ShipInput) {
        self.events.clear();
        self.step_dt(input, SIM_DT);
    }

    pub fn step_dt(&mut self, manual: &ShipInput, dt: f64) {
        let input = match self.autopilot.take() {
            Some(mut ap) => {
                let (inp, face, state) = ap.control(self, dt);
                if let (Some(f), true) = (face, dt > 0.1) {
                    // Time-accelerated: perfect attitude control (no meaningful rotation dynamics at this dt).
                    self.ship.rot = Mat3::look_at(f, self.ship.rot.col(1));
                    self.ship.ang = Vec3::ZERO;
                }
                match state {
                    autopilot::State::Running => self.autopilot = Some(ap),
                    autopilot::State::Arrived => {
                        self.events.push(Event::Arrived { body: ap.target })
                    }
                }
                inp
            }
            None => *manual,
        };
        let clamp = |v: f64| v.clamp(-1.0, 1.0);
        let assist = self.ship.assist;
        let lin_assist = assist && self.ref_body.is_some();
        let ref_vel = self.ref_body.map_or(Vec3::ZERO, |i| self.bodies[i].vel);
        let grav = self.gravity_at(self.ship.pos);
        let s = &mut self.ship;

        // Angular control: chase target rate. Assist always chases (zero when idle).
        let demand = Vec3::new(clamp(input.pitch), clamp(input.yaw), clamp(input.roll));
        let target = demand * MAX_ANG_RATE;
        let sel = |cur: f64, d: f64, tgt: f64| -> f64 {
            if d != 0.0 || assist {
                cur + (tgt - cur).clamp(-ANG_ACCEL * dt, ANG_ACCEL * dt)
            } else {
                cur
            }
        };
        s.ang = Vec3::new(
            sel(s.ang.x, demand.x, target.x),
            sel(s.ang.y, demand.y, target.y),
            sel(s.ang.z, demand.z, target.z),
        );
        let w = s.rot.mul_vec(s.ang);
        let rate = w.length();
        if rate > 1e-12 {
            s.rot = Mat3::from_axis_angle(w * (1.0 / rate), rate * dt)
                .mul(&s.rot)
                .orthonormalized();
        }

        // Linear control in body frame.
        let mass = s.mass();
        let cmd = Vec3::new(clamp(input.lat), clamp(input.vert), clamp(input.fwd));
        let rcs_a = RCS_THRUST / mass;
        let main_a = MAIN_THRUST / mass;
        let vel_body = s.rot.transpose().mul_vec(s.vel - ref_vel);
        let axis_accel = |c: f64, v: f64, amax: f64| -> f64 {
            if c != 0.0 {
                c * amax
            } else if lin_assist {
                (-v / dt).clamp(-amax, amax)
            } else {
                0.0
            }
        };
        let z_max = if cmd.z > 0.0 || (cmd.z == 0.0 && vel_body.z < 0.0) {
            main_a
        } else {
            rcs_a
        };
        let mut acc_b = Vec3::new(
            axis_accel(cmd.x, vel_body.x, rcs_a),
            axis_accel(cmd.y, vel_body.y, rcs_a),
            axis_accel(cmd.z, vel_body.z, z_max),
        );
        if cmd.z > 0.0 {
            acc_b.z = cmd.z * main_a;
        }
        let force = Vec3::new(acc_b.x.abs(), acc_b.y.abs(), acc_b.z.abs()) * mass;
        let burn = mass_flow(force.x + force.y + force.z) * dt;
        if burn > s.fuel {
            if s.fuel > 0.0 {
                self.events.push(Event::OutOfFuel);
            }
            s.fuel = 0.0;
            acc_b = Vec3::ZERO;
        } else {
            s.fuel -= burn;
        }
        s.vel = s.vel + (s.rot.mul_vec(acc_b) + grav) * dt;
        s.pos = s.pos + s.vel * dt;
        self.time += dt;
        self.refresh_bodies();
        self.update_ref();
        self.collide();
    }

    fn collide(&mut self) {
        let s = &mut self.ship;
        for (i, b) in self.bodies.iter().enumerate() {
            let d = s.pos - b.pos;
            let dist = d.length();
            let min = b.radius + SHIP_RADIUS;
            if dist < min && dist > 1e-9 {
                let n = d * (1.0 / dist);
                let vn = (s.vel - b.vel).dot(n);
                s.pos = b.pos + n * min;
                if vn < 0.0 {
                    let speed = -vn;
                    let damage = (0.5 * s.mass() * speed * speed / 5.0e6).min(HULL_MAX * 2.0);
                    if speed > 5.0 {
                        s.hull = (s.hull - damage).max(0.0);
                    }
                    s.vel = s.vel - n * (vn * 1.2);
                    self.events.push(Event::Impact {
                        body: i,
                        speed,
                        damage,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_world::generate_system;

    /// A system with a single, nearly massless star: gravity-free flight.
    fn void_system(star_radius: f64) -> System {
        let mut s = generate_system(1);
        s.bodies.truncate(1);
        s.belts.clear();
        s.bodies[0].mass = 1.0e3;
        s.bodies[0].radius = star_radius;
        s
    }

    fn void_sim() -> Sim {
        Sim::new(
            void_system(1.0),
            Ship::new(Vec3::new(0.0, 0.0, -1.0e9), Vec3::ZERO),
        )
    }

    fn first_planet(sys: &System) -> usize {
        sys.bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(_)))
            .expect("system has a planet")
    }

    #[test]
    fn thrust_burns_fuel_per_tsiolkovsky() {
        let mut s = void_sim();
        let m0 = s.ship.mass();
        let input = ShipInput {
            fwd: 1.0,
            ..Default::default()
        };
        for _ in 0..300 {
            s.step(&input);
        }
        let ideal = ISP * G0 * (m0 / s.ship.mass()).ln();
        let dv = s.ship.vel.length();
        assert!(
            (dv - ideal).abs() / ideal < 0.02,
            "dv {dv} vs ideal {ideal}"
        );
    }

    #[test]
    fn assist_stops_rotation_and_drift_near_a_reference_body() {
        let mut s = Sim::new(void_system(100.0), Ship::new(Vec3::ZERO, Vec3::ZERO));
        // Treat the massless "star" as a moon so it can serve as reference frame.
        s.bodies[0].kind = BodyKind::Moon(vector_world::PlanetKind::Barren);
        s.ship.pos = Vec3::new(0.0, 0.0, -3000.0);
        s.ship.ang = Vec3::new(1.0, 0.5, -1.0);
        s.ship.vel = Vec3::new(30.0, -20.0, 50.0);
        s.update_ref();
        assert_eq!(s.ref_body, Some(0));
        for _ in 0..300 {
            s.step(&ShipInput::default());
        }
        assert!(s.ship.ang.length() < 1e-9);
        assert!(s.rel_velocity().length() < 1e-3, "{:?}", s.rel_velocity());
    }

    #[test]
    fn no_assist_keeps_momentum() {
        let mut s = void_sim();
        s.ship.assist = false;
        s.ship.vel = Vec3::new(0.0, 0.0, 100.0);
        s.ship.ang = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..90 {
            s.step(&ShipInput::default());
        }
        assert!((s.ship.vel.length() - 100.0).abs() < 1e-6);
        assert!((s.ship.ang.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_stays_orthonormal() {
        let mut s = void_sim();
        let input = ShipInput {
            pitch: 1.0,
            yaw: 0.7,
            roll: -0.4,
            ..Default::default()
        };
        for _ in 0..3000 {
            s.step(&input);
        }
        let (f, r) = (s.ship.rot.col(2), s.ship.rot.col(0));
        assert!((f.length() - 1.0).abs() < 1e-9 && f.dot(r).abs() < 1e-9);
    }

    #[test]
    fn collision_pushes_out_and_damages() {
        let mut s = Sim::new(
            void_system(500.0),
            Ship::new(Vec3::new(0.0, 0.0, -1000.0), Vec3::new(0.0, 0.0, 400.0)),
        );
        let mut hit = false;
        for _ in 0..90 {
            s.step(&ShipInput::default());
            hit |= s.events.iter().any(|e| matches!(e, Event::Impact { .. }));
            assert!(s.ship.pos.length() >= 500.0 + SHIP_RADIUS - 1e-6);
        }
        assert!(hit && s.ship.hull < HULL_MAX);
    }

    #[test]
    fn deterministic_replay() {
        let run = || {
            let mut s = Sim::new(
                generate_system(5),
                Ship::new(Vec3::new(1.0e9, 0.0, 0.0), Vec3::ZERO),
            );
            for i in 0..500 {
                let a = (i as f64 * 0.1).sin();
                s.step(&ShipInput {
                    pitch: a,
                    fwd: 1.0,
                    yaw: -a,
                    ..Default::default()
                });
            }
            (s.ship.pos, s.ship.fuel)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn ship_in_circular_orbit_stays_bound() {
        let sys = generate_system(11);
        let pi = first_planet(&sys);
        let mut s = Sim::new(sys, Ship::new(Vec3::ZERO, Vec3::ZERO));
        let (p, gm, r) = (s.bodies[pi].pos, s.bodies[pi].gm, s.bodies[pi].radius);
        let orbit_r = r * 3.0;
        let v = (gm / orbit_r).sqrt();
        s.ship.pos = p + Vec3::new(orbit_r, 0.0, 0.0);
        s.ship.vel = s.bodies[pi].vel + Vec3::new(0.0, 0.0, v);
        s.ship.assist = false;
        let period = std::f64::consts::TAU * (orbit_r.powi(3) / gm).sqrt();
        s.advance(period * 2.0, &ShipInput::default());
        let d = (s.ship.pos - s.bodies[pi].pos).length();
        assert!(
            (d - orbit_r).abs() / orbit_r < 0.05,
            "orbit radius drifted: {d} vs {orbit_r}"
        );
        assert_eq!(s.ship.hull, HULL_MAX);
    }

    #[test]
    fn autopilot_reaches_a_planet() {
        let sys = generate_system(11);
        let pi = first_planet(&sys);
        let mut s = Sim::new(sys, Ship::new(Vec3::ZERO, Vec3::ZERO));
        s.ship.pos = s.bodies[pi].pos + Vec3::new(2.0e9, 3.0e8, 1.0e9);
        s.ship.vel = s.bodies[pi].vel;
        s.update_ref();
        s.autopilot = Some(Autopilot::new(&s, pi).expect("autopilot should accept"));
        let mut arrived = false;
        for _ in 0..4000 {
            s.events.clear();
            s.advance(600.0, &ShipInput::default());
            if s.events.iter().any(|e| matches!(e, Event::Arrived { .. })) {
                arrived = true;
                break;
            }
        }
        let d = (s.bodies[pi].pos - s.ship.pos).length();
        assert!(arrived, "autopilot did not arrive, dist {d}");
        assert!(s.ship.hull == HULL_MAX && s.ship.fuel > 0.0);
    }
}
