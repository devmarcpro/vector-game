//! Cargo hold: mass and volume limited, indexed by resource index in `GameData`.
use serde::{Deserialize, Serialize};
use vector_data::GameData;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Cargo {
    pub kg: Vec<f64>,
    pub mass: f64,
    pub volume: f64,
    /// Capacity, set from the installed cargo modules.
    pub cap_mass: f64,
    pub cap_vol: f64,
}

impl Cargo {
    pub fn new(n_resources: usize) -> Self {
        Cargo {
            kg: vec![0.0; n_resources],
            mass: 0.0,
            volume: 0.0,
            cap_mass: 0.0,
            cap_vol: 0.0,
        }
    }

    /// Adds up to `kg` of resource `res`; returns the amount actually accepted.
    pub fn add(&mut self, data: &GameData, res: usize, kg: f64) -> f64 {
        if self.kg.len() <= res || kg <= 0.0 {
            return 0.0;
        }
        let density = data.resources[res].density;
        let by_mass = self.cap_mass - self.mass;
        let by_vol = (self.cap_vol - self.volume) * density;
        let ok = kg.min(by_mass).min(by_vol).max(0.0);
        self.kg[res] += ok;
        self.mass += ok;
        self.volume += ok / density;
        ok
    }

    /// Removes up to `kg`; returns the amount actually removed.
    pub fn remove(&mut self, data: &GameData, res: usize, kg: f64) -> f64 {
        if self.kg.len() <= res || kg <= 0.0 {
            return 0.0;
        }
        let ok = kg.min(self.kg[res]);
        self.kg[res] -= ok;
        self.mass = (self.mass - ok).max(0.0);
        self.volume = (self.volume - ok / data.resources[res].density).max(0.0);
        ok
    }

    pub fn is_full(&self) -> bool {
        self.mass >= self.cap_mass - 1e-6 || self.volume >= self.cap_vol - 1e-9
    }
}
