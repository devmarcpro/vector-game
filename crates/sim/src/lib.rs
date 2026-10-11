//! Deterministic simulation: flight model, modular ships, gravity, collisions, autopilot, mining, trade, saves.
//! Units: SI (m, s, kg, N). See docs/SIM_MODEL.md.
pub mod autopilot;
pub mod births;
pub mod blackmarket;
pub mod bourse;
pub mod cargo;
pub mod casino;
pub mod civ;
pub mod colony;
pub mod combat;
pub mod companies;
pub mod crew;
pub mod design;
pub mod dialogue;
pub mod ecology;
pub mod economy;
pub mod fleet;
pub mod futures;
pub mod gear;
pub mod genealogy;
pub mod intrusion;
pub mod livestock;
pub mod market;
pub mod multiverse;
pub mod npc;
pub mod planet;
pub mod rogue;
pub mod rules;
#[cfg(test)]
mod rules_effects;
pub mod save;
pub mod scene;
pub mod science;
pub mod surgery;
pub mod trading;
pub mod voyage;

use autopilot::Autopilot;
use cargo::Cargo;
use colony::Colony;
use crew::{Crew, CrewCtx};
use design::{Activity, Effective, ShipDesign, ShipStats, Systems, RCS_ISP};
use economy::{Bank, Contract};
use market::Market;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use vector_core::{hash_coord, Mat3, Vec3};
use vector_data::GameData;
use vector_world::galaxy::{Galaxy, StarEntry, StarRef};
use vector_world::{BodyKind, System};
use voyage::{JumpState, SystemDelta};

pub const G0: f64 = 9.806_65;
/// Base fixed step (30 Hz).
pub const SIM_DT: f64 = 1.0 / 30.0;
/// Smallest sub-step used near surfaces.
pub const MIN_DT: f64 = 1.0 / 240.0;

