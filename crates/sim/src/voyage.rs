//! Interstellar travel and discovery: FTL jumps, system persistence (deltas), progressive scanning.
use crate::design::Activity;
use crate::market::Market;
use crate::{Event, Sim};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use vector_core::{hash_coord, Mat3, Rng, Vec3};
use vector_world::galaxy::{StarEntry, StarRef};
use vector_world::{generate_system, BodyKind};

/// Seconds of charging before the drive engages.
pub const CHARGE_TIME: f64 = 20.0;
/// Game hours spent in transit per light year.
pub const HOURS_PER_LY: f64 = 6.0;
/// Propellant per light year, per kg of ship mass.
pub const FUEL_PER_LY_PER_KG: f64 = 1.5e-3;
/// Base sensor reach of any ship (m); scanner modules add to it.
pub const BASE_SENSOR_RANGE: f64 = 3.0e7;
/// A star is "seen" on the galactic map when this close to a visited system (ly).
pub const SEEN_RADIUS_LY: f64 = 15.0;

pub const SCAN_UNKNOWN: u8 = 0;
pub const SCAN_DETECTED: u8 = 1;
pub const SCAN_SURVEYED: u8 = 2;
pub const SCAN_FULL: u8 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JumpState {
    pub target: StarRef,
    pub target_pos: [f64; 3],
    pub target_seed: u64,
    pub dist_ly: f64,
    pub charge_left: f64,
    pub transit_left: f64,
    pub total_transit: f64,
    pub fuel_cost: f64,
    pub in_transit: bool,
    pub mishap: bool,
}

/// What changed in a system the player has left (everything else is regenerated from the seed).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SystemDelta {
    pub mined: Vec<f64>,
    pub markets: BTreeMap<usize, Market>,
    pub scan: Vec<u8>,
    pub scan_progress: Vec<f32>,
}

impl Sim {
    /// Default knowledge when a system is entered: its star and large worlds are visible from afar.
    pub fn initial_scan(system: &vector_world::System) -> Vec<u8> {
        system
            .bodies
            .iter()
            .map(|b| match b.kind {
                BodyKind::Star(_) => SCAN_FULL,
                BodyKind::Planet(_) | BodyKind::Moon(_) if b.radius >= 1.0e6 => SCAN_DETECTED,
                _ => SCAN_UNKNOWN,
            })
            .collect()
    }

    /// Reveal everything (creative mode / tests).
    pub fn reveal_all(&mut self) {
        self.scan.fill(SCAN_FULL);
    }

    /// Distance within which body `i` is first detected, given the current sensors.
    pub fn detect_range(&self, i: usize) -> f64 {
        let size = (self.system.bodies[i].radius / 6.0e6)
            .sqrt()
            .clamp(0.02, 5.0);
        (BASE_SENSOR_RANGE + self.effective.scan_range) * size
    }

    /// Progressive scanning: close passes upgrade what the player knows about a body.
    pub(crate) fn scan_tick(&mut self, dt: f64) {
        let sci = if self.crew.count() > 0 {
            let role = crate::crew::Crew::role_index(&self.data, "scientist");
            self.crew.role_efficiency(&self.data, role, self.time) as f32
        } else {
            0.7
        };
        for i in 0..self.bodies.len() {
            if self.scan[i] >= SCAN_FULL {
                continue;
            }
            let d = (self.bodies[i].pos - self.ship.pos).length() - self.bodies[i].radius;
            let range = self.detect_range(i);
            if d > range {
                continue;
            }
            if self.scan[i] == SCAN_UNKNOWN {
                self.scan[i] = SCAN_DETECTED;
            }
            if self.scan[i] == SCAN_DETECTED && d < range * 0.3 {
                self.scan_progress[i] += dt as f32;
                if self.scan_progress[i] >= 10.0 {
                    self.scan[i] = SCAN_SURVEYED;
                    self.scan_progress[i] = 0.0;
                }
            } else if self.scan[i] == SCAN_SURVEYED && d < range * 0.08 {
                self.scan_progress[i] += dt as f32 * sci;
                if self.scan_progress[i] >= 40.0 {
                    self.scan[i] = SCAN_FULL;
                    self.scan_progress[i] = 0.0;
                }
            }
        }
    }

    // ---- FTL -----------------------------------------------------------

    pub fn jump_range(&self) -> f64 {
        self.ship.stats.jump_range
    }

    pub fn jump_fuel_cost(&self, dist_ly: f64) -> f64 {
        FUEL_PER_LY_PER_KG * self.ship.mass() * dist_ly
    }

