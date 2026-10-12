//! Versioned binary save format: only the seed plus deltas are stored.
use crate::{market::Market, Ship, Sim};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vector_data::GameData;
use vector_world::generate_system;

pub const MAGIC: [u8; 4] = *b"VECT";
pub const VERSION: u32 = 43;

#[derive(Debug)]
pub enum SaveError {
    BadMagic,
    UnsupportedVersion(u32),
    Corrupt(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::BadMagic => write!(f, "fichier de sauvegarde invalide"),
            SaveError::UnsupportedVersion(v) => {
                write!(
                    f,
                    "sauvegarde d'une autre version du jeu (v{v}, attendue v{VERSION}) : \
                     pas de migration, lancez une nouvelle partie"
                )
            }
            SaveError::Corrupt(e) => write!(f, "sauvegarde corrompue: {e}"),
        }
    }
}
impl std::error::Error for SaveError {}

#[derive(Serialize, Deserialize)]
pub struct SaveGame {
    pub system_seed: u64,
    pub time: f64,
    pub ship: Ship,
    pub credits: f64,
    pub mined: Vec<f64>,
    pub markets: Vec<(usize, Market)>,
    pub docked: Option<usize>,
    pub target: Option<usize>,
    pub module_stock: Vec<u32>,
    pub blueprints: Vec<crate::shipwright::Blueprint>,
    pub next_blueprint: u32,
    pub custom_ships: Vec<crate::yard::CustomShip>,
    pub yard_orders: Vec<crate::yard::YardOrder>,
    pub next_order: u32,
    pub bought_plans: Vec<String>,
    pub crew: crate::crew::Crew,
    pub galaxy_seed: u64,
    pub here: vector_world::galaxy::StarRef,
    pub here_pos: [f64; 3],
    pub visited: Vec<(vector_world::galaxy::StarRef, [f64; 3])>,
    pub deltas: Vec<(vector_world::galaxy::StarRef, crate::voyage::SystemDelta)>,
    pub scan: Vec<u8>,
    pub scan_progress: Vec<f32>,
    pub jump: Option<crate::voyage::JumpState>,
    pub contracts: Vec<crate::economy::Contract>,
    pub done_contracts: Vec<u64>,
    pub bank: crate::economy::Bank,
    pub npcs: Vec<crate::npc::Npc>,
    pub next_npc: u32,
    pub colonies: Vec<crate::colony::Colony>,
    pub next_colony: u32,
    pub fleet: Vec<crate::fleet::FleetShip>,
    pub routes: Vec<crate::fleet::Route>,
    pub next_fleet: u32,
    pub next_route: u32,
    pub fleet_paid_day: i64,
    pub military: crate::arms::Military,
    pub wrecks: Vec<crate::combat::Wreck>,
    pub next_wreck: u32,
    pub wanted_until: Vec<(usize, f64)>,
    pub researched: Vec<bool>,
    pub research_pool: f64,
    pub current_research: Option<usize>,
    pub research_progress: f64,
    pub explored: Vec<bool>,
    pub findings: Vec<String>,
    pub rules: crate::rules::Rules,
    /// Food web of each colony's territory: (colony id, game time, biomass in kg).
    pub ecologies: Vec<(u32, f64, Vec<f32>, Vec<bool>)>,
    /// Every relation of the world with the player (see `relations`).
    pub relations: crate::relations::Relations,
    pub surprises: crate::surprises::SurpriseState,
    pub futures: Vec<crate::futures::Future>,
    pub next_future: u32,
    pub herd: Vec<crate::livestock::Animal>,
    pub herd_counter: i64,
    pub casino_round: i64,
    pub trade: crate::trading::TradeState,
    pub multiverse: crate::multiverse::Multiverse,
    pub rogue: Option<crate::rogue::Rogue>,
    pub rogue_visits: Vec<(u64, u32)>,
    pub births_day: i64,
    pub society: crate::society::SocietyState,
    pub news: Vec<crate::news::NewsItem>,
    pub news_scanned_to: f64,
    pub last_livestock_day: i64,
    pub shares: Vec<(usize, f64)>,
    pub last_dividend_month: i64,
    /// The AI (budget, programs, personality bias) and what the start chose.
    pub mind: crate::mind::Mind,
    pub custom: crate::start_custom::Runtime,
    /// The queue of the navigation computer.
    pub navplan: crate::nav_auto::NavPlan,
}

