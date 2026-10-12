//! Assisted transfer ("pilote automatique"): the ship accelerates towards the target, flips and
//! brakes along a planned straight line in the frame of the target, arriving at rest at the
//! stopping altitude. The plan is analytic, so it works whatever the time step and however
//! deep the gravity wells on the way; the engines really burn (fuel, heat, power) as in
//! manual flight, but the ship's position follows the plan.
use crate::{ShipInput, Sim};
use vector_core::Vec3;
use vector_world::BodyKind;

/// Largest gravity (m/s^2) at the place where the autopilot stops: deeper wells (giant
/// planets) are approached by hand.
const MAX_STOP_GRAVITY: f64 = 10.0;
const MIN_DISTANCE: f64 = 100.0;

pub enum State {
    Running,
    Arrived,
}

#[derive(Clone, Debug)]
pub struct Autopilot {
    pub target: usize,
    /// Cruise acceleration (m/s^2), chosen from the available delta-v.
    pub a_cruise: f64,
    /// Altitude above the target surface at which we stop.
    pub stop_alt: f64,
    /// Position of the ship relative to the target when the plan began.
    r0: Vec3,
    /// Direction of travel in the frame of the target, and the length of the trip.
    u: Vec3,
    len: f64,
    /// Time since the plan began, and its total duration.
    t: f64,
    total: f64,
}

/// What the autopilot asks of the ship in one step.
pub struct Ride {
    pub input: ShipInput,
    /// Direction to point the nose.
    pub face: Option<Vec3>,
    pub state: State,
    /// Where the ship must be, relative to the target, and how fast.
    pub rel_pos: Vec3,
    pub rel_vel: Vec3,
}

impl Autopilot {
    pub fn new(sim: &Sim, target: usize) -> Result<Self, &'static str> {
        let b = sim.bodies.get(target).ok_or("cible inconnue")?;
        let stop_alt = match b.kind {
            BodyKind::Station => 300.0,
            BodyKind::Star(_) => b.radius * 3.0,
            _ => (0.5 * b.radius).max(300.0),
        };
        let r_stop = b.radius + stop_alt;
        if b.gm / (r_stop * r_stop) > MAX_STOP_GRAVITY {
            return Err("gravite trop forte pour le pilote auto : approchez a la main");
        }
        let r0 = sim.ship.pos - b.pos;
        let dist = r0.length();
        let len = dist - r_stop;
        if len < MIN_DISTANCE {
            return Err("deja a destination");
        }
        if sim.ship.stats.thrust <= 0.0 {
            return Err("pas de propulsion");
        }
        let a_main = sim.ship.stats.thrust / sim.ship.mass();
        let dv = sim.ship.delta_v();
        // Speed relative to the target is cancelled first (the plan starts at rest in its frame).
        let rel = (sim.ship.vel - b.vel).length();
        let dv_eff = dv - rel;
        // A better-computed transfer wastes less of the delta-v: the pilotage budget.
        let dv_plan = dv_eff * (0.85 + 0.15 * sim.mind_factor(crate::mind::T_PILOTAGE));
        if dv_eff < 0.3 * dv {
            return Err("carburant insuffisant pour rattraper la cible");
        }
        let a_cruise = (dv_plan * dv_plan / (16.0 * len)).min(0.5 * a_main);
        if a_cruise < 1.0e-3 {
            return Err("carburant insuffisant");
        }
        let u = r0 * (-1.0 / dist);
        Ok(Autopilot {
            target,
            a_cruise,
            stop_alt,
            r0,
            u,
            len,
            t: 0.0,
            total: 2.0 * (len / a_cruise).sqrt(),
        })
    }

    /// Remaining duration of the trip, seconds.
    pub fn eta(&self) -> f64 {
        (self.total - self.t).max(0.0)
    }

    /// Advance the plan by `dt` and say where the ship should be and what the engines do.
    pub fn ride(&mut self, sim: &Sim, dt: f64) -> Ride {
        self.t += dt;
        let a = self.a_cruise;
        let half = self.total / 2.0;
        let (s, v, face_sign) = if self.t >= self.total {
            (self.len, 0.0, -1.0)
        } else if self.t < half {
            (0.5 * a * self.t * self.t, a * self.t, 1.0)
        } else {
            let tau = self.total - self.t;
            (self.len - 0.5 * a * tau * tau, a * tau, -1.0)
        };
        let state = if self.t >= self.total {
            State::Arrived
        } else {
            State::Running
        };
        let a_main = (sim.ship.stats.thrust / sim.ship.mass()).max(1e-9);
        let input = if matches!(state, State::Running) {
            ShipInput {
                fwd: (a / a_main).clamp(0.0, 1.0),
                ..Default::default()
            }
        } else {
            ShipInput::default()
        };
        Ride {
            input,
            face: Some(self.u * face_sign),
            state,
            rel_pos: self.r0 + self.u * s,
            rel_vel: self.u * v,
        }
    }
}

impl Sim {
    /// Engage the autopilot towards `target`. The speed relative to the target is cancelled at
    /// once and paid for with fuel (rocket equation); the transfer then follows its plan.
    pub fn engage_autopilot(&mut self, target: usize) -> Result<(), &'static str> {
        let ap = Autopilot::new(self, target)?;
        let rel = self.ship.vel - self.bodies[target].vel;
        let dv = rel.length();
        let ship = &mut self.ship;
        let burn = ship.mass() * (1.0 - (-dv / (ship.stats.isp.max(1.0) * crate::G0)).exp());
        if burn > ship.fuel {
            return Err("carburant insuffisant pour rattraper la cible");
        }
        ship.fuel -= burn;
        ship.vel = self.bodies[target].vel;
        self.autopilot = Some(ap);
        self.parked = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scene::new_sim;
    use std::sync::Arc;
    use vector_data::GameData;