    /// Stars the galactic map lets the player see: those near any visited system.
    pub fn is_seen(&self, pos: [f64; 3]) -> bool {
        self.visited.values().any(|v| {
            let d = [v[0] - pos[0], v[1] - pos[1], v[2] - pos[2]];
            (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt() <= SEEN_RADIUS_LY
        })
    }

    /// Begin charging the FTL drive toward `target`.
    pub fn start_jump(&mut self, target: &StarEntry) -> Result<(), &'static str> {
        if self.docked.is_some() {
            return Err("decollez d'abord");
        }
        if self.jump.is_some() {
            return Err("saut deja engage");
        }
        if target.id == self.here {
            return Err("deja dans ce systeme");
        }
        let range = self.jump_range();
        if range <= 0.0 {
            return Err("aucun propulseur FTL installe");
        }
        let d = target.distance_to(self.here_pos);
        if d > range {
            return Err("hors de portee de saut");
        }
        if self.ref_body.is_some() {
            return Err("trop pres d'un corps massif");
        }
        let cost = self.jump_fuel_cost(d);
        if cost > self.ship.fuel {
            return Err("carburant insuffisant pour ce saut");
        }
        self.autopilot = None;
        self.jump = Some(JumpState {
            target: target.id,
            target_pos: target.pos,
            target_seed: target.seed,
            dist_ly: d,
            charge_left: CHARGE_TIME,
            transit_left: d * HOURS_PER_LY * 3600.0,
            total_transit: d * HOURS_PER_LY * 3600.0,
            fuel_cost: cost,
            in_transit: false,
            mishap: false,
        });
        self.events.push(Event::JumpCharging);
        Ok(())
    }

    pub fn cancel_jump(&mut self) {
        if self.jump.as_ref().is_some_and(|j| !j.in_transit) {
            self.jump = None;
            self.events.push(Event::JumpAborted("saut annule"));
        }
    }

    pub fn in_transit(&self) -> bool {
        self.jump.as_ref().is_some_and(|j| j.in_transit)
    }

    pub fn is_charging_jump(&self) -> bool {
        self.jump.as_ref().is_some_and(|j| !j.in_transit)
    }

    /// Called every flight step while a jump is charging.
    pub(crate) fn charge_tick(&mut self, dt: f64) {
        let Some(j) = self.jump.as_mut() else { return };
        if j.in_transit {
            return;
        }
        if !self.effective.jump_range.max(0.0).is_finite() || self.effective.jump_range < j.dist_ly
        {
            // The drive is not delivering power (shed, damaged or out of range): abort.
            self.jump = None;
            self.events
                .push(Event::JumpAborted("propulseur FTL sans energie"));
            return;
        }
        j.charge_left -= dt;
        if j.charge_left > 0.0 {
            return;
        }
        if j.fuel_cost > self.ship.fuel {
            self.jump = None;
            self.events
                .push(Event::JumpAborted("carburant insuffisant"));
            return;
        }
        self.ship.fuel -= j.fuel_cost;
        // Risk of a mishap grows with distance and drive wear.
        let wear = self
            .ship
            .design
            .modules(&self.data)
            .filter(|(_, m)| m.jump_range > 0.0)
            .map(|(i, _)| self.ship.sys.health[i])
            .fold(1.0f64, f64::min);
        let risk = (0.01 * j.dist_ly * (1.6 - wear)).min(0.5);
        let mut rng = Rng::new(hash_coord(
            self.system.seed,
            (self.time * 10.0) as i64,
            17,
            3,
        ));
        j.mishap = rng.f64() < risk;
        j.in_transit = true;
        self.ship.vel = Vec3::ZERO;
        self.ship.ang = Vec3::ZERO;
        self.events.push(Event::JumpStarted { dist_ly: j.dist_ly });
    }

    /// One step of FTL transit: the ship is frozen, time and the crew layer keep running.
    pub(crate) fn transit_step(&mut self, dt: f64) {
        self.time += dt;
        self.run_systems(&Activity::default(), dt);
        self.scan_tick(0.0);
        let done = match self.jump.as_mut() {
            Some(j) => {
                j.transit_left -= dt;
                j.transit_left <= 0.0
            }
            None => false,
        };
        if done {
            self.arrive();
        }
    }