/// QA tool: names of the top-level fields whose encoding differs between two saves.
pub fn diff_fields(x: &[u8], y: &[u8]) -> Vec<&'static str> {
    let (Ok(a), Ok(b)) = (
        postcard::from_bytes::<SaveGame>(&x[8..]),
        postcard::from_bytes::<SaveGame>(&y[8..]),
    ) else {
        return vec!["(illisible)"];
    };
    fn enc<T: Serialize>(v: &T) -> Vec<u8> {
        postcard::to_allocvec(v).unwrap_or_default()
    }
    let mut out = Vec::new();
    if enc(&a.system_seed) != enc(&b.system_seed) {
        out.push("system_seed");
    }
    if enc(&a.time) != enc(&b.time) {
        out.push("time");
    }
    if enc(&a.ship) != enc(&b.ship) {
        out.push("ship");
    }
    if enc(&a.credits) != enc(&b.credits) {
        out.push("credits");
    }
    if enc(&a.mined) != enc(&b.mined) {
        out.push("mined");
    }
    if enc(&a.markets) != enc(&b.markets) {
        out.push("markets");
    }
    if enc(&a.docked) != enc(&b.docked) {
        out.push("docked");
    }
    if enc(&a.target) != enc(&b.target) {
        out.push("target");
    }
    if enc(&a.module_stock) != enc(&b.module_stock) {
        out.push("module_stock");
    }
    if enc(&a.blueprints) != enc(&b.blueprints) {
        out.push("blueprints");
    }
    if enc(&a.next_blueprint) != enc(&b.next_blueprint) {
        out.push("next_blueprint");
    }
    if enc(&a.custom_ships) != enc(&b.custom_ships) {
        out.push("custom_ships");
    }
    if enc(&a.yard_orders) != enc(&b.yard_orders) {
        out.push("yard_orders");
    }
    if enc(&a.next_order) != enc(&b.next_order) {
        out.push("next_order");
    }
    if enc(&a.bought_plans) != enc(&b.bought_plans) {
        out.push("bought_plans");
    }
    if enc(&a.crew) != enc(&b.crew) {
        out.push("crew");
    }
    if enc(&a.galaxy_seed) != enc(&b.galaxy_seed) {
        out.push("galaxy_seed");
    }
    if enc(&a.here) != enc(&b.here) {
        out.push("here");
    }
    if enc(&a.here_pos) != enc(&b.here_pos) {
        out.push("here_pos");
    }
    if enc(&a.visited) != enc(&b.visited) {
        out.push("visited");
    }
    if enc(&a.deltas) != enc(&b.deltas) {
        out.push("deltas");
    }
    if enc(&a.scan) != enc(&b.scan) {
        out.push("scan");
    }
    if enc(&a.scan_progress) != enc(&b.scan_progress) {
        out.push("scan_progress");
    }
    if enc(&a.jump) != enc(&b.jump) {
        out.push("jump");
    }
    if enc(&a.contracts) != enc(&b.contracts) {
        out.push("contracts");
    }
    if enc(&a.done_contracts) != enc(&b.done_contracts) {
        out.push("done_contracts");
    }
    if enc(&a.bank) != enc(&b.bank) {
        out.push("bank");
    }
    if enc(&a.npcs) != enc(&b.npcs) {
        out.push("npcs");
    }
    if enc(&a.next_npc) != enc(&b.next_npc) {
        out.push("next_npc");
    }
    if enc(&a.colonies) != enc(&b.colonies) {
        out.push("colonies");
    }
    if enc(&a.next_colony) != enc(&b.next_colony) {
        out.push("next_colony");
    }
    if enc(&a.fleet) != enc(&b.fleet) {
        out.push("fleet");
    }
    if enc(&a.routes) != enc(&b.routes) {
        out.push("routes");
    }
    if enc(&a.next_fleet) != enc(&b.next_fleet) {
        out.push("next_fleet");
    }
    if enc(&a.next_route) != enc(&b.next_route) {
        out.push("next_route");
    }
    if enc(&a.fleet_paid_day) != enc(&b.fleet_paid_day) {
        out.push("fleet_paid_day");
    }
    if enc(&a.wrecks) != enc(&b.wrecks) {
        out.push("wrecks");
    }
    if enc(&a.next_wreck) != enc(&b.next_wreck) {
        out.push("next_wreck");
    }
    if enc(&a.wanted_until) != enc(&b.wanted_until) {
        out.push("wanted_until");
    }
    if enc(&a.researched) != enc(&b.researched) {
        out.push("researched");
    }
    if enc(&a.research_pool) != enc(&b.research_pool) {
        out.push("research_pool");
    }
    if enc(&a.current_research) != enc(&b.current_research) {
        out.push("current_research");
    }
    if enc(&a.research_progress) != enc(&b.research_progress) {
        out.push("research_progress");
    }
    if enc(&a.explored) != enc(&b.explored) {
        out.push("explored");
    }
    if enc(&a.rules) != enc(&b.rules) {
        out.push("rules");
    }
    if enc(&a.ecologies) != enc(&b.ecologies) {
        out.push("ecologies");
    }
    if enc(&a.relations) != enc(&b.relations) {
        out.push("relations");
    }
    if enc(&a.surprises) != enc(&b.surprises) {
        out.push("surprises");
    }
    if enc(&a.futures) != enc(&b.futures) {
        out.push("futures");
    }
    if enc(&a.next_future) != enc(&b.next_future) {
        out.push("next_future");
    }
    if enc(&a.herd) != enc(&b.herd) {
        out.push("herd");
    }
    if enc(&a.herd_counter) != enc(&b.herd_counter) {
        out.push("herd_counter");
    }
    if enc(&a.casino_round) != enc(&b.casino_round) {
        out.push("casino_round");
    }
    if enc(&a.trade) != enc(&b.trade) {
        out.push("trade");
    }
    if enc(&a.multiverse) != enc(&b.multiverse) {
        out.push("multiverse");
    }
    if enc(&a.rogue) != enc(&b.rogue) {
        out.push("rogue");
    }
    if enc(&a.rogue_visits) != enc(&b.rogue_visits) {
        out.push("rogue_visits");
    }
    if enc(&a.births_day) != enc(&b.births_day) {
        out.push("births_day");
    }
    if enc(&a.society) != enc(&b.society) {
        out.push("society");
    }
    if enc(&a.news) != enc(&b.news) {
        out.push("news");
    }
    if enc(&a.news_scanned_to) != enc(&b.news_scanned_to) {
        out.push("news_scanned_to");
    }
    if enc(&a.last_livestock_day) != enc(&b.last_livestock_day) {
        out.push("last_livestock_day");
    }
    if enc(&a.ship.pos) != enc(&b.ship.pos) {
        out.push("ship.pos");
    }
    if enc(&a.ship.vel) != enc(&b.ship.vel) {
        out.push("ship.vel");
    }
    if enc(&a.ship.rot) != enc(&b.ship.rot) {
        out.push("ship.rot");
    }
    if enc(&a.ship.ang) != enc(&b.ship.ang) {
        out.push("ship.ang");
    }
    if enc(&a.ship.fuel) != enc(&b.ship.fuel) {
        out.push("ship.fuel");
    }
    if enc(&a.ship.hull) != enc(&b.ship.hull) {
        out.push("ship.hull");
    }
    if enc(&a.ship.assist) != enc(&b.ship.assist) {
        out.push("ship.assist");
    }
    if enc(&a.ship.biosphere) != enc(&b.ship.biosphere) {
        out.push("ship.biosphere");
    }
    if enc(&a.ship.cargo) != enc(&b.ship.cargo) {
        out.push("ship.cargo");
    }
    if enc(&a.ship.design) != enc(&b.ship.design) {
        out.push("ship.design");
    }
    if enc(&a.ship.sys) != enc(&b.ship.sys) {
        out.push("ship.sys");
    }
    if enc(&a.ship.stats) != enc(&b.ship.stats) {
        out.push("ship.stats");
    }
    out
}