    fn sim_seed(seed: u64) -> Sim {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
        new_sim(seed, Arc::new(GameData::load(&dir).expect("data loads")))
    }

    #[test]
    fn the_autopilot_reaches_every_reachable_world_from_the_start_without_a_crash_and_stays_there()
    {
        let mut tested = 0;
        let mut burned_any = false;
        for seed in [7u64, 31, 41] {
            let n = sim_seed(seed).bodies.len();
            for p in 0..n {
                let mut s = sim_seed(seed);
                if !matches!(
                    s.bodies[p].kind,
                    BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Station
                ) {
                    continue;
                }
                // The far outer worlds take years: this is about the way, not the range.
                if (s.bodies[p].pos - s.ship.pos).length() > 1.5e11 {
                    continue;
                }
                if s.engage_autopilot(p).is_err() {
                    continue; // refused: too deep a well, or too little fuel
                }
                tested += 1;
                let fuel0 = s.ship.fuel;
                let mut arrived = false;
                for _ in 0..150 {
                    s.events.clear();
                    s.advance(86_400.0, &crate::ShipInput::default());
                    let hard: Vec<String> = s
                        .events
                        .iter()
                        .filter_map(|e| match e {
                            crate::Event::Impact { body, speed, .. } if *speed > 20.0 => {
                                Some(format!("{} at {speed:.0} m/s", s.bodies[*body].name))
                            }
                            _ => None,
                        })
                        .collect();
                    assert!(
                        hard.is_empty(),
                        "seed {seed}: crashed on the way to {}: {hard:?}",
                        s.bodies[p].name
                    );
                    if s.events
                        .iter()
                        .any(|e| matches!(e, crate::Event::Arrived { body } if *body == p))
                    {
                        arrived = true;
                        break;
                    }
                }
                assert!(
                    arrived,
                    "seed {seed}: never arrived at {}",
                    s.bodies[p].name
                );
                assert!(
                    s.ship.fuel > 0.0 && s.ship.fuel <= fuel0,
                    "seed {seed} body {p} {}: fuel {} of {fuel0}",
                    s.bodies[p].name,
                    s.ship.fuel
                );
                burned_any |= s.ship.fuel < fuel0;
                // Stays put afterwards (parked above a world), no fall.
                for _ in 0..60 {
                    s.events.clear();
                    s.advance(300.0, &crate::ShipInput::default());
                    assert!(
                        !s.events.iter().any(
                            |e| matches!(e, crate::Event::Impact { speed, .. } if *speed > 20.0)
                        ),
                        "seed {seed}: fell onto {} after arriving",
                        s.bodies[p].name
                    );
                }
            }
        }
        assert!(tested >= 4, "many worlds are reachable, got {tested}");
        assert!(burned_any, "trips cost fuel");
    }

    #[test]
    fn deep_gravity_wells_and_empty_tanks_are_refused_with_a_reason() {
        let mut s = sim_seed(31);
        let giant = (0..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(p) if p.is_giant()))
            .expect("a gas giant in seed 31");
        let err = s.engage_autopilot(giant).unwrap_err();
        assert!(err.contains("gravite"), "{err}");
        assert!(s.autopilot.is_none());
        let moon = (0..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(p) if !p.is_giant()))
            .unwrap();
        s.ship.fuel = 1.0;
        assert!(s.engage_autopilot(moon).is_err(), "no fuel");
    }

    #[test]
    fn the_plan_accelerates_then_brakes_and_covers_the_whole_distance() {
        let s = sim_seed(7);
        let p = (0..s.bodies.len())
            .find(|&i| matches!(s.bodies[i].kind, BodyKind::Planet(g) if !g.is_giant()))
            .expect("a planet");
        let mut ap = Autopilot::new(&s, p).expect("accepted");
        let total = ap.total;
        let mut max_v = 0.0f64;
        let mut last = ap.ride(&s, total / 100.0);
        for _ in 0..99 {
            last = ap.ride(&s, total / 100.0);
            max_v = max_v.max(last.rel_vel.length());
        }
        assert!(max_v > 0.0);
        assert!(matches!(last.state, State::Arrived));
        let r_stop = s.bodies[p].radius + ap.stop_alt;
        assert!(
            (last.rel_pos.length() - r_stop).abs() < 1.0,
            "arrives at the stop altitude"
        );
        assert!(last.rel_vel.length() < 1e-9, "at rest");
        // Peak speed is half way: a * total / 2.
        assert!((max_v - ap.a_cruise * total / 2.0).abs() / max_v < 0.05);
    }

    #[test]
    fn the_pilotage_budget_changes_the_plan_of_the_autopilot() {
        let plan = |alloc: [u8; 5]| {
            let mut best = None;
            for p in 0..sim_seed(7).bodies.len() {
                let mut s = sim_seed(7);
                s.ship.sys.temp_k = 290.0;
                s.mind.alloc = alloc;
                if let Ok(ap) = Autopilot::new(&s, p) {
                    best.get_or_insert_with(Vec::new).push(ap.a_cruise);
                }
            }
            best.expect("some reachable body")
        };
        let low = plan([10, 0, 0, 0, 0].map(|w| 10 - w));
        let high = plan([10, 0, 0, 0, 0]);
        assert_eq!(low.len(), high.len());
        assert!(
            low.iter().zip(&high).all(|(l, h)| h >= l),
            "more pilotage never plans a slower trip"
        );
        assert!(
            low.iter().zip(&high).any(|(l, h)| h > l),
            "and speeds up at least the fuel-limited ones"
        );
    }
}