    /// Leave the current system (saving its changes) and enter the jump target.
    fn arrive(&mut self) {
        let Some(j) = self.jump.take() else { return };
        let old = SystemDelta {
            mined: std::mem::take(&mut self.mined),
            markets: std::mem::take(&mut self.markets),
            scan: std::mem::take(&mut self.scan),
            scan_progress: std::mem::take(&mut self.scan_progress),
        };
        self.deltas.insert(self.here, old);
        self.visited.insert(self.here, self.here_pos);
        self.visited.insert(j.target, j.target_pos);
        self.here = j.target;
        self.here_pos = j.target_pos;
        self.load_system(generate_system(j.target_seed));
        self.compute_faction();
        self.spawn_traffic();
        // Arrive at the outskirts, in a deterministic direction.
        let mut rng = Rng::new(hash_coord(j.target_seed, 5, 5, 5));
        let frost = self.system.frost_line();
        let dist = (2.0 * frost).max(3.0e11);
        let (a, z) = (rng.range(0.0, std::f64::consts::TAU), rng.range(-0.3, 0.3));
        let dir = Vec3::new(a.cos(), z, a.sin()).normalized();
        self.ship.pos = dir * dist;
        self.ship.vel = Vec3::ZERO;
        self.ship.rot = Mat3::look_at(dir * -1.0, Vec3::new(0.0, 1.0, 0.0));
        self.target = None;
        self.autopilot = None;
        self.update_ref();
        if j.mishap {
            let pick = hash_coord(j.target_seed, self.time as i64, 9, 9);
            self.ship
                .sys
                .damage_modules(&self.data, &self.ship.design, 150.0, pick);
            self.ship.refresh_stats(&self.data);
            self.events.push(Event::JumpMishap);
        }
        self.events.push(Event::JumpArrived { dist_ly: j.dist_ly });
    }