impl Sim {
    pub fn save_bytes(&self) -> Result<Vec<u8>, SaveError> {
        let save = SaveGame {
            system_seed: self.system.seed,
            time: self.time,
            ship: self.ship.clone(),
            credits: self.credits,
            mined: self.mined.clone(),
            markets: self.markets.iter().map(|(k, v)| (*k, v.clone())).collect(),
            docked: self.docked,
            target: self.target,
            module_stock: self.module_stock.clone(),
            blueprints: self.blueprints.clone(),
            next_blueprint: self.next_blueprint,
            custom_ships: self.custom_ships.clone(),
            yard_orders: self.yard_orders.clone(),
            next_order: self.next_order,
            bought_plans: self.bought_plans.clone(),
            crew: self.crew.clone(),
            galaxy_seed: self.galaxy.seed,
            here: self.here,
            here_pos: self.here_pos,
            visited: self.visited.iter().map(|(k, v)| (*k, *v)).collect(),
            deltas: self.deltas.iter().map(|(k, v)| (*k, v.clone())).collect(),
            scan: self.scan.clone(),
            scan_progress: self.scan_progress.clone(),
            jump: self.jump.clone(),
            contracts: self.contracts.clone(),
            done_contracts: self.done_contracts.clone(),
            bank: self.bank.clone(),
            npcs: self.npcs.clone(),
            next_npc: self.next_npc,
            colonies: self.colonies.clone(),
            next_colony: self.next_colony,
            fleet: self.fleet.clone(),
            routes: self.routes.clone(),
            next_fleet: self.next_fleet,
            next_route: self.next_route,
            fleet_paid_day: self.fleet_paid_day,
            military: self.military.clone(),
            wrecks: self.wrecks.clone(),
            next_wreck: self.next_wreck,
            wanted_until: self.wanted_until.iter().map(|(k, v)| (*k, *v)).collect(),
            researched: self.researched.clone(),
            research_pool: self.research_pool,
            current_research: self.current_research,
            research_progress: self.research_progress,
            explored: self.explored.clone(),
            findings: self.findings.iter().cloned().collect(),
            rules: self.rules.clone(),
            relations: self.relations.clone(),
            surprises: self.surprises.clone(),
            futures: self.futures.clone(),
            next_future: self.next_future,
            herd: self.herd.clone(),
            herd_counter: self.herd_counter,
            casino_round: self.casino_round,
            trade: self.trade.clone(),
            multiverse: self.multiverse.clone(),
            rogue: self.rogue.clone(),
            rogue_visits: self.rogue_visits.clone(),
            births_day: self.births_day,
            society: self.society.clone(),
            news: self.news.clone(),
            news_scanned_to: self.news_scanned_to,
            last_livestock_day: self.last_livestock_day,
            shares: self.shares.clone(),
            last_dividend_month: self.last_dividend_month,
            mind: self.mind.clone(),
            custom: self.custom.clone(),
            navplan: self.nav.clone(),
            ecologies: self
                .ecologies
                .iter()
                .map(|(k, e)| {
                    (
                        *k,
                        e.time,
                        e.bio.iter().map(|&v| v as f32).collect(),
                        e.native.clone(),
                    )
                })
                .collect(),
        };
        let mut out = MAGIC.to_vec();
        out.extend_from_slice(&VERSION.to_le_bytes());
        let body = postcard::to_allocvec(&save).map_err(|e| SaveError::Corrupt(e.to_string()))?;
        out.extend_from_slice(&body);
        Ok(out)
    }

