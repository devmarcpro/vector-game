//! Deterministic simulation: flight model, modular ships, gravity, collisions, autopilot, mining, trade, saves.
//! Units: SI (m, s, kg, N). See docs/SIM_MODEL.md.
pub mod autopilot;
pub mod cargo;
pub mod design;
pub mod market;
pub mod save;

use autopilot::Autopilot;
use cargo::Cargo;
use design::{Activity, Effective, ShipDesign, ShipStats, Systems, RCS_ISP};
use market::Market;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use vector_core::{hash_coord, Mat3, Vec3};
use vector_data::GameData;
use vector_world::{BodyKind, System};

pub const G0: f64 = 9.806_65;
/// Base fixed step (30 Hz).
pub const SIM_DT: f64 = 1.0 / 30.0;
/// Smallest sub-step used near surfaces.
pub const MIN_DT: f64 = 1.0 / 240.0;

pub const SHIP_RADIUS: f64 = 15.0;
/// Linear flight assist only acts below this speed relative to the reference body (m/s).
pub const ASSIST_MAX_SPEED: f64 = 500.0;
pub const DOCK_RANGE: f64 = 500.0;
pub const DOCK_MAX_SPEED: f64 = 8.0;
pub const START_CREDITS: f64 = 500.0;
/// Hull repair cost per structure point; modules cost 30 % of their price to restore.
pub const REPAIR_COST: f64 = 2.0;
/// Fraction of the price refunded when selling a module or trading in a hull.
pub const RESALE: f64 = 0.5;

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
    pub mine: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ship {
    /// Position in the system's inertial star-centred frame.
    pub pos: Vec3,
    pub vel: Vec3,
    /// Ship-to-world rotation; columns are right, up, forward.
    pub rot: Mat3,
    /// Angular velocity in body frame (pitch, yaw, roll) rad/s.
    pub ang: Vec3,
    pub fuel: f64,
    /// Structure points (separate from per-module health).
    pub hull: f64,
    pub assist: bool,
    pub cargo: Cargo,
    pub design: ShipDesign,
    pub sys: Systems,
    pub stats: ShipStats,
}

impl Ship {
    pub fn new(pos: Vec3, vel: Vec3, data: &GameData, design: ShipDesign) -> Self {
        let sys = Systems::new(&design, 0.0);
        let mut s = Ship {
            pos,
            vel,
            rot: Mat3::IDENTITY,
            ang: Vec3::ZERO,
            fuel: 0.0,
            hull: 0.0,
            assist: true,
            cargo: Cargo::new(data.resources.len()),
            design,
            sys,
            stats: ShipStats::default(),
        };
        s.refresh_stats(data);
        s.fuel = s.stats.fuel_cap;
        s.hull = s.stats.hull_max;
        s.sys.battery_mj = s.stats.battery_cap;
        s
    }

    /// Recompute derived numbers after any design or health change.
    pub fn refresh_stats(&mut self, data: &GameData) {
        self.stats = ShipStats::compute(data, &self.design, &self.sys.health);
        self.cargo.cap_mass = self.stats.cargo_kg;
        self.cargo.cap_vol = self.stats.cargo_m3;
        self.fuel = self.fuel.min(self.stats.fuel_cap);
        self.hull = self.hull.min(self.stats.hull_max);
        self.sys.battery_mj = self.sys.battery_mj.min(self.stats.battery_cap);
    }