pub const SHIP_RADIUS: f64 = 15.0;
/// Linear flight assist only acts below this speed relative to the reference body (m/s).
pub const ASSIST_MAX_SPEED: f64 = 500.0;
/// Overdrive multipliers: main thrust, turn rate and acceleration, fuel burn.
pub const BOOST_THRUST: f64 = 2.5;
pub const BOOST_TURN: f64 = 1.4;
pub const BOOST_FUEL: f64 = 3.0;
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
    /// Primary weapons (beams, cannons) and secondary weapons (missiles, mines).
    pub fire: bool,
    pub fire_missile: bool,
    /// Overdrive: stronger main engine and turning, at a fuel premium.
    pub boost: bool,
    /// Cancel velocity relative to the target (else the reference body, else the star frame).
    pub brake: bool,
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
    /// Colony (id) whose living world the ship last touched: where contamination comes from.
    pub biosphere: Option<u32>,
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
            biosphere: None,
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
    JumpCharging,
    JumpStarted {
        dist_ly: f64,
    },
    JumpArrived {
        dist_ly: f64,
    },
    JumpAborted(&'static str),
    JumpMishap,
    Customs {
        seized_kg: f64,
        fine: f64,
    },
    ContractDone {
        reward: f64,
    },
    ContractExpired,
    BankSeizure {
        amount: f64,
    },
    PatrolScan,
    PirateAlert,
    ColonyFounded {
        index: usize,
    },
    ColonyNews(String),
    /// A world event that moves prices has just begun around the player.
    MarketNews(String),
    FleetDelivery {
        ship: usize,
        revenue: f64,
    },
    PlayerDestroyed,
    PirateDemand {
        amount: f64,
    },
    NpcDestroyed,
    BountyPaid {
        amount: f64,
    },
    CrimeCommitted,
    PatrolHostile,
    Respawned,
    ResearchDone {
        tech: usize,
    },
    AnomalyExplored {
        kind_id: &'static str,
        research: f64,
        credits: f64,
    },
    RiftJump,
    FlareWarning,
    FlareHit {
        dose: f64,
    },
    MeteorImpact {
        damage: f64,
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
    /// A jump the ship first flies out to (away from every gravity well) and then starts.
    pub jump_run: Option<vector_world::galaxy::StarEntry>,
    /// Held in place next to a body (index, offset) until the player thrusts or sets a new course.
    pub parked: Option<(usize, Vec3)>,
    /// The autopilot's plan for this step: (target, position and velocity relative to it).
    pub ride: Option<(usize, Vec3, Vec3)>,
    /// Goodwill of the native civilisations met, by world key (see `civ`).
    pub civ_rep: BTreeMap<u64, f32>,
    /// Futures signed at the exchange.
    pub futures: Vec<futures::Future>,
    pub next_future: u32,
    /// Shares held: (company index, number).
    pub shares: Vec<(usize, f64)>,
    pub last_dividend_month: i64,
    /// Day of the last intrusion roll (not saved).
    pub intrusion_day: i64,
    /// Animals kept aboard, a counter for the dealer's stock, and the last day they were fed.
    pub herd: Vec<livestock::Animal>,
    pub herd_counter: i64,
    /// Rounds played at the casino (feeds the draws).
    pub casino_round: i64,
    /// Insurance, cartel seats and tenders.
    pub trade: trading::TradeState,
    /// Portals, dimensions and time travel.
    pub multiverse: multiverse::Multiverse,
    /// The roguelike exploration under way, and how many times each place was entered.
    pub rogue: Option<rogue::Rogue>,
    pub rogue_visits: Vec<(u64, u32)>,
    pub births_day: i64,
    pub last_livestock_day: i64,
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
    pub crew: Crew,
    pub galaxy: Galaxy,
    /// The star whose system is currently loaded, and its position (ly).
    pub here: StarRef,
    pub here_pos: [f64; 3],
    pub visited: BTreeMap<StarRef, [f64; 3]>,
    pub deltas: BTreeMap<StarRef, SystemDelta>,
    /// Knowledge level per body (0 unknown .. 3 fully scanned) and survey progress.
    pub scan: Vec<u8>,
    pub scan_progress: Vec<f32>,
    pub jump: Option<JumpState>,
    /// Territory owner of the current system (None = unclaimed) and standing with every faction.
    pub faction: Option<usize>,
    pub reputation: Vec<f32>,
    pub contracts: Vec<Contract>,
    pub done_contracts: Vec<u64>,
    pub bank: Bank,
    pub npcs: Vec<npc::Npc>,
    pub next_npc: u32,
    pub target_npc: Option<usize>,
    pub colonies: Vec<Colony>,
    pub next_colony: u32,
    pub fleet: Vec<fleet::FleetShip>,
    pub routes: Vec<fleet::Route>,
    pub next_fleet: u32,
    pub next_route: u32,
    /// Integration steps executed since creation (diagnostic only, never saved).
    pub steps: u64,
    /// Day number of the last market-news check (not saved).
    pub(crate) last_news_day: i64,
    /// Surface maps of colonised worlds and city sites (derived data, never saved).
    pub(crate) world_cache: std::collections::HashMap<(u64, usize), Option<Arc<planet::PlanetMap>>>,
    pub(crate) city_cache: std::collections::HashMap<u32, (usize, usize)>,
    /// Food web of each colony's territory, by colony id: the only ecology that is saved.
    pub ecologies: BTreeMap<u32, ecology::Ecosystem>,
    pub fleet_paid_day: i64,
    pub projectiles: Vec<combat::Projectile>,
    pub wrecks: Vec<combat::Wreck>,
    pub next_wreck: u32,
    /// Seconds until each module slot may fire again.
    pub weapon_cd: Vec<f64>,
    /// Beams drawn this step: (from, to, fired by the player).
    pub beams: Vec<(Vec3, Vec3, bool)>,
    pub enemy_beams: Vec<(Vec3, Vec3)>,
    pub ecm_range: f64,
    pub destroyed: bool,
    /// Outstanding pirate demand: (npc id, credits).
    pub tribute_due: Option<(u32, f64)>,
    /// Factions that consider the player a criminal until the given time.
    pub wanted_until: BTreeMap<usize, f64>,
    /// Technologies known, research points waiting, and the current project.
    pub researched: Vec<bool>,
    pub research_pool: f64,
    pub current_research: Option<usize>,
    pub research_progress: f64,
    /// Anomalies already explored (per body of the current system).
    pub explored: Vec<bool>,
    pub flare_day: i64,
    pub pending_flare: Option<(f64, f64, bool)>,
    pub env_sensor: f64,
    pub env_agility: f64,
    pub env_gravity: Vec3,
    pub rules: rules::Rules,
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
        BodyKind::Star(_) | BodyKind::Asteroid(_) | BodyKind::Anomaly(_) => 0.0,
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
            jump_run: None,
            parked: None,
            ride: None,
            civ_rep: BTreeMap::new(),
            futures: Vec::new(),
            next_future: 0,
            shares: Vec::new(),
            last_dividend_month: i64::MIN,
            intrusion_day: i64::MIN,
            herd: Vec::new(),
            herd_counter: 0,
            casino_round: 0,
            trade: Default::default(),
            multiverse: Default::default(),
            rogue: None,
            rogue_visits: Vec::new(),
            births_day: i64::MIN,
            last_livestock_day: i64::MIN,
            ref_body: None,
            target: None,
            credits: START_CREDITS,
            mined: vec![0.0; n],
            markets: BTreeMap::new(),
            docked: None,
            laser_on: false,
            effective: Effective::default(),
            crew: Crew::new(),
            galaxy: Galaxy::new(0),
            here: StarRef {
                sector: (0, 0, 0),
                index: 0,
            },
            here_pos: [0.0; 3],
            visited: BTreeMap::new(),
            deltas: BTreeMap::new(),
            scan: Vec::new(),
            scan_progress: Vec::new(),
            jump: None,
            faction: None,
            reputation: Vec::new(),
            contracts: Vec::new(),
            done_contracts: Vec::new(),
            bank: Bank::default(),
            npcs: Vec::new(),
            next_npc: 1,
            target_npc: None,
            colonies: Vec::new(),
            next_colony: 1,
            fleet: Vec::new(),
            routes: Vec::new(),
            next_fleet: 1,
            next_route: 1,
            steps: 0,
            last_news_day: i64::MIN,
            world_cache: std::collections::HashMap::new(),
            city_cache: std::collections::HashMap::new(),
            ecologies: BTreeMap::new(),
            fleet_paid_day: 0,
            projectiles: Vec::new(),
            wrecks: Vec::new(),
            next_wreck: 1,
            weapon_cd: Vec::new(),
            beams: Vec::new(),
            enemy_beams: Vec::new(),
            ecm_range: 0.0,
            destroyed: false,
            tribute_due: None,
            wanted_until: BTreeMap::new(),
            researched: Vec::new(),
            research_pool: 0.0,
            current_research: None,
            research_progress: 0.0,
            explored: vec![false; n],
            flare_day: -1,
            pending_flare: None,
            env_sensor: 1.0,
            env_agility: 1.0,
            env_gravity: Vec3::ZERO,
            rules: rules::Rules::default(),
        };
        s.researched = s.data.techs.iter().map(|t| t.start).collect();
        s.weapon_cd = vec![0.0; s.ship.design.slots.len()];
        s.reputation = vec![0.0; s.data.factions.len()];
        s.scan = Self::initial_scan(&s.system);
        s.scan_progress = vec![0.0; n];
        s.visited.insert(s.here, s.here_pos);
        s.refresh_bodies();
        s.update_ref();
        s
    }

    /// Start in the galaxy `galaxy`, in the system of star `entry`.
    pub fn new_in_galaxy(galaxy: Galaxy, entry: &StarEntry, data: Arc<GameData>) -> Self {
        let mut s = Sim::new(vector_world::generate_system(entry.seed), data);
        s.galaxy = galaxy;
        s.visited.clear();
        s.here = entry.id;
        s.here_pos = entry.pos;
        s.visited.insert(s.here, s.here_pos);
        s.compute_faction();
        s.spawn_traffic();
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

    /// Apply a new rule set (also from the pause menu); reveals the map when asked.
    pub fn set_rules(&mut self, rules: rules::Rules) {
        self.rules = rules;
        if self.rules.reveal_map {
            for l in &mut self.scan {
                *l = (*l).max(voyage::SCAN_SURVEYED);
            }
        }
    }

    /// Replace the ship's design outright (creative mode, tests): `hull` id and module ids in fit order.
    pub fn refit(&mut self, hull: &str, modules: &[&str]) -> Result<(), String> {
        let data = Arc::clone(&self.data);
        let hi = data
            .hull_index(hull)
            .ok_or_else(|| format!("coque inconnue: {hull}"))?;
        let mut design = ShipDesign::empty(&data, hi);
        for id in modules {
            design
                .install_first_fit(&data, id)
                .map_err(|e| format!("{id}: {e}"))?;
        }
        self.ship.design = design;
        self.ship.sys = Systems::new(&self.ship.design, 0.0);
        self.ship.cargo.cap_mass = 0.0;
        self.ship.refresh_stats(&data);
        self.ship.hull = self.ship.stats.hull_max;
        self.ship.fuel = self.ship.stats.fuel_cap;
        self.ship.sys.battery_mj = self.ship.stats.battery_cap;
        self.weapon_cd = vec![0.0; self.ship.design.slots.len()];
        Ok(())
    }

    /// Advance the colonies to the current time (tools, console, tests).
    pub fn update_colonies_pub(&mut self) {
        self.update_colonies();
    }

    /// Recompute the reference body after teleporting the ship (tools, console, tests).
    pub fn update_ref_pub(&mut self) {
        self.update_ref();
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
            if matches!(
                b.kind,
                BodyKind::Station | BodyKind::Asteroid(_) | BodyKind::Anomaly(_)
            ) {
                continue;
            }
            let d = b.pos - p;
            let r2 = d.dot(d).max(1.0);
            a = a + d * (b.gm / (r2 * r2.sqrt()));
        }
        a + self.environment_at(p).gravity
    }

    /// Gravity at `p` without the pull of body `skip`.
    pub fn gravity_excluding(&self, p: Vec3, skip: usize) -> Vec3 {
        let mut a = Vec3::ZERO;
        for (i, b) in self.bodies.iter().enumerate() {
            if i == skip
                || matches!(
                    b.kind,
                    BodyKind::Station | BodyKind::Asteroid(_) | BodyKind::Anomaly(_)
                )
            {
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
        if self.in_transit() {
            return 600.0;
        }
        if self.combat_active() {
            return SIM_DT;
        }
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
        // Under the autopilot the ship follows an analytic plan: long steps are exact.
        let longest = if self.autopilot.is_some() {
            1_800.0
        } else {
            120.0
        };
        dt.clamp(MIN_DT, longest)
    }

    /// Advance by `seconds` of game time, sub-stepping adaptively (time acceleration).
    pub fn advance(&mut self, seconds: f64, input: &ShipInput) {
        let mut left = seconds;
        let mut guard = 0;
        while left > 1e-9 && guard < 200_000 {
            guard += 1;
            let dt = if self.docked.is_some() {
                left.min(3600.0)
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

    pub(crate) fn run_systems(&mut self, act: &Activity, dt: f64) {
        self.livestock_tick();
        self.cartel_tick();
        self.tender_tick();
        self.multiverse_tick();
        self.births_tick();
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
        self.update_crew();
        self.update_colonies();
        self.update_fleet();
        self.tick_research(dt);
    }

    /// Advance the crew layer to the current time and apply what the engineers repaired.
    fn update_crew(&mut self) {
        let data = Arc::clone(&self.data);
        let eff = &self.effective;
        let mut ctx = CrewCtx {
            data: &data,
            cargo: &mut self.ship.cargo,
            credits: &mut self.credits,
            life_ok: eff.life_ok,
            life_capacity: eff.life_capacity,
            crew_cap: self.ship.stats.crew_cap,
            food_per_day: eff.food_per_day,
            medbay: eff.medbay,
            workshop: eff.workshop,
            overheating: self.ship.sys.overheating,
            seed: self.system.seed,
            needs: self.rules.crew_needs,
            event_freq: self.rules.event_freq,
        };
        let out = self.crew.update(&mut ctx, self.time);
        let mut left = out.repair;
        if left > 0.0 {
            let mut order: Vec<usize> = self.ship.design.modules(&data).map(|(i, _)| i).collect();
            order.sort_by(|&a, &b| self.ship.sys.health[a].total_cmp(&self.ship.sys.health[b]));
            for i in order {
                let missing = 1.0 - self.ship.sys.health[i];
                if missing <= 0.0 || left <= 0.0 {
                    break;
                }
                let add = missing.min(left);
                self.ship.sys.health[i] += add;
                left -= add;
            }
            self.ship.refresh_stats(&data);
        }
    }

    pub fn step_dt(&mut self, manual: &ShipInput, dt: f64) {
        self.steps += 1;
        self.laser_on = false;
        if let Some(st) = self.docked {
            self.time += dt;
            self.run_systems(&Activity::default(), dt);
            self.expire_contracts();
            self.bank_tick();
            self.market_news_tick();
            self.futures_tick();
            self.dividend_tick();
            self.intrusion_tick();
            self.refresh_bodies();
            self.update_npcs();
            self.hazard_tick(dt);
            let b = &self.bodies[st];
            self.ship.pos = b.pos - Vec3::new(0.0, 0.0, b.radius + 60.0);
            self.ship.vel = b.vel;
            self.ship.ang = Vec3::ZERO;
            self.update_ref();
            return;
        }
        if self.in_transit() {
            self.transit_step(dt);
            return;
        }
        if self.jump_run.is_some() && self.ref_body.is_none() {
            // Clear of every gravity well: charge the drive now.
            if let Some(star) = self.jump_run.take() {
                if let Err(e) = self.start_jump(&star) {
                    self.events.push(Event::JumpAborted(e));
                }
            }
        }
        let input = match self.autopilot.take() {
            None if self.jump_run.is_some() => self.jump_run_input(),
            Some(mut ap) => {
                let ride = ap.ride(self, dt);
                if let Some(f) = ride.face {
                    // The nose follows the plan (no meaningful rotation dynamics at these steps).
                    self.ship.rot = Mat3::look_at(f, self.ship.rot.col(1));
                    self.ship.ang = Vec3::ZERO;
                }
                self.ride = Some((ap.target, ride.rel_pos, ride.rel_vel));
                match ride.state {
                    autopilot::State::Running => self.autopilot = Some(ap),
                    autopilot::State::Arrived => {
                        // The ship is left parked at its destination: hanging above a world it
                        // would fall on it the moment the pilot lets go.
                        if matches!(
                            self.bodies[ap.target].kind,
                            BodyKind::Planet(_) | BodyKind::Moon(_) | BodyKind::Station
                        ) {
                            self.parked = Some((ap.target, ride.rel_pos));
                        }
                        self.events.push(Event::Arrived { body: ap.target })
                    }
                }
                ride.input
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
            jumping: self.is_charging_jump(),
            firing: input.fire || input.fire_missile,
        };
        self.run_systems(&act, dt);
        let mut eff = self.effective.clone();
        eff.max_rate *= self.env_agility.sqrt();
        eff.ang_accel *= self.env_agility;
        let pilot = Crew::role_index(&self.data, "pilot");
        eff.ang_accel *= (self.crew.role_efficiency(&self.data, pilot, self.time)
            / crew::NO_CREW_EFF)
            .clamp(0.8, 1.3);

        let brake = input.brake && self.autopilot.is_none();
        let boost = input.boost && self.autopilot.is_none();
        if boost {
            eff.thrust *= BOOST_THRUST;
            eff.max_rate *= BOOST_TURN;
            eff.ang_accel *= BOOST_TURN;
        }
        let assist = self.ship.assist || brake;
        // Linear assist is for station-keeping: off under autopilot and at high relative speed.
        // The brake works anywhere and at any speed, against the target if there is one.
        let lin_assist = brake
            || (assist
                && self.autopilot.is_none()
                && self.ref_body.is_some()
                && self.rel_velocity().length() < ASSIST_MAX_SPEED);
        let ref_vel = match (brake, self.target.filter(|&t| t < self.bodies.len())) {
            (true, Some(t)) => self.bodies[t].vel,
            _ => self.ref_body.map_or(Vec3::ZERO, |i| self.bodies[i].vel),
        };
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
        let cmd = if brake {
            Vec3::ZERO
        } else {
            Vec3::new(clamp(input.lat), clamp(input.vert), clamp(input.fwd))
        };
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
        let burn = (mass_flow(main_force, eff.isp) + mass_flow(rcs_force, RCS_ISP))
            * dt
            * self.rules.fuel_mult()
            * if boost && main_force > 0.0 {
                BOOST_FUEL
            } else {
                1.0
            };
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
        let riding = self.ride.take();
        if let Some((bi, rp, rv)) = riding {
            self.ship.pos = self.bodies[bi].pos + rp;
            self.ship.vel = self.bodies[bi].vel + rv;
        }
        if let Some((bi, off)) = self.parked {
            let thrusting = input.fwd != 0.0 || input.lat != 0.0 || input.vert != 0.0;
            if thrusting || self.autopilot.is_some() || self.jump.is_some() || self.docked.is_some()
            {
                self.parked = None;
            } else {
                self.ship.pos = self.bodies[bi].pos + off;
                self.ship.vel = self.bodies[bi].vel;
            }
        }
        self.update_ref();
        // Under the autopilot the plan clears the way: no collisions on the transfer itself.
        if riding.is_none() {
            self.collide();
        }
        self.charge_tick(dt);
        self.scan_tick(dt);
        self.expire_contracts();
        self.bank_tick();
        self.market_news_tick();
        self.update_npcs();
        self.hazard_tick(dt);
        self.drift_wrecks(dt);
        if self.weapon_cd.len() != self.ship.design.slots.len() {
            self.weapon_cd = vec![0.0; self.ship.design.slots.len()];
        }
        self.combat_tick(dt, &input);
        if aimed && eff.mining_rate > 0.0 {
            let miner = Crew::role_index(&self.data, "miner");
            let eff_mult = self.crew.role_efficiency(&self.data, miner, self.time);
            self.mine_tick(dt, eff.mining_rate * eff_mult, eff.mining_range);
        }
    }

    fn collide(&mut self) {
        let mut hits = Vec::new();
        {
            let s = &mut self.ship;
            for (i, b) in self.bodies.iter().enumerate() {
                if matches!(b.kind, BodyKind::Anomaly(_)) {
                    continue;
                }
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
            if speed > 5.0 && !self.rules.invulnerable {
                let damage = damage * self.rules.damage;
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
        let scale = self.ore_scale();
        let mut taken = 0.0;
        for (rid, frac) in &cls.composition {
            if let Some(res) = self.data.resource_index(rid) {
                taken += self.ship.cargo.add(&self.data, res, want * frac * scale) / scale;
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
        let industry = self.station_industry(i);
        let day = (time / bourse::DAY).floor() as i64;
        // What the world does to prices changes slowly: refresh it once a day.
        let stale = self.markets.get(&i).is_none_or(|m| m.world_day != day);
        let world =
            stale.then(|| bourse::world_factors(&data, self.galaxy.seed, self.here_pos, time));
        let m = self.markets.entry(i).or_insert_with(|| {
            Market::new(hash_coord(seed, i as i64, 0, 0), i, &data, time, industry)
        });
        m.relax_to(time);
        if let Some(w) = world {
            m.world = w;
            m.world_day = day;
        }
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

    /// The station the ship is docked at (market services), as opposed to a colony port.
    pub fn docked_station(&self) -> Result<usize, &'static str> {
        let i = self.docked.ok_or("non amarre")?;
        if self.bodies[i].kind == BodyKind::Station {
            Ok(i)
        } else {
            Err("pas de marche sur cette base")
        }
    }

    /// Dock at the spaceport of one of our colonies in this system.
    pub fn try_dock_colony(&mut self) -> Result<usize, &'static str> {
        let data = Arc::clone(&self.data);
        let (ci, range) = self
            .colonies
            .iter()
            .enumerate()
            .filter(|(_, c)| c.star == self.here && c.has_port(&data))
            .map(|(ci, c)| {
                (
                    ci,
                    (self.bodies[c.body].pos - self.ship.pos).length() - self.bodies[c.body].radius,
                )
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .ok_or("aucun astroport colonial")?;
        if range > colony::PORT_RANGE {
            return Err("trop loin de l'astroport");
        }
        if (self.ship.vel - self.bodies[self.colonies[ci].body].vel).length() > 40.0 {
            return Err("vitesse relative trop elevee");
        }
        let body = self.colonies[ci].body;
        self.docked = Some(body);
        self.autopilot = None;
        self.events.push(Event::Docked { body });
        self.biosecurity_check(ci);
        Ok(body)
    }

    pub fn try_dock(&mut self) -> Result<usize, &'static str> {
        // A colony port in reach wins over a more distant station.
        let station_range = self.nearest_station().map(|(_, r)| r);
        let port_ok = self.colonies.iter().any(|c| {
            c.star == self.here
                && c.has_port(&self.data)
                && (self.bodies[c.body].pos - self.ship.pos).length() - self.bodies[c.body].radius
                    <= colony::PORT_RANGE
        });
        if port_ok && station_range.is_none_or(|r| r > DOCK_RANGE) {
            return self.try_dock_colony();
        }
        let (i, range) = self.nearest_station().ok_or("aucune station")?;
        if range > DOCK_RANGE {
            return Err("trop loin de la station");
        }
        if (self.ship.vel - self.bodies[i].vel).length() > DOCK_MAX_SPEED {
            return Err("vitesse relative trop elevee");
        }
        if self.reputation_with(self.faction) <= economy::HOSTILE_REP {
            return Err("acces refuse: reputation hostile");
        }
        self.docked = Some(i);
        self.autopilot = None;
        self.events.push(Event::Docked { body: i });
        if self.bodies[i].kind == BodyKind::Station {
            self.customs_check();
        }
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
        let st = self.docked_station()?;
        let have = self.ship.cargo.kg[res];
        let kg = kg.unwrap_or(have).min(have);
        if kg <= 0.0 {
            return Err("rien a vendre");
        }
        let data = Arc::clone(&self.data);
        let (edge, tax) = (self.trade_edge(), self.tax_rate());
        let earned = self.market(st).sell(&data, res, kg, edge) * (1.0 - tax);
        self.ship.cargo.remove(&data, res, kg);
        self.credits += earned;
        self.change_reputation(self.faction, (earned / 20_000.0) as f32);
        Ok(earned)
    }

    /// Buy up to `kg` of resource `res` into the hold.
    pub fn buy(&mut self, res: usize, kg: f64) -> Result<(f64, f64), &'static str> {
        let st = self.docked_station()?;
        let data = Arc::clone(&self.data);
        let edge = self.trade_edge();
        let ask = self.market(st).ask(&data, res) / edge;
        let room = self.ship.cargo.cap_mass - self.ship.cargo.mass;
        let afford = self.credits / ask;
        let kg = kg.min(afford).min(room).max(0.0);
        if kg <= 0.0 {
            return Err("achat impossible (credits ou soute)");
        }
        let (got, cost) = self.market(st).buy(&data, res, kg, edge);
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
        if !self.rules.infinite_resources {
            self.credits -= cost - refund;
        }
        self.change_reputation(self.faction, ((cost - refund) / 20_000.0) as f32);
        Ok((accepted, cost - refund))
    }

    /// Fill the propellant tank at the station's fuel price. Returns (kg, cost).
    pub fn refuel(&mut self) -> Result<(f64, f64), &'static str> {
        let st = self.docked_station()?;
        let data = Arc::clone(&self.data);
        let fuel = data
            .resource_index("fuel")
            .ok_or("pas de carburant dans les donnees")?;
        let edge = self.trade_edge();
        let ask = self.market(st).ask(&data, fuel) / edge;
        let need = self.ship.stats.fuel_cap - self.ship.fuel;
        let kg = need.min(self.credits / ask);
        if kg < 1e-6 {
            return Err("deja plein ou fonds insuffisants");
        }
        let (got, cost) = self.market(st).buy(&data, fuel, kg, edge);
        self.ship.fuel += got;
        if !self.rules.infinite_resources {
            self.credits -= cost;
        }
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

    // ---- crew services ----------------------------------------------------

    /// Recruits available at the docked station.
    pub fn candidates(&self) -> Vec<crew::Person> {
        match self.docked {
            Some(st) => {
                crew::candidates(&self.data, self.system.seed, st, self.time, 6, &self.crew)
            }
            None => Vec::new(),
        }
    }

    /// Hire candidate `k` of the docked station's pool into `role`.
    pub fn hire(&mut self, k: usize, role: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let cand = self
            .candidates()
            .into_iter()
            .nth(k)
            .ok_or("candidat inconnu")?;
        if self
            .crew
            .members
            .iter()
            .any(|m| m.alive && m.name == cand.name)
        {
            return Err("deja a bord");
        }
        if self.crew.count() as f64 >= self.ship.stats.crew_cap {
            return Err("pas assez de couchettes (modules d'habitation)");
        }
        let cost = crew::hire_cost(&cand);
        if cost > self.credits {
            return Err("fonds insuffisants");
        }
        self.credits -= cost;
        self.crew.hire(&self.data, cand, role, self.time);
        Ok(cost)
    }

    pub fn dismiss(&mut self, idx: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        if idx >= self.crew.members.len() {
            return Err("membre inconnu");
        }
        self.crew.dismiss(&self.data, idx, self.time);
        Ok(())
    }

    pub fn set_role(&mut self, idx: usize, role: usize) {
        if let Some(p) = self.crew.members.get_mut(idx) {
            p.role = role % self.data.roles.len();
        }
    }

    // ---- shipyard ---------------------------------------------------------

    /// Buy a module into the spare-parts stock.
    pub fn shop_buy_module(&mut self, mi: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let module = self.data.modules.get(mi).ok_or("module inconnu")?;
        if !self.is_unlocked("module", &module.id) {
            return Err("technologie non recherchee (F4, ou regle: technologies)");
        }
        let cost = if self.rules.infinite_resources {
            0.0
        } else {
            module.cost * self.rules.build_cost
        };
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

    /// Replace the module in `slot` by `mi` in one go: buy `mi` if none is in stock, send the old
    /// module to stock, install the new one. Everything is rolled back if any step fails.
    pub fn swap_module(&mut self, slot: usize, mi: usize) -> Result<(), &'static str> {
        self.docked.ok_or("non amarre")?;
        let saved = (
            self.ship.design.clone(),
            self.module_stock.clone(),
            self.credits,
        );
        let go = |s: &mut Sim| -> Result<(), &'static str> {
            if s.module_stock[mi] == 0 {
                s.shop_buy_module(mi)?;
            }
            if s.ship.design.slots[slot].is_some() {
                s.uninstall_module(slot)?;
            }
            s.install_module(slot, mi)
        };
        let res = go(self);
        if res.is_err() {
            (self.ship.design, self.module_stock, self.credits) = saved;
            self.ship.refresh_stats(&self.data);
        }
        res
    }

    /// Trade the current hull in for a new (empty) one; modules go to stock.
    pub fn buy_hull(&mut self, hi: usize) -> Result<f64, &'static str> {
        self.docked.ok_or("non amarre")?;
        let new = self.data.hulls.get(hi).ok_or("coque inconnue")?;
        if !self.is_unlocked("hull", &new.id) {
            return Err("technologie non recherchee (F4, ou regle: technologies)");
        }
        if hi == self.ship.design.hull {
            return Err("deja cette coque");
        }
        if self.ship.cargo.mass > 0.0 {
            return Err("videz la soute d'abord");
        }
        let trade_in = self.data.hulls[self.ship.design.hull].cost * RESALE;
        let price = if self.rules.infinite_resources {
            0.0
        } else {
            new.cost * self.rules.build_cost - trade_in
        };
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
    fn boost_thrusts_harder_and_burns_more_fuel_per_delta_v() {
        let run = |boost: bool| {
            let mut s = void_sim();
            let f0 = s.ship.fuel;
            let input = ShipInput {
                fwd: 1.0,
                boost,
                ..Default::default()
            };
            for _ in 0..60 {
                s.step(&input);
            }
            (s.ship.vel.length(), f0 - s.ship.fuel)
        };
        let (v0, f0) = run(false);
        let (v1, f1) = run(true);
        assert!(v1 > 2.2 * v0, "boost dv {v1} vs {v0}");
        assert!(
            f1 / v1 > 2.5 * (f0 / v0),
            "fuel per dv {} vs {}",
            f1 / v1,
            f0 / v0
        );
    }

    #[test]
    fn boost_turns_faster() {
        let run = |boost: bool| {
            let mut s = void_sim();
            let input = ShipInput {
                yaw: 1.0,
                boost,
                ..Default::default()
            };
            for _ in 0..120 {
                s.step(&input);
            }
            s.ship.ang.y
        };
        assert!(run(true) > 1.2 * run(false));
    }

    #[test]
    fn brake_kills_drift_in_deep_space_without_a_reference_body() {
        let mut s = void_sim();
        s.ship.assist = false;
        s.ship.vel = Vec3::new(40.0, -25.0, 60.0);
        assert!(s.ref_body.is_none());
        let input = ShipInput {
            brake: true,
            ..Default::default()
        };
        for _ in 0..600 {
            s.step(&input);
        }
        assert!(s.ship.vel.length() < 1e-3, "{:?}", s.ship.vel);
        // Without the brake the same drift persists.
        let mut t = void_sim();
        t.ship.assist = false;
        t.ship.vel = Vec3::new(40.0, -25.0, 60.0);
        for _ in 0..600 {
            t.step(&ShipInput::default());
        }
        assert!(t.ship.vel.length() > 70.0);
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
        s.researched.fill(true);
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
    fn swap_replaces_in_one_step_and_rolls_back_on_failure() {
        let mut s = docked_sim();
        let find = |s: &Sim, id: &str| s.data.module_index(id).expect("module");
        let (mk1, mk3) = (find(&s, "radiator_mk1"), find(&s, "radiator_mk3"));
        let slot = s
            .ship
            .design
            .modules(&s.data)
            .find(|(_, m)| m.category == "radiator")
            .map(|(i, _)| i)
            .expect("radiator slot");
        let old = s.ship.design.slots[slot].expect("occupied");
        let c0 = s.credits;
        s.swap_module(slot, mk1)
            .expect("swap into an occupied slot");
        assert_eq!(s.ship.design.slots[slot], Some(mk1));
        assert_eq!(s.module_stock[old], 1, "the old module goes to stock");
        assert!((c0 - s.credits - s.data.modules[mk1].cost).abs() < 1e-6);
        // Too big for the slot: refused, and nothing changes (no credits lost, stock intact).
        let (design, stock, credits) = (s.ship.design.clone(), s.module_stock.clone(), s.credits);
        assert!(s.swap_module(slot, mk3).is_err());
        assert_eq!(s.ship.design.slots, design.slots);
        assert_eq!(s.module_stock, stock);
        assert_eq!(s.credits, credits);
    }

    #[test]
    fn hiring_respects_capacity_funds_and_survives_saves() {
        let mut s = docked_sim();
        assert_eq!(s.candidates().len(), 6);
        let miner = Crew::role_index(&s.data, "miner");
        let c0 = s.credits;
        let cost = s.hire(0, miner).expect("hire");
        assert!((c0 - s.credits - cost).abs() < 1e-9 && cost > 0.0);
        assert!(
            s.hire(0, miner).is_err(),
            "same person cannot be hired twice"
        );
        assert!(s.ship.stats.crew_cap >= 2.0);
        s.hire(1, miner).expect("second berth");
        assert!(s.hire(2, miner).is_err(), "no third berth");
        let n = s.crew.members.len();
        let bytes = s.save_bytes().expect("save");
        let loaded = Sim::load_bytes(&bytes, data()).expect("load");
        assert_eq!(loaded.crew.members.len(), n);
        assert_eq!(loaded.crew.members[0].name, s.crew.members[0].name);
        s.credits = 0.0;
        s.dismiss(0).expect("dismiss");
        assert_eq!(s.crew.members.len(), n - 1);
    }

    #[test]
    fn engineers_repair_damaged_modules_over_time() {
        let mut s = docked_sim();
        let eng = Crew::role_index(&s.data, "engineer");
        s.hire(0, eng).expect("hire");
        s.crew.members[0].shift = 0;
        let sk = s.data.skill_index("ingenierie").expect("skill");
        s.crew.members[0].skills[sk] = 8.0;
        let data = Arc::clone(&s.data);
        for (id, kg) in [("spare_parts", 40.0), ("food", 50.0), ("water", 100.0)] {
            s.ship
                .cargo
                .add(&data, data.resource_index(id).expect("res"), kg);
        }
        s.ship.sys.health[3] = 0.3;
        s.advance(6.0 * crew::DAY, &ShipInput::default());
        assert!(
            s.ship.sys.health[3] > 0.45,
            "engineer should have mended the module: {}",
            s.ship.sys.health[3]
        );
        let spare = data.resource_index("spare_parts").expect("spare");
        assert!(s.ship.cargo.kg[spare] < 40.0, "repairs consume spare parts");
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
