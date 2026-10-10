//! Versioned binary save format: only the seed plus deltas are stored.
use crate::{market::Market, Ship, Sim};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use vector_data::GameData;
use vector_world::generate_system;

pub const MAGIC: [u8; 4] = *b"VECT";
pub const VERSION: u32 = 3;

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
                write!(f, "version de sauvegarde non supportee: {v}")
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
        let mut sim = Sim::new(generate_system(save.system_seed), data);
        if save.mined.len() != sim.mined.len()
            || save.ship.cargo.kg.len() != sim.data.resources.len()
        {
            return Err(SaveError::Corrupt(
                "incompatible avec les donnees actuelles".into(),
            ));
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