    /// Replace the active system (used on arrival and when loading a save), restoring any saved delta.
    pub(crate) fn load_system(&mut self, system: vector_world::System) {
        let n = system.bodies.len();
        self.bodies = system
            .bodies
            .iter()
            .map(|b| crate::Body {
                name: b.name.clone(),
                kind: b.kind,
                pos: Vec3::ZERO,
                vel: Vec3::ZERO,
                radius: b.radius,
                gm: b.gm(),
            })
            .collect();
        self.system = system;
        match self.deltas.remove(&self.here) {
            Some(d) if d.mined.len() == n && d.scan.len() == n => {
                self.mined = d.mined;
                self.markets = d.markets;
                self.scan = d.scan;
                self.scan_progress = d.scan_progress;
            }
            _ => {
                self.mined = vec![0.0; n];
                self.markets = BTreeMap::new();
                self.scan = Self::initial_scan(&self.system);
                self.scan_progress = vec![0.0; n];
            }
        }
        self.refresh_bodies();
        for i in 0..n {
            let m = self.system.bodies[i].mass;
            if self.mined[i] > 0.0 {
                let frac = (1.0 - self.mined[i] / m).max(0.0);
                self.bodies[i].radius = self.system.bodies[i].radius * frac.cbrt();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShipInput, SIM_DT};
    use std::sync::Arc;
    use vector_data::GameData;
    use vector_world::galaxy::Galaxy;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    fn home() -> (Sim, Galaxy) {
        let g = Galaxy::new(5);
        // A start star with at least two neighbours inside the basic jump range.
        let entry = g
            .nearby([0.0; 3], 30.0)
            .into_iter()
            .find(|e| g.nearby(e.pos, 4.0).len() >= 3)
            .expect("a star with neighbours");
        let sim = Sim::new_in_galaxy(g, &entry, data());
        (sim, g)
    }

    /// Fly the ship out of any gravity well so a jump is allowed.
    fn deep_space(s: &mut Sim) {
        s.ship.pos = Vec3::new(4.0e11, 0.0, 0.0);
        s.ship.vel = Vec3::ZERO;
        s.update_ref();
    }

    #[test]
    fn jump_flow_moves_the_ship_to_another_system() {
        let (mut s, g) = home();
        deep_space(&mut s);
        let from = s.system.seed;
        let targets: Vec<StarEntry> = g
            .nearby(s.here_pos, s.jump_range())
            .into_iter()
            .filter(|e| e.id != s.here)
            .collect();
        let target = targets
            .first()
            .expect("a reachable neighbour within 4 ly")
            .clone();
        let fuel0 = s.ship.fuel;
        s.start_jump(&target).expect("start");
        assert!(s.is_charging_jump());
        let mut arrived = false;
        for _ in 0..400 {
            s.events.clear();
            s.advance(3600.0, &ShipInput::default());
            if s.events
                .iter()
                .any(|e| matches!(e, Event::JumpArrived { .. }))
            {
                arrived = true;
                break;
            }
        }
        assert!(arrived, "jump should complete");
        assert_ne!(s.system.seed, from);
        assert_eq!(s.system.seed, target.seed);
        assert!(s.ship.fuel < fuel0, "jumping burns propellant");
        assert!(s.visited.contains_key(&target.id) && s.visited.len() >= 2);
        assert!(
            s.time > target.distance_to([0.0; 3]) * 0.5 * 3600.0,
            "transit takes game time"
        );
        assert!(
            s.ship.pos.length() >= 3.0e11 - 1.0,
            "arrival at the outskirts"
        );
        assert!(s.jump.is_none());
    }

    #[test]
    fn jump_rules_are_enforced() {
        let (mut s, g) = home();
        let far = g
            .nearby(s.here_pos, 40.0)
            .into_iter()
            .find(|e| e.distance_to(s.here_pos) > 12.0)
            .expect("far star");
        deep_space(&mut s);
        assert!(s.start_jump(&far).is_err(), "out of range");
        let near = g
            .nearby(s.here_pos, 4.0)
            .into_iter()
            .find(|e| e.id != s.here)
            .expect("near");
        s.ship.pos = s.bodies[1].pos + Vec3::new(s.bodies[1].radius * 2.0, 0.0, 0.0);
        s.update_ref();
        assert!(s.start_jump(&near).is_err(), "inside a gravity well");
        deep_space(&mut s);
        s.ship.fuel = 1.0;
        assert!(s.start_jump(&near).is_err(), "not enough fuel");
        s.ship.fuel = s.ship.stats.fuel_cap;
        s.start_jump(&near).expect("ok");
        assert!(s.start_jump(&near).is_err(), "already charging");
        s.cancel_jump();
        assert!(s.jump.is_none());
        // Without a powered drive the charge aborts.
        let ftl = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.category == "ftl")
            .map(|(i, _)| i)
            .expect("ftl");
        s.ship.sys.health[ftl] = 0.0;
        s.ship.refresh_stats(&s.data.clone());
        assert!(s.start_jump(&near).is_err());
    }

    #[test]
    fn system_changes_persist_across_a_round_trip() {
        let (mut s, g) = home();
        deep_space(&mut s);
        s.mined[1] = 1234.0;
        s.markets.insert(0, Market::new(1, 0, &s.data, 0.0, 0));
        let home_id = s.here;
        let home_seed = s.system.seed;
        let target = g
            .nearby(s.here_pos, 4.0)
            .into_iter()
            .find(|e| e.id != home_id)
            .expect("near");
        for hop in [target.clone(), g.star(home_id).expect("home")] {
            deep_space(&mut s);
            s.ship.fuel = s.ship.stats.fuel_cap;
            s.start_jump(&hop).expect("start");
            for _ in 0..300 {
                s.advance(3600.0, &ShipInput::default());
                if s.jump.is_none() {
                    break;
                }
            }
            assert!(s.jump.is_none());
        }
        assert_eq!(s.system.seed, home_seed, "back home");
        assert_eq!(s.mined[1], 1234.0, "mining deltas must come back");
        assert!(s.markets.contains_key(&0), "markets must come back");
    }

    #[test]
    fn scanning_progresses_with_proximity_and_scanner_quality() {
        let (mut s, _) = home();
        let rock = s
            .system
            .bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Asteroid(_)))
            .or_else(|| {
                s.system
                    .bodies
                    .iter()
                    .position(|b| b.kind == BodyKind::Station)
            })
            .expect("small body");
        assert_eq!(s.scan[rock], SCAN_UNKNOWN);
        // Far away: nothing.
        s.ship.pos = s.bodies[rock].pos + Vec3::new(0.0, 0.0, -1.0e9);
        s.scan_tick(1.0);
        assert_eq!(s.scan[rock], SCAN_UNKNOWN);
        // Within basic sensor range of a small body: detected, then surveyed, then fully scanned.
        let r = s.detect_range(rock);
        s.ship.pos = s.bodies[rock].pos + Vec3::new(0.0, 0.0, -(r * 0.9));
        s.scan_tick(SIM_DT);
        assert_eq!(s.scan[rock], SCAN_DETECTED);
        s.ship.pos = s.bodies[rock].pos + Vec3::new(0.0, 0.0, -(r * 0.2));
        for _ in 0..400 {
            s.scan_tick(0.1);
        }
        assert_eq!(s.scan[rock], SCAN_SURVEYED);
        s.ship.pos = s.bodies[rock].pos + Vec3::new(0.0, 0.0, -(r * 0.05));
        for _ in 0..1000 {
            s.scan_tick(0.1);
        }
        assert_eq!(s.scan[rock], SCAN_FULL);
        // A scanner module extends detection range.
        s.effective.scan_range = 3.0e10;
        assert!(s.detect_range(rock) > r * 100.0);
    }

    #[test]
    fn fog_of_war_follows_visited_systems() {
        let (s, g) = home();
        let near = g.nearby(s.here_pos, 10.0);
        assert!(near.iter().all(|e| s.is_seen(e.pos)));
        assert!(!s.is_seen([5000.0, 0.0, 0.0]));
    }
}