    pub fn load_bytes(bytes: &[u8], data: Arc<GameData>) -> Result<Sim, SaveError> {
        if bytes.len() < 8 || bytes[..4] != MAGIC {
            return Err(SaveError::BadMagic);
        }
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != VERSION {
            return Err(SaveError::UnsupportedVersion(version));
        }
        let save: SaveGame =
            postcard::from_bytes(&bytes[8..]).map_err(|e| SaveError::Corrupt(e.to_string()))?;
        let mut system = generate_system(save.system_seed);
        if save.multiverse.dimension > 0 {
            vector_world::ensure_portal(&mut system);
        }
        let mut sim = Sim::new(system, data);
        if save.mined.len() != sim.mined.len()
            || save.ship.cargo.kg.len() != sim.data.resources.len()
        {
            return Err(SaveError::Corrupt(
                "incompatible avec les donnees actuelles".into(),
            ));
        }
        sim.blueprints = save.blueprints;
        sim.next_blueprint = save.next_blueprint;
        sim.custom_ships = save.custom_ships;
        sim.restore_custom();
        sim.yard_orders = save.yard_orders;
        sim.next_order = save.next_order;
        sim.bought_plans = save.bought_plans;
        sim.galaxy = vector_world::galaxy::Galaxy::new(save.galaxy_seed);
        sim.here = save.here;
        sim.here_pos = save.here_pos;
        sim.visited = save.visited.into_iter().collect();
        sim.deltas = save.deltas.into_iter().collect();
        if save.scan.len() == sim.scan.len() {
            sim.scan = save.scan;
            sim.scan_progress = save.scan_progress;
        }
        sim.jump = save.jump;
        sim.contracts = save.contracts;
        sim.done_contracts = save.done_contracts;
        sim.bank = save.bank;
        sim.compute_faction();
        sim.npcs = save.npcs;
        sim.next_npc = save.next_npc;
        sim.colonies = save.colonies;
        sim.next_colony = save.next_colony;
        sim.fleet = save.fleet;
        sim.routes = save.routes;
        sim.next_fleet = save.next_fleet;
        sim.next_route = save.next_route;
        sim.fleet_paid_day = save.fleet_paid_day;
        sim.military = save.military;
        sim.wrecks = save.wrecks;
        sim.next_wreck = save.next_wreck;
        sim.wanted_until = save.wanted_until.into_iter().collect();
        if save.researched.len() == sim.researched.len() {
            sim.researched = save.researched;
        }
        sim.rules = save.rules;
        sim.relations = save.relations;
        sim.surprises = save.surprises;
        sim.futures = save.futures;
        sim.next_future = save.next_future;
        sim.herd = save.herd;
        sim.herd_counter = save.herd_counter;
        sim.casino_round = save.casino_round;
        sim.trade = save.trade;
        sim.multiverse = save.multiverse;
        sim.rogue = save.rogue;
        sim.rogue_visits = save.rogue_visits;
        sim.births_day = save.births_day;
        sim.society = save.society;
        sim.news = save.news;
        sim.news_scanned_to = save.news_scanned_to;
        sim.last_livestock_day = save.last_livestock_day;
        sim.shares = save.shares;
        sim.last_dividend_month = save.last_dividend_month;
        sim.mind = save.mind;
        sim.custom = save.custom;
        sim.nav = save.navplan;
        // A loaded game never flies by itself: the plan that was running waits for the pilot.
        if sim.nav.run == crate::nav_auto::Run::Running {
            sim.nav.run = crate::nav_auto::Run::Paused;
        }
        for (id, time, bio, native) in save.ecologies {
            let Some(ci) = sim.colonies.iter().position(|c| c.id == id) else {
                continue;
            };
            let Some((map, city)) = sim.colony_world(ci) else {
                continue;
            };
            let b = map.biome_at(city.0, city.1);
            let mut eco = crate::ecology::Ecosystem::region(&sim.data, &map, b, false);
            if eco.bio.len() == bio.len() {
                eco.bio = bio.iter().map(|&v| f64::from(v)).collect();
                eco.time = time;
                eco.native = native;
                sim.ecologies.insert(id, eco);
            }
        }
        sim.research_pool = save.research_pool;
        sim.findings = save.findings.into_iter().collect();
        sim.current_research = save.current_research;
        sim.research_progress = save.research_progress;
        if save.explored.len() == sim.explored.len() {
            sim.explored = save.explored;
        }
        sim.time = save.time;
        sim.ship = save.ship;
        sim.credits = save.credits;
        sim.mined = save.mined;
        sim.markets = save.markets.into_iter().collect();
        sim.docked = save.docked;
        sim.target = save.target;
        sim.crew = save.crew;
        if save.module_stock.len() == sim.module_stock.len() {
            sim.module_stock = save.module_stock;
        }
        sim.ship.refresh_stats(&sim.data.clone());
        sim.refresh_bodies();
        for i in 0..sim.bodies.len() {
            let m = sim.system.bodies[i].mass;
            let frac = (1.0 - sim.mined[i] / m).max(0.0);
            sim.bodies[i].radius = sim.system.bodies[i].radius * frac.cbrt();
        }
        // Positions of the traffic derive from the clock and the bodies: only now are both right.
        sim.place_npcs();
        sim.update_ref();
        Ok(sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShipInput, SIM_DT};

    fn data() -> Arc<GameData> {
        crate::test_support::data()
    }

    #[test]
    fn roundtrip_and_continuation_are_identical() {
        let mut a = Sim::new(generate_system(42), data());
        a.ship.pos = a.bodies[1].pos + vector_core::Vec3::new(1.0e8, 0.0, 0.0);
        a.credits = 1234.5;
        a.mined[1] = 10.0;
        a.market(0);
        for _ in 0..200 {
            a.step(&ShipInput {
                fwd: 0.5,
                yaw: 0.2,
                ..Default::default()
            });
        }
        let bytes = a.save_bytes().expect("save");
        let mut b = Sim::load_bytes(&bytes, data()).expect("load");
        assert_eq!(a.ship.pos, b.ship.pos);
        assert_eq!(a.credits, b.credits);
        for _ in 0..200 {
            let i = ShipInput {
                fwd: 0.3,
                pitch: -0.1,
                ..Default::default()
            };
            a.step(&i);
            b.step(&i);
        }
        assert_eq!(a.ship.pos, b.ship.pos);
        assert_eq!(a.ship.fuel, b.ship.fuel);
        assert!((a.time - b.time).abs() < SIM_DT);
    }

    #[test]
    fn rejects_garbage_and_wrong_version() {
        assert!(matches!(
            Sim::load_bytes(b"nope", data()),
            Err(SaveError::BadMagic)
        ));
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&99u32.to_le_bytes());
        assert!(matches!(
            Sim::load_bytes(&v, data()),
            Err(SaveError::UnsupportedVersion(99))
        ));
        let msg = SaveError::UnsupportedVersion(10).to_string();
        assert!(
            msg.contains("v10")
                && msg.contains(&format!("v{VERSION}"))
                && msg.contains("nouvelle partie"),
            "{msg}"
        );
        let mut v = MAGIC.to_vec();
        v.extend_from_slice(&VERSION.to_le_bytes());
        v.extend_from_slice(&[1, 2, 3]);
        assert!(matches!(
            Sim::load_bytes(&v, data()),
            Err(SaveError::Corrupt(_))
        ));
    }

    #[test]
    fn save_is_small() {
        let s = Sim::new(generate_system(7), data());
        assert!(s.save_bytes().expect("save").len() < 20_000);
    }
    #[test]
    fn reload_keeps_npcs_exactly() {
        let mut a = crate::scene::new_sim(7, data());
        a.advance(20.0 * 86400.0, &ShipInput::default());
        let b1 = a.save_bytes().unwrap();
        let b = Sim::load_bytes(&b1, data()).unwrap();
        let b2 = b.save_bytes().unwrap();
        assert_eq!(diff_fields(&b1, &b2), Vec::<&str>::new());
    }
}
