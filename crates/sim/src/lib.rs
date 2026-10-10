//! Deterministic fixed-step simulation: flight model, bodies, collisions.
//! Units: SI (m, s, kg, N). See docs/SIM_MODEL.md.
use vector_core::{Mat3, Vec3};

pub const G0: f64 = 9.806_65;
pub const SIM_DT: f64 = 1.0 / 30.0;

pub const DRY_MASS: f64 = 20_000.0;
pub const FUEL_CAP: f64 = 8_000.0;
pub const MAIN_THRUST: f64 = 1.4e6;
pub const RCS_THRUST: f64 = 2.8e5;
pub const ISP: f64 = 30_000.0;
pub const MAX_ANG_RATE: f64 = 1.5;
pub const ANG_ACCEL: f64 = 2.0;
pub const HULL_MAX: f64 = 1000.0;
pub const SHIP_RADIUS: f64 = 15.0;

/// Player (or NPC) control demand, each axis in [-1, 1].
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
    pub fn new(pos: Vec3) -> Self {
        Ship {
            pos,
            vel: Vec3::ZERO,
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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BodyKind {
    Star,
    Planet,
    Moon,
    Asteroid,
    Station,
}

#[derive(Clone, Debug)]
pub struct Body {
    pub name: String,
    pub kind: BodyKind,
    pub pos: Vec3,
    pub radius: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Impact {
        body: usize,
        speed: f64,
        damage: f64,
    },
    OutOfFuel,
}

pub struct Sim {
    pub ship: Ship,
    pub bodies: Vec<Body>,
    pub time: f64,
    pub events: Vec<Event>,
}

/// Tsiolkovsky: propellant mass flow (kg/s) for a thrust force (N).
pub fn mass_flow(force: f64) -> f64 {
    force.abs() / (ISP * G0)
}

impl Sim {
    pub fn new(ship: Ship, bodies: Vec<Body>) -> Self {
        Sim {
            ship,
            bodies,
            time: 0.0,
            events: Vec::new(),
        }
    }

    pub fn step(&mut self, input: &ShipInput) {
        let dt = SIM_DT;
        self.events.clear();
        let s = &mut self.ship;
        let clamp = |v: f64| v.clamp(-1.0, 1.0);

        // Angular control: target rate per axis, accelerate toward it.
        let demand = Vec3::new(clamp(input.pitch), clamp(input.yaw), clamp(input.roll));
        let target = demand * MAX_ANG_RATE;
        let assist = s.assist;
        // With assist: always chase target (zero when idle). Without: only when commanded.
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
        let w = s.rot.mul_vec(s.ang); // world-frame angular velocity
        let rate = w.length();
        if rate > 1e-12 {
            let dr = Mat3::from_axis_angle(w * (1.0 / rate), rate * dt);
            s.rot = dr.mul(&s.rot).orthonormalized();
        }

        // Linear control in body frame.
        let mass = s.mass();
        let cmd = Vec3::new(clamp(input.lat), clamp(input.vert), clamp(input.fwd));
        let rcs_a = RCS_THRUST / mass;
        let main_a = MAIN_THRUST / mass;
        let vel_body = s.rot.transpose().mul_vec(s.vel);
        let axis_accel = |c: f64, v: f64, amax: f64| -> f64 {
            if c != 0.0 {
                c * amax
            } else if assist {
                // Counter-thrust, no overshoot past zero this step.
                (-v / dt).clamp(-amax, amax)
            } else {
                0.0
            }
        };
        let fwd_max = if cmd.z >= 0.0 { main_a } else { rcs_a };
        let mut acc_b = Vec3::new(
            axis_accel(cmd.x, vel_body.x, rcs_a),
            axis_accel(cmd.y, vel_body.y, rcs_a),
            axis_accel(
                cmd.z,
                vel_body.z,
                fwd_max.max(if vel_body.z > 0.0 { rcs_a } else { main_a }),
            ),
        );
        // Fuel use from the force actually produced.
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
        s.vel = s.vel + s.rot.mul_vec(acc_b) * dt;
        s.pos = s.pos + s.vel * dt;

        // Collisions with spherical bodies.
        for (i, b) in self.bodies.iter().enumerate() {
            let d = s.pos - b.pos;
            let dist = d.length();
            let min = b.radius + SHIP_RADIUS;
            if dist < min && dist > 1e-9 {
                let n = d * (1.0 / dist);
                let vn = s.vel.dot(n);
                s.pos = b.pos + n * min;
                if vn < 0.0 {
                    let speed = -vn;
                    let energy = 0.5 * s.mass() * speed * speed;
                    let damage = (energy / 5.0e6).min(HULL_MAX * 2.0);
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
        self.time += dt;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sim() -> Sim {
        Sim::new(Ship::new(Vec3::ZERO), vec![])
    }

    #[test]
    fn thrust_burns_fuel_per_tsiolkovsky() {
        let mut s = sim();
        let m0 = s.ship.mass();
        let input = ShipInput {
            fwd: 1.0,
            ..Default::default()
        };
        for _ in 0..300 {
            s.step(&input);
        }
        let m1 = s.ship.mass();
        let dv = s.ship.vel.length();
        let ideal = ISP * G0 * (m0 / m1).ln();
        assert!(
            (dv - ideal).abs() / ideal < 0.02,
            "dv {dv} vs ideal {ideal}"
        );
    }

    #[test]
    fn assist_stops_rotation_and_drift() {
        let mut s = sim();
        s.ship.ang = Vec3::new(1.0, 0.5, -1.0);
        s.ship.vel = Vec3::new(30.0, -20.0, 50.0);
        for _ in 0..300 {
            s.step(&ShipInput::default());
        }
        assert!(s.ship.ang.length() < 1e-9);
        assert!(s.ship.vel.length() < 1e-6, "{:?}", s.ship.vel);
    }

    #[test]
    fn no_assist_keeps_momentum() {
        let mut s = sim();
        s.ship.assist = false;
        s.ship.vel = Vec3::new(0.0, 0.0, 100.0);
        s.ship.ang = Vec3::new(0.0, 1.0, 0.0);
        for _ in 0..90 {
            s.step(&ShipInput::default());
        }
        assert!((s.ship.vel.length() - 100.0).abs() < 1e-9);
        assert!((s.ship.ang.y - 1.0).abs() < 1e-9);
    }

    #[test]
    fn rotation_stays_orthonormal() {
        let mut s = sim();
        let input = ShipInput {
            pitch: 1.0,
            yaw: 0.7,
            roll: -0.4,
            ..Default::default()
        };
        for _ in 0..3000 {
            s.step(&input);
        }
        let f = s.ship.rot.col(2);
        let r = s.ship.rot.col(0);
        assert!((f.length() - 1.0).abs() < 1e-9 && f.dot(r).abs() < 1e-9);
    }

    #[test]
    fn collision_pushes_out_and_damages() {
        let body = Body {
            name: "rock".into(),
            kind: BodyKind::Moon,
            pos: Vec3::new(0.0, 0.0, 1000.0),
            radius: 500.0,
        };
        let mut s = Sim::new(Ship::new(Vec3::ZERO), vec![body]);
        s.ship.vel = Vec3::new(0.0, 0.0, 400.0);
        let mut hit = false;
        for _ in 0..90 {
            s.step(&ShipInput::default());
            hit |= s.events.iter().any(|e| matches!(e, Event::Impact { .. }));
            assert!(
                (s.ship.pos - Vec3::new(0.0, 0.0, 1000.0)).length() >= 500.0 + SHIP_RADIUS - 1e-6
            );
        }
        assert!(hit && s.ship.hull < HULL_MAX);
    }

    #[test]
    fn deterministic_replay() {
        let run = || {
            let mut s = sim();
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
}