    /// Mass without propellant (hull + modules + cargo).
    pub fn dry_mass(&self) -> f64 {
        self.stats.mass_dry + self.cargo.mass
    }
    pub fn mass(&self) -> f64 {
        self.dry_mass() + self.fuel
    }
    pub fn forward(&self) -> Vec3 {
        self.rot.col(2)
    }
    /// Remaining delta-v (m/s), Tsiolkovsky.
    pub fn delta_v(&self) -> f64 {
        self.stats.isp * G0 * (self.mass() / self.dry_mass()).ln()
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
    CargoFull,
    Docked {
        body: usize,
    },
    Undocked,
    Overheat,
    ModuleLost {
        slot: usize,
    },
}

pub struct Sim {
    pub system: System,
    pub data: Arc<GameData>,
    pub ship: Ship,
    pub bodies: Vec<Body>,
    /// Seconds since the system epoch.
    pub time: f64,
    pub events: Vec<Event>,
    pub autopilot: Option<Autopilot>,
    /// Body used as velocity reference for flight assist (within its influence radius).
    pub ref_body: Option<usize>,
    pub target: Option<usize>,
    pub credits: f64,
    /// Mass removed from each body by mining (kg).
    pub mined: Vec<f64>,
    pub markets: BTreeMap<usize, Market>,
    pub docked: Option<usize>,
    /// True while the mining laser is actually hitting something (for rendering).
    pub laser_on: bool,
    /// Spare modules owned by the player, by module index.
    pub module_stock: Vec<u32>,
    pub effective: Effective,
}

/// Propellant mass flow (kg/s) for a thrust force (N) at a given specific impulse.
pub fn mass_flow(force: f64, isp: f64) -> f64 {
    if isp <= 0.0 {
        0.0
    } else {
        force.abs() / (isp * G0)
    }
}

fn influence_radius(b: &Body) -> f64 {
    match b.kind {
        BodyKind::Star(_) | BodyKind::Asteroid(_) => 0.0,
        BodyKind::Station => 20_000.0,
        _ => 60.0 * b.radius,
    }
}

impl Sim {
    pub fn new(system: System, data: Arc<GameData>) -> Self {
        let n = system.bodies.len();
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
        let design = ShipDesign::starter(&data);
        let ship = Ship::new(Vec3::ZERO, Vec3::ZERO, &data, design);
        let mut s = Sim {
            system,
            module_stock: vec![0; data.modules.len()],
            data,
            ship,
            bodies,
            time: 0.0,
            events: Vec::new(),
            autopilot: None,
            ref_body: None,
            target: None,
            credits: START_CREDITS,
            mined: vec![0.0; n],
            markets: BTreeMap::new(),
            docked: None,
            laser_on: false,
            effective: Effective::default(),
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

    /// Reference frame = innermost body (smallest influence sphere) containing the ship.
    pub(crate) fn update_ref(&mut self) {
        let mut best: Option<(usize, f64)> = None;
        for (i, b) in self.bodies.iter().enumerate() {
            let infl = influence_radius(b);
            if infl > 0.0
                && (self.ship.pos - b.pos).length() < infl
                && best.is_none_or(|(_, r)| infl < r)
            {
                best = Some((i, infl));
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

    /// Gravitational acceleration at `p` (stations and asteroids are negligible).
    pub fn gravity_at(&self, p: Vec3) -> Vec3 {
        let mut a = Vec3::ZERO;
        for b in &self.bodies {
            if matches!(b.kind, BodyKind::Station | BodyKind::Asteroid(_)) {
                continue;
            }
            let d = b.pos - p;
            let r2 = d.dot(d).max(1.0);
            a = a + d * (b.gm / (r2 * r2.sqrt()));
        }
        a
    }

    /// Stellar flux at the ship relative to 1 AU (for solar panels).
    pub fn solar_flux(&self) -> f64 {
        let d_au = self.ship.pos.length() / vector_world::orbit::AU;
        self.system.luminosity / (d_au * d_au).max(1e-6)
    }

    /// Largest step that keeps integration accurate near attractors and surfaces.
    pub fn safe_dt(&self) -> f64 {
        let mut dt = f64::MAX;
        for b in &self.bodies {
            let d = (self.ship.pos - b.pos).length();
            let alt = (d - b.radius).max(1.0);
            if b.gm > 1e6 && !matches!(b.kind, BodyKind::Asteroid(_)) {
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
            let dt = if self.docked.is_some() {
                left.min(60.0)
            } else {
                left.min(self.safe_dt())
            };
            self.step_dt(input, dt);
            left -= dt;
        }
    }

    /// One standard fixed step.
    pub fn step(&mut self, input: &ShipInput) {
        self.events.clear();
        self.step_dt(input, SIM_DT);
    }

    /// The asteroid the laser is currently pointed at, if any (target, in range, aimed).
    fn laser_target(&self, range: f64) -> Option<(usize, vector_world::AstType)> {
        let t = self.target?;
        let BodyKind::Asteroid(class) = self.bodies[t].kind else {
            return None;
        };
        let b = &self.bodies[t];
        let r = b.pos - self.ship.pos;
        let d = r.length();
        if d - b.radius > range || self.remaining_mass(t) <= 1.0 {
            return None;
        }
        let tolerance = 0.06 + (b.radius / d.max(1.0)).atan();
        (self.ship.forward().dot(r * (1.0 / d)) >= tolerance.cos()).then_some((t, class))
    }

    fn run_systems(&mut self, act: &Activity, dt: f64) {
        let flux = self.solar_flux();
        let mass = self.ship.mass();
        let was_hot = self.ship.sys.overheating;
        let lost_before: Vec<bool> = self.ship.sys.health.iter().map(|&h| h <= 0.0).collect();
        self.effective = self.ship.sys.step(
            &self.data,
            &self.ship.design,
            &self.ship.stats,
            mass,
            act,
            flux,
            dt,
        );
        if self.ship.sys.overheating && !was_hot {
            self.events.push(Event::Overheat);
        }
        for (i, &h) in self.ship.sys.health.iter().enumerate() {
            if h <= 0.0 && !lost_before[i] && self.ship.design.slots[i].is_some() {
                self.events.push(Event::ModuleLost { slot: i });
            }
        }
        self.ship.refresh_stats(&self.data);
    }

    pub fn step_dt(&mut self, manual: &ShipInput, dt: f64) {
        self.laser_on = false;
        if let Some(st) = self.docked {
            self.time += dt;
            self.run_systems(&Activity::default(), dt);
            self.refresh_bodies();
            let b = &self.bodies[st];
            self.ship.pos = b.pos - Vec3::new(0.0, 0.0, b.radius + 60.0);
            self.ship.vel = b.vel;
            self.ship.ang = Vec3::ZERO;
            self.update_ref();
            return;
        }
        let input = match self.autopilot.take() {
            Some(mut ap) => {
                let (mut inp, face, state) = ap.control(self, dt);
                if let (Some(f), true) = (face, dt > 0.1) {
                    // Time-accelerated: perfect attitude control (no meaningful rotation dynamics at this dt).
                    self.ship.rot = Mat3::look_at(f, self.ship.rot.col(1));
                    self.ship.ang = Vec3::ZERO;
                    inp.yaw = 0.0;
                    inp.pitch = 0.0;
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

        // Power bus, heat, module health -> what the ship can actually do this step.
        let range_hint = self
            .ship
            .design
            .modules(&self.data)
            .map(|(_, m)| m.mining_range)
            .fold(0.0, f64::max);
        let aimed = input.mine && self.laser_target(range_hint).is_some();
        let rcs_use = [input.pitch, input.yaw, input.roll, input.lat, input.vert]
            .iter()
            .fold(0.0f64, |a, v| a.max(v.abs()));
        let act = Activity {
            throttle: clamp(input.fwd).max(0.0),
            mining: aimed,
            rcs_use,
        };
        self.run_systems(&act, dt);
        let eff = self.effective.clone();

        let assist = self.ship.assist;
        // Linear assist is for station-keeping: off under autopilot and at high relative speed.
        let lin_assist = assist
            && self.autopilot.is_none()
            && self.ref_body.is_some()
            && self.rel_velocity().length() < ASSIST_MAX_SPEED;
        let ref_vel = self.ref_body.map_or(Vec3::ZERO, |i| self.bodies[i].vel);
        let grav = self.gravity_at(self.ship.pos);
        let s = &mut self.ship;

        // Angular control: chase target rate. Assist always chases (zero when idle).
        let demand = Vec3::new(clamp(input.pitch), clamp(input.yaw), clamp(input.roll));
        let target = demand * eff.max_rate;
        let accel = eff.ang_accel;
        let sel = |cur: f64, d: f64, tgt: f64| -> f64 {
            if d != 0.0 || assist {
                cur + (tgt - cur).clamp(-accel * dt, accel * dt)
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
        let rcs_a = eff.rcs_n / mass;
        let main_a = eff.thrust / mass;
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
        // Propellant: main engine at its own isp, RCS-class motion at RCS isp.
        let main_force = if acc_b.z > 0.0 || z_max == main_a {
            acc_b.z.abs() * mass
        } else {
            0.0
        };
        let rcs_force = (acc_b.x.abs() + acc_b.y.abs()) * mass
            + if main_force == 0.0 {
                acc_b.z.abs() * mass
            } else {
                0.0
            };
        let burn = (mass_flow(main_force, eff.isp) + mass_flow(rcs_force, RCS_ISP)) * dt;
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
        if aimed && eff.mining_rate > 0.0 {
            self.mine_tick(dt, eff.mining_rate, eff.mining_range);
        }
    }

    fn collide(&mut self) {
        let mut hits = Vec::new();
        {
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
                        let damage =
                            (0.5 * s.mass() * speed * speed / 5.0e6).min(s.stats.hull_max * 2.0);
                        s.vel = s.vel - n * (vn * 1.2);
                        hits.push((i, speed, damage));
                    }
                }
            }
        }
        for (i, speed, damage) in hits {
            if speed > 5.0 {
                self.ship.hull = (self.ship.hull - damage).max(0.0);
                let pick = hash_coord(self.system.seed, i as i64, (self.time * 1000.0) as i64, 99);
                self.ship
                    .sys
                    .damage_modules(&self.data, &self.ship.design, damage * 0.5, pick);
                self.ship.refresh_stats(&self.data);
            }
            self.events.push(Event::Impact {
                body: i,
                speed,
                damage,
            });
        }
    }

    /// Remaining minable mass of body `i` (kg).
    pub fn remaining_mass(&self, i: usize) -> f64 {
        (self.system.bodies[i].mass - self.mined[i]).max(0.0)
    }

    /// Mining laser: the aimed asteroid within range yields ore into the hold (magnetic scoop).
    fn mine_tick(&mut self, dt: f64, rate: f64, range: f64) {
        let Some((t, class)) = self.laser_target(range) else {
            return;
        };
        self.laser_on = true;
        let want = (rate * dt).min(self.remaining_mass(t));
        let Some(cls) = self.data.asteroids.iter().find(|a| a.id == class.id()) else {
            return;
        };
        let mut taken = 0.0;
        for (rid, frac) in &cls.composition {
            if let Some(res) = self.data.resource_index(rid) {
                taken += self.ship.cargo.add(&self.data, res, want * frac);
            }
        }
        if taken < want * 0.999 {
            self.events.push(Event::CargoFull);
        }
        self.mined[t] += taken;
        let frac = (1.0 - self.mined[t] / self.system.bodies[t].mass).max(0.0);
        self.bodies[t].radius = self.system.bodies[t].radius * frac.cbrt();
    }

    // ---- station services -------------------------------------------------

    /// Market of station `i` (created on first use, relaxed to the current time).
    pub fn market(&mut self, i: usize) -> &mut Market {
        let (seed, time) = (self.system.seed, self.time);
        let data = Arc::clone(&self.data);
        let m = self
            .markets
            .entry(i)
            .or_insert_with(|| Market::new(hash_coord(seed, i as i64, 0, 0), i, &data, time));
        m.relax_to(time);
        m
    }

    pub fn nearest_station(&self) -> Option<(usize, f64)> {
        self.bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.kind == BodyKind::Station)
            .map(|(i, b)| (i, (b.pos - self.ship.pos).length() - b.radius))
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    pub fn try_dock(&mut self) -> Result<usize, &'static str> {
        let (i, range) = self.nearest_station().ok_or("aucune station")?;
        if range > DOCK_RANGE {
            return Err("trop loin de la station");
        }
        if (self.ship.vel - self.bodies[i].vel).length() > DOCK_MAX_SPEED {
            return Err("vitesse relative trop elevee");
        }
        self.docked = Some(i);
        self.autopilot = None;
        self.events.push(Event::Docked { body: i });
        Ok(i)
    }

    pub fn undock(&mut self) {
        if let Some(i) = self.docked.take() {
            let b = &self.bodies[i];
            self.ship.pos = b.pos - Vec3::new(0.0, 0.0, b.radius + 120.0);
            self.ship.vel = b.vel;
            self.ship.rot = Mat3::IDENTITY;
            self.update_ref();
            self.events.push(Event::Undocked);
        }
    }

    /// Sell `kg` of resource `res` (None = everything) to the docked station. Returns credits earned.
    pub fn sell(&mut self, res: usize, kg: Option<f64>) -> Result<f64, &'static str> {
        let st = self.docked.ok_or("non amarre")?;
        let have = self.ship.cargo.kg[res];
        let kg = kg.unwrap_or(have).min(have);
        if kg <= 0.0 {
            return Err("rien a vendre");
        }
        let data = Arc::clone(&self.data);
        let earned = self.market(st).sell(&data, res, kg);
        self.ship.cargo.remove(&data, res, kg);
        self.credits += earned;
        Ok(earned)
    }

    /// Buy up to `kg` of resource `res` into the hold.
    pub fn buy(&mut self, res: usize, kg: f64) -> Result<(f64, f64), &'static str> {
        let st = self.docked.ok_or("non amarre")?;
        let data = Arc::clone(&self.data);
        let ask = self.market(st).ask(&data, res);
        let room = self.ship.cargo.cap_mass - self.ship.cargo.mass;
        let afford = self.credits / ask;
        let kg = kg.min(afford).min(room).max(0.0);
        if kg <= 0.0 {
            return Err("achat impossible (credits ou soute)");
        }
        let (got, cost) = self.market(st).buy(&data, res, kg);
        let accepted = self.ship.cargo.add(&data, res, got);
        // Refund anything the hold could not take (volume limit).
        let refund = if got > 0.0 {
            cost * (got - accepted) / got
        } else {
            0.0
        };
        if got > accepted {
            self.market(st).stock[res] += got - accepted;
        }
        self.credits -= cost - refund;
        Ok((accepted, cost - refund))
    }

    /// Fill the propellant tank at the station's fuel price. Returns (kg, cost).
    pub fn refuel(&mut self) -> Result<(f64, f64), &'static str> {
        let st = self.docked.ok_or("non amarre")?;
        let data = Arc::clone(&self.data);
        let fuel = data
            .resource_index("fuel")
            .ok_or("pas de carburant dans les donnees")?;
        let ask = self.market(st).ask(&data, fuel);
        let need = self.ship.stats.fuel_cap - self.ship.fuel;
        let kg = need.min(self.credits / ask);
        if kg < 1e-6 {
            return Err("deja plein ou fonds insuffisants");
        }
        let (got, cost) = self.market(st).buy(&data, fuel, kg);
        self.ship.fuel += got;
        self.credits -= cost;
        Ok((got, cost))
    }

    /// Restore hull structure and module health. Returns the cost paid.
    pub fn repair(&mut self) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let hull_missing = self.ship.stats.hull_max - self.ship.hull;
        let mut module_cost = 0.0;
        for (i, m) in self.ship.design.modules(&self.data) {
            module_cost += (1.0 - self.ship.sys.health[i]) * m.cost * 0.3;
        }
        let total = hull_missing * REPAIR_COST + module_cost;
        if total < 1e-6 {
            return Err("rien a reparer");
        }
        if total > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= total;
        self.ship.hull = self.ship.stats.hull_max;
        self.ship.sys.health.fill(1.0);
        self.ship.refresh_stats(&self.data);
        Ok(total)
    }

    /// Throw cargo into space.
    pub fn jettison(&mut self, res: usize) -> f64 {
        let data = Arc::clone(&self.data);
        let kg = self.ship.cargo.kg[res];
        self.ship.cargo.remove(&data, res, kg)
    }

    // ---- shipyard ---------------------------------------------------------

    /// Buy a module into the spare-parts stock.
    pub fn shop_buy_module(&mut self, mi: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let cost = self.data.modules.get(mi).ok_or("module inconnu")?.cost;
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        self.module_stock[mi] += 1;
        Ok(cost)
    }

    pub fn shop_sell_module(&mut self, mi: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        if self.module_stock.get(mi).copied().unwrap_or(0) == 0 {
            return Err("aucun exemplaire en stock");
        }
        let v = self.data.modules[mi].cost * RESALE;
        self.module_stock[mi] -= 1;
        self.credits += v;
        Ok(v)
    }

    /// Check the hold still fits after a design change.
    fn design_fits(&self, design: &ShipDesign) -> Result<(), &'static str> {
        let st = ShipStats::compute(&self.data, design, &self.ship.sys.health);
        if self.ship.cargo.mass > st.cargo_kg + 1e-6 || self.ship.cargo.volume > st.cargo_m3 + 1e-9
        {
            return Err("videz la soute d'abord");
        }
        Ok(())
    }

    /// Install a spare module from stock into `slot`.
    pub fn install_module(&mut self, slot: usize, mi: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if self.module_stock.get(mi).copied().unwrap_or(0) == 0 {
            return Err("module absent du stock");
        }
        self.ship.design.install(&self.data, slot, mi)?;
        self.module_stock[mi] -= 1;
        self.ship.sys.health[slot] = 1.0;
        self.ship.refresh_stats(&self.data);
        Ok(())
    }

    /// Remove the module in `slot` back into stock.
    pub fn uninstall_module(&mut self, slot: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        let mut trial = self.ship.design.clone();
        let mi = trial.remove(slot).ok_or("emplacement vide")?;
        self.design_fits(&trial)?;
        self.ship.design = trial;
        self.module_stock[mi] += 1;
        self.ship.refresh_stats(&self.data);
        Ok(())
    }

    /// Trade the current hull in for a new (empty) one; modules go to stock.
    pub fn buy_hull(&mut self, hi: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let new = self.data.hulls.get(hi).ok_or("coque inconnue")?;
        if hi == self.ship.design.hull {
            return Err("deja cette coque");
        }
        if self.ship.cargo.mass > 0.0 {
            return Err("videz la soute d'abord");
        }
        let trade_in = self.data.hulls[self.ship.design.hull].cost * RESALE;
        let price = new.cost - trade_in;
        if price > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= price;
        for m in self.ship.design.slots.iter().flatten() {
            self.module_stock[*m] += 1;
        }
        self.ship.design = ShipDesign::empty(&self.data, hi);
        self.ship.sys = Systems::new(&self.ship.design, 0.0);
        self.ship.refresh_stats(&self.data);
        self.ship.hull = self.ship.stats.hull_max;
        Ok(price)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vector_world::generate_system;

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
    }

    /// A system with a single, nearly massless star: gravity-free flight.
    fn void_system(star_radius: f64) -> System {
        let mut s = generate_system(1);
        s.bodies.truncate(1);
        s.belts.clear();
        s.bodies[0].mass = 1.0e3;
        s.bodies[0].radius = star_radius;
        s
    }

    fn sim_with(sys: System, pos: Vec3, vel: Vec3) -> Sim {
        let mut s = Sim::new(sys, data());
        s.ship.pos = pos;
        s.ship.vel = vel;
        s.update_ref();
        s
    }

    fn void_sim() -> Sim {
        sim_with(void_system(1.0), Vec3::new(0.0, 0.0, -1.0e9), Vec3::ZERO)
    }

    fn first_planet(sys: &System) -> usize {
        sys.bodies
            .iter()
            .position(|b| matches!(b.kind, BodyKind::Planet(_)))
            .expect("system has a planet")
    }

    fn system_with<F: Fn(&vector_world::CelBody) -> bool>(f: F) -> (System, usize) {
        for seed in 0..2000u64 {
            let sys = generate_system(seed);
            if let Some(i) = sys.bodies.iter().position(&f) {
                return (sys, i);
            }
        }
        panic!("no system with requested body");
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
        let ideal = s.ship.stats.isp * G0 * (m0 / s.ship.mass()).ln();
        let dv = s.ship.vel.length();
        assert!(
            (dv - ideal).abs() / ideal < 0.02,
            "dv {dv} vs ideal {ideal}"
        );
    }

    #[test]
    fn assist_stops_rotation_and_drift_near_a_reference_body() {
        let mut s = sim_with(
            void_system(100.0),
            Vec3::new(0.0, 0.0, -3000.0),
            Vec3::new(30.0, -20.0, 50.0),
        );
        // Treat the massless "star" as a moon so it can serve as reference frame.
        s.bodies[0].kind = BodyKind::Moon(vector_world::PlanetKind::Barren);
        s.ship.ang = Vec3::new(1.0, 0.5, -1.0);
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
    fn collision_pushes_out_damages_hull_and_modules() {
        let mut s = sim_with(
            void_system(500.0),
            Vec3::new(0.0, 0.0, -1000.0),
            Vec3::new(0.0, 0.0, 400.0),
        );
        let max = s.ship.stats.hull_max;
        let mut hit = false;
        for _ in 0..90 {
            s.step(&ShipInput::default());
            hit |= s.events.iter().any(|e| matches!(e, Event::Impact { .. }));
            assert!(s.ship.pos.length() >= 500.0 + SHIP_RADIUS - 1e-6);
        }
        assert!(hit && s.ship.hull < max);
        assert!(
            s.ship.sys.health.iter().any(|&h| h < 1.0),
            "an impact must hurt some module"
        );
    }

    #[test]
    fn deterministic_replay() {
        let run = || {
            let mut s = sim_with(generate_system(5), Vec3::new(1.0e9, 0.0, 0.0), Vec3::ZERO);
            for i in 0..500 {
                let a = (i as f64 * 0.1).sin();
                s.step(&ShipInput {
                    pitch: a,
                    fwd: 1.0,
                    yaw: -a,
                    ..Default::default()
                });
            }
            (s.ship.pos, s.ship.fuel, s.ship.sys.temp_k)
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn ship_in_circular_orbit_stays_bound() {
        let sys = generate_system(11);
        let pi = first_planet(&sys);
        let mut s = Sim::new(sys, data());
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
        assert_eq!(s.ship.hull, s.ship.stats.hull_max);
    }

    #[test]
    fn autopilot_reaches_a_planet() {
        let sys = generate_system(11);
        let pi = first_planet(&sys);
        let mut s = Sim::new(sys, data());
        s.ship.pos = s.bodies[pi].pos + Vec3::new(2.0e9, 3.0e8, 1.0e9);
        s.ship.vel = s.bodies[pi].vel;
        s.update_ref();
        s.autopilot = Some(Autopilot::new(&s, pi).expect("autopilot should accept"));
        let mut arrived = false;
        for _ in 0..400 {
            s.events.clear();
            s.advance(600.0, &ShipInput::default());
            if s.events.iter().any(|e| matches!(e, Event::Arrived { .. })) {
                arrived = true;
                break;
            }
        }
        let d = (s.bodies[pi].pos - s.ship.pos).length();
        assert!(arrived, "autopilot did not arrive, dist {d}");
        assert!(s.ship.hull == s.ship.stats.hull_max && s.ship.fuel > 0.0);
    }

    /// Put the ship 800 m from asteroid `ai`, aimed at it, co-moving.
    fn aim_at(s: &mut Sim, ai: usize) {
        let b = &s.bodies[ai];
        let off = Vec3::new(0.0, 0.0, -(800.0 + b.radius));
        s.ship.pos = b.pos + off;
        s.ship.vel = b.vel;
        s.ship.rot = Mat3::IDENTITY;
        s.target = Some(ai);
        s.update_ref();
    }

    #[test]
    fn mining_conserves_mass_and_follows_composition() {
        let (sys, ai) =
            system_with(|b| matches!(b.kind, BodyKind::Asteroid(vector_world::AstType::M)));
        let mut s = Sim::new(sys, data());
        s.ship.assist = false;
        aim_at(&mut s, ai);
        let mass0 = s.remaining_mass(ai);
        let input = ShipInput {
            mine: true,
            ..Default::default()
        };
        for _ in 0..600 {
            aim_at(&mut s, ai);
            s.step(&input);
        }
        let got = s.ship.cargo.mass;
        assert!(got > 200.0, "laser should have yielded ore: {got}");
        assert!(
            (mass0 - s.remaining_mass(ai) - got).abs() < 1e-6,
            "mass must be conserved"
        );
        let iron = s.data.resource_index("iron").expect("iron");
        let nickel = s.data.resource_index("nickel").expect("nickel");
        assert!(s.ship.cargo.kg[iron] > s.ship.cargo.kg[nickel] * 2.0);
        assert!(s.bodies[ai].radius < s.system.bodies[ai].radius);
    }

    #[test]
    fn laser_needs_aim_range_and_power() {
        let (sys, ai) = system_with(|b| matches!(b.kind, BodyKind::Asteroid(_)));
        let mut s = Sim::new(sys, data());
        let mine = ShipInput {
            mine: true,
            ..Default::default()
        };
        aim_at(&mut s, ai);
        s.ship.rot = Mat3::rot_y(1.5); // looking away
        s.step(&mine);
        assert_eq!(s.ship.cargo.mass, 0.0);
        aim_at(&mut s, ai);
        s.ship.pos = s.ship.pos - Vec3::new(0.0, 0.0, 5000.0); // out of range
        s.step(&mine);
        assert_eq!(s.ship.cargo.mass, 0.0);
        // No power: reactor dead and battery empty -> the laser stays off even when aimed.
        aim_at(&mut s, ai);
        let reactor = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.category == "reactor")
            .map(|(i, _)| i)
            .expect("reactor");
        s.ship.sys.health[reactor] = 0.0;
        s.ship.sys.battery_mj = 0.0;
        s.step(&mine);
        assert_eq!(s.ship.cargo.mass, 0.0, "unpowered laser cannot mine");
    }

    #[test]
    fn cargo_capacity_is_enforced() {
        let d = data();
        let mut c = Cargo::new(d.resources.len());
        c.cap_mass = 30_000.0;
        c.cap_vol = 40.0;
        let gold = d.resource_index("gold").expect("gold");
        let regolith = d.resource_index("regolith").expect("regolith");
        assert!(c.add(&d, gold, 1.0e6) < 1.0e6);
        assert!(c.volume <= 40.0 + 1e-9 && c.mass <= 30_000.0 + 1e-9);
        let mut c2 = Cargo::new(d.resources.len());
        c2.cap_mass = 30_000.0;
        c2.cap_vol = 40.0;
        let got = c2.add(&d, regolith, 1.0e6);
        assert!(got <= 30_000.0 + 1e-9 && c2.is_full());
        assert!((c2.remove(&d, regolith, got) - got).abs() < 1e-9 && c2.mass < 1e-9);
    }

    #[test]
    fn docking_trade_and_refuel() {
        let (sys, si) = system_with(|b| b.kind == BodyKind::Station);
        let mut s = Sim::new(sys, data());
        s.spawn_near(si, 3000.0);
        assert!(s.try_dock().is_err(), "too far");
        s.spawn_near(si, s.bodies[si].radius + 200.0);
        s.ship.vel = s.bodies[si].vel + Vec3::new(0.0, 0.0, 100.0);
        assert!(s.try_dock().is_err(), "too fast");
        s.ship.vel = s.bodies[si].vel;
        s.try_dock().expect("dock");
        let iron = s.data.resource_index("iron").expect("iron");
        s.ship.cargo.add(&s.data.clone(), iron, 1000.0);
        let c0 = s.credits;
        let earned = s.sell(iron, None).expect("sell");
        assert!(earned > 0.0 && (s.credits - c0 - earned).abs() < 1e-9 && s.ship.cargo.mass < 1e-9);
        s.ship.fuel = 1000.0;
        s.credits = 1.0e6;
        let (kg, cost) = s.refuel().expect("refuel");
        assert!(kg > 0.0 && cost > 0.0 && s.ship.fuel > 1000.0);
        s.ship.hull = 500.0;
        s.ship.sys.health[0] = 0.5;
        s.repair().expect("repair");
        assert_eq!(s.ship.hull, s.ship.stats.hull_max);
        assert!(s.ship.sys.health.iter().all(|&h| h == 1.0));
        s.undock();
        assert!(s.docked.is_none());
    }

    #[test]
    fn docked_ship_follows_the_station() {
        let (sys, si) = system_with(|b| b.kind == BodyKind::Station);
        let mut s = Sim::new(sys, data());
        s.spawn_near(si, s.bodies[si].radius + 100.0);
        s.try_dock().expect("dock");
        s.advance(3600.0, &ShipInput::default());
        let d = (s.ship.pos - s.bodies[si].pos).length();
        assert!((d - (s.bodies[si].radius + 60.0)).abs() < 1.0);
    }

    fn docked_sim() -> Sim {
        let (sys, si) = system_with(|b| b.kind == BodyKind::Station);
        let mut s = Sim::new(sys, data());
        s.spawn_near(si, s.bodies[si].radius + 100.0);
        s.try_dock().expect("dock");
        s.credits = 5.0e6;
        s
    }

    #[test]
    fn shipyard_buy_install_remove_and_rules() {
        let mut s = docked_sim();
        let radiator = s.data.module_index("radiator_mk1").expect("module");
        assert!(s.install_module(7, radiator).is_err(), "not in stock yet");
        let c0 = s.credits;
        s.shop_buy_module(radiator).expect("buy");
        assert!(s.credits < c0);
        // Free an external slot first (the starter ship uses both).
        let slot = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.category == "radiator")
            .map(|(i, _)| i)
            .expect("radiator slot");
        s.uninstall_module(slot).expect("remove stock radiator");
        let rad_before = s.ship.stats.radiate_kw;
        s.install_module(slot, radiator).expect("install");
        assert!(s.ship.stats.radiate_kw > rad_before);
        assert!(s.install_module(0, radiator).is_err());
        s.uninstall_module(slot).expect("uninstall");
        assert_eq!(s.module_stock[radiator], 1);
        assert!(s.ship.stats.radiate_kw < rad_before + 1.0);
        // Removing the only cargo module while the hold is loaded must be refused.
        let iron = s.data.resource_index("iron").expect("iron");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, iron, 5000.0);
        let cargo_slot = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.category == "cargo")
            .map(|(i, _)| i)
            .expect("cargo");
        assert!(s.uninstall_module(cargo_slot).is_err());
        let sold = s.shop_sell_module(radiator).expect("sell back");
        assert!((sold - s.data.modules[radiator].cost * RESALE).abs() < 1e-9);
    }

    #[test]
    fn buying_a_hull_moves_modules_to_stock() {
        let mut s = docked_sim();
        let hauler = s.data.hull_index("hauler").expect("hull");
        let fitted = s.ship.design.slots.iter().flatten().count();
        let iron = s.data.resource_index("iron").expect("iron");
        let data = Arc::clone(&s.data);
        s.ship.cargo.add(&data, iron, 100.0);
        assert!(s.buy_hull(hauler).is_err(), "cargo must be emptied first");
        s.jettison(iron);
        let price = s.buy_hull(hauler).expect("buy hull");
        assert!(price > 0.0);
        assert_eq!(s.ship.design.hull, hauler);
        assert_eq!(s.module_stock.iter().sum::<u32>() as usize, fitted);
        assert_eq!(s.ship.sys.health.len(), s.data.hulls[hauler].slots.len());
        assert_eq!(s.ship.stats.fuel_cap, 0.0, "empty hull has no tanks");
    }
}
