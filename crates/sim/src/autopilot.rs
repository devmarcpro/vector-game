//! Fuel-limited brachistochrone autopilot: accelerate toward the target, flip, brake.
use crate::{ShipInput, Sim, DRY_MASS, MAIN_THRUST};
use vector_world::BodyKind;

pub enum State {
    Running,
    Arrived,
}

#[derive(Clone, Debug)]
pub struct Autopilot {
    pub target: usize,
    /// Cruise acceleration (m/s^2), chosen from available delta-v.
    pub a_cruise: f64,
    /// Altitude above the target surface at which we stop.
    pub stop_alt: f64,
    tol: f64,
}

impl Autopilot {
    pub fn new(sim: &Sim, target: usize) -> Result<Self, &'static str> {
        let b = sim.bodies.get(target).ok_or("cible inconnue")?;
        let stop_alt = match b.kind {
            BodyKind::Station => 300.0,
            BodyKind::Star(_) => b.radius * 3.0,
            _ => (0.5 * b.radius).max(300.0),
        };
        let d_surf = (b.pos - sim.ship.pos).length() - b.radius - stop_alt;
        if d_surf < 100.0 {
            return Err("deja a destination");
        }
        let a_main = MAIN_THRUST / (DRY_MASS + sim.ship.fuel);
        let dv = sim.ship.delta_v();
        let a_cruise = (dv * dv / (16.0 * d_surf)).min(0.5 * a_main);
        if a_cruise < 1.0e-3 {
            return Err("carburant insuffisant");
        }
        Ok(Autopilot {
            target,
            a_cruise,
            stop_alt,
            tol: (0.001 * d_surf).max(50.0),
        })
    }

    fn geometry(&self, sim: &Sim) -> (vector_core::Vec3, f64, vector_core::Vec3) {
        let b = &sim.bodies[self.target];
        let r = b.pos - sim.ship.pos;
        let d = r.length();
        (
            r * (1.0 / d.max(1e-9)),
            d - b.radius - self.stop_alt,
            sim.ship.vel - b.vel,
        )
    }

    /// Upper bound on the step so the terminal phase stays accurate.
    pub fn max_dt(&self, sim: &Sim) -> f64 {
        let (_, d_surf, _) = self.geometry(sim);
        0.05 * (d_surf.max(1.0) / self.a_cruise).sqrt()
    }

    /// Returns the control input, the desired facing direction, and the state.
    pub fn control(&mut self, sim: &Sim, dt: f64) -> (ShipInput, Option<vector_core::Vec3>, State) {
        let (dir, d_surf, vr) = self.geometry(sim);
        let a_c = self.a_cruise;
        let vtol = 1.5 * (2.0 * a_c * self.tol).sqrt() + 1.0;
        if d_surf < self.tol && vr.length() < vtol {
            return (ShipInput::default(), None, State::Arrived);
        }
        let v_des = 0.85 * (2.0 * a_c * d_surf.max(0.0)).sqrt();
        let e = dir * v_des - vr;
        let emag = e.length();
        if emag < 1e-6 {
            return (ShipInput::default(), None, State::Running);
        }
        let edir = e * (1.0 / emag);
        let db = sim.ship.rot.transpose().mul_vec(edir);
        let yaw_err = db.x.atan2(db.z);
        let pitch_err = db.y.atan2(db.x.hypot(db.z));
        let aligned = dt > 0.1 || yaw_err.hypot(pitch_err) < 0.1;
        let a_main = MAIN_THRUST / sim.ship.mass();
        let throttle = if aligned {
            (a_c / a_main) * (emag / (2.0 * a_c)).min(1.0)
        } else {
            0.0
        };
        let k = 0.8;
        let input = ShipInput {
            yaw: (k * yaw_err).clamp(-1.0, 1.0),
            pitch: (-k * pitch_err).clamp(-1.0, 1.0),
            fwd: throttle,
            ..Default::default()
        };
        (input, Some(edir), State::Running)
    }
}
