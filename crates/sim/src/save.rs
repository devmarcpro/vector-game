//! Versioned binary save format: only the seed plus deltas are stored.
use crate::{market::Market, Ship, Sim};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vector_data::GameData;
use vector_world::generate_system;

pub const MAGIC: [u8; 4] = *b"VECT";
pub const VERSION: u32 = 28;

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
    pub crew: crate::crew::Crew,
    pub galaxy_seed: u64,
    pub here: vector_world::galaxy::StarRef,
    pub here_pos: [f64; 3],
    pub visited: Vec<(vector_world::galaxy::StarRef, [f64; 3])>,
    pub deltas: Vec<(vector_world::galaxy::StarRef, crate::voyage::SystemDelta)>,
    pub scan: Vec<u8>,
    pub scan_progress: Vec<f32>,
    pub jump: Option<crate::voyage::JumpState>,
    pub reputation: Vec<f32>,
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
    pub wrecks: Vec<crate::combat::Wreck>,
    pub next_wreck: u32,
    pub wanted_until: Vec<(usize, f64)>,
    pub researched: Vec<bool>,
    pub research_pool: f64,
    pub current_research: Option<usize>,
    pub research_progress: f64,
    pub explored: Vec<bool>,
    pub rules: crate::rules::Rules,
    /// Food web of each colony's territory: (colony id, game time, biomass in kg).
    pub ecologies: Vec<(u32, f64, Vec<f32>, Vec<bool>)>,
    /// Goodwill of the native civilisations (world key, value).
    pub civ_rep: Vec<(u64, f32)>,
    pub futures: Vec<crate::futures::Future>,
    pub next_future: u32,
    pub herd: Vec<crate::livestock::Animal>,
    pub herd_counter: i64,
    pub casino_round: i64,
    pub trade: crate::trading::TradeState,
    pub multiverse: crate::multiverse::Multiverse,
    pub last_livestock_day: i64,
    pub shares: Vec<(usize, f64)>,
    pub last_dividend_month: i64,
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
            crew: self.crew.clone(),
            galaxy_seed: self.galaxy.seed,
            here: self.here,
            here_pos: self.here_pos,
            visited: self.visited.iter().map(|(k, v)| (*k, *v)).collect(),
            deltas: self.deltas.iter().map(|(k, v)| (*k, v.clone())).collect(),
            scan: self.scan.clone(),
            scan_progress: self.scan_progress.clone(),
            jump: self.jump.clone(),
            reputation: self.reputation.clone(),
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
            wrecks: self.wrecks.clone(),
            next_wreck: self.next_wreck,
            wanted_until: self.wanted_until.iter().map(|(k, v)| (*k, *v)).collect(),
            researched: self.researched.clone(),
            research_pool: self.research_pool,
            current_research: self.current_research,
            research_progress: self.research_progress,
            explored: self.explored.clone(),
            rules: self.rules.clone(),
            civ_rep: self.civ_rep.iter().map(|(k, v)| (*k, *v)).collect(),
            futures: self.futures.clone(),
            next_future: self.next_future,
            herd: self.herd.clone(),
            herd_counter: self.herd_counter,
            casino_round: self.casino_round,
            trade: self.trade.clone(),
            multiverse: self.multiverse.clone(),
            last_livestock_day: self.last_livestock_day,
            shares: self.shares.clone(),
            last_dividend_month: self.last_dividend_month,
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
        if save.reputation.len() == sim.reputation.len() {
            sim.reputation = save.reputation;
        }
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
        sim.wrecks = save.wrecks;
        sim.next_wreck = save.next_wreck;
        sim.wanted_until = save.wanted_until.into_iter().collect();
        if save.researched.len() == sim.researched.len() {
            sim.researched = save.researched;
        }
        sim.rules = save.rules;
        sim.civ_rep = save.civ_rep.into_iter().collect();
        sim.futures = save.futures;
        sim.next_future = save.next_future;
        sim.herd = save.herd;
        sim.herd_counter = save.herd_counter;
        sim.casino_round = save.casino_round;
        sim.trade = save.trade;
        sim.multiverse = save.multiverse;
        sim.last_livestock_day = save.last_livestock_day;
        sim.shares = save.shares;
        sim.last_dividend_month = save.last_dividend_month;
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
        sim.current_research = save.current_research;
        sim.research_progress = save.research_progress;
        if save.explored.len() == sim.explored.len() {
            sim.explored = save.explored;
        }
        sim.place_npcs();
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
        sim.update_ref();
        Ok(sim)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ShipInput, SIM_DT};

    fn data() -> Arc<GameData> {
        Arc::new(
            GameData::load(&std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data"))
                .expect("data"),
        )
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
}
